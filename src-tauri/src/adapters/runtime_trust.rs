use crate::adapters::runtime_paths::{fsync_directory, RuntimePaths};
use crate::adapters::runtime_source::TrustedRelease;
use crate::domain::runtime::{
    parse_strict_json, RuntimeError, RuntimePolicy, RuntimeTrustState, SafeIdentifier,
    Sha256Digest, MAX_TRUST_STATE_BYTES,
};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

static TRUST_WRITE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct RuntimeTrustStore {
    path: PathBuf,
}

impl RuntimeTrustStore {
    pub fn new(paths: &RuntimePaths) -> Self {
        Self {
            path: paths.trust_state().to_path_buf(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<RuntimeTrustState, RuntimeError> {
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(
                RuntimeError::Trust("runtime trust state is not a regular file".to_owned()),
            ),
            Ok(metadata) if metadata.len() > MAX_TRUST_STATE_BYTES => Err(RuntimeError::Trust(
                "runtime trust state is too large".to_owned(),
            )),
            Ok(_) => {
                let bytes = fs::read(&self.path)?;
                let state: RuntimeTrustState = parse_strict_json(&bytes)
                    .map_err(|error| RuntimeError::Trust(error.to_owned()))?;
                state.validate()?;
                Ok(state)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(RuntimeTrustState::new())
            }
            Err(error) => Err(RuntimeError::Io(error)),
        }
    }

    pub fn record_release(&self, release: &TrustedRelease) -> Result<(), RuntimeError> {
        release.validate()?;
        let mut state = self.load()?;
        apply_policy(&mut state, &release.policy)?;
        if release.metadata_versions.timestamp < state.metadata_versions.timestamp
            || release.metadata_versions.snapshot < state.metadata_versions.snapshot
            || release.metadata_versions.targets < state.metadata_versions.targets
        {
            return Err(RuntimeError::Trust(
                "TUF metadata version regressed below persisted state".to_owned(),
            ));
        }
        if release.manifest.release.release_sequence < state.minimum_safe_release_sequence {
            return Err(RuntimeError::Trust(
                "release sequence is below the persisted anti-rollback floor".to_owned(),
            ));
        }
        if state
            .revoked_release_ids
            .iter()
            .any(|id| id == &release.manifest.release.release_id)
        {
            return Err(RuntimeError::Trust("release is revoked".to_owned()));
        }
        let record = crate::domain::runtime::TrustedReleaseRecord {
            release_id: release.manifest.release.release_id.clone(),
            release_sequence: release.manifest.release.release_sequence,
            manifest_sha256: release.manifest_sha256,
        };
        if !state.trusted_releases.iter().any(|existing| {
            existing.release_id == record.release_id
                && existing.release_sequence == record.release_sequence
                && existing.manifest_sha256 == record.manifest_sha256
        }) {
            state.trusted_releases.push(record);
        }
        state.metadata_versions.timestamp = state
            .metadata_versions
            .timestamp
            .max(release.metadata_versions.timestamp);
        state.metadata_versions.snapshot = state
            .metadata_versions
            .snapshot
            .max(release.metadata_versions.snapshot);
        state.metadata_versions.targets = state
            .metadata_versions
            .targets
            .max(release.metadata_versions.targets);
        state.validate()?;
        let bytes = serde_json::to_vec(&state).map_err(|error| {
            RuntimeError::Trust(format!("trust state serialization failed: {error}"))
        })?;
        atomic_replace(&self.path, &bytes, MAX_TRUST_STATE_BYTES, |bytes| {
            let parsed: RuntimeTrustState =
                parse_strict_json(bytes).map_err(|error| RuntimeError::Trust(error.to_owned()))?;
            parsed.validate()
        })
    }

    pub fn is_trusted(
        &self,
        release_id: &SafeIdentifier,
        release_sequence: u64,
        manifest_sha256: Sha256Digest,
    ) -> Result<bool, RuntimeError> {
        let state = self.load()?;
        Ok(state.permits(release_id, release_sequence, manifest_sha256))
    }
}

fn apply_policy(state: &mut RuntimeTrustState, policy: &RuntimePolicy) -> Result<(), RuntimeError> {
    policy.validate()?;
    state.minimum_safe_release_sequence = state
        .minimum_safe_release_sequence
        .max(policy.minimum_safe_release_sequence);
    for release_id in &policy.revoked_release_ids {
        if !state
            .revoked_release_ids
            .iter()
            .any(|existing| existing == release_id)
        {
            state.revoked_release_ids.push(release_id.clone());
        }
    }
    Ok(())
}

pub fn atomic_replace(
    path: &Path,
    bytes: &[u8],
    max_size: u64,
    validate: impl Fn(&[u8]) -> Result<(), RuntimeError>,
) -> Result<(), RuntimeError> {
    if bytes.len() as u64 > max_size {
        return Err(RuntimeError::Storage(
            "atomic JSON value is too large".to_owned(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| RuntimeError::Storage("atomic file has no parent".to_owned()))?;
    fs::create_dir_all(parent)?;
    ensure_real_directory(parent)?;
    let counter = TRUST_WRITE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| RuntimeError::Storage("atomic file name is not UTF-8".to_owned()))?;
    let temporary = parent.join(format!(
        ".{name}.tmp-{}-{counter}-{stamp}",
        std::process::id()
    ));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);

        let mut reopened = Vec::new();
        File::open(&temporary)?.read_to_end(&mut reopened)?;
        if reopened != bytes {
            return Err(RuntimeError::Storage(
                "atomic file changed before rename".to_owned(),
            ));
        }
        validate(&reopened)?;
        fs::rename(&temporary, path)?;
        fsync_directory(parent)?;

        let mut final_bytes = Vec::new();
        File::open(path)?.read_to_end(&mut final_bytes)?;
        if final_bytes != bytes {
            return Err(RuntimeError::Storage(
                "atomic file did not survive rename".to_owned(),
            ));
        }
        validate(&final_bytes)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn ensure_real_directory(path: &Path) -> Result<(), RuntimeError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RuntimeError::Storage(format!(
            "runtime state parent is not a real directory: {}",
            path.display()
        )));
    }
    Ok(())
}
