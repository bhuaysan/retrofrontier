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
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
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
pub fn snapshot_state_tree(states_root: &Path) -> StateTreeSnapshot {
    let mut snapshot = StateTreeSnapshot {
        entries: BTreeMap::new(),
        complete: true,
    };
    walk(states_root, "", 0, &mut snapshot);
    snapshot
}

fn walk(states_root: &Path, prefix: &str, depth: usize, snapshot: &mut StateTreeSnapshot) {
    if depth > MAX_SNAPSHOT_DEPTH {
        snapshot.complete = false;
        return;
    }
    let directory = if prefix.is_empty() {
        states_root.to_path_buf()
    } else {
        states_root.join(prefix)
    };
    let Ok(read) = std::fs::read_dir(&directory) else {
        // An unreadable directory is uncertainty, not absence.
        snapshot.complete = false;
        return;
    };
    for entry in read {
        let Ok(entry) = entry else {
            snapshot.complete = false;
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            // A non-UTF-8 name cannot become a validated relative path, so the tree cannot be
            // described completely.
            snapshot.complete = false;
            continue;
        };
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let Ok(metadata) = entry.path().symlink_metadata() else {
            snapshot.complete = false;
            continue;
        };
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            // A symbolic link in a directory RetroFrontier owns is an anomaly. It is not part of
            // the managed tree, and the enumeration is reported incomplete so the anomaly can
            // never contribute to a destructive decision.
            snapshot.complete = false;
            continue;
        }
        if file_type.is_dir() {
            walk(states_root, &relative, depth + 1, snapshot);
            continue;
        }
        if !file_type.is_file() {
            snapshot.complete = false;
            continue;
        }
        let Ok(relative_path) = RelativePath::new(relative) else {
            snapshot.complete = false;
            continue;
        };
        if snapshot.entries.len() >= MAX_SNAPSHOT_ENTRIES {
            snapshot.complete = false;
            return;
        }
        snapshot
            .entries
            .insert(relative_path, physical_identity(&metadata));
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
        None,
    )
}

/// Remove quarantine files left behind by a crash between the rename and the unlink.
///
/// A quarantine name parses as `Unsupported`, so a leftover is inert: it is never attributed, never
/// listed, and never loaded. Sweeping it keeps the owned tree tidy and nothing more, and it only
/// ever touches names RetroFrontier itself creates.
pub fn sweep_delete_quarantine(states_root: &Path) -> usize {
    let snapshot = snapshot_state_tree(states_root);
    let mut removed = 0;
    for (relative_path, _) in snapshot.entries.iter() {
        let name = relative_path
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or_default();
        if !name.starts_with(QUARANTINE_PREFIX) {
            continue;
        }
        let Ok(parent) = open_parent_directory(states_root, relative_path) else {
            continue;
        };
        if rustix::fs::unlinkat(&parent, name, rustix::fs::AtFlags::empty()).is_ok() {
            removed += 1;
        }
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

    let descriptor = rustix::fs::openat(
        &parent,
        name.as_str(),
        FILE_OPEN_FLAGS,
        rustix::fs::Mode::empty(),
    )
    .map_err(|_| SaveStateError::UnsafeFilesystemTarget)?;
    let file = std::fs::File::from(descriptor);
    let metadata = file
        .metadata()
        .map_err(|_| SaveStateError::UnsafeFilesystemTarget)?;
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
            .map_err(|_| SaveStateError::UnsafeFilesystemTarget)?;

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
        .map_err(|_| SaveStateError::UnsafeFilesystemTarget)?;
    }
    Ok(current)
}

fn hash_descriptor(mut file: std::fs::File) -> Result<Sha256Digest, SaveStateError> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; HASH_BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| SaveStateError::UnsafeFilesystemTarget)?;
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

static QUARANTINE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn quarantine_name() -> String {
    format!(
        "{QUARANTINE_PREFIX}{}-{}",
        std::process::id(),
        QUARANTINE_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// The delete implementation, with an injectable hook for the adversarial replacement test.
///
/// The hook runs exactly in the window an attacker would need: after the verifying open, before
/// the destructive step. Production passes `None`.
fn delete_verified_managed_file_inner(
    states_root: &Path,
    relative_path: &RelativePath,
    expected_size: u64,
    expected_sha256: Sha256Digest,
    before_rename: Option<&(dyn Fn() + Send + Sync)>,
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

    if let Some(hook) = before_rename {
        hook();
    }

    // Move the verified inode to a name only RetroFrontier knows. `renameat` is atomic within one
    // directory, so after this either the rename happened or it did not — and a replacement racing
    // us can only end up owning the *old* name, which we no longer touch.
    let quarantine = quarantine_name();
    rustix::fs::renameat(&parent, name.as_str(), &parent, quarantine.as_str())
        .map_err(|_| SaveStateError::DeleteFailed)?;

    // Re-verify at the quarantine name. This is the authoritative check: it proves the inode now
    // sitting there is the one that was verified, so the unlink below cannot remove anything else.
    let restore = || {
        // Nothing was deleted. Put the name back so the filesystem is left as it was found; a
        // failure to restore is logged by the caller and still deletes nothing.
        let _ = rustix::fs::renameat(&parent, quarantine.as_str(), &parent, name.as_str());
    };
    let descriptor = match rustix::fs::openat(
        &parent,
        quarantine.as_str(),
        FILE_OPEN_FLAGS,
        rustix::fs::Mode::empty(),
    ) {
        Ok(descriptor) => descriptor,
        Err(_) => {
            restore();
            return Err(SaveStateError::UnsafeFilesystemTarget);
        }
    };
    let quarantined = std::fs::File::from(descriptor);
    let matches = quarantined
        .metadata()
        .map(|metadata| {
            metadata.file_type().is_file()
                && metadata.dev() == verified_device
                && metadata.ino() == verified_identity.inode
                && metadata.size() == verified_identity.size_bytes
        })
        .unwrap_or(false);
    drop(quarantined);
    if !matches {
        restore();
        return Err(SaveStateError::UnsafeFilesystemTarget);
    }

    rustix::fs::unlinkat(&parent, quarantine.as_str(), rustix::fs::AtFlags::empty()).map_err(|_| {
        restore();
        SaveStateError::DeleteFailed
    })
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
            assert!(!quarantine_name().contains(STATE_SUFFIX));
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
            Some(&swap),
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
                Some(&swap),
            ),
            Err(SaveStateError::UnsafeFilesystemTarget)
        );
        assert_eq!(fs::read(&target).unwrap(), attacker);
        assert_eq!(fs::read(&stashed).unwrap(), bytes);
        assert_eq!(no_quarantine_files(root.path()), 0);
    }

    #[test]
    fn a_quarantine_file_left_by_a_crash_is_inert_and_can_be_swept() {
        let root = states_root();
        write(root.path(), "Nestopia/Synthetic.state1", b"real state");
        write(
            root.path(),
            "Nestopia/Synthetic.state1.png",
            b"real thumbnail",
        );
        // Exactly what a crash between the rename and the unlink leaves behind.
        write(root.path(), "Nestopia/.rf-delete-4242-0", b"orphaned bytes");

        // It is not attributable, so reconciliation would never register it.
        assert_eq!(
            parse_state_candidate(&path("Nestopia/.rf-delete-4242-0")),
            StateCandidate::Unsupported
        );

        assert_eq!(sweep_delete_quarantine(root.path()), 1);

        assert!(!root.path().join("Nestopia/.rf-delete-4242-0").exists());
        // The sweep only ever removes names RetroFrontier itself creates.
        assert!(root.path().join("Nestopia/Synthetic.state1").exists());
        assert!(root.path().join("Nestopia/Synthetic.state1.png").exists());
        assert_eq!(sweep_delete_quarantine(root.path()), 0);
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
