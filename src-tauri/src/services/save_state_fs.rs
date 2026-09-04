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
//! `savestate_directory`. With `sort_savestates_enable` — which the generated configuration sets —
//! it inserts the **core-reported `sysinfo->library_name`** as a subdirectory. On the qualified
//! managed runtime that produced `states/Nestopia/`, `states/bsnes-mercury/`, and
//! `states/dolphin-emu/`, which are emphatically *not* the RetroFrontier `CoreId`s `nestopia`,
//! `bsnes-mercury-balanced`, and `dolphin`. **Nothing here reverse-maps a directory name to a
//! core.** The parse result carries no core field at all; core provenance comes from the
//! controlled launch that produced the file.
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
//! that descriptor. Deletion additionally renames the file to a private same-directory quarantine
//! name and re-verifies the inode *there* before unlinking, which closes the window entirely:
//!
//! > RetroFrontier deletes exactly the previously verified regular file under its owned Save-State
//! > root, or deletes nothing.

use crate::adapters::runtime_integrity::HASH_BUFFER_BYTES;
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

/// Delete exactly the previously verified regular file, or delete nothing.
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
/// written *before* the object was ever moved there, recording the exact size and digest of the
/// content that was quarantined. Only a `.rf-delete-*` name with such a proof is touched at all:
///
/// - no matching journal entry → left completely alone (it was never proven RF-owned);
/// - a matching entry whose recorded identity the file no longer has → left in place, not deleted
///   (the same content re-verification an in-flight delete performs, so a crash window cannot
///   relax the guarantee an interrupted delete makes over an uninterrupted one);
/// - a matching entry whose recorded identity the file still has → finished: unlinked, and the
///   journal entry removed with it.
///
/// This makes the sweep idempotent and safe to run on every startup: a genuine RetroFrontier
/// delete that crashed between its rename and its unlink is completed exactly as it would have
/// completed uninterrupted, and nothing else is ever touched.
pub fn sweep_delete_quarantine(states_root: &Path) -> usize {
    let snapshot = snapshot_state_tree(states_root);
    let mut removed = 0;
    for (relative_path, _) in snapshot.entries.iter() {
        let name = relative_path
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or_default();
        let Some(id) = name.strip_prefix(QUARANTINE_PREFIX) else {
            continue;
        };
        let Some((expected_size, expected_sha256)) = read_journal_entry(states_root, id) else {
            continue;
        };
        let Ok(parent) = open_parent_directory(states_root, relative_path) else {
            continue;
        };
        let matches = rustix::fs::openat(&parent, name, FILE_OPEN_FLAGS, rustix::fs::Mode::empty())
            .ok()
            .and_then(|descriptor| {
                let file = std::fs::File::from(descriptor);
                let metadata = file.metadata().ok()?;
                if !metadata.file_type().is_file() || metadata.len() != expected_size {
                    return Some(false);
                }
                Some(hash_descriptor(file).ok() == Some(expected_sha256))
            })
            .unwrap_or(false);
        if matches {
            if rustix::fs::unlinkat(&parent, name, rustix::fs::AtFlags::empty()).is_ok() {
                removed += 1;
            }
        } else {
            tracing::warn!(
                "a quarantined save-state file no longer matches the identity its delete journal \
                 recorded; it was left in place rather than removed"
            );
        }
        remove_journal_entry(states_root, id);
    }
    removed
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

/// Bound on one journal entry's serialized size — comfortably larger than `"<u64>:<64 hex
/// chars>"` ever is. Anything larger at read time is refused rather than parsed.
const MAX_JOURNAL_ENTRY_BYTES: u64 = 256;

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
/// A durable journal entry recording the verified size and digest is written *before* the move, so
/// `sweep_delete_quarantine` can later *prove*, not merely assume, that a given `.rf-delete-*` name
/// is one RetroFrontier itself created for this exact content. The move itself uses `NOREPLACE`: an
/// (astronomically unlikely) name collision fails and retries with a fresh identifier rather than
/// destroying a file this operation never verified.
fn quarantine_verified_file(
    states_root: &Path,
    parent: &rustix::fd::OwnedFd,
    name: &str,
    expected_size: u64,
    expected_sha256: Sha256Digest,
) -> Result<String, SaveStateError> {
    let journal = open_or_create_delete_journal_dir(states_root)?;
    let entry = format!("{expected_size}:{}", expected_sha256.to_hex());
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

/// Read back one journal entry's recorded identity, or `None` if it does not exist, is unsafe, or
/// does not parse — every one of those is treated as "not proven", never as a fact to act on.
fn read_journal_entry(states_root: &Path, id: &str) -> Option<(u64, Sha256Digest)> {
    // Journal ids are always produced by `quarantine_id`, but this is read at sweep time from a
    // filename, so it is revalidated as a safe single path component rather than trusted.
    if id.is_empty() || id.contains('/') || id.contains('\0') {
        return None;
    }
    let journal = open_delete_journal_dir(states_root).ok()?;
    let descriptor = rustix::fs::openat(&journal, id, FILE_OPEN_FLAGS, rustix::fs::Mode::empty())
        .ok()?;
    let mut file = std::fs::File::from(descriptor);
    let metadata = file.metadata().ok()?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_JOURNAL_ENTRY_BYTES {
        return None;
    }
    let mut contents = String::new();
    file.read_to_string(&mut contents).ok()?;
    let (size, sha256) = contents.split_once(':')?;
    Some((size.parse().ok()?, Sha256Digest::from_hex(sha256).ok()?))
}

/// Remove one journal entry, if it exists. Best-effort: a leftover entry is never mistaken for
/// proof of anything by itself — only a matching `.rf-delete-*` name together with its entry is.
fn remove_journal_entry(states_root: &Path, id: &str) {
    if let Ok(journal) = open_delete_journal_dir(states_root) {
        let _ = rustix::fs::unlinkat(&journal, id, rustix::fs::AtFlags::empty());
    }
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
    let quarantine = quarantine_verified_file(
        states_root,
        &parent,
        name.as_str(),
        expected_size,
        expected_sha256,
    )?;
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
    // What is still guaranteed: the exact previously verified bytes are deleted, or nothing is,
    // against every actor path substitution, hard links, symlinks, and directory swaps can
    // produce; for a hostile same-inode writer, the remaining window is narrowed to the instant
    // between this re-hash and the `unlinkat` immediately below, not the whole delete.
    let content_matches = matches!(hash_descriptor(quarantined), Ok(sha256) if sha256 == expected_sha256);
    if !content_matches {
        restore(&quarantine);
        return Err(SaveStateError::UnsafeFilesystemTarget);
    }

    rustix::fs::unlinkat(&parent, quarantine.as_str(), rustix::fs::AtFlags::empty()).map_err(
        |_| {
            restore(&quarantine);
            SaveStateError::DeleteFailed
        },
    )?;
    if let Some(id) = quarantine.strip_prefix(QUARANTINE_PREFIX) {
        remove_journal_entry(states_root, id);
    }
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
            bytes.len() as u64,
            digest_of(bytes),
        )
        .unwrap();
        assert!(!root.path().join("Nestopia/ToQuarantine.state1").exists());
        assert_eq!(no_quarantine_files(root.path()), 1);

        // The startup sweep proves ownership from the durable journal entry, re-verifies the
        // content one last time, and finishes the interrupted delete.
        assert_eq!(sweep_delete_quarantine(root.path()), 1);
        assert!(!root.path().join("Nestopia").join(&quarantine).exists());
        assert_eq!(no_quarantine_files(root.path()), 0);
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
        write(root.path(), "Nestopia/.rf-delete-fake", b"a user's own file");

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
