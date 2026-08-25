use crate::domain::runtime::{RuntimeError, SafeIdentifier};
use std::fs;
use std::path::{Path, PathBuf};

/// Files owned by the managed runtime lifecycle.
///
/// This type deliberately exposes no ROM, BIOS, save, state, metadata, or database paths. The
/// runtime manager can therefore only create and clean up paths below this structure.
#[derive(Debug, Clone)]
pub struct RuntimePaths {
    app_data_root: PathBuf,
    runtime_root: PathBuf,
    versions_root: PathBuf,
    staging_root: PathBuf,
    locks_root: PathBuf,
    active_pointer: PathBuf,
    game_process_record: PathBuf,
    trust_root: PathBuf,
    trust_datastore: PathBuf,
    trust_state: PathBuf,
}

impl RuntimePaths {
    pub fn new(app_data_root: impl Into<PathBuf>) -> Self {
        let app_data_root = app_data_root.into();
        let runtime_root = app_data_root.join("runtime");
        let trust_root = app_data_root.join("runtime-trust");
        Self {
            app_data_root,
            versions_root: runtime_root.join("versions"),
            staging_root: runtime_root.join("staging"),
            locks_root: runtime_root.join("locks"),
            active_pointer: runtime_root.join("active.json"),
            game_process_record: runtime_root.join("game-process.json"),
            runtime_root,
            trust_datastore: trust_root.join("tuf"),
            trust_state: trust_root.join("trust-state.json"),
            trust_root,
        }
    }

    pub fn app_data_root(&self) -> &Path {
        &self.app_data_root
    }

    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }

    pub fn versions_root(&self) -> &Path {
        &self.versions_root
    }

    pub fn staging_root(&self) -> &Path {
        &self.staging_root
    }

    pub fn locks_root(&self) -> &Path {
        &self.locks_root
    }

    pub fn active_pointer(&self) -> &Path {
        &self.active_pointer
    }

    pub fn game_process_record(&self) -> &Path {
        &self.game_process_record
    }

    pub fn trust_root(&self) -> &Path {
        &self.trust_root
    }

    pub fn trust_datastore(&self) -> &Path {
        &self.trust_datastore
    }

    pub fn trust_state(&self) -> &Path {
        &self.trust_state
    }

    pub fn mutation_lock(&self) -> PathBuf {
        self.locks_root.join("runtime-mutation.lock")
    }

    pub fn application_lock(&self) -> PathBuf {
        self.locks_root.join("application.lock")
    }

    pub fn version_path(&self, installation_id: &SafeIdentifier) -> PathBuf {
        self.versions_root.join(installation_id.as_str())
    }

    pub fn staging_path(&self, operation_id: &SafeIdentifier) -> PathBuf {
        self.staging_root.join(operation_id.as_str())
    }

    /// Create and validate only directories owned by RuntimeManager.
    pub fn prepare(&self) -> Result<(), RuntimeError> {
        ensure_directory(&self.app_data_root)?;
        ensure_directory(&self.runtime_root)?;
        ensure_directory(&self.versions_root)?;
        ensure_directory(&self.staging_root)?;
        ensure_directory(&self.locks_root)?;
        ensure_directory(&self.trust_root)?;
        ensure_directory(&self.trust_datastore)?;
        Ok(())
    }

    pub fn is_owned_version_path(&self, path: &Path) -> bool {
        path.parent() == Some(self.versions_root.as_path())
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| SafeIdentifier::new(name).is_ok())
    }

    pub fn is_owned_staging_path(&self, path: &Path) -> bool {
        path.parent() == Some(self.staging_root.as_path())
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| SafeIdentifier::new(name).is_ok())
    }

    pub fn refuse_symlink(&self, path: &Path) -> Result<(), RuntimeError> {
        if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(RuntimeError::Storage(format!(
                "runtime-owned path is a symlink: {}",
                path.display()
            )));
        }
        Ok(())
    }
}

fn ensure_directory(path: &Path) -> Result<(), RuntimeError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(RuntimeError::Storage(format!(
            "runtime directory cannot be a symlink: {}",
            path.display()
        ))),
        Ok(metadata) if !metadata.is_dir() => Err(RuntimeError::Storage(format!(
            "runtime path is not a directory: {}",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|create_error| {
                RuntimeError::Storage(format!("{}: {create_error}", path.display()))
            })
        }
        Err(error) => Err(RuntimeError::Storage(format!(
            "{}: {error}",
            path.display()
        ))),
    }
}

pub fn ensure_empty_directory(path: &Path) -> Result<(), RuntimeError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(RuntimeError::Storage(format!(
            "extraction destination cannot be a symlink: {}",
            path.display()
        ))),
        Ok(metadata) if !metadata.is_dir() => Err(RuntimeError::Storage(format!(
            "extraction destination is not a directory: {}",
            path.display()
        ))),
        Ok(_) => {
            if fs::read_dir(path)
                .map_err(|error| RuntimeError::Storage(error.to_string()))?
                .next()
                .is_some()
            {
                return Err(RuntimeError::Storage(format!(
                    "extraction destination is not empty: {}",
                    path.display()
                )));
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(RuntimeError::Io)
        }
        Err(error) => Err(RuntimeError::Io(error)),
    }
}

pub fn fsync_directory(path: &Path) -> Result<(), RuntimeError> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}
