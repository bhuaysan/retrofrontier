use crate::domain::runtime::{RuntimeError, SafeIdentifier};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

#[derive(Debug)]
pub struct RuntimeMutationLock {
    file: File,
    path: PathBuf,
}

impl RuntimeMutationLock {
    pub fn acquire(path: &Path) -> Result<Self, RuntimeError> {
        let file = open_lock_file(path)?;
        match fs4::FileExt::try_lock(&file) {
            Ok(()) => Ok(Self {
                file,
                path: path.to_path_buf(),
            }),
            Err(fs4::TryLockError::WouldBlock) => Err(RuntimeError::Lock(format!(
                "another runtime mutation owns {}",
                path.display()
            ))),
            Err(error) => Err(RuntimeError::Lock(format!(
                "could not lock {}: {error}",
                path.display()
            ))),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Test-only: a second descriptor onto the same open file description.
    ///
    /// This is what a `fork` in a parallel test thread hands its child, and it is the only way to
    /// set that condition up deterministically.
    #[cfg(test)]
    pub fn duplicate_descriptor(&self) -> File {
        self.file
            .try_clone()
            .expect("the lock descriptor should be duplicable")
    }
}

/// Test-only deterministic release.
///
/// Production releases this lock by closing its descriptor. `flock` belongs to the open file
/// description rather than to one descriptor, so a `fork` that copies the descriptor keeps the
/// lock alive until that copy is closed or `execve`d away. Production has a single runtime root
/// and forks only from under that same lock, so a copy can only ever extend the lifetime of the
/// lock the application already owns, and a mutation that loses the race is told the runtime is
/// busy — which is true.
///
/// The test binary is the opposite: dozens of unrelated harnesses, each with its own temporary
/// runtime root and its own lock, live in one process, and every child any of them spawns copies
/// the whole descriptor table. One harness's spawn therefore strands *another* harness's lock past
/// the drop that should have released it, and that harness's next launch reports `RuntimeNotReady`
/// instead of its real outcome.
///
/// Unlocking the open file description explicitly releases it whatever copies exist. This changes
/// nothing about which lock is taken, when, or how contention is reported — only that release does
/// not wait for the last copy of a descriptor to disappear — and it is compiled only into tests.
///
/// Production's release-by-close cannot be asserted from inside this binary for the same reason:
/// no test here owns "the last descriptor", because any parallel test may copy it at any moment.
#[cfg(test)]
impl Drop for RuntimeMutationLock {
    fn drop(&mut self) {
        let _ = fs4::FileExt::unlock(&self.file);
    }
}

#[derive(Debug)]
pub struct ApplicationInstanceLock {
    file: File,
    path: PathBuf,
}

impl ApplicationInstanceLock {
    pub fn acquire(path: &Path) -> Result<Self, RuntimeError> {
        let file = open_lock_file(path)?;
        match fs4::FileExt::try_lock(&file) {
            Ok(()) => Ok(Self {
                file,
                path: path.to_path_buf(),
            }),
            Err(fs4::TryLockError::WouldBlock) => Err(RuntimeError::Lock(format!(
                "another RetroFrontier instance owns {}",
                path.display()
            ))),
            Err(error) => Err(RuntimeError::Lock(format!(
                "could not lock {}: {error}",
                path.display()
            ))),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn open_lock_file(path: &Path) -> Result<File, RuntimeError> {
    let parent = path.parent().ok_or_else(|| {
        RuntimeError::Lock(format!("lock path has no parent: {}", path.display()))
    })?;
    std::fs::create_dir_all(parent)?;
    let metadata = std::fs::symlink_metadata(parent)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RuntimeError::Lock(format!(
            "lock parent is not a private directory: {}",
            parent.display()
        )));
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(RuntimeError::Lock(format!(
                "lock path is not a regular file: {}",
                path.display()
            )))
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(RuntimeError::Io(error)),
    }
    #[cfg(unix)]
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(path)?;
    #[cfg(unix)]
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

pub fn operation_identifier(prefix: &str, counter: u64) -> Result<SafeIdentifier, RuntimeError> {
    SafeIdentifier::new(format!("{prefix}-{}-{counter}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::RuntimeMutationLock;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn lock_is_exclusive_and_releases_on_drop() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("runtime.lock");
        let first = RuntimeMutationLock::acquire(&path).expect("first lock should work");
        assert!(RuntimeMutationLock::acquire(&path).is_err());
        drop(first);
        assert!(RuntimeMutationLock::acquire(&path).is_ok());
    }

    /// A parallel test that spawns a child copies the whole process descriptor table into that
    /// child, so the child transiently owns a duplicate of *this* lock's open file description.
    /// `flock` belongs to the open file description, not to a single descriptor, so closing the
    /// owner's descriptor cannot release the lock while such a duplicate exists. `try_clone`
    /// reproduces that duplicate deterministically, without depending on fork/exec timing.
    #[test]
    fn releasing_the_lock_does_not_depend_on_being_the_last_descriptor() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("runtime.lock");
        let owner = RuntimeMutationLock::acquire(&path).expect("first lock should work");
        let inherited = owner.duplicate_descriptor();
        drop(owner);
        RuntimeMutationLock::acquire(&path)
            .expect("the lock should be free once its owner has released it");
        drop(inherited);
    }

    #[cfg(unix)]
    #[test]
    fn lock_refuses_symlink_or_directory_targets() {
        let directory = tempdir().expect("temporary directory should be created");
        let symlink = directory.path().join("runtime-link");
        let target = directory.path().join("target");
        fs::write(&target, b"do not replace").unwrap();
        std::os::unix::fs::symlink(&target, &symlink).unwrap();
        assert!(RuntimeMutationLock::acquire(&symlink).is_err());

        let lock_directory = directory.path().join("runtime-dir");
        fs::create_dir(&lock_directory).unwrap();
        assert!(RuntimeMutationLock::acquire(&lock_directory).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"do not replace");
    }
}
