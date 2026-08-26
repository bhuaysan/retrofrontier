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
