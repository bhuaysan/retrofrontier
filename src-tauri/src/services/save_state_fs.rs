//! The RetroArch save-state filesystem adapter.
//!
//! **Every fact about how RetroArch names a save state lives here and nowhere else.** The domain,
//! the repository, and the application service all speak in slots, digests, and validated relative
//! paths; this module is the only place that knows the pinned RetroArch 1.22.2 layout. A future
//! Runtime upgrade that changes it must break the `retroarch_1_22_2_contract` tests deliberately
//! rather than silently changing what RetroFrontier attributes.
//!
//! ## The pinned layout
//!
//! RetroArch builds its state base from the content basename plus `.state`, inside
//! `savestate_directory`, and appends the slot number for a numbered slot. What that base is
//! depends on one setting RetroFrontier owns:
//!
//! - with `sort_savestates_enable`, RetroArch inserts the **core-reported `sysinfo->library_name`**
//!   as a subdirectory. That is a value RetroFrontier has no authenticated source for: on the
//!   qualified managed runtime it produced `states/Nestopia/`, `states/bsnes-mercury/`, and
//!   `states/dolphin-emu/`, which are emphatically *not* the RetroFrontier `CoreId`s `nestopia`,
//!   `bsnes-mercury-balanced`, and `dolphin`;
//! - **without it, RetroArch inserts nothing at all**, and the base is exactly
//!   `<savestate_directory>/<content basename>.state`.
//!
//! HIGH-2: the generated configuration therefore sets `sort_savestates_enable = false` and points
//! `savestate_directory` at `<states root>/<CoreId>` — a segment RetroFrontier *writes* rather than
//! reads back. Both halves of the target are then RetroFrontier's own, so `state_target` below can
//! compute the exact path a controlled launch will resolve, and every attribution and every
//! authorization compares against that one path rather than against a basename. **Nothing here
//! reverse-maps a directory name to a core**, and the parse result still carries no core field:
//! a directory that merely *looks* like a `CoreId` proves nothing, and only equality with the
//! computed target does.
//!
//! | Slot | File |
//! | --- | --- |
//! | 0 | `<base>.state` — not managed |
//! | N in 1..=999 | `<base>.stateN` |
//! | AUTO | `<base>.state.auto` — not managed |
//! | thumbnail | `<state path>.png` |
//!
//! ## Why the `*at` family
//!
//! `canonicalize` then `remove_file` leaves a window in which the resolved name can be replaced,
//! and the removal would then delete whatever occupies the *pathname* rather than the file that was
//! verified. Every operation here therefore walks the states root by directory handle with
//! `O_DIRECTORY | O_NOFOLLOW`, opens the final component `O_NOFOLLOW`, and reads its identity from
//! that descriptor. Deletion — and the startup quarantine sweep, which finishes an interrupted one
//! — additionally renames the file to a private same-directory quarantine name and re-proves the
//! inode, the device, the size, and the digest *there* before unlinking. **Every** durable record
//! that authorizes that object is retired first, never after, because an inode number outlives
//! nothing but is reusable the moment its last link is gone — see `sweep_delete_quarantine`.
//!
//! What that guarantees, precisely:
//!
//! > RetroFrontier deletes exactly the previously verified regular file under its owned Save-State
//! > root, or deletes nothing — against pathname replacement, symbolic-link traversal, hard links,
//! > a wrong inode, a wrong digest, and ordinary TOCTOU substitution.
//!
//! What it does **not** claim: a hostile *same-user* writer that already holds an open writable
//! descriptor onto the exact inode can still change the file's bytes after the final re-hash, in
//! the instant before the `unlinkat`. That is a documented POSIX limitation, not a closed window —
//! see `delete_verified_managed_file_inner` for the full statement of the accepted threat model.

use crate::adapters::runtime_integrity::HASH_BUFFER_BYTES;
use crate::domain::core::CoreId;
use crate::domain::runtime::{RelativePath, Sha256Digest};
use crate::domain::save_state::{
    LaunchStateBaselineEntry, SaveStateError, SaveStateSlot, MAX_MANAGED_SLOT, MIN_MANAGED_SLOT,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

/// The suffix RetroArch appends to the content basename for its state base.
pub const STATE_SUFFIX: &str = ".state";
/// RetroArch's automatic slot. Deliberately not managed.
pub const AUTO_STATE_SUFFIX: &str = ".state.auto";
/// The suffix RetroArch appends to a state path for its state thumbnail.
pub const THUMBNAIL_SUFFIX: &str = ".png";
/// The private name a file is renamed to for the instant between verification and unlinking.
///
/// It deliberately cannot parse as a state or a thumbnail, so a crash mid-delete leaves an inert
/// file that `sweep_delete_quarantine` removes and reconciliation ignores.
const QUARANTINE_PREFIX: &str = ".rf-delete-";

/// Upper bounds on one state-tree enumeration.
///
/// A tree larger or deeper than this is reported as *incomplete* rather than truncated, because a
/// truncated enumeration would look exactly like files that are really gone — which is the one
/// input that may drive a destructive `missing` transition.
const MAX_SNAPSHOT_ENTRIES: usize = 20_000;
const MAX_SNAPSHOT_DEPTH: usize = 16;

/// What one file in the state tree is, as far as RetroArch's layout is concerned.
///
/// There is no `core` field, by design: a directory name is not a `CoreId`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateCandidate {
    /// A manual slot RetroFrontier manages.
    ManagedSlot(SaveStateSlot),
    /// The state thumbnail of a managed slot, naming the state it belongs to.
    ThumbnailOf(RelativePath),
    /// Anything else: slot 0, the automatic slot, a thumbnail of an unmanaged state, normal save
    /// data, a quarantine leftover, or a file RetroFrontier has no opinion about.
    Unsupported,
}

/// The cheap physical identity of one file, as the baseline and the delta compare it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalIdentity {
    pub size_bytes: u64,
    pub mtime_nanos: i128,
    pub inode: u64,
}

/// One enumeration of the RetroFrontier-owned state tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateTreeSnapshot {
    entries: BTreeMap<RelativePath, PhysicalIdentity>,
    complete: bool,
}

impl StateTreeSnapshot {
    /// Whether the whole owned tree was really enumerated.
    ///
    /// Only a complete enumeration may drive a `missing` transition. An unreadable subdirectory, a
    /// symbolic link where a directory was expected, or a tree beyond the bounds above all make
    /// this false, and reconciliation then registers what it proved and destroys nothing.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Inspection affordance for tests; production compares whole snapshots through
    /// `state_tree_delta` and `contains`.
    #[cfg(test)]
    pub fn get(&self, relative_path: &RelativePath) -> Option<PhysicalIdentity> {
        self.entries.get(relative_path).copied()
    }

    pub fn contains(&self, relative_path: &RelativePath) -> bool {
        self.entries.contains_key(relative_path)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[cfg(test)]
    pub fn entries(&self) -> impl Iterator<Item = (&RelativePath, &PhysicalIdentity)> {
        self.entries.iter()
    }

    /// The baseline entries this snapshot would record for a launch.
    pub fn to_baseline_entries(&self) -> Vec<LaunchStateBaselineEntry> {
        self.entries
            .iter()
            .map(|(relative_path, identity)| LaunchStateBaselineEntry {
                relative_path: relative_path.clone(),
                size_bytes: identity.size_bytes,
                mtime_nanos: identity.mtime_nanos,
                inode: identity.inode,
            })
            .collect()
    }
}

/// Classify one file in the state tree.
///
/// Only the file *name* is interpreted. The directory it sits in is never interpreted at all — see
/// the module documentation for why.
pub fn parse_state_candidate(relative_path: &RelativePath) -> StateCandidate {
    let name = relative_path
        .as_str()
        .rsplit('/')
        .next()
        .unwrap_or_default();

    if let Some(state_name) = name.strip_suffix(THUMBNAIL_SUFFIX) {
        // A thumbnail belongs to a managed slot or to nothing. `<base>.state.auto.png` and
        // `<base>.state.png` therefore fall through to `Unsupported` with their states.
        let Some(parent) = relative_path.parent() else {
            return thumbnail_of(RelativePath::new(state_name));
        };
        return thumbnail_of(parent.join(state_name));
    }

    if name.ends_with(AUTO_STATE_SUFFIX) {
        return StateCandidate::Unsupported;
    }
    let Some((_, slot)) = name.rsplit_once(STATE_SUFFIX) else {
        return StateCandidate::Unsupported;
    };
    // `<base>.state` is slot 0 and is not managed.
    if slot.is_empty() {
        return StateCandidate::Unsupported;
    }
    // RetroArch renders the slot with `%d`, so a leading zero, a sign, whitespace, or a
    // non-ASCII digit is not something it wrote. Refusing is the honest answer: an ambiguous name
    // must never be attributed.
    if slot.len() > 3 || !slot.bytes().all(|byte| byte.is_ascii_digit()) || slot.starts_with('0') {
        return StateCandidate::Unsupported;
    }
    match slot.parse::<u16>().ok().and_then(|slot| {
        (MIN_MANAGED_SLOT..=MAX_MANAGED_SLOT)
            .contains(&slot)
            .then(|| SaveStateSlot::new(slot).ok())
            .flatten()
    }) {
        Some(slot) => StateCandidate::ManagedSlot(slot),
        None => StateCandidate::Unsupported,
    }
}

fn thumbnail_of(
    state_path: Result<RelativePath, crate::domain::runtime::RuntimeError>,
) -> StateCandidate {
    match state_path {
        Ok(state_path)
            if matches!(
                parse_state_candidate(&state_path),
                StateCandidate::ManagedSlot(_)
            ) =>
        {
            StateCandidate::ThumbnailOf(state_path)
        }
        _ => StateCandidate::Unsupported,
    }
}

/// The path RetroArch would write the state thumbnail of `state_path` to.
pub fn thumbnail_relative_path(state_path: &RelativePath) -> Option<RelativePath> {
    RelativePath::new(format!("{}{THUMBNAIL_SUFFIX}", state_path.as_str())).ok()
}

// ------------------------------------------------------------------ state target binding (HIGH-2)

/// The exact expected RetroArch state-file basename for one piece of content, per the pinned
/// RetroArch 1.22.2 layout: the content file's own basename, without its extension.
///
/// `content_relative_path` is the library's own stored relative path to the primary content
/// file — the same file RetroFrontier hands RetroArch as its content argument. RetroArch derives
/// its state base from that file's basename; this reproduces exactly that derivation and nothing
/// more, so it stays in this adapter rather than becoming a domain assumption.
fn content_state_basename(content_relative_path: &str) -> Option<&str> {
    let name = content_relative_path
        .rsplit('/')
        .next()
        .unwrap_or(content_relative_path);
    let base = name.rsplit_once('.').map_or(name, |(base, _)| base);
    (!base.is_empty()).then_some(base)
}

/// The absolute directory a controlled launch of `core_id` must be given as its
/// `savestate_directory` (HIGH-2).
///
/// The generated configuration disables `sort_savestates_enable`, so this directory is the *whole*
/// of the state path RetroArch composes below the states root: it inserts nothing of its own. The
/// segment is the RetroFrontier `CoreId`, which `CoreId::new` already restricts to ASCII
/// alphanumerics, `-`, `_`, and `.` with an alphanumeric first character — so it is always exactly
/// one safe path component, can never be `.`, `..`, or a hidden name, and can never collide with
/// the `.rf-delete-*` quarantine names or the `.rf-delete-journal` directory.
pub fn state_directory(states_root: &Path, core_id: &CoreId) -> std::path::PathBuf {
    states_root.join(core_id.as_str())
}

/// The exact state-tree relative path a controlled RetroArch launch resolves for one
/// (core, content, slot) triple — the *only* file such a launch can read or write for that slot.
///
/// This is the HIGH-2 binding, and it is an equality, not a heuristic. Every input is a fact
/// RetroFrontier controls or has recorded:
///
/// - `core_id` is the core this launch resolved, and it is written verbatim into the generated
///   configuration's `savestate_directory` by [`state_directory`];
/// - `content_relative_path` is the primary content file this launch hands RetroArch;
/// - `slot` is the managed slot, which reaches RetroArch as `--entryslot` and `state_slot`.
///
/// Verified against a real RetroArch 1.22.x binary rather than assumed: with
/// `sort_savestates_enable = false` and `savestate_directory = D`, the frontend logs
/// `Redirecting save state to "D/<content basename>.state"`, and `--entryslot 3` then resolves
/// `D/<content basename>.state3`. With sorting *enabled* the same run resolves
/// `D/Nestopia/<content basename>.state3` — the core-reported `library_name` segment this design
/// exists to stop depending on. See `M9_REVIEW.md`.
///
/// `None` means no target can be named at all (a content path with no usable basename, or a
/// composed path the domain would refuse), which is always a refusal, never a fallback.
pub fn state_target(
    core_id: &CoreId,
    content_relative_path: &str,
    slot: SaveStateSlot,
) -> Option<RelativePath> {
    let base = content_state_basename(content_relative_path)?;
    RelativePath::new(format!(
        "{}/{base}{STATE_SUFFIX}{}",
        core_id.as_str(),
        slot.get()
    ))
    .ok()
}

/// Whether one state-tree relative path *is* the exact target a controlled launch of this core,
/// content, and slot resolves (HIGH-2) — never merely a `.stateN` file found somewhere in the
/// owned tree, and never merely one whose basename happens to match.
///
/// A file under a foreign directory — a leftover `Nestopia/` tree from before this binding, another
/// frontend's states, or a hostile namespace planted to look managed — is not this target and is
/// therefore neither attributable at reconciliation nor loadable at launch, however correct its
/// basename, slot, and digest are.
pub fn is_state_target(
    relative_path: &RelativePath,
    core_id: &CoreId,
    content_relative_path: &str,
    slot: SaveStateSlot,
) -> bool {
    state_target(core_id, content_relative_path, slot)
        .is_some_and(|target| target == *relative_path)
}

/// Enumerate the RetroFrontier-owned state tree.
///
/// Only regular files are recorded. A symbolic link is never followed and never recorded, so a
/// link planted inside the tree cannot smuggle a foreign file into a delta.
///
/// **Descriptor-relative, like every other operation in this module (HIGH-3).** Each directory is
/// opened exactly once, `O_DIRECTORY | O_NOFOLLOW`, and every child — file or directory — is then
/// examined and, if it is itself a directory, opened *relative to that already-open descriptor*.
/// Nothing here ever re-resolves a pathname built by string concatenation (`states_root.join(...)`)
/// a second time the way the previous implementation did: that re-resolution was exactly the
/// window in which a directory observed as a real directory could be replaced by a symlink before
/// the recursive step followed it. Here, if that replacement happens, the subsequent `openat` with
/// `O_NOFOLLOW` simply fails — the walk never has a second pathname lookup to be fooled by.
pub fn snapshot_state_tree(states_root: &Path) -> StateTreeSnapshot {
    snapshot_state_tree_inner(states_root, &SnapshotRaceHooks::default())
}

fn snapshot_state_tree_inner(
    states_root: &Path,
    hooks: &SnapshotRaceHooks<'_>,
) -> StateTreeSnapshot {
    let mut snapshot = StateTreeSnapshot {
        entries: BTreeMap::new(),
        complete: true,
    };
    match rustix::fs::open(states_root, DIRECTORY_OPEN_FLAGS, rustix::fs::Mode::empty()) {
        Ok(root) => walk_fd(root, "", 0, &mut snapshot, hooks),
        // A missing, symlinked, or otherwise unopenable root is uncertainty, not an empty tree.
        Err(_) => snapshot.complete = false,
    }
    snapshot
}

fn walk_fd(
    directory: rustix::fd::OwnedFd,
    prefix: &str,
    depth: usize,
    snapshot: &mut StateTreeSnapshot,
    hooks: &SnapshotRaceHooks<'_>,
) {
    if depth > MAX_SNAPSHOT_DEPTH {
        snapshot.complete = false;
        return;
    }
    // Borrows the descriptor to read its entries; it does not consume or re-resolve it.
    let Ok(entries) = rustix::fs::Dir::read_from(&directory) else {
        // An unreadable directory is uncertainty, not absence.
        snapshot.complete = false;
        return;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            snapshot.complete = false;
            continue;
        };
        let name = entry.file_name();
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        let Ok(name) = name.to_str() else {
            // A non-UTF-8 name cannot become a validated relative path, so the tree cannot be
            // described completely.
            snapshot.complete = false;
            continue;
        };
        // The entry's type and identity, read *relative to the directory descriptor already
        // opened above* — never by re-joining `states_root` with a path string.
        let Ok(stat) = rustix::fs::statat(&directory, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
        else {
            snapshot.complete = false;
            continue;
        };
        let relative = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        };
        let file_type = rustix::fs::FileType::from_raw_mode(stat.st_mode);
        // Test seam only: fires after the type above is known but before it is acted on, so an
        // adversarial test can substitute a symlink in exactly that window.
        hooks.after_type_observed();
        match file_type {
            rustix::fs::FileType::Symlink => {
                // A symbolic link in a directory RetroFrontier owns is an anomaly. It is not part
                // of the managed tree, and the enumeration is reported incomplete so the anomaly
                // can never contribute to a destructive decision.
                snapshot.complete = false;
            }
            rustix::fs::FileType::Directory => {
                // Opened relative to `directory`'s own descriptor, `O_NOFOLLOW`: if this entry was
                // replaced by a symlink since the `statat` above, this fails outright instead of
                // following it — there is no pathname here to have been substituted.
                match rustix::fs::openat(
                    &directory,
                    name,
                    DIRECTORY_OPEN_FLAGS,
                    rustix::fs::Mode::empty(),
                ) {
                    Ok(child) => walk_fd(child, &relative, depth + 1, snapshot, hooks),
                    Err(_) => snapshot.complete = false,
                }
            }
            rustix::fs::FileType::RegularFile => {
                let Ok(relative_path) = RelativePath::new(relative) else {
                    snapshot.complete = false;
                    continue;
                };
                if snapshot.entries.len() >= MAX_SNAPSHOT_ENTRIES {
                    snapshot.complete = false;
                    return;
                }
                snapshot.entries.insert(
                    relative_path,
                    PhysicalIdentity {
                        size_bytes: stat.st_size as u64,
                        mtime_nanos: i128::from(stat.st_mtime) * 1_000_000_000
                            + i128::from(stat.st_mtime_nsec),
                        inode: stat.st_ino,
                    },
                );
            }
            _ => {
                snapshot.complete = false;
            }
        }
    }
}

/// The one adversarial window HIGH-3 closes: after a directory entry's type is observed, before
/// that observation is acted on. Production passes the default (a no-op).
#[cfg(test)]
#[derive(Default)]
struct SnapshotRaceHooks<'a> {
    after_type_observed: Option<&'a (dyn Fn() + Send + Sync)>,
}

#[cfg(not(test))]
#[derive(Default)]
struct SnapshotRaceHooks<'a> {
    marker: std::marker::PhantomData<&'a ()>,
}

impl SnapshotRaceHooks<'_> {
    fn after_type_observed(&self) {
        #[cfg(test)]
        if let Some(hook) = self.after_type_observed {
            hook();
        }
    }
}

fn physical_identity(metadata: &std::fs::Metadata) -> PhysicalIdentity {
    use std::os::unix::fs::MetadataExt;
    PhysicalIdentity {
        size_bytes: metadata.size(),
        mtime_nanos: i128::from(metadata.mtime()) * 1_000_000_000
            + i128::from(metadata.mtime_nsec()),
        inode: metadata.ino(),
    }
}

/// Which files this session's launch changed, relative to its durable baseline.
///
/// A path is a delta when it is new, or when its size, modification time, or inode differ. A file
/// that vanished is deliberately not a delta: reconciliation never attributes an absence.
pub fn state_tree_delta(
    baseline: &[LaunchStateBaselineEntry],
    snapshot: &StateTreeSnapshot,
) -> Vec<RelativePath> {
    let before: BTreeMap<&RelativePath, PhysicalIdentity> = baseline
        .iter()
        .map(|entry| {
            (
                &entry.relative_path,
                PhysicalIdentity {
                    size_bytes: entry.size_bytes,
                    mtime_nanos: entry.mtime_nanos,
                    inode: entry.inode,
                },
            )
        })
        .collect();

    snapshot
        .entries
        .iter()
        .filter(|(relative_path, identity)| {
            before
                .get(*relative_path)
                .is_none_or(|previous| *previous != **identity)
        })
        .map(|(relative_path, _)| relative_path.clone())
        .collect()
}

/// Whether a candidate has stopped changing since the process ended.
///
/// A pathname existing is not evidence that RetroArch finished writing it, so nothing is hashed or
/// registered until this says so. It is a trait purely so tests can make both outcomes reachable
/// without sleeping.
pub trait StabilityProbe: Send + Sync {
    fn is_stable(&self, states_root: &Path, relative_path: &RelativePath) -> bool;
}

/// The production probe: identical physical identity across consecutive observations.
#[derive(Debug, Clone, Copy)]
pub struct PollingStabilityProbe {
    pub samples: u8,
    pub interval: Duration,
}

impl Default for PollingStabilityProbe {
    fn default() -> Self {
        Self {
            samples: 3,
            interval: Duration::from_millis(120),
        }
    }
}

impl StabilityProbe for PollingStabilityProbe {
    fn is_stable(&self, states_root: &Path, relative_path: &RelativePath) -> bool {
        let observe = || {
            states_root
                .join(relative_path.to_path_buf())
                .symlink_metadata()
                .ok()
                .filter(|metadata| metadata.file_type().is_file())
                .map(|metadata| physical_identity(&metadata))
        };
        let Some(first) = observe() else {
            return false;
        };
        for _ in 1..self.samples.max(2) {
            std::thread::sleep(self.interval);
            // A file that vanished, became a link, or changed is not stable — and neither answer
            // is a guess: both mean "do not register this candidate".
            if observe() != Some(first) {
                return false;
            }
        }
        true
    }
}

/// One file whose exact current content RetroFrontier has proved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedStateFile {
    pub relative_path: RelativePath,
    pub sha256: Sha256Digest,
    pub size_bytes: u64,
}

/// Read the exact identity of one managed file, with no expectation to compare against.
///
/// Used by reconciliation, which is *establishing* an identity rather than confirming one.
pub fn hash_managed_file(
    states_root: &Path,
    relative_path: &RelativePath,
) -> Result<VerifiedStateFile, SaveStateError> {
    let opened = open_managed_file(states_root, relative_path)?;
    let size_bytes = opened.identity.size_bytes;
    let sha256 = hash_descriptor(opened.file)?;
    Ok(VerifiedStateFile {
        relative_path: relative_path.clone(),
        sha256,
        size_bytes,
    })
}

/// Confirm that one managed file still *is* what was registered.
///
/// Every check is a refusal, never a repair: a mismatch reports `IntegrityMismatch` and leaves the
/// file exactly as it is. The stored digest is never refreshed from an unexplained change.
pub fn verify_managed_file(
    states_root: &Path,
    relative_path: &RelativePath,
    expected_size: u64,
    expected_sha256: Sha256Digest,
) -> Result<VerifiedStateFile, SaveStateError> {
    let opened = open_managed_file(states_root, relative_path)?;
    if opened.identity.size_bytes != expected_size {
        return Err(SaveStateError::IntegrityMismatch);
    }
    let sha256 = hash_descriptor(opened.file)?;
    if sha256 != expected_sha256 {
        return Err(SaveStateError::IntegrityMismatch);
    }
    Ok(VerifiedStateFile {
        relative_path: relative_path.clone(),
        sha256,
        size_bytes: expected_size,
    })
}

/// Read one managed file's bytes **from the descriptor that verified them**.
///
/// This exists because "verify, then read the path again" is not the same operation. `std::fs::read`
/// resolves the pathname afresh and *follows symbolic links*, so between a successful verification
/// and that second open, a same-user attacker can replace the file with a link to any file of the
/// same length and have its bytes served instead. Deletion already refuses to be fooled that way;
/// reading has to hold the same line, so the digest is computed and the bytes are returned from one
/// descriptor that was opened `O_NOFOLLOW` and never re-resolved.
///
/// `expected_size` bounds the read, so the caller decides how much it is willing to hold.
pub fn read_verified_managed_file(
    states_root: &Path,
    relative_path: &RelativePath,
    expected_size: u64,
    expected_sha256: Sha256Digest,
) -> Result<Vec<u8>, SaveStateError> {
    let opened = open_managed_file(states_root, relative_path)?;
    if opened.identity.size_bytes != expected_size {
        return Err(SaveStateError::IntegrityMismatch);
    }
    let capacity = usize::try_from(expected_size).map_err(|_| SaveStateError::IntegrityMismatch)?;
    let mut file = opened.file;
    let mut bytes = Vec::with_capacity(capacity);
    // One byte beyond the expected size is already a mismatch, so the read is bounded by the
    // registered length rather than by whatever the file now claims to be.
    Read::by_ref(&mut file)
        .take(expected_size.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| SaveStateError::Indeterminate)?;
    if bytes.len() != capacity {
        return Err(SaveStateError::IntegrityMismatch);
    }
    if crate::adapters::runtime_integrity::sha256_bytes(&bytes) != expected_sha256 {
        return Err(SaveStateError::IntegrityMismatch);
    }
    Ok(bytes)
}

/// The cheap re-check the Save-State listing performs.
///
/// Containment, file type, and size only — hashing every state on every list would read the whole
/// state tree to render one screen. The capability it produces is explicitly a snapshot; a
/// same-size tamper is caught by `verify_managed_file` when an action is actually invoked.
pub fn managed_file_matches_size(
    states_root: &Path,
    relative_path: &RelativePath,
    expected_size: u64,
) -> Result<(), SaveStateError> {
    let opened = open_managed_file(states_root, relative_path)?;
    if opened.identity.size_bytes != expected_size {
        return Err(SaveStateError::IntegrityMismatch);
    }
    Ok(())
}

/// Delete exactly the previously verified regular file, or delete nothing — against pathname
/// replacement, symbolic-link traversal, hard links, a wrong inode, a wrong digest, and ordinary
/// TOCTOU substitution.
///
/// The qualifier is load-bearing and is not a hedge: a hostile *same-user* writer already holding an
/// open writable descriptor onto the exact inode remains outside what this can promise. See
/// `delete_verified_managed_file_inner` for the full statement of that accepted limitation.
pub fn delete_verified_managed_file(
    states_root: &Path,
    relative_path: &RelativePath,
    expected_size: u64,
    expected_sha256: Sha256Digest,
) -> Result<(), SaveStateError> {
    delete_verified_managed_file_inner(
        states_root,
        relative_path,
        expected_size,
        expected_sha256,
        &DeleteRaceHooks::default(),
    )
}

/// Finish or clean up every quarantine object a crash may have left behind between the rename and
/// the unlink (HIGH-4).
///
/// A `.rf-delete-*` **name** proves nothing by itself — it is only a naming convention, and a
/// user's own file can coincidentally start with the same prefix. What proves RetroFrontier
/// created a given quarantine object is a matching entry in the durable delete-operation journal,
/// written *before* the object was ever moved there, recording the **physical identity** of the
/// object that was quarantined: its device, inode, size, and content digest. Only a `.rf-delete-*`
/// name with such a proof is touched at all:
///
/// - no matching journal entry, or one that does not parse strictly → left completely alone (it was
///   never proven RF-owned);
/// - a matching entry the file at that name does not satisfy → left in place, not deleted, and
///   **its journal entry is kept** (the same re-verification an in-flight delete performs, so a
///   crash window cannot relax the guarantee an interrupted delete makes over an uninterrupted one);
/// - a matching entry the file at that name still satisfies in full → finished, through the same
///   verify-requarantine-reprove-retire-unlink sequence a live delete uses.
///
/// ## HIGH-7: the record must name an object, not describe bytes
///
/// The journal used to record only `(size, sha256)`. That describes *content*, and content is
/// reproducible by anyone, so a stale entry could be satisfied by a completely different file. The
/// reachable consequence was a cross-startup ownership transfer: a race refused in one sweep leaves
/// a substitute sitting at the first-stage quarantine name; the next startup finds bytes that match
/// the surviving entry, adopts that substitute's own inode as the expected one, and deletes a file
/// RetroFrontier never quarantined and never verified.
///
/// The entry therefore names the object — `device` and `inode` alongside size and digest — and the
/// journaled identity is a **requirement** at every stage, never something learned from whatever
/// currently occupies a pathname. The invariant is exact:
///
/// > A journal entry created for physical file A never authorizes the deletion of physical file B,
/// > even when B has the same name, the same size, and byte-identical content.
///
/// Note that this is a different class from the accepted HIGH-5 residual, and closing it does not
/// touch that one: HIGH-5 is a hostile writer mutating bytes through an already-open descriptor on
/// the *same* inode, which remains narrowed rather than closed. A *different* inode reached through
/// a replaced pathname is fully defeated.
///
/// Naming an inode only holds the invariant while that inode exists, which is what the terminal
/// ordering below is for: an inode number is reusable once its last link is gone, so *every* record
/// naming the object is retired before the object rather than after it (HIGH-8, HIGH-9). The
/// invariant therefore survives inode reuse — not because reuse is prevented, but because no record
/// is ever left to be reused against.
///
/// ## HIGH-6: verifying a descriptor does not license unlinking a *name*
///
/// This used to open the quarantine object, hash it, drop that descriptor, and then
/// `unlinkat(parent, name)`. Linux has no unlink-by-descriptor, so the destructive step named the
/// *pathname* — and a racing same-user process could rename the verified object away and drop an
/// unrelated file at that name in between, making the sweep delete a file it had never seen. The
/// journal proves RetroFrontier once owned that content; it proves nothing about what the pathname
/// resolves to now.
///
/// So the sweep now uses exactly the discipline the live delete path uses: after verifying the
/// object, it moves that directory entry to a *fresh* RetroFrontier-owned second-stage quarantine
/// name with `RENAME_NOREPLACE`, durably journaled before the move, and then re-proves device,
/// inode, size, and digest **at the new name** before unlinking it. A pathname substituted in the
/// race window is carried to the second stage instead, fails that re-proof, is renamed back to
/// where it was found, and is never unlinked.
///
/// ## MEDIUM-5, HIGH-7, HIGH-8 and HIGH-9: when evidence is kept, and when it is retired
///
/// Evidence is kept through every non-terminal failure and retired the moment it would become
/// stale authority — and at the terminal boundary that means *all* of it. The ownership chain is:
///
/// ```text
/// J1 names object A at the first-stage name Q1
///   J2 is written, naming the same physical A, before anything moves
///   Q1 → Q2 (NOREPLACE)
///   Q2 is re-proved against the journaled identity
///   every record naming A — J1, J2, and any redundant record an earlier
///   interrupted generation left — is retired together, durably
///                                                ← the terminal transition (HIGH-8, HIGH-9)
///   Q2 is unlinked
/// ```
///
/// Up to the re-proof there is deliberately no step at which the live object has no durable record:
/// J2 exists before the move, and J1 survives until after the move is proved. A crash anywhere in
/// that region leaves the next startup either the first stage or the second, each with its own
/// proof, and never neither. A transient I/O failure, an identity or content mismatch, an
/// indeterminate verification, and a refused race all *keep* the entry that still names a real
/// object, because forgetting it would strand that object forever. The entry is never rewritten
/// onto whatever now occupies a pathname — adopting a replacement is exactly what these refusals
/// exist to prevent.
///
/// **The last step reverses that priority, and it has to.** `(device, inode)` identifies an object
/// only while the object exists; once its last link is gone the inode number is eligible for reuse,
/// so a record that outlives its object is a capability that some future, unrelated file could
/// satisfy. The authorizing records are therefore retired *before* the unlink rather than after it,
/// and if they cannot all be retired the unlink is not attempted at all.
///
/// HIGH-9 is why that is stated over the *object* rather than over one stage's id. J1 names the
/// same physical object as J2 for as long as the second stage is being proved, and it used to be
/// removed here best-effort by a helper that returns nothing and discards filesystem errors — so a
/// J1 that refused to go outlived the inode it authenticated while J2 retired cleanly and the
/// object was unlinked. The condition enforced is therefore:
///
/// > Before the final link of a quarantined physical object is removed, no durable delete-journal
/// > record may remain anywhere that authorizes that same physical object identity.
///
/// which is a property of the whole journal, proved by enumerating it, and not of whichever id this
/// pass happens to hold. The asymmetry that produces is intended:
///
/// | Crash point | State | Meaning |
/// | --- | --- | --- |
/// | before the final re-proof | Q2 + J1 + J2 | recoverable; the next startup retries |
/// | after the re-proof, before retirement | Q2 + J1 + J2 | recoverable; the next startup retries |
/// | during retirement (partial, or not provable) | Q2 + whatever records remain | nothing is unlinked; the object is kept |
/// | after retirement, before the unlink | Q2 only | inert orphan, never swept again |
/// | after the unlink | neither | done |
///
/// RetroFrontier would rather leak one tiny owned orphan than keep a record that could later
/// authorize a different physical object. Such an orphan is never automatically deleted, and its
/// ownership is never reconstructed from the file's name, size, digest, currently observed inode,
/// or any database row — doing so would rebuild precisely the stale authority this ordering
/// removes. It stays inert: it cannot parse as a state or a thumbnail, so nothing attributes,
/// lists, or loads it either.
///
/// A retirement that removes some records and then cannot prove the rest lands in the same family
/// of outcomes rather than a new one: **the object is kept**, whatever is left of its evidence is
/// left exactly as it is, and nothing is manufactured to replace what has gone. If the record
/// naming the object's current name is still there, a later startup simply retries; if it was among
/// those already removed, the object becomes an inert orphan of the kind above. Both are
/// fail-closed, and neither can produce a record without its object.
///
/// This makes the sweep idempotent and safe to run on every startup: a genuine RetroFrontier
/// delete that crashed between its rename and its unlink is completed exactly as it would have
/// completed uninterrupted, a sweep that cannot finish *before* the terminal transition is retried
/// at the next startup with its evidence intact, and nothing else is ever touched.
pub fn sweep_delete_quarantine(states_root: &Path) -> usize {
    sweep_delete_quarantine_inner(states_root, &SweepRaceHooks::default())
}

fn sweep_delete_quarantine_inner(states_root: &Path, hooks: &SweepRaceHooks<'_>) -> usize {
    let snapshot = snapshot_state_tree(states_root);
    let mut removed = 0;
    for relative_path in snapshot.entries.keys() {
        let name = relative_path
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or_default();
        let Some(id) = name.strip_prefix(QUARANTINE_PREFIX) else {
            continue;
        };
        // Ownership proof first: without a journal entry naming this exact physical object, it is
        // not RetroFrontier's to touch, whatever it is called and whatever it contains.
        let Some(ownership) = read_journal_entry(states_root, id) else {
            continue;
        };
        let Ok(parent) = open_parent_directory(states_root, relative_path) else {
            // An unreadable parent proves nothing; the journal entry stays for the next startup.
            continue;
        };

        // Stage one: prove the object at this pathname *is* the object the journal was written
        // for. Every recorded fact has to hold — device, inode, size, and then content.
        //
        // HIGH-7: the journaled device and inode are a *requirement*, never something learned from
        // whatever currently occupies the name. Learning it was the defect: a byte-identical file
        // on a different inode, left at this name by an earlier refused race, would satisfy a
        // content-only record, have its own inode adopted as the expected one, and be deleted.
        let verified =
            rustix::fs::openat(&parent, name, FILE_OPEN_FLAGS, rustix::fs::Mode::empty())
                .ok()
                .and_then(|descriptor| {
                    let file = std::fs::File::from(descriptor);
                    let metadata = file.metadata().ok()?;
                    if !ownership.describes(&metadata) {
                        return None;
                    }
                    (hash_descriptor(file).ok()? == ownership.sha256).then_some(())
                });
        if verified.is_none() {
            // MEDIUM-5: not deleted, and *not forgotten*. The object stays, its journal entry stays
            // with it, and a later sweep can still tell it apart from a stranger's file. The entry
            // is never rewritten onto whatever now occupies the pathname, either: adopting a
            // replacement is precisely the thing this refusal exists to prevent.
            tracing::warn!(
                "a quarantined save-state file is not the physical object its delete journal \
                 records, or could not be verified; it was left in place with its ownership \
                 evidence intact rather than removed"
            );
            continue;
        }

        // The HIGH-6 race window: everything above spoke about a descriptor, everything below has
        // to speak about a name.
        hooks.after_verified();

        // Stage two: claim a fresh RetroFrontier-owned name for the verified entry, journaled with
        // the *same* physical identity before the move, and `NOREPLACE` so it can never land on top
        // of anything. The first-stage entry is deliberately still in place here: until the second
        // stage is proven, it is the only durable evidence of this object, and there must be no
        // crash window in which neither stage has any.
        let Ok(second_stage) = quarantine_verified_file(states_root, &parent, name, ownership)
        else {
            tracing::warn!(
                "a quarantined save-state file could not be moved to a fresh second-stage name; \
                 it was left in place with its ownership evidence intact"
            );
            continue;
        };
        let second_stage_id = second_stage
            .strip_prefix(QUARANTINE_PREFIX)
            .unwrap_or_default()
            .to_owned();

        // Re-prove *at the new name*, against the journal rather than against a remembered value.
        // If the pathname was substituted in the window above, what moved is the substitute, and
        // this refuses it — on identity, before content even matters.
        let still_ours = rustix::fs::openat(
            &parent,
            second_stage.as_str(),
            FILE_OPEN_FLAGS,
            rustix::fs::Mode::empty(),
        )
        .ok()
        .and_then(|descriptor| {
            let file = std::fs::File::from(descriptor);
            let metadata = file.metadata().ok()?;
            if !ownership.describes(&metadata) {
                return Some(false);
            }
            Some(hash_descriptor(file).ok() == Some(ownership.sha256))
        })
        .unwrap_or(false);

        if !still_ours {
            // Someone else's file was carried here by the rename. Put it back where it was found —
            // `NOREPLACE`, so restoring can never destroy a third file — and delete nothing.
            //
            // The second-stage entry goes only if the restore succeeded, because there is then
            // nothing at that name to prove ownership of. The **first-stage entry stays**: it names
            // RetroFrontier's own object, which is still out there somewhere under whatever name
            // the racing actor moved it to, and it can never be satisfied by the substitute now
            // sitting at the first-stage name, because that substitute is a different inode.
            let restored = rustix::fs::renameat_with(
                &parent,
                second_stage.as_str(),
                &parent,
                name,
                rustix::fs::RenameFlags::NOREPLACE,
            )
            .is_ok();
            if restored {
                remove_journal_entry(states_root, &second_stage_id);
            }
            tracing::warn!(
                restored,
                "a quarantined save-state file was replaced between its verification and its \
                 removal; nothing was deleted"
            );
            continue;
        }

        // HIGH-8: the terminal transition. Up to this point the object was a *recoverable
        // delete-in-progress* and MEDIUM-5's rule applied — keep the evidence, because it is the
        // only thing that makes a retry possible. Re-proving the exact object at its final name
        // changes what this is: it is now a terminal deletion attempt, and from here the danger
        // reverses. A record that outlives its object is a capability over a reusable inode
        // number; losing automatic cleanup of one file is not.
        //
        // HIGH-9: and what must be retired is the *capability over this physical object*, not one
        // stage's id. The first-stage entry still names the very same object — same device, same
        // inode, same size, same digest — and it used to be removed here best-effort with its
        // failure ignored, so a first-stage entry that refused to go survived the destruction of
        // the inode it authenticated: exactly the stale capability this ordering exists to remove,
        // merely one generation older. Retirement is therefore identity-wide, durable, and
        // all-or-nothing, and the unlink is reached only once *no* record anywhere in the journal
        // authorizes this object. A partial or unprovable retirement keeps the object instead.
        if retire_all_authorizing_journal_entries(states_root, ownership).is_err() {
            tracing::warn!(
                "a quarantined save-state file's ownership retirement could not be proven \
                 complete, so its removal was not attempted and the object was left in place; \
                 some of its records may already be gone, and none are ever recreated"
            );
            continue;
        }

        hooks.before_unlink();

        if rustix::fs::unlinkat(&parent, second_stage.as_str(), rustix::fs::AtFlags::empty())
            .is_ok()
        {
            removed += 1;
        } else {
            // The deliberate fail-closed outcome. The record is already gone, and it is **not**
            // recreated: reconstructing ownership from the file's name, size, digest, observed
            // inode, or a database row would rebuild exactly the stale authority this ordering
            // exists to prevent. The file is inert — it cannot parse as a state or a thumbnail, so
            // nothing attributes, lists, or loads it — and no future sweep will ever touch it
            // again.
            tracing::warn!(
                "a quarantined save-state file could not be unlinked after its ownership record \
                 was retired; it was left as an inert orphan and will never be swept automatically"
            );
        }
    }
    removed
}

/// The windows an attacker racing the startup sweep can act in. Production passes `None` for both.
#[cfg(test)]
#[derive(Default)]
struct SweepRaceHooks<'a> {
    /// After the quarantine object's content is verified from a descriptor, before its directory
    /// entry is moved to the second-stage name (HIGH-6).
    after_verified: Option<&'a (dyn Fn() + Send + Sync)>,
    /// After the second-stage re-proof, immediately before the destructive `unlinkat` (MEDIUM-5).
    before_unlink: Option<&'a (dyn Fn() + Send + Sync)>,
}

#[cfg(not(test))]
#[derive(Default)]
struct SweepRaceHooks<'a> {
    marker: std::marker::PhantomData<&'a ()>,
}

impl SweepRaceHooks<'_> {
    fn after_verified(&self) {
        #[cfg(test)]
        if let Some(hook) = self.after_verified {
            hook();
        }
    }

    fn before_unlink(&self) {
        #[cfg(test)]
        if let Some(hook) = self.before_unlink {
            hook();
        }
    }
}

// ------------------------------------------------------------------ no-follow primitives

/// How every managed file is opened.
///
/// `NOFOLLOW` is the safety flag; `NONBLOCK` is the *liveness* flag, and it is not optional. A
/// same-user attacker cannot create a device node without privileges, but they can create a named
/// pipe — and opening a FIFO read-only blocks until a writer arrives. Without `NONBLOCK`, planting
/// one FIFO where a registered state used to be would hang whichever RetroFrontier task touched it
/// forever. With it, the open returns at once and the file-type check below refuses the target. A
/// Unix domain socket cannot be opened with `open` at all, so it refuses itself.
const FILE_OPEN_FLAGS: rustix::fs::OFlags = rustix::fs::OFlags::RDONLY
    .union(rustix::fs::OFlags::NOFOLLOW)
    .union(rustix::fs::OFlags::CLOEXEC)
    .union(rustix::fs::OFlags::NONBLOCK);

/// How every intermediate directory is opened: a symlinked step is `ELOOP`, never a resolution.
const DIRECTORY_OPEN_FLAGS: rustix::fs::OFlags = rustix::fs::OFlags::RDONLY
    .union(rustix::fs::OFlags::DIRECTORY)
    .union(rustix::fs::OFlags::NOFOLLOW)
    .union(rustix::fs::OFlags::CLOEXEC)
    .union(rustix::fs::OFlags::NONBLOCK);

/// One managed file, held open by a descriptor that cannot have followed a symbolic link.
struct OpenedManagedFile {
    /// The directory the file's final component lives in, held open so a later `renameat` and
    /// `unlinkat` act relative to *this* directory rather than re-resolving a pathname.
    parent: rustix::fd::OwnedFd,
    name: String,
    file: std::fs::File,
    identity: PhysicalIdentity,
    device: u64,
}

/// Open one managed file without ever following a symbolic link, at any component.
///
/// A `..` component, an absolute form, and a backslash were already refused by `RelativePath`.
/// What is left is exactly the filesystem's own trickery: a symlinked final component, a symlinked
/// intermediate directory, a directory or FIFO where a file was expected, and a hard link. All of
/// them are refused rather than resolved.
fn open_managed_file(
    states_root: &Path,
    relative_path: &RelativePath,
) -> Result<OpenedManagedFile, SaveStateError> {
    use std::os::unix::fs::MetadataExt;

    let parent = open_parent_directory(states_root, relative_path)?;
    let name = relative_path
        .as_str()
        .rsplit('/')
        .next()
        .ok_or(SaveStateError::UnsafeFilesystemTarget)?
        .to_owned();

    // `ENOENT` is *proof* the file is gone; `ELOOP` and `ENOTDIR` are proof the target is not the
    // managed regular file it must be. Everything else — out of descriptors, an I/O error, a
    // momentarily unreadable tree — proves nothing at all, and must not be allowed to retire a
    // save state whose lifecycle can never be reopened.
    let descriptor = rustix::fs::openat(
        &parent,
        name.as_str(),
        FILE_OPEN_FLAGS,
        rustix::fs::Mode::empty(),
    )
    .map_err(open_failure)?;
    let file = std::fs::File::from(descriptor);
    let metadata = file.metadata().map_err(|_| SaveStateError::Indeterminate)?;
    if !metadata.file_type().is_file() {
        return Err(SaveStateError::UnsafeFilesystemTarget);
    }
    // A hard-linked file is reachable under a name RetroFrontier does not own, so deleting "the"
    // file would not remove the content and verifying it proves less than it appears to.
    if metadata.nlink() != 1 {
        return Err(SaveStateError::UnsafeFilesystemTarget);
    }

    Ok(OpenedManagedFile {
        parent,
        name,
        identity: physical_identity(&metadata),
        device: metadata.dev(),
        file,
    })
}

/// Walk to the directory holding the final component, refusing any symlinked step.
fn open_parent_directory(
    states_root: &Path,
    relative_path: &RelativePath,
) -> Result<rustix::fd::OwnedFd, SaveStateError> {
    let mut current =
        rustix::fs::open(states_root, DIRECTORY_OPEN_FLAGS, rustix::fs::Mode::empty())
            .map_err(open_failure)?;

    let mut components: Vec<&str> = relative_path.as_str().split('/').collect();
    // The final component is the file itself; every earlier one must be a real directory.
    components.pop();
    for component in components {
        current = rustix::fs::openat(
            &current,
            component,
            DIRECTORY_OPEN_FLAGS,
            rustix::fs::Mode::empty(),
        )
        .map_err(open_failure)?;
    }
    Ok(current)
}

/// Classify an open failure into a *proof* or an inconclusive observation.
fn open_failure(error: rustix::io::Errno) -> SaveStateError {
    match error {
        // The file, or a directory on the way to it, is genuinely not there.
        rustix::io::Errno::NOENT => SaveStateError::UnsafeFilesystemTarget,
        // A symbolic link where a real component must be, or a non-directory used as one: both
        // prove the target is not what RetroFrontier registered.
        rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR => {
            SaveStateError::UnsafeFilesystemTarget
        }
        // Everything else is a failed observation, not a fact about the file.
        _ => SaveStateError::Indeterminate,
    }
}

fn hash_descriptor(mut file: std::fs::File) -> Result<Sha256Digest, SaveStateError> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; HASH_BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| SaveStateError::Indeterminate)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let mut output = [0_u8; 32];
    output.copy_from_slice(&digest);
    Ok(Sha256Digest::from_bytes(output))
}

// ------------------------------------------------------------------ delete-operation journal (HIGH-4)

/// The directory RetroFrontier's durable delete-operation journal lives in.
///
/// It lives *inside* the states root, alongside the quarantine objects it proves ownership of,
/// rather than in a second, separately-threaded app-data path: every existing no-follow,
/// states-root-scoped primitive already protects it exactly as it protects everything else
/// RetroFrontier owns here, with no new trust boundary to reason about. Its name is never
/// `.stateN`- or `.png`-shaped, so `parse_state_candidate` never attributes anything under it and
/// no session delta ever notices it.
const DELETE_JOURNAL_DIR: &str = ".rf-delete-journal";

/// Bound on one journal entry's serialized size — comfortably larger than the fixed-shape record
/// below ever is. Anything larger at read time is refused rather than parsed.
const MAX_JOURNAL_ENTRY_BYTES: u64 = 256;

/// The version marker every journal entry starts with.
///
/// It is an explicit marker rather than an inferred field count so a future format change is a
/// deliberate, readable break: an entry whose version is not this one does not parse, and an entry
/// that does not parse proves nothing, which means the quarantine object it names is left strictly
/// alone rather than acted on. That is also exactly how a `size:sha256` record from before HIGH-7
/// is treated — as no proof at all — which is the conservative answer, and the only sound one,
/// since such a record cannot identify a physical file in the first place.
const JOURNAL_VERSION: &str = "rfdj1";

/// What one durable delete-operation journal entry proves: the identity of a **specific physical
/// filesystem object**, not merely of some bytes (HIGH-7).
///
/// Recording only `(size, sha256)` describes *content*, and content is reproducible by anyone. A
/// journal entry is the durable claim "RetroFrontier itself quarantined this object", and it has to
/// survive across process restarts, so it must name the object rather than describe what is inside
/// it. Otherwise a stale entry left at a quarantine pathname by a refused race authorizes whatever
/// later occupies that name with matching bytes — a file RetroFrontier never quarantined and never
/// verified — and a subsequent startup deletes it.
///
/// `device` and `inode` together are that name-independent identity. They are read from the same
/// descriptor whose content was hashed, never from a second pathname lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuarantineOwnership {
    device: u64,
    inode: u64,
    size_bytes: u64,
    sha256: Sha256Digest,
}

impl QuarantineOwnership {
    /// `rfdj1:<device>:<inode>:<size>:<64 hex digest>` — fixed shape, bounded, and containing no
    /// value that is ever used as a path.
    fn render(&self) -> String {
        format!(
            "{JOURNAL_VERSION}:{}:{}:{}:{}",
            self.device,
            self.inode,
            self.size_bytes,
            self.sha256.to_hex()
        )
    }

    /// Strict parsing: exactly the expected version and exactly five fields, each of which must
    /// parse completely. Anything else is `None`, which the caller treats as "not proven" — never
    /// as a partially trusted record.
    fn parse(value: &str) -> Option<Self> {
        let mut fields = value.trim().split(':');
        if fields.next()? != JOURNAL_VERSION {
            return None;
        }
        let device = fields.next()?.parse().ok()?;
        let inode = fields.next()?.parse().ok()?;
        let size_bytes = fields.next()?.parse().ok()?;
        let sha256 = Sha256Digest::from_hex(fields.next()?).ok()?;
        // A sixth field means this is not the record this version writes.
        if fields.next().is_some() {
            return None;
        }
        Some(Self {
            device,
            inode,
            size_bytes,
            sha256,
        })
    }

    /// Whether one open object *is* the object this entry was written for.
    ///
    /// Every recorded fact must hold. A hard-linked file is refused for the same reason
    /// `open_managed_file` refuses one: its content is reachable under a name RetroFrontier does
    /// not own, so unlinking "the" file would not remove it.
    fn describes(&self, metadata: &std::fs::Metadata) -> bool {
        use std::os::unix::fs::MetadataExt;
        metadata.file_type().is_file()
            && metadata.nlink() == 1
            && metadata.dev() == self.device
            && metadata.ino() == self.inode
            && metadata.size() == self.size_bytes
    }
}

/// How many fresh quarantine identifiers one delete will try before giving up.
///
/// A 128-bit random identifier colliding even once is astronomically unlikely; this bound exists
/// only so a genuinely unexpected collision fails the delete instead of looping forever.
const MAX_QUARANTINE_ATTEMPTS: u8 = 8;

#[cfg(test)]
thread_local! {
    /// Test-only override queue for `quarantine_id`, so a test can force a specific (and
    /// therefore collidable) identifier instead of racing real randomness.
    static FORCED_QUARANTINE_IDS: std::cell::RefCell<std::collections::VecDeque<String>> =
        const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}

#[cfg(test)]
fn push_forced_quarantine_id(id: impl Into<String>) {
    FORCED_QUARANTINE_IDS.with(|ids| ids.borrow_mut().push_back(id.into()));
}

#[cfg(test)]
thread_local! {
    /// Journal entries whose removal must fail, so a test can make the retirement of *one*
    /// specific record fail while every other record would retire successfully (HIGH-9).
    static FORCED_JOURNAL_UNLINK_FAILURES: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
    /// Whether committing the journal directory must fail, so a test can prove that a retirement
    /// whose durability cannot be proven refuses the unlink.
    static FORCED_JOURNAL_FSYNC_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn force_journal_unlink_failure(id: impl Into<String>) {
    FORCED_JOURNAL_UNLINK_FAILURES.with(|ids| ids.borrow_mut().insert(id.into()));
}

#[cfg(test)]
fn force_journal_fsync_failure() {
    FORCED_JOURNAL_FSYNC_FAILURE.with(|failing| failing.set(true));
}

/// A collision-resistant, unguessable quarantine identifier — never a predictable `<pid>-<counter>`
/// (HIGH-4).
fn quarantine_id() -> String {
    #[cfg(test)]
    if let Some(id) = FORCED_QUARANTINE_IDS.with(|ids| ids.borrow_mut().pop_front()) {
        return id;
    }
    format!("{:032x}", rand::random::<u128>())
}

/// Open the durable delete-operation journal directory, creating it if this is the first delete.
fn open_or_create_delete_journal_dir(
    states_root: &Path,
) -> Result<rustix::fd::OwnedFd, SaveStateError> {
    let root = rustix::fs::open(states_root, DIRECTORY_OPEN_FLAGS, rustix::fs::Mode::empty())
        .map_err(open_failure)?;
    match rustix::fs::openat(
        &root,
        DELETE_JOURNAL_DIR,
        DIRECTORY_OPEN_FLAGS,
        rustix::fs::Mode::empty(),
    ) {
        Ok(fd) => Ok(fd),
        Err(rustix::io::Errno::NOENT) => {
            rustix::fs::mkdirat(
                &root,
                DELETE_JOURNAL_DIR,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR | rustix::fs::Mode::XUSR,
            )
            .map_err(|_| SaveStateError::DeleteFailed)?;
            rustix::fs::openat(
                &root,
                DELETE_JOURNAL_DIR,
                DIRECTORY_OPEN_FLAGS,
                rustix::fs::Mode::empty(),
            )
            .map_err(open_failure)
        }
        Err(_) => Err(SaveStateError::DeleteFailed),
    }
}

/// Open the journal directory read-only, never creating it: a sweep with nothing to prove should
/// not leave a filesystem side effect behind.
fn open_delete_journal_dir(states_root: &Path) -> Result<rustix::fd::OwnedFd, SaveStateError> {
    let root = rustix::fs::open(states_root, DIRECTORY_OPEN_FLAGS, rustix::fs::Mode::empty())
        .map_err(open_failure)?;
    rustix::fs::openat(
        &root,
        DELETE_JOURNAL_DIR,
        DIRECTORY_OPEN_FLAGS,
        rustix::fs::Mode::empty(),
    )
    .map_err(open_failure)
}

/// Durably claim a quarantine name nothing else could have produced, and move the verified file
/// there — never overwriting an existing destination (HIGH-4).
///
/// A durable journal entry recording the verified object's **physical identity** — device, inode,
/// size, and digest — is written *before* the move, so `sweep_delete_quarantine` can later *prove*,
/// not merely assume, that a given `.rf-delete-*` name still holds the very object RetroFrontier
/// itself put there (HIGH-7). `ownership` must be read from the descriptor whose content was
/// hashed; the rename below does not change it, because a rename moves a directory entry and never
/// the inode it points at. The move itself uses `NOREPLACE`: an (astronomically unlikely) name
/// collision fails and retries with a fresh identifier rather than destroying a file this operation
/// never verified.
fn quarantine_verified_file(
    states_root: &Path,
    parent: &rustix::fd::OwnedFd,
    name: &str,
    ownership: QuarantineOwnership,
) -> Result<String, SaveStateError> {
    let journal = open_or_create_delete_journal_dir(states_root)?;
    let entry = ownership.render();
    debug_assert!(entry.len() as u64 <= MAX_JOURNAL_ENTRY_BYTES);

    for _ in 0..MAX_QUARANTINE_ATTEMPTS {
        let id = quarantine_id();
        // `EXCL` is the ownership claim: a collision here is detected, never silently overwritten.
        let marker = match rustix::fs::openat(
            &journal,
            id.as_str(),
            rustix::fs::OFlags::WRONLY
                .union(rustix::fs::OFlags::CREATE)
                .union(rustix::fs::OFlags::EXCL),
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        ) {
            Ok(descriptor) => descriptor,
            Err(rustix::io::Errno::EXIST) => continue,
            Err(_) => return Err(SaveStateError::DeleteFailed),
        };
        let mut marker_file = std::fs::File::from(marker);
        if marker_file.write_all(entry.as_bytes()).is_err() || marker_file.sync_all().is_err() {
            drop(marker_file);
            let _ = rustix::fs::unlinkat(&journal, id.as_str(), rustix::fs::AtFlags::empty());
            return Err(SaveStateError::DeleteFailed);
        }
        drop(marker_file);

        let quarantine = format!("{QUARANTINE_PREFIX}{id}");
        match rustix::fs::renameat_with(
            parent,
            name,
            parent,
            quarantine.as_str(),
            rustix::fs::RenameFlags::NOREPLACE,
        ) {
            Ok(()) => return Ok(quarantine),
            Err(rustix::io::Errno::EXIST) => {
                let _ = rustix::fs::unlinkat(&journal, id.as_str(), rustix::fs::AtFlags::empty());
                continue;
            }
            Err(_) => {
                let _ = rustix::fs::unlinkat(&journal, id.as_str(), rustix::fs::AtFlags::empty());
                return Err(SaveStateError::DeleteFailed);
            }
        }
    }
    Err(SaveStateError::DeleteFailed)
}

/// One journal entry's name as a single safe path component.
///
/// Journal ids are always produced by `quarantine_id`, but they are read back at sweep time from a
/// *filename*, so a name is revalidated here rather than trusted: never empty, never a path, and
/// never NUL-bearing. Nothing derived from a journal entry's *contents* ever reaches this.
fn journal_entry_name(id: &str) -> Option<std::ffi::CString> {
    if id.is_empty() || id.contains('/') {
        return None;
    }
    std::ffi::CString::new(id).ok()
}

/// Read back one journal entry's recorded physical identity, relative to an already-open journal
/// directory descriptor.
///
/// The distinction between the two negative answers is load-bearing at the terminal boundary:
///
/// - `Ok(None)` — this entry provably authorizes nothing: it is gone, it is not a regular file, it
///   is oversized, or it does not parse strictly. None of those are ever repaired or half-trusted.
/// - `Err(..)` — this entry could not be *read*, so what it authorizes is unknown. A caller
///   deciding whether a destructive unlink is safe must treat that as no answer, never as a no.
fn journal_record_at(
    journal: &rustix::fd::OwnedFd,
    name: &std::ffi::CStr,
) -> Result<Option<QuarantineOwnership>, SaveStateError> {
    let descriptor =
        match rustix::fs::openat(journal, name, FILE_OPEN_FLAGS, rustix::fs::Mode::empty()) {
            Ok(descriptor) => descriptor,
            // Already absent: it authorizes nothing, which is a fact and not an uncertainty.
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            // A symbolic link is the same kind of fact: every reader of the journal opens
            // `NOFOLLOW`, so a link can never be read as a record by any code path here. Treating
            // it as undecidable instead would let one stray link block every future delete.
            Err(rustix::io::Errno::LOOP) => return Ok(None),
            Err(_) => return Err(SaveStateError::DeleteFailed),
        };
    let file = std::fs::File::from(descriptor);
    let metadata = file.metadata().map_err(|_| SaveStateError::DeleteFailed)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_JOURNAL_ENTRY_BYTES {
        return Ok(None);
    }
    // Bounded at the read as well as at the `stat`, so a file that grew in between is refused
    // rather than read. A bounded regular file that cannot be read *at all* is indeterminate, not
    // empty — but contents that are not even text are a fact: no reader here could ever parse them
    // into a record, so they authorize nothing.
    let mut contents = Vec::new();
    file.take(MAX_JOURNAL_ENTRY_BYTES + 1)
        .read_to_end(&mut contents)
        .map_err(|_| SaveStateError::DeleteFailed)?;
    if contents.len() as u64 > MAX_JOURNAL_ENTRY_BYTES {
        return Ok(None);
    }
    let Ok(contents) = std::str::from_utf8(&contents) else {
        return Ok(None);
    };
    Ok(QuarantineOwnership::parse(contents))
}

/// Read back one journal entry's recorded physical identity by id, or `None` if it does not exist,
/// cannot be read, is unsafe, is oversized, or does not parse strictly — every one of those is
/// treated as "not proven", never as a fact to act on, and never as something to repair.
fn read_journal_entry(states_root: &Path, id: &str) -> Option<QuarantineOwnership> {
    let name = journal_entry_name(id)?;
    let journal = open_delete_journal_dir(states_root).ok()?;
    journal_record_at(&journal, &name).ok().flatten()
}

/// Remove one journal entry by name, relative to an open journal directory.
///
/// Every removal of an *established* record goes through here — the non-terminal cleanups and the
/// terminal retirement alike — so there is exactly one seam a test has to influence to make one
/// specific record refuse to go. (`quarantine_verified_file` rolling back a marker it has only just
/// created is not that: nothing has ever depended on that marker.)
fn unlink_journal_entry(
    journal: &rustix::fd::OwnedFd,
    name: &std::ffi::CStr,
) -> Result<(), rustix::io::Errno> {
    #[cfg(test)]
    {
        let name: &str = &name.to_string_lossy();
        if FORCED_JOURNAL_UNLINK_FAILURES.with(|ids| ids.borrow().contains(name)) {
            return Err(rustix::io::Errno::IO);
        }
    }
    rustix::fs::unlinkat(journal, name, rustix::fs::AtFlags::empty())
}

/// Commit the journal directory's own entries. The seam exists so a test can prove that a
/// retirement whose *durability* cannot be proven still refuses to unlink the object.
fn fsync_journal_dir(journal: &rustix::fd::OwnedFd) -> Result<(), rustix::io::Errno> {
    #[cfg(test)]
    if FORCED_JOURNAL_FSYNC_FAILURE.with(std::cell::Cell::get) {
        return Err(rustix::io::Errno::IO);
    }
    rustix::fs::fsync(journal)
}

/// Remove one journal entry, if it exists. Best-effort: a leftover entry is never mistaken for
/// proof of anything by itself — only a matching `.rf-delete-*` name together with its entry is.
///
/// Used for the *non-terminal* cleanups, where the entry has stopped describing anything the sweep
/// will ever look at — a restored file back at its own name whose second-stage record now names
/// nothing. The terminal case uses `retire_all_authorizing_journal_entries` instead, because there
/// both the ordering *and* the completeness of the retirement are security properties.
fn remove_journal_entry(states_root: &Path, id: &str) {
    if let (Some(name), Ok(journal)) =
        (journal_entry_name(id), open_delete_journal_dir(states_root))
    {
        let _ = unlink_journal_entry(&journal, &name);
    }
}

/// Every journal entry currently authorizing one exact physical object, enumerated
/// descriptor-relatively.
///
/// An incomplete enumeration is an error rather than a short list: "no record authorizes this
/// object" is a claim about the *whole* journal, so an unreadable directory or an entry whose
/// contents cannot be determined must not be reported as an absence.
///
/// Journal *contents* are never used as a filesystem path — the returned names come from the
/// directory itself, and only from it. A malformed entry matches nothing and is left exactly as
/// found: it is never adopted, repaired, or rewritten.
fn authorizing_journal_entries(
    journal: &rustix::fd::OwnedFd,
    ownership: QuarantineOwnership,
) -> Result<Vec<std::ffi::CString>, SaveStateError> {
    // Borrows the descriptor to read its entries; it does not consume or re-resolve it.
    let entries = rustix::fs::Dir::read_from(journal).map_err(|_| SaveStateError::DeleteFailed)?;
    let mut authorizing = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| SaveStateError::DeleteFailed)?;
        let name = entry.file_name();
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        if journal_record_at(journal, name)? == Some(ownership) {
            authorizing.push(name.to_owned());
        }
    }
    Ok(authorizing)
}

/// Retire **every** record that authorizes one physical object, durably, and report success only
/// when zero authorizing records are left (HIGH-8, HIGH-9).
///
/// This is a security boundary, not a tidy-up, and the condition it enforces is deliberately about
/// the object rather than about one stage's id:
///
/// > Before the final link of a quarantined physical object is removed, no durable delete-journal
/// > record may remain anywhere that authorizes that same physical object identity.
///
/// `(device, inode)` identifies an object only while that object exists: once the last link is gone
/// the inode number becomes eligible for reuse, so *any* surviving record naming it is a capability
/// a future unrelated file could satisfy. Retiring only the id this stage happens to hold is not
/// enough, because a recovery has more than one stage — the first-stage record still names the same
/// physical object while the second stage is being proved, and a crash can leave further redundant
/// records naming it as well. Identity-wide retirement handles all of them without needing to track
/// a predecessor chain.
///
/// The whole set is one decision. If the journal cannot be opened, if the enumeration is
/// incomplete, if any matching record cannot be removed, or if the commit below cannot be proven,
/// this fails and the caller must not unlink anything. "The record this stage created is gone" is
/// explicitly *not* the success condition.
///
/// **Durability, stated precisely.** The entries are unlinked and the journal *directory* is then
/// `fsync`ed, which is what POSIX offers for committing a directory-entry removal, so on a
/// filesystem and storage stack that honour `fsync` the retirement is durable before the object's
/// own unlink is even attempted. RetroFrontier does not claim more than that: on a stack that
/// ignores `fsync` or reorders across it, the guarantee degrades to process-crash ordering — which
/// this ordering is structurally correct for regardless, and which is the window the finding
/// actually described.
///
/// A failure here says only that retirement could not be *proven* complete; it does not promise the
/// records are still there. Removals already issued may well have taken effect, and that is
/// harmless — the object is kept, nothing is destroyed, and no record is ever manufactured to
/// replace one that has gone.
///
/// `ENOENT` on a removal is success: the entry is already gone, which is precisely the state being
/// asked for.
fn retire_all_authorizing_journal_entries(
    states_root: &Path,
    ownership: QuarantineOwnership,
) -> Result<(), SaveStateError> {
    let journal = open_delete_journal_dir(states_root)?;
    for name in authorizing_journal_entries(&journal, ownership)? {
        match unlink_journal_entry(&journal, &name) {
            Ok(()) | Err(rustix::io::Errno::NOENT) => {}
            Err(_) => return Err(SaveStateError::DeleteFailed),
        }
    }
    // The removals have to be committed, not merely issued, before the object they authorize is
    // destroyed — otherwise a crash could still surface a record without the object.
    fsync_journal_dir(&journal).map_err(|_| SaveStateError::DeleteFailed)?;
    // Proven, not assumed: the terminal invariant is re-read from the filesystem after the commit,
    // so an entry that reappeared or was missed refuses the unlink instead of being unlinked past.
    if !authorizing_journal_entries(&journal, ownership)?.is_empty() {
        return Err(SaveStateError::DeleteFailed);
    }
    Ok(())
}

/// The windows an attacker racing a delete can act in. Production passes `None` for all of them.
#[cfg(test)]
#[derive(Default)]
struct DeleteRaceHooks<'a> {
    /// After the verifying open, before the file is moved to its quarantine name.
    before_rename: Option<&'a (dyn Fn() + Send + Sync)>,
    /// After the quarantine rename, while the original name is briefly free — the window in which
    /// a restore could otherwise clobber whatever took it.
    after_rename: Option<&'a (dyn Fn() + Send + Sync)>,
    /// After the quarantined object's inode/device/size are re-verified, before its *content* is
    /// re-hashed (HIGH-5). The one window in which an already-open writer holding the same inode
    /// can still change what gets deleted.
    after_inode_reverified: Option<&'a (dyn Fn() + Send + Sync)>,
    /// Immediately before the final destructive `unlinkat` (HIGH-8). At this instant the object
    /// must still exist and its authorizing journal entry must already be gone.
    before_unlink: Option<&'a (dyn Fn() + Send + Sync)>,
}

#[cfg(not(test))]
#[derive(Default)]
struct DeleteRaceHooks<'a> {
    marker: std::marker::PhantomData<&'a ()>,
}

impl DeleteRaceHooks<'_> {
    fn before_rename(&self) {
        #[cfg(test)]
        if let Some(hook) = self.before_rename {
            hook();
        }
    }

    fn after_rename(&self) {
        #[cfg(test)]
        if let Some(hook) = self.after_rename {
            hook();
        }
    }

    fn after_inode_reverified(&self) {
        #[cfg(test)]
        if let Some(hook) = self.after_inode_reverified {
            hook();
        }
    }

    fn before_unlink(&self) {
        #[cfg(test)]
        if let Some(hook) = self.before_unlink {
            hook();
        }
    }
}

/// The delete implementation, with injectable hooks for the adversarial replacement tests.
fn delete_verified_managed_file_inner(
    states_root: &Path,
    relative_path: &RelativePath,
    expected_size: u64,
    expected_sha256: Sha256Digest,
    hooks: &DeleteRaceHooks<'_>,
) -> Result<(), SaveStateError> {
    use std::os::unix::fs::MetadataExt;

    let opened = open_managed_file(states_root, relative_path)?;
    if opened.identity.size_bytes != expected_size {
        return Err(SaveStateError::IntegrityMismatch);
    }
    let verified_identity = opened.identity;
    let verified_device = opened.device;
    let parent = opened.parent;
    let name = opened.name;
    if hash_descriptor(opened.file)? != expected_sha256 {
        return Err(SaveStateError::IntegrityMismatch);
    }

    hooks.before_rename();

    // Move the verified inode to a name only RetroFrontier knows, durably journaled first so the
    // move can later be *proved* RetroFrontier's own (HIGH-4). `NOREPLACE` makes the move itself
    // collision-safe: it can never destroy a file this operation never verified.
    //
    // HIGH-7: what is journaled is this *object's* identity — the device and inode read from the
    // descriptor that was just hashed — not merely its content, so a later startup can tell the
    // object apart from any byte-identical file that might occupy the same name.
    let ownership = QuarantineOwnership {
        device: verified_device,
        inode: verified_identity.inode,
        size_bytes: verified_identity.size_bytes,
        sha256: expected_sha256,
    };
    let quarantine = quarantine_verified_file(states_root, &parent, name.as_str(), ownership)?;
    hooks.after_rename();

    // Put the name back so the filesystem is left as it was found — but **never over something
    // else**. A plain `renameat` replaces its destination silently, and the situations that reach
    // this closure are exactly the ones where another actor raced the delete; if that actor has
    // since created a file at the original name, restoring on top of it would destroy a file
    // RetroFrontier never verified. That would break the same invariant this whole function
    // exists to keep.
    //
    // When the name is taken, the quarantined file simply stays where it is: it is inert to the
    // parser, so it is never attributed, listed, or loaded, and `sweep_delete_quarantine` proves
    // and finishes it at the next startup — which is exactly why the journal entry is left in
    // place rather than removed in that branch.
    let restore = |quarantine: &str| {
        let restored = rustix::fs::renameat_with(
            &parent,
            quarantine,
            &parent,
            name.as_str(),
            rustix::fs::RenameFlags::NOREPLACE,
        );
        match restored {
            Ok(()) => {
                // No longer quarantined, so nothing will ever look for this journal entry again.
                if let Some(id) = quarantine.strip_prefix(QUARANTINE_PREFIX) {
                    remove_journal_entry(states_root, id);
                }
            }
            Err(_) => {
                tracing::warn!(
                    "a save-state delete could not restore its original name, so the verified \
                     file was left quarantined for the next startup sweep"
                );
            }
        }
    };

    // Re-verify at the quarantine name. This is the authoritative check: it proves the inode now
    // sitting there is the one that was verified, so the unlink below cannot remove anything else.
    let descriptor = match rustix::fs::openat(
        &parent,
        quarantine.as_str(),
        FILE_OPEN_FLAGS,
        rustix::fs::Mode::empty(),
    ) {
        Ok(descriptor) => descriptor,
        Err(_) => {
            restore(&quarantine);
            return Err(SaveStateError::UnsafeFilesystemTarget);
        }
    };
    let quarantined = std::fs::File::from(descriptor);
    let identity_matches = quarantined
        .metadata()
        .map(|metadata| {
            metadata.file_type().is_file()
                && metadata.dev() == verified_device
                && metadata.ino() == verified_identity.inode
                && metadata.size() == verified_identity.size_bytes
        })
        .unwrap_or(false);
    if !identity_matches {
        drop(quarantined);
        restore(&quarantine);
        return Err(SaveStateError::UnsafeFilesystemTarget);
    }
    hooks.after_inode_reverified();

    // HIGH-5: re-verify *content*, not only inode/device/size, immediately before destruction.
    //
    // Stated threat model: this closes the ordinary races the same way the rest of this module
    // does — a different actor replacing the pathname, a hard link, a symlink, a directory swap.
    // It narrows, but does not and cannot fully close, one specific and much rarer threat: a
    // hostile *same-user* process that already holds an open writable descriptor onto this exact
    // inode before RetroFrontier ever opens it, and keeps writing through that descriptor after
    // this very re-hash. POSIX gives no dependable, portable way to exclude a concurrent writer
    // holding an already-open descriptor short of mandatory locking, which Linux does not offer as
    // a mechanism this project can rely on — and a same-user hostile process capable of racing a
    // delete this precisely already has unrestricted access to the user's own files regardless.
    // What is guaranteed, stated exactly: the exact previously verified bytes are deleted, or
    // nothing is, against pathname replacement, symlink traversal, hard links, a wrong inode, a
    // wrong digest, and ordinary TOCTOU substitution. What is *not* claimed: for a hostile
    // same-inode writer the window is narrowed to the instant between this re-hash and the
    // `unlinkat` immediately below, rather than closed. That is a documented POSIX limitation, and
    // the module documentation and `docs/SAVE_STATES.md` say so in the same terms.
    let content_matches =
        matches!(hash_descriptor(quarantined), Ok(sha256) if sha256 == expected_sha256);
    if !content_matches {
        restore(&quarantine);
        return Err(SaveStateError::UnsafeFilesystemTarget);
    }

    // HIGH-8: retire the authorizing record *before* the unlink, durably, exactly as the startup
    // sweep does — there must not be one destructive path that still unlinks first. After the last
    // link to an inode is gone its number may be reused, so a surviving record is a capability over
    // whatever object comes to hold that number next. If the record cannot be retired, nothing is
    // unlinked: the file is restored and the delete fails, which leaves the same safe, retryable
    // state every other refusal here leaves.
    //
    // HIGH-9: retirement is identity-wide on this path too, and uses the same helper the sweep
    // uses. An uninterrupted live delete writes exactly one record, so in the ordinary case this
    // retires that one record and nothing else; the reason it is not narrowed to that id is that
    // the security condition is "no record authorizes this object", which is a property of the
    // journal rather than of this call — and asserting the same terminal invariant on both
    // destructive paths costs one directory scan of a directory that is normally empty.
    if retire_all_authorizing_journal_entries(states_root, ownership).is_err() {
        tracing::warn!(
            "a save-state delete could not prove its ownership retirement complete, so the file \
             was not removed; the object was left in place and no record is ever recreated"
        );
        restore(&quarantine);
        return Err(SaveStateError::DeleteFailed);
    }

    hooks.before_unlink();

    rustix::fs::unlinkat(&parent, quarantine.as_str(), rustix::fs::AtFlags::empty()).map_err(
        |_| {
            // The record is already retired and is deliberately never recreated. `restore` puts the
            // file back under its own registered name where it is a tracked state again rather than
            // an orphan; if even that fails, the file stays inert under its quarantine name and no
            // sweep will ever touch it, which is the accepted fail-closed outcome.
            restore(&quarantine);
            SaveStateError::DeleteFailed
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::{tempdir, TempDir};

    fn path(value: &str) -> RelativePath {
        RelativePath::new(value).unwrap()
    }

    fn digest_of(bytes: &[u8]) -> Sha256Digest {
        crate::adapters::runtime_integrity::sha256_bytes(bytes)
    }

    fn states_root() -> TempDir {
        tempdir().unwrap()
    }

    fn write(root: &Path, relative: &str, bytes: &[u8]) {
        let target = root.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, bytes).unwrap();
    }

    /// A deterministic probe, so both stability outcomes are reachable without sleeping.
    struct StaticStabilityProbe(bool);

    impl StabilityProbe for StaticStabilityProbe {
        fn is_stable(&self, _states_root: &Path, _relative_path: &RelativePath) -> bool {
            self.0
        }
    }

    // ============================================================ adapter contract

    /// The pinned RetroArch 1.22.2 save-state layout.
    ///
    /// These assertions are a *contract with a specific managed runtime version*, not domain
    /// invariants. A Runtime upgrade that changes the layout must break them deliberately.
    mod retroarch_1_22_2_contract {
        use super::*;

        #[test]
        fn numbered_manual_slots_one_to_nine_hundred_and_ninety_nine_are_managed() {
            for (name, slot) in [
                ("Nestopia/Synthetic.state1", 1_u16),
                ("Nestopia/Synthetic.state2", 2),
                ("Nestopia/Synthetic.state42", 42),
                ("Nestopia/Synthetic.state999", 999),
                // A basename containing dots, spaces, and its own `.state` text still resolves by
                // its *last* `.state` boundary.
                ("beetle-psx/Final Fantasy VII (Disc 1).state7", 7),
                ("bsnes-mercury/weird.state.name.state3", 3),
            ] {
                assert_eq!(
                    parse_state_candidate(&path(name)),
                    StateCandidate::ManagedSlot(SaveStateSlot::new(slot).unwrap()),
                    "{name}"
                );
            }
        }

        #[test]
        fn slot_zero_and_the_automatic_slot_are_not_managed() {
            for name in [
                // Slot 0 is the unnumbered base state.
                "Nestopia/Synthetic.state",
                "Synthetic.state",
                // The automatic slot.
                "Nestopia/Synthetic.state.auto",
                "Synthetic.state.auto",
                // And an explicit zero, which RetroArch's `%d` would never write for a managed
                // slot anyway.
                "Nestopia/Synthetic.state0",
            ] {
                assert_eq!(
                    parse_state_candidate(&path(name)),
                    StateCandidate::Unsupported,
                    "{name}"
                );
            }
        }

        #[test]
        fn an_ambiguous_or_out_of_range_slot_suffix_is_never_attributed() {
            for name in [
                "Nestopia/Synthetic.state1000",
                "Nestopia/Synthetic.state01",
                "Nestopia/Synthetic.state001",
                "Nestopia/Synthetic.state+1",
                "Nestopia/Synthetic.state-1",
                "Nestopia/Synthetic.state 1",
                "Nestopia/Synthetic.state1a",
                "Nestopia/Synthetic.stateN",
                // A full-width digit is not what `%d` writes.
                "Nestopia/Synthetic.state１",
                // Normal save data and unrelated files.
                "Nestopia/Synthetic.srm",
                "Nestopia/Synthetic.sav",
                "Nestopia/Synthetic",
                "Nestopia",
            ] {
                assert_eq!(
                    parse_state_candidate(&path(name)),
                    StateCandidate::Unsupported,
                    "{name}"
                );
            }
        }

        #[test]
        fn a_state_thumbnail_names_the_managed_state_it_belongs_to() {
            assert_eq!(
                parse_state_candidate(&path("Nestopia/Synthetic.state1.png")),
                StateCandidate::ThumbnailOf(path("Nestopia/Synthetic.state1"))
            );
            assert_eq!(
                parse_state_candidate(&path("Synthetic.state9.png")),
                StateCandidate::ThumbnailOf(path("Synthetic.state9"))
            );
            assert_eq!(
                thumbnail_relative_path(&path("Nestopia/Synthetic.state1")),
                Some(path("Nestopia/Synthetic.state1.png"))
            );

            // A thumbnail of something RetroFrontier does not manage is not a thumbnail here.
            for name in [
                "Nestopia/Synthetic.state.png",
                "Nestopia/Synthetic.state.auto.png",
                "Nestopia/Synthetic.state0.png",
                "Nestopia/Synthetic.state1000.png",
                "screenshots/Synthetic.png",
                "Nestopia/Synthetic.png",
            ] {
                assert_eq!(
                    parse_state_candidate(&path(name)),
                    StateCandidate::Unsupported,
                    "{name}"
                );
            }
        }

        /// The per-core directory `sort_savestates_enable` produces is the core-reported
        /// `library_name`, which is **not** a RetroFrontier `CoreId`.
        ///
        /// The real managed runtime produced `Nestopia`, `bsnes-mercury`, and `dolphin-emu` for the
        /// cores RetroFrontier calls `nestopia`, `bsnes-mercury-balanced`, and `dolphin`. Nothing
        /// here may reverse-map one to the other, so the parse result carries no core at all.
        #[test]
        fn the_per_core_directory_is_never_reverse_mapped_to_a_core() {
            for directory in [
                "Nestopia",
                "bsnes-mercury",
                "dolphin-emu",
                "not-a-core-at-all",
            ] {
                assert_eq!(
                    parse_state_candidate(&path(&format!("{directory}/Synthetic.state1"))),
                    StateCandidate::ManagedSlot(SaveStateSlot::new(1).unwrap()),
                    "the directory must not change the parse result ({directory})"
                );
            }
            // A state directly in the root parses identically: the directory carries no meaning.
            assert_eq!(
                parse_state_candidate(&path("Synthetic.state1")),
                StateCandidate::ManagedSlot(SaveStateSlot::new(1).unwrap())
            );
        }

        /// HIGH-2: the exact state target a controlled launch resolves.
        ///
        /// Pinned against a real RetroArch 1.22.x binary, not inferred. Run with
        /// `sort_savestates_enable = false`, `savestate_directory = D`, content
        /// `<...>/Synthetic Probe.nes` and `--entryslot 3`, RetroArch logs:
        ///
        /// ```text
        /// [INFO] [Override] Redirecting save state to "D/Synthetic Probe.state".
        /// [INFO] [State] Entry state found in "D/Synthetic Probe.state3".
        /// ```
        ///
        /// The same run with sorting *enabled* resolves `D/Nestopia/Synthetic Probe.state3`
        /// instead — the core-reported `library_name` segment RetroFrontier cannot authenticate,
        /// and the reason the generated configuration turns sorting off.
        #[test]
        fn the_state_target_is_the_core_namespace_the_content_basename_and_the_slot() {
            let core = CoreId::new("nestopia").unwrap();
            let slot = SaveStateSlot::new(3).unwrap();

            // The directory RetroArch is given, and the path RetroFrontier proves against, are the
            // same composition.
            assert_eq!(
                state_directory(Path::new("/app-data/states"), &core),
                Path::new("/app-data/states/nestopia")
            );
            assert_eq!(
                state_target(&core, "NES/Synthetic Probe.nes", slot).unwrap(),
                path("nestopia/Synthetic Probe.state3")
            );
            // The extension is dropped, further directories in the content path are irrelevant,
            // and a basename carrying its own dots keeps everything but the last extension.
            for (content, expected) in [
                ("Synthetic.nes", "nestopia/Synthetic.state3"),
                ("a/b/c/Synthetic.nes", "nestopia/Synthetic.state3"),
                (
                    "PS1/Final Fantasy VII (Disc 1).chd",
                    "nestopia/Final Fantasy VII (Disc 1).state3",
                ),
                ("NoExtension", "nestopia/NoExtension.state3"),
            ] {
                assert_eq!(
                    state_target(&core, content, slot).unwrap(),
                    path(expected),
                    "{content}"
                );
            }
            // Every slot the layout manages composes the same way.
            for number in [MIN_MANAGED_SLOT, 2, 42, MAX_MANAGED_SLOT] {
                let slot = SaveStateSlot::new(number).unwrap();
                assert_eq!(
                    state_target(&core, "NES/Synthetic.nes", slot).unwrap(),
                    path(&format!("nestopia/Synthetic.state{number}"))
                );
            }
            // A content path with no usable basename names no target, which is a refusal.
            assert!(state_target(&core, "NES/.nes", slot).is_none());
            assert!(state_target(&core, "", slot).is_none());
        }

        /// The binding is an equality. A file that is merely *shaped* like the target — right
        /// basename, right slot, wrong namespace — is not the target, and neither is the right
        /// namespace with the wrong slot or basename.
        #[test]
        fn only_the_exact_target_path_satisfies_the_binding() {
            let core = CoreId::new("nestopia").unwrap();
            let slot = SaveStateSlot::new(1).unwrap();
            let content = "NES/Synthetic.nes";

            assert!(is_state_target(
                &path("nestopia/Synthetic.state1"),
                &core,
                content,
                slot
            ));
            for foreign in [
                // The core-reported `library_name` directory sorting would have produced.
                "Nestopia/Synthetic.state1",
                "ForeignNamespace/Synthetic.state1",
                "bsnes-mercury-balanced/Synthetic.state1",
                // No namespace at all.
                "Synthetic.state1",
                // Nested below the right namespace is still not the right path.
                "nestopia/sub/Synthetic.state1",
                // Right namespace, wrong slot.
                "nestopia/Synthetic.state2",
                // Right namespace, wrong content.
                "nestopia/Foreign.state1",
            ] {
                assert!(
                    !is_state_target(&path(foreign), &core, content, slot),
                    "{foreign} must not satisfy the binding"
                );
            }
        }

        /// A `CoreId` is always exactly one safe path component, so the namespace segment can
        /// never escape the states root or collide with RetroFrontier's own private names.
        #[test]
        fn a_core_namespace_is_always_one_safe_component() {
            for core_id in [
                "nestopia",
                "bsnes-mercury-balanced",
                "beetle-psx",
                "dolphin",
                "a",
            ] {
                let core = CoreId::new(core_id).unwrap();
                let composed = state_target(&core, "Synthetic.nes", SaveStateSlot::new(1).unwrap())
                    .expect("a valid core id always composes a target");
                assert_eq!(composed.as_str().split('/').count(), 2, "{core_id}");
                assert!(!core_id.starts_with('.'), "{core_id}");
                assert!(!core_id.starts_with(QUARANTINE_PREFIX), "{core_id}");
                assert_ne!(core_id, DELETE_JOURNAL_DIR);
            }
            // The domain refuses everything that would not be one component in the first place.
            for unsafe_id in ["..", ".", "a/b", "", ".hidden", "a\\b", "a b"] {
                assert!(CoreId::new(unsafe_id).is_err(), "{unsafe_id}");
            }
        }

        /// The quarantine name a delete uses can never be mistaken for content.
        #[test]
        fn a_delete_quarantine_name_is_inert_to_the_parser() {
            assert_eq!(
                parse_state_candidate(&path("Nestopia/.rf-delete-1234-0")),
                StateCandidate::Unsupported
            );
            assert!(QUARANTINE_PREFIX.starts_with('.'));
            let full_name = format!("{QUARANTINE_PREFIX}{}", quarantine_id());
            assert!(!full_name.contains(STATE_SUFFIX));
            assert_eq!(
                parse_state_candidate(&path(&format!("Nestopia/{full_name}"))),
                StateCandidate::Unsupported
            );
        }
    }

    // ============================================================ snapshot and delta

    #[test]
    fn a_snapshot_records_only_regular_files_with_their_cheap_physical_identity() {
        let root = states_root();
        write(root.path(), "Nestopia/Synthetic.state1", b"one");
        write(root.path(), "Nestopia/Synthetic.state2", b"twotwo");
        write(root.path(), "bsnes-mercury/Other.state1", b"three");
        fs::create_dir_all(root.path().join("empty")).unwrap();

        let snapshot = snapshot_state_tree(root.path());

        assert!(snapshot.is_complete());
        assert_eq!(snapshot.len(), 3);
        assert_eq!(
            snapshot
                .get(&path("Nestopia/Synthetic.state1"))
                .unwrap()
                .size_bytes,
            3
        );
        assert_eq!(
            snapshot
                .get(&path("Nestopia/Synthetic.state2"))
                .unwrap()
                .size_bytes,
            6
        );
        assert!(snapshot.contains(&path("bsnes-mercury/Other.state1")));
        assert!(!snapshot.contains(&path("empty")));
        // The baseline projection is the same set.
        assert_eq!(snapshot.to_baseline_entries().len(), 3);
    }

    #[test]
    fn a_symlink_is_never_recorded_and_makes_the_enumeration_incomplete() {
        let root = states_root();
        let outside = tempdir().unwrap();
        write(outside.path(), "foreign.state1", b"foreign");
        write(root.path(), "Nestopia/Synthetic.state1", b"one");
        symlink(
            outside.path().join("foreign.state1"),
            root.path().join("Nestopia/linked.state2"),
        )
        .unwrap();
        symlink(outside.path(), root.path().join("linked-directory")).unwrap();

        let snapshot = snapshot_state_tree(root.path());

        // Neither the linked file nor anything behind the linked directory is in the tree.
        assert!(snapshot.contains(&path("Nestopia/Synthetic.state1")));
        assert!(!snapshot.contains(&path("Nestopia/linked.state2")));
        assert!(!snapshot.contains(&path("linked-directory/foreign.state1")));
        // And the anomaly suppresses every destructive decision the snapshot could drive.
        assert!(!snapshot.is_complete());
    }

    /// HIGH-3 regression: a directory substituted for a symlink *between* the walker observing its
    /// type and recursing into it must never be followed.
    ///
    /// The previous, path-based walker observed a directory's type via `symlink_metadata`, then
    /// recursed by re-joining `states_root` with the accumulated relative path and calling
    /// `read_dir` on that string a second time. Swapping the directory for a symlink in exactly
    /// that window made the second lookup resolve the link. The descriptor-relative walker has no
    /// second lookup to exploit: it opens the child `O_NOFOLLOW` *relative to the parent's already-
    /// open descriptor*, so the same swap simply makes that `openat` fail.
    #[test]
    fn a_directory_swapped_for_a_symlink_between_observation_and_traversal_is_never_followed() {
        let root = states_root();
        let outside = tempdir().unwrap();
        write(outside.path(), "Foreign.state1", b"foreign content");
        write(root.path(), "RealDir/Synthetic.state1", b"real content");

        let real_dir = root.path().join("RealDir");
        let outside_path = outside.path().to_path_buf();
        let swap = move || {
            fs::remove_dir_all(&real_dir).unwrap();
            symlink(&outside_path, &real_dir).unwrap();
        };

        let snapshot = snapshot_state_tree_inner(
            root.path(),
            &SnapshotRaceHooks {
                after_type_observed: Some(&swap),
            },
        );

        // The foreign content behind the substituted symlink was never smuggled in...
        assert!(!snapshot.contains(&path("RealDir/Foreign.state1")));
        // ...and neither was the original directory's own content, since the walker never
        // resolved "RealDir" a second time to find it either.
        assert!(!snapshot.contains(&path("RealDir/Synthetic.state1")));
        // The substitution makes the enumeration provably incomplete — never falsely "complete",
        // which is the one input that may drive a destructive `missing` transition.
        assert!(!snapshot.is_complete());
    }

    #[test]
    fn an_unreadable_subdirectory_makes_the_enumeration_incomplete_rather_than_smaller() {
        let root = states_root();
        write(root.path(), "Nestopia/Synthetic.state1", b"one");
        write(root.path(), "protected/Hidden.state1", b"hidden");
        let protected = root.path().join("protected");
        fs::set_permissions(&protected, fs::Permissions::from_mode(0o000)).unwrap();

        let snapshot = snapshot_state_tree(root.path());

        assert!(snapshot.contains(&path("Nestopia/Synthetic.state1")));
        assert!(!snapshot.is_complete());

        fs::set_permissions(&protected, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(snapshot_state_tree(root.path()).is_complete());
    }

    #[test]
    fn a_missing_states_root_is_an_incomplete_enumeration_not_an_empty_tree() {
        let root = states_root();
        let snapshot = snapshot_state_tree(&root.path().join("does-not-exist"));

        assert!(snapshot.is_empty());
        assert!(!snapshot.is_complete());
    }

    #[test]
    fn the_delta_reports_new_and_changed_files_and_ignores_everything_else() {
        let root = states_root();
        write(root.path(), "Nestopia/Unchanged.state1", b"same");
        write(root.path(), "Nestopia/Changed.state2", b"before");
        write(root.path(), "Nestopia/Vanishing.state3", b"gone soon");
        let baseline = snapshot_state_tree(root.path()).to_baseline_entries();

        // A new file, a changed file, and a removed file.
        write(root.path(), "Nestopia/New.state4", b"new");
        write(root.path(), "Nestopia/Changed.state2", b"after the change");
        fs::remove_file(root.path().join("Nestopia/Vanishing.state3")).unwrap();

        let delta = state_tree_delta(&baseline, &snapshot_state_tree(root.path()));

        assert_eq!(
            delta,
            vec![path("Nestopia/Changed.state2"), path("Nestopia/New.state4")]
        );
        // A file that vanished is never a delta: reconciliation does not attribute an absence.
        assert!(!delta.contains(&path("Nestopia/Vanishing.state3")));
        assert!(!delta.contains(&path("Nestopia/Unchanged.state1")));
    }

    #[test]
    fn a_file_replaced_by_a_different_inode_of_the_same_size_is_still_a_delta() {
        let root = states_root();
        write(root.path(), "Nestopia/Synthetic.state1", b"aaaa");
        let baseline = snapshot_state_tree(root.path()).to_baseline_entries();

        // Same size, and a filesystem could plausibly reuse the modification time; the inode
        // still changes when the file is recreated.
        fs::remove_file(root.path().join("Nestopia/Synthetic.state1")).unwrap();
        write(root.path(), "Nestopia/Synthetic.state1", b"bbbb");

        let delta = state_tree_delta(&baseline, &snapshot_state_tree(root.path()));
        assert_eq!(delta, vec![path("Nestopia/Synthetic.state1")]);
    }

    // ============================================================ stability

    #[test]
    fn stability_requires_consecutive_identical_observations() {
        let root = states_root();
        write(root.path(), "Nestopia/Synthetic.state1", b"settled");
        let probe = PollingStabilityProbe {
            samples: 2,
            interval: Duration::from_millis(1),
        };

        assert!(probe.is_stable(root.path(), &path("Nestopia/Synthetic.state1")));
        // An absent file is not stable — and that is a refusal, not a guess.
        assert!(!probe.is_stable(root.path(), &path("Nestopia/Absent.state2")));
        // Neither is a directory or a symbolic link standing where a state should be.
        fs::create_dir_all(root.path().join("Nestopia/Directory.state3")).unwrap();
        assert!(!probe.is_stable(root.path(), &path("Nestopia/Directory.state3")));
        symlink(
            root.path().join("Nestopia/Synthetic.state1"),
            root.path().join("Nestopia/Linked.state4"),
        )
        .unwrap();
        assert!(!probe.is_stable(root.path(), &path("Nestopia/Linked.state4")));

        // The deterministic test probe makes both outcomes reachable without sleeping.
        assert!(StaticStabilityProbe(true).is_stable(root.path(), &path("anything.state1")));
        assert!(!StaticStabilityProbe(false).is_stable(root.path(), &path("anything.state1")));
    }

    // ============================================================ verification

    #[test]
    fn a_plain_regular_file_verifies_by_size_and_digest_read_from_the_descriptor() {
        let root = states_root();
        let bytes = b"synthetic state payload";
        write(root.path(), "Nestopia/Synthetic.state1", bytes);
        let relative = path("Nestopia/Synthetic.state1");

        let hashed = hash_managed_file(root.path(), &relative).unwrap();
        assert_eq!(hashed.sha256, digest_of(bytes));
        assert_eq!(hashed.size_bytes, bytes.len() as u64);

        let verified =
            verify_managed_file(root.path(), &relative, bytes.len() as u64, digest_of(bytes))
                .unwrap();
        assert_eq!(verified, hashed);
        managed_file_matches_size(root.path(), &relative, bytes.len() as u64).unwrap();
    }

    #[test]
    fn a_digest_or_size_mismatch_is_refused_and_the_file_is_left_untouched() {
        let root = states_root();
        let bytes = b"synthetic state payload";
        write(root.path(), "Nestopia/Synthetic.state1", bytes);
        let relative = path("Nestopia/Synthetic.state1");

        assert_eq!(
            verify_managed_file(
                root.path(),
                &relative,
                bytes.len() as u64,
                digest_of(b"other")
            ),
            Err(SaveStateError::IntegrityMismatch)
        );
        assert_eq!(
            verify_managed_file(root.path(), &relative, 1, digest_of(bytes)),
            Err(SaveStateError::IntegrityMismatch)
        );
        assert_eq!(
            managed_file_matches_size(root.path(), &relative, 1),
            Err(SaveStateError::IntegrityMismatch)
        );
        // Nothing was repaired and nothing was removed.
        assert_eq!(
            fs::read(root.path().join("Nestopia/Synthetic.state1")).unwrap(),
            bytes
        );
    }

    #[test]
    fn a_symlink_a_directory_a_fifo_or_a_hard_link_is_refused_rather_than_resolved() {
        let root = states_root();
        let outside = tempdir().unwrap();
        let bytes = b"payload";
        write(root.path(), "Nestopia/Real.state1", bytes);
        write(outside.path(), "foreign.state1", bytes);

        // A symlinked final component, pointing inside and outside the root.
        symlink(
            root.path().join("Nestopia/Real.state1"),
            root.path().join("Nestopia/InsideLink.state2"),
        )
        .unwrap();
        symlink(
            outside.path().join("foreign.state1"),
            root.path().join("Nestopia/OutsideLink.state3"),
        )
        .unwrap();
        // A symlinked *intermediate directory*.
        symlink(outside.path(), root.path().join("LinkedDir")).unwrap();
        // A directory where a state should be.
        fs::create_dir_all(root.path().join("Nestopia/Directory.state4")).unwrap();
        // A named pipe. Opening it read-only would otherwise block on a writer, so the file-type
        // check has to come from the descriptor RetroFrontier itself opened.
        let pipes = rustix::fs::open(
            root.path().join("Nestopia"),
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY,
            rustix::fs::Mode::empty(),
        )
        .unwrap();
        rustix::fs::mknodat(
            &pipes,
            "Fifo.state6",
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .unwrap();
        // A hard link: the same content is reachable under a name RetroFrontier does not own.
        fs::hard_link(
            root.path().join("Nestopia/Real.state1"),
            root.path().join("Nestopia/Hard.state5"),
        )
        .unwrap();

        for relative in [
            "Nestopia/InsideLink.state2",
            "Nestopia/OutsideLink.state3",
            "LinkedDir/foreign.state1",
            "Nestopia/Directory.state4",
            "Nestopia/Hard.state5",
            "Nestopia/Fifo.state6",
            // And the original is now hard-linked too, so it is refused as well.
            "Nestopia/Real.state1",
        ] {
            assert_eq!(
                hash_managed_file(root.path(), &path(relative)),
                Err(SaveStateError::UnsafeFilesystemTarget),
                "{relative}"
            );
        }

        // Removing the extra link restores an ordinary, verifiable file.
        fs::remove_file(root.path().join("Nestopia/Hard.state5")).unwrap();
        assert!(hash_managed_file(root.path(), &path("Nestopia/Real.state1")).is_ok());
    }

    /// Regression: a planted named pipe must not hang RetroFrontier.
    ///
    /// Opening a FIFO read-only blocks until a writer arrives, so without `O_NONBLOCK` a
    /// same-user attacker could freeze whichever task verified or deleted a save state simply by
    /// leaving one FIFO behind. This asserts the *liveness* half of the refusal, not only its
    /// verdict.
    #[test]
    fn a_named_pipe_where_a_state_should_be_is_refused_promptly_rather_than_blocking() {
        let root = states_root();
        fs::create_dir_all(root.path().join("Nestopia")).unwrap();
        let directory = rustix::fs::open(
            root.path().join("Nestopia"),
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY,
            rustix::fs::Mode::empty(),
        )
        .unwrap();
        rustix::fs::mknodat(
            &directory,
            "Synthetic.state1",
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .unwrap();
        let relative = path("Nestopia/Synthetic.state1");

        let started = std::time::Instant::now();
        assert_eq!(
            hash_managed_file(root.path(), &relative),
            Err(SaveStateError::UnsafeFilesystemTarget)
        );
        assert_eq!(
            delete_verified_managed_file(root.path(), &relative, 0, digest_of(b"")),
            Err(SaveStateError::UnsafeFilesystemTarget)
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "a named pipe must be refused without waiting for a writer"
        );
        // And it was refused, not removed.
        assert!(root.path().join("Nestopia/Synthetic.state1").exists());
    }

    #[test]
    fn an_absent_file_or_an_absent_states_root_is_refused() {
        let root = states_root();
        assert_eq!(
            hash_managed_file(root.path(), &path("Nestopia/Absent.state1")),
            Err(SaveStateError::UnsafeFilesystemTarget)
        );
        assert_eq!(
            hash_managed_file(&root.path().join("nope"), &path("Absent.state1")),
            Err(SaveStateError::UnsafeFilesystemTarget)
        );
    }

    /// `RelativePath` already refuses every unsafe *stored* form, which is why this adapter needs
    /// no traversal parser of its own.
    #[test]
    fn an_unsafe_stored_path_can_never_reach_the_adapter_at_all() {
        for unsafe_path in [
            "/etc/passwd",
            "../escape.state1",
            "Nestopia/../../escape.state1",
            "Nestopia/./Synthetic.state1",
            "Nestopia\\Synthetic.state1",
            "",
        ] {
            assert!(
                RelativePath::new(unsafe_path).is_err(),
                "{unsafe_path} must never become a relative path"
            );
        }
    }

    /// Reading must not re-resolve the pathname after verifying it.
    ///
    /// "Verify, then `std::fs::read(path)`" is two operations: the second resolves the name afresh
    /// and follows symbolic links, so a same-user attacker can swap in a link to another file of
    /// the same length between them and have *its* bytes served. This asserts the bytes come from
    /// the descriptor that verified them.
    #[test]
    fn reading_a_verified_file_cannot_be_redirected_by_a_symlink_swapped_in_afterwards() {
        let root = states_root();
        let outside = tempdir().unwrap();
        let registered = b"the registered thumbnail";
        // Exactly the same length, so a length check alone would not notice the substitution.
        let secret = b"the attacker's secret!!!";
        assert_eq!(registered.len(), secret.len());
        write(root.path(), "Nestopia/Synthetic.state1.png", registered);
        write(outside.path(), "secret", secret);
        let relative = path("Nestopia/Synthetic.state1.png");

        // The honest read returns the registered bytes.
        assert_eq!(
            read_verified_managed_file(
                root.path(),
                &relative,
                registered.len() as u64,
                digest_of(registered)
            )
            .unwrap(),
            registered
        );

        // Now the file is replaced by a link to a file outside the managed root entirely.
        let target = root.path().join("Nestopia/Synthetic.state1.png");
        fs::remove_file(&target).unwrap();
        symlink(outside.path().join("secret"), &target).unwrap();

        // The open is `O_NOFOLLOW`, so the link is refused outright — the secret is never read,
        // and nothing about its length or content can make it deliverable.
        assert_eq!(
            read_verified_managed_file(
                root.path(),
                &relative,
                registered.len() as u64,
                digest_of(registered)
            ),
            Err(SaveStateError::UnsafeFilesystemTarget)
        );
        // And a link whose target's digest would match is refused for the same reason.
        write(outside.path(), "clone", registered);
        fs::remove_file(&target).unwrap();
        symlink(outside.path().join("clone"), &target).unwrap();
        assert_eq!(
            read_verified_managed_file(
                root.path(),
                &relative,
                registered.len() as u64,
                digest_of(registered)
            ),
            Err(SaveStateError::UnsafeFilesystemTarget)
        );
    }

    #[test]
    fn a_read_is_bounded_by_the_registered_length_and_refuses_a_changed_file() {
        let root = states_root();
        let bytes = b"registered";
        write(root.path(), "Nestopia/Synthetic.state1.png", bytes);
        let relative = path("Nestopia/Synthetic.state1.png");

        // Grown, shrunk, and same-length-but-different all refuse rather than deliver.
        for replacement in [
            &b"registered and then some more"[..],
            b"short",
            b"REGISTERED",
        ] {
            write(root.path(), "Nestopia/Synthetic.state1.png", replacement);
            assert_eq!(
                read_verified_managed_file(
                    root.path(),
                    &relative,
                    bytes.len() as u64,
                    digest_of(bytes)
                ),
                Err(SaveStateError::IntegrityMismatch),
                "{replacement:?}"
            );
        }
    }

    // ============================================================ deletion

    #[test]
    fn a_verified_file_is_deleted_exactly_and_nothing_is_left_behind() {
        let root = states_root();
        let bytes = b"synthetic state payload";
        write(root.path(), "Nestopia/Synthetic.state1", bytes);
        write(root.path(), "Nestopia/Sibling.state2", b"keep me");
        let relative = path("Nestopia/Synthetic.state1");

        delete_verified_managed_file(root.path(), &relative, bytes.len() as u64, digest_of(bytes))
            .unwrap();

        assert!(!root.path().join("Nestopia/Synthetic.state1").exists());
        // The sibling is untouched, and no quarantine file survives.
        assert_eq!(
            fs::read(root.path().join("Nestopia/Sibling.state2")).unwrap(),
            b"keep me"
        );
        let remaining: Vec<_> = fs::read_dir(root.path().join("Nestopia"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(remaining, vec!["Sibling.state2".to_owned()]);
    }

    #[test]
    fn a_mismatched_or_unsafe_target_deletes_nothing_and_leaves_no_quarantine_file() {
        let root = states_root();
        let bytes = b"synthetic state payload";
        write(root.path(), "Nestopia/Synthetic.state1", bytes);
        let relative = path("Nestopia/Synthetic.state1");

        for (expected_size, expected_digest, expected_error) in [
            (
                bytes.len() as u64,
                digest_of(b"other"),
                SaveStateError::IntegrityMismatch,
            ),
            (1, digest_of(bytes), SaveStateError::IntegrityMismatch),
        ] {
            assert_eq!(
                delete_verified_managed_file(
                    root.path(),
                    &relative,
                    expected_size,
                    expected_digest
                ),
                Err(expected_error)
            );
            assert_eq!(
                fs::read(root.path().join("Nestopia/Synthetic.state1")).unwrap(),
                bytes
            );
        }

        // A symlink standing where the registered file was: the link is refused, and neither the
        // link nor its target is deleted.
        let target = root.path().join("Nestopia/Synthetic.state1");
        let moved = root.path().join("Nestopia/moved.bin");
        fs::rename(&target, &moved).unwrap();
        symlink(&moved, &target).unwrap();
        assert_eq!(
            delete_verified_managed_file(
                root.path(),
                &relative,
                bytes.len() as u64,
                digest_of(bytes)
            ),
            Err(SaveStateError::UnsafeFilesystemTarget)
        );
        assert!(target.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read(&moved).unwrap(), bytes);
        assert_eq!(no_quarantine_files(root.path()), 0);
    }

    /// The adversarial case the quarantine rename exists for.
    ///
    /// The hook replaces the pathname with a *different* file in exactly the window between the
    /// verifying open and the destructive step. The rename then moves the attacker's file, the
    /// inode re-verification at the quarantine name fails, the original name is restored, and
    /// nothing is deleted.
    #[test]
    fn a_replacement_between_verification_and_deletion_deletes_nothing() {
        let root = states_root();
        let bytes = b"the registered state";
        let attacker = b"the attacker's file";
        write(root.path(), "Nestopia/Synthetic.state1", bytes);
        let relative = path("Nestopia/Synthetic.state1");
        let target = root.path().join("Nestopia/Synthetic.state1");
        let stashed = root.path().join("Nestopia/stashed.bin");

        let swap = {
            let target = target.clone();
            let stashed = stashed.clone();
            move || {
                fs::rename(&target, &stashed).unwrap();
                fs::write(&target, attacker).unwrap();
            }
        };

        let outcome = delete_verified_managed_file_inner(
            root.path(),
            &relative,
            bytes.len() as u64,
            digest_of(bytes),
            &DeleteRaceHooks {
                before_rename: Some(&swap),
                ..Default::default()
            },
        );

        assert_eq!(outcome, Err(SaveStateError::UnsafeFilesystemTarget));
        // The attacker's file is back at its own name, untouched, and nothing was deleted.
        assert_eq!(fs::read(&target).unwrap(), attacker);
        assert_eq!(fs::read(&stashed).unwrap(), bytes);
        assert_eq!(no_quarantine_files(root.path()), 0);
    }

    /// A replacement by a file that happens to have the *same size* is caught too, because the
    /// re-verification compares the inode, not only the length.
    #[test]
    fn a_same_size_replacement_between_verification_and_deletion_deletes_nothing() {
        let root = states_root();
        let bytes = b"AAAAAAAA";
        let attacker = b"BBBBBBBB";
        write(root.path(), "Nestopia/Synthetic.state1", bytes);
        let relative = path("Nestopia/Synthetic.state1");
        let target = root.path().join("Nestopia/Synthetic.state1");
        let stashed = root.path().join("Nestopia/stashed.bin");

        let swap = {
            let target = target.clone();
            let stashed = stashed.clone();
            move || {
                fs::rename(&target, &stashed).unwrap();
                fs::write(&target, attacker).unwrap();
            }
        };

        assert_eq!(
            delete_verified_managed_file_inner(
                root.path(),
                &relative,
                bytes.len() as u64,
                digest_of(bytes),
                &DeleteRaceHooks {
                    before_rename: Some(&swap),
                    ..Default::default()
                },
            ),
            Err(SaveStateError::UnsafeFilesystemTarget)
        );
        assert_eq!(fs::read(&target).unwrap(), attacker);
        assert_eq!(fs::read(&stashed).unwrap(), bytes);
        assert_eq!(no_quarantine_files(root.path()), 0);
    }

    /// HIGH-4 regression: a pre-existing file occupying the exact quarantine name a delete is
    /// about to claim must never be overwritten. The delete retries with a fresh, genuinely
    /// unique destination instead, and completes normally.
    #[test]
    fn a_pre_existing_file_at_the_candidate_quarantine_name_is_never_overwritten() {
        let root = states_root();
        let bytes = b"the registered state";
        write(root.path(), "Nestopia/Synthetic.state1", bytes);
        let relative = path("Nestopia/Synthetic.state1");

        let colliding_id = "forced-collision-identifier-0001".to_owned();
        let unrelated_bytes = b"an unrelated file that was already there";
        write(
            root.path(),
            &format!("Nestopia/{QUARANTINE_PREFIX}{colliding_id}"),
            unrelated_bytes,
        );
        // The very first identifier the delete tries collides with that pre-existing file; the
        // second is genuinely free.
        push_forced_quarantine_id(colliding_id.clone());
        push_forced_quarantine_id("forced-fresh-identifier-0002".to_owned());

        delete_verified_managed_file(root.path(), &relative, bytes.len() as u64, digest_of(bytes))
            .unwrap();

        // The unrelated file at the candidate name was never overwritten...
        assert_eq!(
            fs::read(
                root.path()
                    .join(format!("Nestopia/{QUARANTINE_PREFIX}{colliding_id}"))
            )
            .unwrap(),
            unrelated_bytes
        );
        // ...the delete retried and completed normally...
        assert!(!root.path().join("Nestopia/Synthetic.state1").exists());
        // ...and the only quarantine-prefixed name left is the pre-existing, untouched one — the
        // real quarantine object the successful retry created was fully cleaned up.
        assert_eq!(no_quarantine_files(root.path()), 1);
    }

    /// HIGH-5 regression: a hostile same-user writer that already holds the quarantined inode
    /// open and mutates its bytes — preserving inode, device, and length — between the
    /// inode/device/size re-verification and the final unlink must not have its mutated bytes
    /// deleted as if they were the verified content.
    ///
    /// This proves the narrowing HIGH-5's chosen threat model (Option B) actually delivers: an
    /// inode/device/size match alone (the previous re-verification) does not detect this
    /// mutation, but re-hashing content immediately before destruction does. It does not, and by
    /// the stated threat model cannot, close the residual window between this re-hash and the
    /// `unlinkat` immediately after it — see the threat model documented at the call site.
    #[test]
    fn a_same_inode_content_mutation_after_requarantine_is_refused() {
        let root = states_root();
        let bytes = b"AAAAAAAA";
        let mutated = b"BBBBBBBB"; // Same length: only the bytes change, not the inode or size.
        write(root.path(), "Nestopia/Synthetic.state1", bytes);
        let relative = path("Nestopia/Synthetic.state1");
        let directory = root.path().join("Nestopia");

        let mutate_same_inode = move || {
            for entry in fs::read_dir(&directory).unwrap().flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(QUARANTINE_PREFIX) {
                    // An in-place write, exactly what an already-open writable descriptor would
                    // do: the inode, device, and length are all unchanged, only the content is.
                    let mut file = fs::OpenOptions::new()
                        .write(true)
                        .open(entry.path())
                        .unwrap();
                    file.write_all(mutated).unwrap();
                }
            }
        };

        let outcome = delete_verified_managed_file_inner(
            root.path(),
            &relative,
            bytes.len() as u64,
            digest_of(bytes),
            &DeleteRaceHooks {
                after_inode_reverified: Some(&mutate_same_inode),
                ..Default::default()
            },
        );

        assert_eq!(outcome, Err(SaveStateError::UnsafeFilesystemTarget));
        // Nothing was deleted: the file is restored to its original name, mutated bytes and all,
        // rather than destroyed on the strength of a now-stale hash.
        assert_eq!(no_quarantine_files(root.path()), 0);
        assert_eq!(
            fs::read(root.path().join("Nestopia/Synthetic.state1")).unwrap(),
            mutated
        );
    }

    /// Restoring the original name must never overwrite something that appeared there.
    ///
    /// The situations that reach the restore path are exactly the ones where another actor raced
    /// the delete. If that actor created a file at the original name, restoring on top of it would
    /// destroy a file RetroFrontier never verified — breaking the very invariant the quarantine
    /// exists to keep.
    #[test]
    fn a_failed_delete_never_restores_over_a_file_that_took_the_original_name() {
        let root = states_root();
        let registered = b"the registered state";
        write(root.path(), "Nestopia/Synthetic.state1", registered);
        let relative = path("Nestopia/Synthetic.state1");
        let target = root.path().join("Nestopia/Synthetic.state1");

        // The racing actor acts in the window *after* the verified file was moved to quarantine —
        // the only window in which a restore could clobber the original name. It takes that name,
        // and it also replaces the quarantined inode so re-verification fails and the restore path
        // really runs.
        let directory = root.path().join("Nestopia");
        let target_for_hook = target.clone();
        let after_rename = move || {
            fs::write(&target_for_hook, b"a completely unrelated file").unwrap();
            for entry in fs::read_dir(&directory).unwrap().flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(QUARANTINE_PREFIX) {
                    fs::remove_file(entry.path()).unwrap();
                    fs::write(entry.path(), b"not the verified inode").unwrap();
                }
            }
        };

        assert_eq!(
            delete_verified_managed_file_inner(
                root.path(),
                &relative,
                registered.len() as u64,
                digest_of(registered),
                &DeleteRaceHooks {
                    after_rename: Some(&after_rename),
                    ..Default::default()
                },
            ),
            Err(SaveStateError::UnsafeFilesystemTarget)
        );

        // The unrelated file that took the name is intact — restoring refused to clobber it.
        assert_eq!(fs::read(&target).unwrap(), b"a completely unrelated file");
        // The quarantined file stayed quarantined rather than being forced back over that name.
        // It is inert to the parser either way.
        assert_eq!(no_quarantine_files(root.path()), 1);
        // The startup sweep proves this quarantine object is RetroFrontier's own — a journal
        // entry does exist for it — but the hook also overwrote its *content*, so it no longer
        // matches the identity that entry records. The sweep must not destroy it on the strength
        // of the name and the journal entry alone: HIGH-5's same discipline applies here too, so
        // nothing is removed, and the unrelated file at the original name stays untouched.
        assert_eq!(sweep_delete_quarantine(root.path()), 0);
        assert_eq!(no_quarantine_files(root.path()), 1);
        assert_eq!(fs::read(&target).unwrap(), b"a completely unrelated file");
    }

    /// Only a *proven* absence or mismatch may close a Save State's lifecycle.
    ///
    /// `missing` is never reopened, so an observation that merely failed — out of descriptors, an
    /// I/O error, an unreadable tree — must be distinguishable from one that proves something.
    #[test]
    fn a_failed_observation_is_reported_as_indeterminate_rather_than_as_proof() {
        let root = states_root();
        let bytes = b"registered";
        write(root.path(), "Nestopia/Synthetic.state1", bytes);

        // Proofs: the file is genuinely gone, a link stands in its place, and the content changed.
        assert_eq!(
            hash_managed_file(root.path(), &path("Nestopia/Absent.state1")),
            Err(SaveStateError::UnsafeFilesystemTarget)
        );
        assert!(SaveStateError::UnsafeFilesystemTarget.proves_absence_or_mismatch());
        assert!(SaveStateError::IntegrityMismatch.proves_absence_or_mismatch());

        // A directory standing where a state should be is likewise proof, via `ENOTDIR` on the
        // component below it.
        fs::create_dir_all(root.path().join("Nestopia/Directory.state2")).unwrap();
        assert_eq!(
            hash_managed_file(root.path(), &path("Nestopia/Directory.state2/inner.state1")),
            Err(SaveStateError::UnsafeFilesystemTarget)
        );

        // A failed observation proves nothing, and must never close a lifecycle.
        assert!(!SaveStateError::Indeterminate.proves_absence_or_mismatch());
        let protected = root.path().join("locked");
        fs::create_dir_all(&protected).unwrap();
        fs::write(protected.join("Hidden.state1"), bytes).unwrap();
        fs::set_permissions(&protected, fs::Permissions::from_mode(0o000)).unwrap();
        assert_eq!(
            hash_managed_file(root.path(), &path("locked/Hidden.state1")),
            Err(SaveStateError::Indeterminate)
        );
        fs::set_permissions(&protected, fs::Permissions::from_mode(0o700)).unwrap();
    }

    /// HIGH-4 regression: an interrupted delete that really is RetroFrontier's own — exactly what
    /// a crash between the rename and the unlink leaves behind — is finished safely and
    /// idempotently by the startup sweep, proven by its durable journal entry rather than assumed
    /// from its name.
    #[test]
    fn an_actual_rf_owned_interrupted_delete_is_finished_safely_and_idempotently() {
        let root = states_root();
        write(root.path(), "Nestopia/Synthetic.state1", b"real state");
        write(
            root.path(),
            "Nestopia/Synthetic.state1.png",
            b"real thumbnail",
        );
        let bytes = b"quarantined state bytes";
        write(root.path(), "Nestopia/ToQuarantine.state1", bytes);

        // Exactly what a genuine delete does, moments before a crash that prevents it from ever
        // reaching its own unlink: the file is durably journaled and moved into quarantine.
        let parent =
            open_parent_directory(root.path(), &path("Nestopia/ToQuarantine.state1")).unwrap();
        let quarantine = quarantine_verified_file(
            root.path(),
            &parent,
            "ToQuarantine.state1",
            ownership_of(&root.path().join("Nestopia/ToQuarantine.state1"), bytes),
        )
        .unwrap();
        assert!(!root.path().join("Nestopia/ToQuarantine.state1").exists());
        assert_eq!(no_quarantine_files(root.path()), 1);

        // The startup sweep proves ownership from the durable journal entry, re-verifies the
        // physical identity and the content one last time, and finishes the interrupted delete.
        assert_eq!(sweep_delete_quarantine(root.path()), 1);
        assert!(!root.path().join("Nestopia").join(&quarantine).exists());
        assert_eq!(no_quarantine_files(root.path()), 0);
        // Both stages are terminal: no quarantine object and no journal entry of either stage
        // outlives a completed recovery.
        assert!(
            journal_ids(root.path()).is_empty(),
            "a completed recovery leaves no durable evidence behind"
        );
        // Nothing else was touched.
        assert!(root.path().join("Nestopia/Synthetic.state1").exists());
        assert!(root.path().join("Nestopia/Synthetic.state1.png").exists());
        // Idempotent: nothing is left to finish on a second pass.
        assert_eq!(sweep_delete_quarantine(root.path()), 0);
    }

    /// HIGH-4 regression: a user-created file that merely happens to be named like a quarantine
    /// object — with no durable journal entry proving RetroFrontier created it — must survive
    /// untouched. A filename prefix alone is never proof of ownership.
    #[test]
    fn a_fake_quarantine_file_with_no_journal_entry_survives_the_sweep() {
        let root = states_root();
        write(root.path(), "Nestopia/Synthetic.state1", b"real state");
        write(
            root.path(),
            "Nestopia/.rf-delete-fake",
            b"a user's own file",
        );

        // It is not attributable, so reconciliation would never register it either.
        assert_eq!(
            parse_state_candidate(&path("Nestopia/.rf-delete-fake")),
            StateCandidate::Unsupported
        );

        assert_eq!(sweep_delete_quarantine(root.path()), 0);

        assert!(root.path().join("Nestopia/.rf-delete-fake").exists());
        assert_eq!(
            fs::read(root.path().join("Nestopia/.rf-delete-fake")).unwrap(),
            b"a user's own file"
        );
        assert!(root.path().join("Nestopia/Synthetic.state1").exists());
    }

    /// Quarantine an already-written file exactly as an interrupted delete would have left it,
    /// and hand back the quarantine name and the journal id that proves RetroFrontier owns it.
    fn interrupted_delete(root: &Path, relative: &str, bytes: &[u8]) -> (String, String) {
        let relative_path = path(relative);
        let parent = open_parent_directory(root, &relative_path).unwrap();
        let name = relative.rsplit('/').next().unwrap();
        let quarantine = quarantine_verified_file(
            root,
            &parent,
            name,
            ownership_of(&root.join(relative), bytes),
        )
        .unwrap();
        let id = quarantine
            .strip_prefix(QUARANTINE_PREFIX)
            .unwrap()
            .to_owned();
        (quarantine, id)
    }

    fn journal_entry_exists(root: &Path, id: &str) -> bool {
        read_journal_entry(root, id).is_some()
    }

    /// HIGH-6 regression: the startup sweep verifies a *descriptor* and then has to act on a
    /// *name*. A racing same-user process that swaps the pathname in that window must not be able
    /// to make the sweep delete its file.
    ///
    /// The swap here is deterministic rather than timing-dependent: the hook fires exactly once,
    /// after the intended file's content has been verified and before the sweep's first
    /// destructive step, and renames the verified object away so an unrelated file can take its
    /// name — precisely the substitution a `unlinkat(parent, name)` after verification would have
    /// destroyed.
    #[test]
    fn a_pathname_swapped_after_the_sweep_verified_it_deletes_nothing() {
        let root = states_root();
        write(
            root.path(),
            "nestopia/Synthetic.state1",
            b"an unrelated state",
        );
        let owned = b"the quarantined bytes";
        write(root.path(), "nestopia/ToQuarantine.state1", owned);
        let (quarantine, id) =
            interrupted_delete(root.path(), "nestopia/ToQuarantine.state1", owned);

        let directory = root.path().join("nestopia");
        let quarantine_path = directory.join(&quarantine);
        let moved_aside = directory.join("rf-owned-moved-aside");
        let swapped = std::sync::atomic::AtomicBool::new(false);
        let swap = || {
            if swapped.swap(true, std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            // The verified object is renamed away, and a file RetroFrontier has never seen takes
            // the pathname the sweep is about to act on.
            fs::rename(&quarantine_path, &moved_aside).unwrap();
            fs::write(&quarantine_path, b"a replacement file").unwrap();
        };

        let removed = sweep_delete_quarantine_inner(
            root.path(),
            &SweepRaceHooks {
                after_verified: Some(&swap),
                ..SweepRaceHooks::default()
            },
        );

        assert_eq!(removed, 0, "nothing may be reported as finished");
        // The replacement survives, with its own bytes, back at the name it was planted at.
        assert_eq!(fs::read(&quarantine_path).unwrap(), b"a replacement file");
        // The unrelated state next to it was never touched.
        assert_eq!(
            fs::read(root.path().join("nestopia/Synthetic.state1")).unwrap(),
            b"an unrelated state"
        );
        // RetroFrontier's own object still exists and is still provably RetroFrontier's: its
        // journal entry was not discarded by the refusal.
        assert_eq!(fs::read(&moved_aside).unwrap(), owned);
        assert!(journal_entry_exists(root.path(), &id));

        // Retryable: once the impostor is out of the way and the object is back at a quarantine
        // name, an ordinary sweep finishes the interrupted delete and nothing else.
        fs::remove_file(&quarantine_path).unwrap();
        fs::rename(&moved_aside, &quarantine_path).unwrap();
        assert_eq!(sweep_delete_quarantine(root.path()), 1);
        assert!(!quarantine_path.exists());
        assert!(!journal_entry_exists(root.path(), &id));
        assert!(root.path().join("nestopia/Synthetic.state1").exists());
        assert_eq!(sweep_delete_quarantine(root.path()), 0);
    }

    /// MEDIUM-5 regression, at a **non-terminal** failure point: a recovery that fails before the
    /// terminal re-proof must not destroy the ownership evidence, because that evidence is the only
    /// thing that makes the retry possible.
    ///
    /// The second-stage transfer is made to fail deterministically — the journal directory is
    /// sealed in the window after the object is verified and before its second-stage record can be
    /// written, so `quarantine_verified_file` cannot claim a new name. Nothing has been destroyed
    /// and nothing has been handed on, so the first-stage record must survive intact.
    ///
    /// This is deliberately the *non-terminal* half of the rule. Its terminal counterpart is
    /// `a_failure_after_journal_retirement_leaves_an_inert_orphan_that_is_never_swept_again`
    /// (HIGH-8): once the exact object has been re-proved at its final name, a surviving record
    /// becomes more dangerous than losing automatic cleanup, and the ordering reverses.
    #[test]
    fn a_failed_second_stage_transfer_keeps_the_object_and_its_journal_for_a_later_sweep() {
        let root = states_root();
        let owned = b"the quarantined bytes";
        write(root.path(), "nestopia/ToQuarantine.state1", owned);
        let (quarantine, id) =
            interrupted_delete(root.path(), "nestopia/ToQuarantine.state1", owned);

        let journal_dir = root.path().join(DELETE_JOURNAL_DIR);
        let seal = || {
            fs::set_permissions(&journal_dir, fs::Permissions::from_mode(0o500)).unwrap();
        };
        let removed = sweep_delete_quarantine_inner(
            root.path(),
            &SweepRaceHooks {
                after_verified: Some(&seal),
                ..SweepRaceHooks::default()
            },
        );
        fs::set_permissions(&journal_dir, fs::Permissions::from_mode(0o700)).unwrap();

        assert_eq!(removed, 0, "the recovery did not finish");
        // The object never moved, and its ownership evidence is intact and unchanged.
        assert_eq!(no_quarantine_files(root.path()), 1);
        assert_eq!(surviving_quarantine_id(root.path()), id);
        assert_eq!(journal_ids(root.path()), vec![id.clone()]);
        assert_eq!(
            fs::read(root.path().join("nestopia").join(&quarantine)).unwrap(),
            owned
        );

        // A later sweep, with the transient failure gone, finishes it — and is then idempotent.
        assert_eq!(sweep_delete_quarantine(root.path()), 1);
        assert_eq!(no_quarantine_files(root.path()), 0);
        assert!(
            journal_ids(root.path()).is_empty(),
            "no evidence is left over"
        );
        assert_eq!(sweep_delete_quarantine(root.path()), 0);
    }

    /// MEDIUM-5 regression: a quarantine object whose bytes no longer match its journal entry is
    /// never deleted — and never forgotten either. Repeating the sweep keeps refusing rather than
    /// losing the ownership history that makes a safe recovery possible at all.
    #[test]
    fn a_mismatching_quarantine_object_is_never_deleted_and_never_loses_its_evidence() {
        let root = states_root();
        let owned = b"the quarantined bytes";
        write(root.path(), "nestopia/ToQuarantine.state1", owned);
        let (quarantine, id) =
            interrupted_delete(root.path(), "nestopia/ToQuarantine.state1", owned);
        let quarantine_path = root.path().join("nestopia").join(&quarantine);

        // The interrupted operation's own object is mutated after the fact.
        fs::write(&quarantine_path, b"mutated after the interruption").unwrap();

        for _ in 0..3 {
            assert_eq!(sweep_delete_quarantine(root.path()), 0);
            assert_eq!(
                fs::read(&quarantine_path).unwrap(),
                b"mutated after the interruption",
                "a mismatching quarantine file is never deleted"
            );
            assert!(
                journal_entry_exists(root.path(), &id),
                "the ownership evidence must survive every refusal"
            );
        }
    }

    /// HIGH-7 regression: a delete-journal entry created for one physical file must never
    /// authorize the deletion of a *different* physical file, even one with the same name, the same
    /// size, and byte-identical content.
    ///
    /// This is the cross-startup case, and it is precisely what content-only ownership evidence
    /// cannot see. A journal entry recording only `(size, sha256)` describes *bytes*, and bytes are
    /// reproducible by anyone; the whole point of the record is to identify the physical object
    /// RetroFrontier itself quarantined. The first sweep correctly refuses the substitute — it
    /// remembers the inode it verified within that one pass — but if the first-stage entry survives
    /// the refusal while still describing only content, the *next* startup starts from scratch,
    /// finds a file at that name whose bytes match, adopts its inode as the expected one, and
    /// deletes it.
    ///
    /// Note that this is deliberately **not** the accepted HIGH-5 residual. HIGH-5 concerns a
    /// hostile writer mutating bytes through an already-open descriptor on the *same* inode. This
    /// is a different inode reached through a replaced pathname, which is a class M9 claims to
    /// defeat outright.
    #[test]
    fn a_journal_entry_never_authorizes_a_same_digest_file_on_a_different_inode() {
        let root = states_root();
        let owned = b"the exact quarantined bytes";
        write(
            root.path(),
            "nestopia/Sibling.state1",
            b"an unrelated state",
        );
        write(root.path(), "nestopia/ToQuarantine.state1", owned);
        let (quarantine, id) =
            interrupted_delete(root.path(), "nestopia/ToQuarantine.state1", owned);

        let directory = root.path().join("nestopia");
        let quarantine_path = directory.join(&quarantine);
        let moved_aside = directory.join("rf-owned-moved-aside");
        let genuine_inode = inode_of(&quarantine_path);

        let swapped = std::sync::atomic::AtomicBool::new(false);
        let swap = || {
            if swapped.swap(true, std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            // The genuine RetroFrontier-owned object is renamed away, and a *different physical
            // file* carrying byte-identical content takes the quarantine pathname.
            fs::rename(&quarantine_path, &moved_aside).unwrap();
            fs::write(&quarantine_path, owned).unwrap();
        };

        // First sweep: the substitution happens in the one window that exists, and nothing is
        // deleted. This much already held before HIGH-7.
        assert_eq!(
            sweep_delete_quarantine_inner(
                root.path(),
                &SweepRaceHooks {
                    after_verified: Some(&swap),
                    ..SweepRaceHooks::default()
                },
            ),
            0,
            "the first sweep must delete nothing"
        );

        let replacement_inode = inode_of(&quarantine_path);
        assert_ne!(
            replacement_inode, genuine_inode,
            "the replacement must genuinely be a different physical file"
        );
        assert_eq!(fs::read(&quarantine_path).unwrap(), owned);
        assert_eq!(fs::read(&moved_aside).unwrap(), owned);

        // The second, entirely ordinary startup sweep. Nothing is put back by the test: the
        // replacement is simply left sitting at the old quarantine name, exactly as an attacker
        // would leave it, and the sweep is run again with no hooks at all.
        assert_eq!(
            sweep_delete_quarantine(root.path()),
            0,
            "a later startup must not adopt a replacement file as RetroFrontier's own"
        );

        // The replacement was never deleted and never modified.
        assert!(
            quarantine_path.exists(),
            "the replacement file must survive the second sweep"
        );
        assert_eq!(fs::read(&quarantine_path).unwrap(), owned);
        assert_eq!(inode_of(&quarantine_path), replacement_inode);
        // The genuine object is untouched too, and so is everything around it.
        assert_eq!(fs::read(&moved_aside).unwrap(), owned);
        assert_eq!(inode_of(&moved_aside), genuine_inode);
        assert_eq!(
            fs::read(root.path().join("nestopia/Sibling.state1")).unwrap(),
            b"an unrelated state"
        );

        // No journal now claims the replacement. Whatever durable evidence survives describes the
        // genuine object's own physical identity and can never be satisfied by another inode.
        for surviving in journal_ids(root.path()) {
            let ownership = read_journal_entry(root.path(), &surviving)
                .expect("a surviving journal entry must still parse");
            assert_ne!(
                ownership.inode, replacement_inode,
                "no journal entry may claim the replacement file"
            );
        }
        // MEDIUM-5 still holds alongside it: the first-stage evidence for RetroFrontier's *own*
        // object survived the refusal, and still names that object rather than the substitute now
        // sitting at its old pathname.
        let first_stage = read_journal_entry(root.path(), &id)
            .expect("the genuine object's ownership evidence must survive the refused race");
        assert_eq!(first_stage.inode, genuine_inode);
        assert_ne!(first_stage.inode, replacement_inode);

        // And it stays that way however many times recovery runs.
        assert_eq!(sweep_delete_quarantine(root.path()), 0);
        assert!(quarantine_path.exists());
        assert_eq!(fs::read(&quarantine_path).unwrap(), owned);
    }

    /// HIGH-7: the same rule stated directly against the journal, with no race in sight. A
    /// byte-identical file that simply *is not* the object the entry was written for is refused.
    #[test]
    fn a_same_digest_object_on_a_different_inode_is_never_adopted() {
        let root = states_root();
        let owned = b"the exact quarantined bytes";
        write(root.path(), "nestopia/ToQuarantine.state1", owned);
        let (quarantine, id) =
            interrupted_delete(root.path(), "nestopia/ToQuarantine.state1", owned);
        let quarantine_path = root.path().join("nestopia").join(&quarantine);

        // Replace the quarantined object with a different physical file holding identical bytes,
        // in place, with no sweep in flight at all.
        //
        // The replacement is created *alongside* the genuine object and then renamed over its
        // name, rather than written after removing it: a filesystem is free to hand the new file
        // the inode number the removed one just released — that reuse is precisely the hazard
        // HIGH-8 and HIGH-9 exist for — and this test's premise is that the two are different
        // physical files. Two files that exist simultaneously cannot share an inode, and a rename
        // moves a directory entry without touching the inode it points at, so the premise holds by
        // construction rather than by luck.
        let genuine_inode = inode_of(&quarantine_path);
        let directory = root.path().join("nestopia");
        let staged = directory.join("rf-replacement-staging");
        fs::write(&staged, owned).unwrap();
        let replacement_inode = inode_of(&staged);
        assert_ne!(replacement_inode, genuine_inode);
        fs::remove_file(&quarantine_path).unwrap();
        fs::rename(&staged, &quarantine_path).unwrap();
        assert_eq!(inode_of(&quarantine_path), replacement_inode);
        assert_ne!(inode_of(&quarantine_path), genuine_inode);

        assert_eq!(sweep_delete_quarantine(root.path()), 0);
        assert!(quarantine_path.exists(), "the file must not be adopted");
        assert_eq!(fs::read(&quarantine_path).unwrap(), owned);
        // The entry is not silently rewritten onto whatever now occupies the pathname either.
        let ownership = read_journal_entry(root.path(), &id).expect("the entry survives");
        assert_eq!(ownership.inode, genuine_inode);
        assert_ne!(ownership.inode, inode_of(&quarantine_path));
    }

    /// HIGH-7: the journal record round-trips, and every malformed shape is refused outright rather
    /// than half-trusted. A record that does not parse is not a weaker proof — it is no proof, and
    /// the quarantine object it names is then left strictly alone.
    #[test]
    fn a_journal_record_round_trips_and_refuses_every_malformed_shape() {
        let ownership = QuarantineOwnership {
            device: 66_309,
            inode: 4_198_401,
            size_bytes: 8192,
            sha256: digest_of(b"bytes"),
        };
        let rendered = ownership.render();
        assert!(rendered.starts_with("rfdj1:"));
        assert_eq!(QuarantineOwnership::parse(&rendered), Some(ownership));
        // Trailing whitespace from a durable write is tolerated; nothing else is.
        assert_eq!(
            QuarantineOwnership::parse(&format!("{rendered}\n")),
            Some(ownership)
        );
        assert!(rendered.len() as u64 <= MAX_JOURNAL_ENTRY_BYTES);

        let hex = ownership.sha256.to_hex();
        for malformed in [
            // The pre-HIGH-7 content-only record: it cannot identify a physical object at all, so
            // it must never be read as if it could.
            format!("8192:{hex}"),
            // Wrong or absent version marker.
            format!("rfdj2:66309:4198401:8192:{hex}"),
            format!("66309:4198401:8192:{hex}"),
            // Missing, extra, empty, negative, non-numeric, or overflowing fields.
            format!("rfdj1:66309:4198401:{hex}"),
            format!("rfdj1:66309:4198401:8192:{hex}:extra"),
            format!("rfdj1::4198401:8192:{hex}"),
            format!("rfdj1:-1:4198401:8192:{hex}"),
            format!("rfdj1:66309:four:8192:{hex}"),
            format!("rfdj1:66309:4198401:99999999999999999999999:{hex}"),
            // A digest that is not a digest.
            "rfdj1:66309:4198401:8192:not-a-digest".to_owned(),
            format!("rfdj1:66309:4198401:8192:{}", &hex[..63]),
            String::new(),
        ] {
            assert_eq!(
                QuarantineOwnership::parse(&malformed),
                None,
                "{malformed} must not parse"
            );
        }
    }

    /// A journal entry whose record does not parse proves nothing, so the object it names is left
    /// completely alone — never deleted, and never "repaired" by rewriting the entry.
    #[test]
    fn an_unparsable_journal_record_authorizes_nothing() {
        let root = states_root();
        let owned = b"the quarantined bytes";
        write(root.path(), "nestopia/ToQuarantine.state1", owned);
        let (quarantine, id) =
            interrupted_delete(root.path(), "nestopia/ToQuarantine.state1", owned);
        let quarantine_path = root.path().join("nestopia").join(&quarantine);

        // Exactly the record the pre-HIGH-7 format wrote, for this very object's own bytes.
        fs::write(
            root.path().join(DELETE_JOURNAL_DIR).join(&id),
            format!("{}:{}", owned.len(), digest_of(owned).to_hex()),
        )
        .unwrap();

        assert_eq!(sweep_delete_quarantine(root.path()), 0);
        assert_eq!(fs::read(&quarantine_path).unwrap(), owned);
        // The unreadable entry is left exactly as found rather than rewritten into a record that
        // would then authorize whatever occupies the pathname.
        assert_eq!(
            fs::read_to_string(root.path().join(DELETE_JOURNAL_DIR).join(&id)).unwrap(),
            format!("{}:{}", owned.len(), digest_of(owned).to_hex())
        );
    }

    // ============================================================ HIGH-8: the terminal ordering

    /// What the filesystem looked like at the instant before a destructive unlink.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct AtUnlink {
        quarantine_objects: usize,
        journal_entries: Vec<String>,
    }

    fn observe_at_unlink(root: &Path) -> AtUnlink {
        AtUnlink {
            quarantine_objects: no_quarantine_files(root),
            journal_entries: journal_ids(root),
        }
    }

    /// HIGH-8 regression: a durable ownership record must never outlive the physical object it
    /// authenticates.
    ///
    /// `(device, inode)` identifies an object only while that object exists. After the last link to
    /// it is unlinked the inode number becomes eligible for reuse, so a journal entry that survives
    /// its own object is a capability that a *future, unrelated* file could later satisfy — and a
    /// later sweep would then delete a file RetroFrontier never quarantined. The window is real
    /// because `unlink` and "remove the record" are two operations: a crash between them leaves
    /// exactly that stale capability.
    ///
    /// The fix is ordering, and this asserts it from production behaviour rather than by reading
    /// the source: at the instant before the destructive unlink, the object is still present and
    /// its authorizing entry is *already gone*.
    #[test]
    fn the_startup_sweep_retires_its_journal_before_the_destructive_unlink() {
        let root = states_root();
        let owned = b"the quarantined bytes";
        write(root.path(), "nestopia/ToQuarantine.state1", owned);
        interrupted_delete(root.path(), "nestopia/ToQuarantine.state1", owned);

        let observed = std::sync::Mutex::new(None);
        let observe = || {
            *observed.lock().unwrap() = Some(observe_at_unlink(root.path()));
        };

        assert_eq!(
            sweep_delete_quarantine_inner(
                root.path(),
                &SweepRaceHooks {
                    before_unlink: Some(&observe),
                    ..SweepRaceHooks::default()
                },
            ),
            1
        );

        let at_unlink = observed.lock().unwrap().take().expect("the hook fired");
        assert_eq!(
            at_unlink.quarantine_objects, 1,
            "the object must still exist when the unlink is attempted"
        );
        assert!(
            at_unlink.journal_entries.is_empty(),
            "no ownership capability may still exist at the unlink point, got {:?}",
            at_unlink.journal_entries
        );

        // And the successful path is terminal in both directions.
        assert_eq!(no_quarantine_files(root.path()), 0);
        assert!(journal_ids(root.path()).is_empty());
    }

    /// The same rule on the live delete path. There must not be one destructive path that still
    /// unlinks before retiring its record.
    #[test]
    fn a_live_delete_retires_its_journal_before_the_destructive_unlink() {
        let root = states_root();
        let bytes = b"the registered state bytes";
        write(root.path(), "nestopia/Synthetic.state1", bytes);

        let observed = std::sync::Mutex::new(None);
        let observe = || {
            *observed.lock().unwrap() = Some(observe_at_unlink(root.path()));
        };

        delete_verified_managed_file_inner(
            root.path(),
            &path("nestopia/Synthetic.state1"),
            bytes.len() as u64,
            digest_of(bytes),
            &DeleteRaceHooks {
                before_unlink: Some(&observe),
                ..DeleteRaceHooks::default()
            },
        )
        .unwrap();

        let at_unlink = observed.lock().unwrap().take().expect("the hook fired");
        assert_eq!(
            at_unlink.quarantine_objects, 1,
            "the quarantined object must still exist when the unlink is attempted"
        );
        assert!(
            at_unlink.journal_entries.is_empty(),
            "no ownership capability may still exist at the unlink point, got {:?}",
            at_unlink.journal_entries
        );

        assert!(!root.path().join("nestopia/Synthetic.state1").exists());
        assert_eq!(no_quarantine_files(root.path()), 0);
        assert!(journal_ids(root.path()).is_empty());
    }

    /// HIGH-8 regression: a crash — or any failure — *after* the record is retired but before the
    /// unlink completes leaves an inert orphan, and that is the deliberate outcome.
    ///
    /// The alternative is retaining a capability that outlives its object, which is exactly the
    /// defect. RetroFrontier leaks a tiny owned orphan rather than keep a record that a reused
    /// inode could later satisfy. The orphan is never automatically deleted again, and no later
    /// sweep manufactures a replacement record for it from its name, size, digest, or observed
    /// inode.
    #[test]
    fn a_failure_after_journal_retirement_leaves_an_inert_orphan_that_is_never_swept_again() {
        let root = states_root();
        let owned = b"the quarantined bytes";
        write(
            root.path(),
            "nestopia/Sibling.state1",
            b"an unrelated state",
        );
        write(root.path(), "nestopia/ToQuarantine.state1", owned);
        interrupted_delete(root.path(), "nestopia/ToQuarantine.state1", owned);

        // The unlink is made to fail deterministically at exactly the moment it is attempted, by
        // removing write permission from the containing directory — a stand-in for a crash landing
        // in the same window.
        let directory = root.path().join("nestopia");
        let seal = || {
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();
        };
        let removed = sweep_delete_quarantine_inner(
            root.path(),
            &SweepRaceHooks {
                before_unlink: Some(&seal),
                ..SweepRaceHooks::default()
            },
        );
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();

        assert_eq!(removed, 0, "nothing was deleted");
        // The intended terminal state: the object is still there, and nothing authorizes it.
        assert_eq!(no_quarantine_files(root.path()), 1);
        assert!(
            journal_ids(root.path()).is_empty(),
            "the retired record must not come back because the unlink failed"
        );

        let orphan = root.path().join("nestopia").join(format!(
            "{QUARANTINE_PREFIX}{}",
            surviving_quarantine_id(root.path())
        ));
        assert_eq!(fs::read(&orphan).unwrap(), owned);

        // An ordinary later startup leaves it strictly alone: no proof, no action, and no record
        // reconstructed from the file's own name, size, digest, or current inode.
        for _ in 0..3 {
            assert_eq!(sweep_delete_quarantine(root.path()), 0);
            assert_eq!(no_quarantine_files(root.path()), 1);
            assert!(
                journal_ids(root.path()).is_empty(),
                "ownership must never be manufactured for an inert orphan"
            );
            assert_eq!(fs::read(&orphan).unwrap(), owned);
        }
        // Everything around it is untouched.
        assert_eq!(
            fs::read(root.path().join("nestopia/Sibling.state1")).unwrap(),
            b"an unrelated state"
        );
    }

    /// HIGH-8: the successful destructive ordering cannot produce "object gone, record present".
    ///
    /// Inode reuse is not something a test can force deterministically, so the property proved here
    /// is the one that makes reuse unexploitable: the capability never survives the object. Both
    /// destructive paths are exercised, and after each the journal is empty.
    #[test]
    fn a_successful_delete_never_leaves_a_record_behind_its_object() {
        let root = states_root();

        // Live delete.
        let bytes = b"live delete bytes";
        write(root.path(), "nestopia/Live.state1", bytes);
        delete_verified_managed_file(
            root.path(),
            &path("nestopia/Live.state1"),
            bytes.len() as u64,
            digest_of(bytes),
        )
        .unwrap();
        assert!(!root.path().join("nestopia/Live.state1").exists());
        assert_eq!(no_quarantine_files(root.path()), 0);
        assert!(
            journal_ids(root.path()).is_empty(),
            "a completed live delete leaves no capability"
        );

        // Startup recovery.
        let recovered = b"recovered bytes";
        write(root.path(), "nestopia/Recovered.state1", recovered);
        interrupted_delete(root.path(), "nestopia/Recovered.state1", recovered);
        assert_eq!(sweep_delete_quarantine(root.path()), 1);
        assert_eq!(no_quarantine_files(root.path()), 0);
        assert!(
            journal_ids(root.path()).is_empty(),
            "a completed recovery leaves no capability"
        );
        assert_eq!(sweep_delete_quarantine(root.path()), 0);
    }

    // ======================================= HIGH-9: the terminal invariant is journal-wide

    /// What the filesystem looked like at the instant before the *final* unlink, stated as the
    /// HIGH-9 invariant rather than as "one id is gone".
    #[derive(Debug)]
    struct AtFinalUnlink {
        /// The object about to be destroyed, as it physically is at this instant.
        object: QuarantineOwnership,
        /// Every valid journal record anywhere in the journal that authorizes that exact object.
        authorizing_records: Vec<String>,
        /// Everything still in the journal, whether it authorizes this object or not.
        journal_entries: Vec<String>,
    }

    /// Observe the terminal invariant from the outside: find the object that is about to be
    /// unlinked, read its *actual* physical identity, and then ask the whole journal whether
    /// anything still authorizes it.
    ///
    /// The ownership is deliberately recomputed from the live object rather than remembered from a
    /// record, so the comparison is against the physical thing being destroyed.
    fn observe_at_final_unlink(root: &Path, bytes: &[u8]) -> AtFinalUnlink {
        let object = surviving_quarantine_path(root);
        assert!(object.exists(), "the object must still exist at the unlink");
        let ownership = ownership_of(&object, bytes);
        AtFinalUnlink {
            object: ownership,
            authorizing_records: authorizing_journal_ids(root, ownership),
            journal_entries: journal_ids(root),
        }
    }

    /// HIGH-9 regression: a *previous-stage* record whose retirement fails must stop the final
    /// unlink, even when the current stage's record would have retired perfectly well.
    ///
    /// A recovery has two live records at its terminal boundary. `J2` names the object at its
    /// second-stage name and is retired durably before the unlink (HIGH-8). `J1` names the *same
    /// physical object* — same device, same inode, same size, same digest — and used to be removed
    /// here best-effort, by a helper that returns nothing and discards filesystem errors. So this
    /// state was reachable: `J1`'s removal fails and is ignored, `J2` retires, the object is
    /// unlinked, and `J1` survives as a durable capability over an inode number that is now free
    /// for reuse. That is precisely the class HIGH-8 was meant to eliminate, one generation older.
    ///
    /// The failure is injected at the single point where a journal entry is removed, for exactly
    /// one id, so `J1` cannot be retired while everything else could be.
    #[test]
    fn a_previous_stage_journal_failure_prevents_final_unlink() {
        let root = states_root();
        let owned = b"the quarantined bytes";
        write(
            root.path(),
            "nestopia/Sibling.state1",
            b"an unrelated state",
        );
        write(root.path(), "nestopia/ToQuarantine.state1", owned);
        let (quarantine, first_stage) =
            interrupted_delete(root.path(), "nestopia/ToQuarantine.state1", owned);
        let object = ownership_of(&root.path().join("nestopia").join(&quarantine), owned);

        // Only the first-stage record refuses to go; the second-stage record the sweep is about to
        // write would retire without trouble.
        force_journal_unlink_failure(&first_stage);

        let removed = sweep_delete_quarantine(root.path());

        // The invariant, stated exactly as the finding states it: the system must never reach
        // "object destroyed, while a record still authorizes that object's physical identity".
        let surviving = authorizing_journal_ids(root.path(), object);
        if no_quarantine_files(root.path()) == 0 {
            assert!(
                surviving.is_empty(),
                "the object was unlinked while {surviving:?} still authorized its physical identity"
            );
        }

        assert_eq!(removed, 0, "nothing may be reported as finished");
        assert_eq!(
            no_quarantine_files(root.path()),
            1,
            "the physical object must survive an incomplete retirement"
        );
        let object_path = surviving_quarantine_path(root.path());
        assert_eq!(fs::read(&object_path).unwrap(), owned);
        assert_eq!(
            ownership_of(&object_path, owned),
            object,
            "it is still the same physical object, not a replacement"
        );
        // And it survived *because* the previous-stage record is still authorizing it.
        assert!(
            surviving.contains(&first_stage),
            "the un-retirable first-stage record must still be present, got {surviving:?}"
        );
        // Nothing unrelated was touched.
        assert_eq!(
            fs::read(root.path().join("nestopia/Sibling.state1")).unwrap(),
            b"an unrelated state"
        );

        // Repeating recovery keeps refusing: it never destroys the object while a record for it
        // cannot be retired, and it never manufactures a replacement record either.
        for _ in 0..3 {
            assert_eq!(sweep_delete_quarantine(root.path()), 0);
            assert_eq!(no_quarantine_files(root.path()), 1);
            let object_path = surviving_quarantine_path(root.path());
            assert_eq!(fs::read(&object_path).unwrap(), owned);
            assert!(authorizing_journal_ids(root.path(), object).contains(&first_stage));
        }
    }

    /// HIGH-9 regression, stated as the terminal invariant itself:
    ///
    /// > Immediately before the final unlink, no valid journal record exists anywhere whose
    /// > `QuarantineOwnership` equals the physical object about to be destroyed.
    ///
    /// This deliberately does **not** check that one particular id is absent. It reads the object's
    /// real physical identity off the object that is about to be destroyed, parses *every* valid
    /// record in the journal, and requires that none of them names it.
    ///
    /// The startup sweep is the multi-record path: `J1` from the interrupted delete, `J2` written
    /// for the second stage, plus — planted here — a redundant duplicate record of the kind a crash
    /// between generations can leave, all naming the same object. An uninterrupted live delete
    /// writes exactly one record, which is asserted below as well, on the same invariant.
    #[test]
    fn no_authorizing_record_exists_for_the_object_at_final_unlink() {
        // --- the startup sweep, with several records naming one object.
        let root = states_root();
        let owned = b"the quarantined bytes";
        write(root.path(), "nestopia/ToQuarantine.state1", owned);
        let (quarantine, first_stage) =
            interrupted_delete(root.path(), "nestopia/ToQuarantine.state1", owned);
        let object = ownership_of(&root.path().join("nestopia").join(&quarantine), owned);

        // A redundant record naming the same physical object, as an interrupted earlier generation
        // would have left behind. It authorizes the object just as fully as the others do.
        let redundant = "0123456789abcdef0123456789abcdef";
        fs::write(
            root.path().join(DELETE_JOURNAL_DIR).join(redundant),
            object.render(),
        )
        .unwrap();
        assert_eq!(
            authorizing_journal_ids(root.path(), object).len(),
            2,
            "the sweep must start with more than one authorizing record"
        );

        let observed = std::sync::Mutex::new(None);
        let observe = || {
            *observed.lock().unwrap() = Some(observe_at_final_unlink(root.path(), owned));
        };
        assert_eq!(
            sweep_delete_quarantine_inner(
                root.path(),
                &SweepRaceHooks {
                    before_unlink: Some(&observe),
                    ..SweepRaceHooks::default()
                },
            ),
            1
        );

        let at_unlink = observed.lock().unwrap().take().expect("the hook fired");
        assert_eq!(
            at_unlink.object, object,
            "the object at the unlink is the one the records named"
        );
        assert!(
            at_unlink.authorizing_records.is_empty(),
            "no record may authorize this object at the final unlink, got {:?} of {:?}",
            at_unlink.authorizing_records,
            at_unlink.journal_entries
        );
        // Every generation went, not merely the current one.
        assert!(!at_unlink.journal_entries.contains(&first_stage.to_string()));
        assert!(!at_unlink.journal_entries.contains(&redundant.to_owned()));
        assert_eq!(no_quarantine_files(root.path()), 0);
        assert!(journal_ids(root.path()).is_empty());

        // --- the live delete. It can only ever create one record for the object it quarantines,
        // because it quarantines a file it has just verified under its own registered name; the
        // same journal-wide invariant is required of it regardless.
        let live = b"the registered state bytes";
        write(root.path(), "nestopia/Live.state1", live);
        let observed = std::sync::Mutex::new(None);
        let observe = || {
            *observed.lock().unwrap() = Some(observe_at_final_unlink(root.path(), live));
        };
        delete_verified_managed_file_inner(
            root.path(),
            &path("nestopia/Live.state1"),
            live.len() as u64,
            digest_of(live),
            &DeleteRaceHooks {
                before_unlink: Some(&observe),
                ..DeleteRaceHooks::default()
            },
        )
        .unwrap();

        let at_unlink = observed.lock().unwrap().take().expect("the hook fired");
        assert!(
            at_unlink.authorizing_records.is_empty(),
            "no record may authorize this object at the final unlink, got {:?} of {:?}",
            at_unlink.authorizing_records,
            at_unlink.journal_entries
        );
        assert!(!root.path().join("nestopia/Live.state1").exists());
        assert_eq!(no_quarantine_files(root.path()), 0);
        assert!(journal_ids(root.path()).is_empty());
    }

    /// HIGH-9: the retirement set is one security decision, and *proving* it is part of the
    /// decision. A retirement whose durability cannot be proven must not be treated as a
    /// retirement, even though its removals may already have taken effect in the current
    /// filesystem view.
    ///
    /// This is also the case the failure logging must not overclaim: the object is left in place,
    /// but the records are not promised to still be there.
    #[test]
    fn a_retirement_that_cannot_be_proven_durable_never_unlinks_the_object() {
        let root = states_root();
        let owned = b"the quarantined bytes";
        write(root.path(), "nestopia/ToQuarantine.state1", owned);
        interrupted_delete(root.path(), "nestopia/ToQuarantine.state1", owned);

        // The removals are allowed to happen; committing them is not.
        force_journal_fsync_failure();

        assert_eq!(sweep_delete_quarantine(root.path()), 0);

        // The object stays — that is the whole point of failing closed here.
        assert_eq!(no_quarantine_files(root.path()), 1);
        let object = surviving_quarantine_path(root.path());
        assert_eq!(fs::read(&object).unwrap(), owned);

        // Whatever survived of the journal, no ownership is ever manufactured for the object from
        // its name, size, digest, or currently observed inode.
        let ownership = ownership_of(&object, owned);
        for _ in 0..3 {
            assert_eq!(sweep_delete_quarantine(root.path()), 0);
            assert_eq!(no_quarantine_files(root.path()), 1);
            assert_eq!(
                fs::read(surviving_quarantine_path(root.path())).unwrap(),
                owned
            );
            assert_eq!(
                ownership_of(&surviving_quarantine_path(root.path()), owned),
                ownership,
                "the object is never replaced or re-adopted"
            );
        }
    }

    /// The journal-wide retirement removes exactly the records that authorize the object, and
    /// nothing else — and an entry it cannot parse neither authorizes the object nor blocks it.
    ///
    /// Both halves matter. If an unparseable entry counted as authority, one stray file in the
    /// journal directory would block every delete forever; if it were "repaired", the module would
    /// be manufacturing ownership. It is instead treated as what it is: no proof, and no business
    /// of this operation's.
    #[test]
    fn a_malformed_journal_entry_neither_authorizes_nor_blocks_the_terminal_retirement() {
        let root = states_root();
        let owned = b"the quarantined bytes";
        write(root.path(), "nestopia/ToQuarantine.state1", owned);
        interrupted_delete(root.path(), "nestopia/ToQuarantine.state1", owned);

        let journal = root.path().join(DELETE_JOURNAL_DIR);
        let planted: Vec<(&str, Vec<u8>)> = vec![
            ("not-a-record", b"rfdj1:nonsense".to_vec()),
            ("not-even-text", vec![0xff, 0xfe, 0x00, 0x01]),
            (
                "far-too-large",
                vec![b'x'; MAX_JOURNAL_ENTRY_BYTES as usize + 1],
            ),
        ];
        for (name, bytes) in &planted {
            fs::write(journal.join(name), bytes).unwrap();
        }
        // A symbolic link is unreadable to every reader here, so it can never be a record either.
        std::os::unix::fs::symlink("../nestopia", journal.join("a-link")).unwrap();

        assert_eq!(
            sweep_delete_quarantine(root.path()),
            1,
            "unparseable clutter must not block a proven recovery"
        );
        assert_eq!(no_quarantine_files(root.path()), 0);

        // Everything the retirement had no authority over is still there, byte for byte.
        for (name, bytes) in &planted {
            assert_eq!(&fs::read(journal.join(name)).unwrap(), bytes, "{name}");
        }
        assert!(fs::symlink_metadata(journal.join("a-link"))
            .unwrap()
            .file_type()
            .is_symlink());
    }

    /// The same live-delete refusal, from the caller's side: an unprovable retirement fails the
    /// delete rather than removing the file.
    #[test]
    fn a_live_delete_refuses_when_its_retirement_cannot_be_proven() {
        let root = states_root();
        let bytes = b"the registered state bytes";
        write(root.path(), "nestopia/Synthetic.state1", bytes);

        force_journal_fsync_failure();

        assert_eq!(
            delete_verified_managed_file(
                root.path(),
                &path("nestopia/Synthetic.state1"),
                bytes.len() as u64,
                digest_of(bytes),
            ),
            Err(SaveStateError::DeleteFailed)
        );
        // The verified file was put back under its own name, and it is still the state it was.
        assert_eq!(
            fs::read(root.path().join("nestopia/Synthetic.state1")).unwrap(),
            bytes
        );
        assert_eq!(no_quarantine_files(root.path()), 0);
    }

    fn inode_of(path: &Path) -> u64 {
        use std::os::unix::fs::MetadataExt;
        fs::symlink_metadata(path).unwrap().ino()
    }

    /// The real physical identity of an on-disk file, as a genuine delete would journal it.
    fn ownership_of(path: &Path, bytes: &[u8]) -> QuarantineOwnership {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(path).unwrap();
        QuarantineOwnership {
            device: metadata.dev(),
            inode: metadata.ino(),
            size_bytes: metadata.size(),
            sha256: digest_of(bytes),
        }
    }

    /// Every id currently present in the durable delete-operation journal.
    fn journal_ids(root: &Path) -> Vec<String> {
        let Ok(entries) = fs::read_dir(root.join(DELETE_JOURNAL_DIR)) else {
            return Vec::new();
        };
        entries
            .filter_map(|entry| Some(entry.ok()?.file_name().to_str()?.to_owned()))
            .collect()
    }

    /// Every journal id whose record is valid *and* names one exact physical object.
    ///
    /// This reads and parses the journal itself rather than asking the adapter about one id: the
    /// HIGH-9 invariant is a statement about the whole journal, so the test has to enumerate it.
    fn authorizing_journal_ids(root: &Path, ownership: QuarantineOwnership) -> Vec<String> {
        let mut ids: Vec<String> = journal_ids(root)
            .into_iter()
            .filter(|id| {
                fs::read_to_string(root.join(DELETE_JOURNAL_DIR).join(id))
                    .ok()
                    .and_then(|record| QuarantineOwnership::parse(&record))
                    == Some(ownership)
            })
            .collect();
        ids.sort();
        ids
    }

    /// The path of the single quarantine object left in the tree.
    fn surviving_quarantine_path(root: &Path) -> std::path::PathBuf {
        let snapshot = snapshot_state_tree(root);
        let mut paths: Vec<std::path::PathBuf> = snapshot
            .entries()
            .filter(|(relative_path, _)| {
                relative_path
                    .as_str()
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .starts_with(QUARANTINE_PREFIX)
            })
            .map(|(relative_path, _)| root.join(relative_path.as_str()))
            .collect();
        assert_eq!(paths.len(), 1);
        paths.pop().unwrap()
    }

    /// The journal id of the single quarantine object left in the tree.
    fn surviving_quarantine_id(root: &Path) -> String {
        let snapshot = snapshot_state_tree(root);
        let mut ids: Vec<String> = snapshot
            .entries()
            .filter_map(|(relative_path, _)| {
                relative_path
                    .as_str()
                    .rsplit('/')
                    .next()?
                    .strip_prefix(QUARANTINE_PREFIX)
                    .map(str::to_owned)
            })
            .collect();
        assert_eq!(ids.len(), 1);
        ids.pop().unwrap()
    }

    fn no_quarantine_files(root: &Path) -> usize {
        snapshot_state_tree(root)
            .entries()
            .filter(|(relative_path, _)| {
                relative_path
                    .as_str()
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .starts_with(QUARANTINE_PREFIX)
            })
            .count()
    }
}
