use crate::adapters::runtime_archive::{LinuxRuntimeArchiveExtractor, RuntimeArchiveExtractor};
use crate::adapters::runtime_installed::{
    apply_inventory_permissions, directory_size, validate_app_run, verify_installation,
    verify_tree, write_complete_marker, write_release_manifest, VerifiedInstallation,
};
use crate::adapters::runtime_lock::{operation_identifier, RuntimeMutationLock};
use crate::adapters::runtime_paths::{ensure_empty_directory, fsync_directory, RuntimePaths};
use crate::adapters::runtime_pointer::{
    read_active_pointer, remove_pointer_temporary_files, write_active_pointer,
};
use crate::adapters::runtime_process::{LinuxManagedProcessInspector, ManagedProcessInspector};
use crate::adapters::runtime_source::{TrustedRelease, TrustedReleaseSource};
use crate::adapters::runtime_trust::RuntimeTrustStore;
use crate::domain::runtime::{
    ActivePointer, RuntimeError, RuntimeManifest, RuntimeState, RuntimeStatus, SafeIdentifier,
    Sha256Digest,
};
use async_trait::async_trait;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_MAX_RETAINED_INSTALLATIONS: usize = 2;
pub const DEFAULT_MAX_RUNTIME_STORAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct RetentionPolicy {
    pub max_retained_installations: usize,
    pub max_storage_bytes: u64,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_retained_installations: DEFAULT_MAX_RETAINED_INSTALLATIONS,
            max_storage_bytes: DEFAULT_MAX_RUNTIME_STORAGE_BYTES,
        }
    }
}

impl RetentionPolicy {
    fn validate(self) -> Result<(), RuntimeError> {
        if self.max_retained_installations == 0 || self.max_storage_bytes == 0 {
            return Err(RuntimeError::Storage(
                "runtime retention policy must retain at least one installation".to_owned(),
            ));
        }
        Ok(())
    }
}

pub trait RuntimeSmokeValidator: Send + Sync {
    fn validate(
        &self,
        installation_path: &Path,
        manifest: &RuntimeManifest,
    ) -> Result<(), RuntimeError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StructuralSmokeValidator;

impl RuntimeSmokeValidator for StructuralSmokeValidator {
    fn validate(
        &self,
        installation_path: &Path,
        manifest: &RuntimeManifest,
    ) -> Result<(), RuntimeError> {
        // M2 does not execute downloaded code. This verifies the authenticated AppRun contract
        // without granting an untrusted candidate process execution before activation.
        validate_app_run(installation_path, manifest)
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct UnavailableTrustedReleaseSource;

#[async_trait]
impl TrustedReleaseSource for UnavailableTrustedReleaseSource {
    async fn resolve_release(
        &self,
        _manifest_target_name: &str,
    ) -> Result<TrustedRelease, RuntimeError> {
        Err(RuntimeError::Trust(
            "no approved managed runtime source is configured".to_owned(),
        ))
    }

    async fn download_target(
        &self,
        _target: &crate::adapters::runtime_source::TrustedTarget,
        _destination: &Path,
        _max_size: u64,
    ) -> Result<(), RuntimeError> {
        Err(RuntimeError::Trust(
            "no approved managed runtime source is configured".to_owned(),
        ))
    }
}

#[derive(Clone)]
pub struct RuntimeManager {
    paths: RuntimePaths,
    source: Arc<dyn TrustedReleaseSource>,
    extractor: Arc<dyn RuntimeArchiveExtractor>,
    process_inspector: Arc<dyn ManagedProcessInspector>,
    smoke_validator: Arc<dyn RuntimeSmokeValidator>,
    trust_store: RuntimeTrustStore,
    retention: RetentionPolicy,
    operation_counter: Arc<AtomicU64>,
}

impl RuntimeManager {
    pub fn for_app(paths: RuntimePaths) -> Result<Self, RuntimeError> {
        Self::new(
            paths,
            Arc::new(UnavailableTrustedReleaseSource),
            Arc::new(LinuxRuntimeArchiveExtractor),
            Arc::new(LinuxManagedProcessInspector),
            Arc::new(StructuralSmokeValidator),
            RetentionPolicy::default(),
        )
    }

    pub fn new(
        paths: RuntimePaths,
        source: Arc<dyn TrustedReleaseSource>,
        extractor: Arc<dyn RuntimeArchiveExtractor>,
        process_inspector: Arc<dyn ManagedProcessInspector>,
        smoke_validator: Arc<dyn RuntimeSmokeValidator>,
        retention: RetentionPolicy,
    ) -> Result<Self, RuntimeError> {
        retention.validate()?;
        paths.prepare()?;
        Ok(Self {
            trust_store: RuntimeTrustStore::new(&paths),
            paths,
            source,
            extractor,
            process_inspector,
            smoke_validator,
            retention,
            operation_counter: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn paths(&self) -> &RuntimePaths {
        &self.paths
    }

    pub fn retention(&self) -> RetentionPolicy {
        self.retention
    }

    pub fn status(&self) -> Result<RuntimeStatus, RuntimeError> {
        self.paths.prepare()?;
        self.reconcile_status()
    }

    /// Reconcile startup-owned leftovers while holding the same kernel lock used by mutations.
    pub fn startup_reconcile(&self) -> Result<RuntimeStatus, RuntimeError> {
        let _lock = self.acquire_mutation_lock()?;
        // A process record can outlive the UI process. Refuse cleanup until the inspector has
        // established that no managed RetroArch process is still using a runtime tree.
        self.process_inspector.ensure_no_active_game(&self.paths)?;
        remove_pointer_temporary_files(&self.paths)?;
        self.cleanup_staging_locked()?;
        self.cleanup_incomplete_versions_locked()?;
        self.cleanup_retained_versions_locked()?;
        self.reconcile_status()
    }

    pub async fn install(&self, manifest_target_name: &str) -> Result<RuntimeStatus, RuntimeError> {
        self.install_or_repair(manifest_target_name, false).await
    }

    pub async fn repair(&self, manifest_target_name: &str) -> Result<RuntimeStatus, RuntimeError> {
        self.install_or_repair(manifest_target_name, true).await
    }

    async fn install_or_repair(
        &self,
        manifest_target_name: &str,
        repair: bool,
    ) -> Result<RuntimeStatus, RuntimeError> {
        self.paths.prepare()?;
        let release = self.source.resolve_release(manifest_target_name).await?;
        release.validate()?;
        let _lock = self.acquire_mutation_lock()?;
        self.process_inspector.ensure_no_active_game(&self.paths)?;
        self.trust_store.record_release(&release)?;
        let candidate = self
            .construct_candidate(&release, if repair { "repair" } else { "install" })
            .await?;
        self.activate_locked(candidate, repair)?;
        self.reconcile_status()
    }

    /// Activate an already complete and trusted installation. This is useful to the future launch
    /// service and keeps activation independent from download/extraction code.
    pub fn activate_candidate(
        &self,
        installation_id: &SafeIdentifier,
        expected_manifest_sha256: Sha256Digest,
    ) -> Result<RuntimeStatus, RuntimeError> {
        let _lock = self.acquire_mutation_lock()?;
        self.process_inspector.ensure_no_active_game(&self.paths)?;
        let candidate = self.load_verified_installation(installation_id)?;
        if candidate.manifest_sha256 != expected_manifest_sha256 {
            return Err(RuntimeError::Trust(
                "candidate manifest digest does not match the requested digest".to_owned(),
            ));
        }
        self.activate_locked(candidate, false)?;
        self.reconcile_status()
    }

    pub fn rollback(&self) -> Result<RuntimeStatus, RuntimeError> {
        let _lock = self.acquire_mutation_lock()?;
        self.process_inspector.ensure_no_active_game(&self.paths)?;
        let pointer = read_active_pointer(&self.paths)?.ok_or(RuntimeError::NoRollback)?;
        let current = self
            .load_verified_installation(&pointer.installation_id)
            .map_err(|_| {
                RuntimeError::Pointer("current active installation is not verified".to_owned())
            })?;
        if current.manifest_sha256 != pointer.manifest_sha256 {
            return Err(RuntimeError::Pointer(
                "active pointer does not match the current installation".to_owned(),
            ));
        }
        let mut candidates = self.list_verified_installations()?;
        candidates.retain(|candidate| candidate.installation_id != current.installation_id);
        candidates.sort_by(|left, right| {
            right
                .manifest
                .release
                .release_sequence
                .cmp(&left.manifest.release.release_sequence)
                .then_with(|| left.installation_id.cmp(&right.installation_id))
        });
        let fallback = candidates
            .into_iter()
            .next()
            .ok_or(RuntimeError::NoRollback)?;
        self.activate_locked(fallback, false)?;
        self.reconcile_status()
    }

    pub fn cleanup(&self) -> Result<RuntimeStatus, RuntimeError> {
        let _lock = self.acquire_mutation_lock()?;
        self.process_inspector.ensure_no_active_game(&self.paths)?;
        self.cleanup_staging_locked()?;
        self.cleanup_incomplete_versions_locked()?;
        self.cleanup_retained_versions_locked()?;
        self.reconcile_status()
    }

    fn acquire_mutation_lock(&self) -> Result<RuntimeMutationLock, RuntimeError> {
        RuntimeMutationLock::acquire(&self.paths.mutation_lock())
    }

    async fn construct_candidate(
        &self,
        release: &TrustedRelease,
        prefix: &str,
    ) -> Result<VerifiedInstallation, RuntimeError> {
        let operation_id = operation_identifier(
            prefix,
            self.operation_counter.fetch_add(1, Ordering::Relaxed),
        )?;
        let operation_path = self.paths.staging_path(&operation_id);
        if operation_path.exists() {
            return Err(RuntimeError::Storage(
                "runtime staging operation identifier already exists".to_owned(),
            ));
        }
        fs::create_dir(&operation_path)?;
        let downloads = operation_path.join("downloads");
        let tree = operation_path.join("tree");
        fs::create_dir(&downloads)?;
        fs::create_dir(&tree)?;

        let result = async {
            for component in &release.manifest.release.components {
                let target = release.target(&component.target_name)?;
                let artifact = downloads.join(format!("{}.artifact", component.id));
                self.source
                    .download_target(target, &artifact, component.archive_size_bytes)
                    .await?;
                crate::adapters::runtime_integrity::verify_file(
                    &artifact,
                    component.archive_size_bytes,
                    component.sha256,
                )?;

                let destination = tree.join(component.install_path.to_path_buf());
                if !destination.starts_with(&tree) {
                    return Err(RuntimeError::Extraction(
                        "component installation path escaped the candidate tree".to_owned(),
                    ));
                }
                create_real_directory(&tree, &component.install_path.to_path_buf())?;
                ensure_empty_directory(&destination)?;
                self.extractor.extract(
                    component,
                    &artifact,
                    &destination,
                    &release.manifest.release.inventory,
                    &release.manifest.release.extraction,
                )?;
            }

            apply_inventory_permissions(&tree, &release.manifest)?;
            write_release_manifest(&tree, &release.manifest_bytes)?;
            verify_tree(&tree, &release.manifest)?;
            self.smoke_validator.validate(&tree, &release.manifest)?;

            let installation_id = self.new_installation_id()?;
            let installation_path = self.paths.version_path(&installation_id);
            if installation_path.exists() {
                return Err(RuntimeError::Storage(
                    "runtime installation identifier already exists".to_owned(),
                ));
            }
            fs::rename(&tree, &installation_path)?;
            fsync_directory(self.paths.versions_root())?;
            write_complete_marker(
                &installation_path,
                &installation_id,
                release.manifest_sha256,
            )?;
            let candidate = verify_installation(
                &self.paths,
                &installation_id,
                &release.manifest,
                release.manifest_sha256,
            )?;
            fs::remove_dir_all(&operation_path)?;
            fsync_directory(self.paths.staging_root())?;
            Ok(candidate)
        }
        .await;
        if result.is_err() {
            // The operation directory contains only staging files and is reconciled on next
            // startup. Leaving it intact makes an interrupted download observable and recoverable.
        }
        result
    }

    fn new_installation_id(&self) -> Result<SafeIdentifier, RuntimeError> {
        let counter = self.operation_counter.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        SafeIdentifier::new(format!(
            "i-{timestamp:x}-{counter:x}-{}",
            std::process::id()
        ))
    }

    fn activate_locked(
        &self,
        candidate: VerifiedInstallation,
        allow_invalid_current: bool,
    ) -> Result<(), RuntimeError> {
        // TODO(Sol Max review): re-audit activation ordering and durability on all supported Linux
        // filesystems before treating this as a production update guarantee.
        self.process_inspector.ensure_no_active_game(&self.paths)?;
        let trust_state = self.trust_store.load()?;
        if !trust_state.permits(
            &candidate.manifest.release.release_id,
            candidate.manifest.release.release_sequence,
            candidate.manifest_sha256,
        ) {
            return Err(RuntimeError::Trust(
                "candidate is not present in the persisted trusted-release set".to_owned(),
            ));
        }
        let pointer = match read_active_pointer(&self.paths) {
            Ok(pointer) => pointer,
            Err(_error) if allow_invalid_current => None,
            Err(error) => return Err(error),
        };
        let mut current_storage = 0_u64;
        match pointer.as_ref() {
            Some(pointer) => match self.load_verified_installation(&pointer.installation_id) {
                Ok(current) if current.manifest_sha256 == pointer.manifest_sha256 => {
                    if current.installation_id != candidate.installation_id {
                        current_storage = current.storage_bytes;
                    }
                }
                Ok(_) | Err(_) if !allow_invalid_current => {
                    return Err(RuntimeError::Pointer(
                        "current active pointer does not resolve to a verified installation"
                            .to_owned(),
                    ))
                }
                Ok(_) | Err(_) => {
                    current_storage =
                        directory_size(&self.paths.version_path(&pointer.installation_id))
                            .unwrap_or(0);
                }
            },
            None => {
                let has_complete =
                    self.has_other_complete_installation(&candidate.installation_id)?;
                if has_complete && !allow_invalid_current {
                    return Err(RuntimeError::Pointer(
                        "active.json is missing while retained installations exist".to_owned(),
                    ));
                }
            }
        }
        if candidate.storage_bytes.saturating_add(current_storage)
            > self.retention.max_storage_bytes
        {
            return Err(RuntimeError::StorageLimit);
        }
        let new_pointer = ActivePointer {
            schema_version: crate::domain::runtime::ACTIVE_POINTER_SCHEMA_VERSION,
            installation_id: candidate.installation_id.clone(),
            manifest_sha256: candidate.manifest_sha256,
        };
        write_active_pointer(&self.paths, &new_pointer)?;
        self.cleanup_retained_versions_locked()?;
        Ok(())
    }

    fn reconcile_status(&self) -> Result<RuntimeStatus, RuntimeError> {
        let trust_state = match self.trust_store.load() {
            Ok(state) => state,
            Err(_) => return Ok(RuntimeStatus::broken()),
        };
        let pointer = match read_active_pointer(&self.paths) {
            Ok(pointer) => pointer,
            Err(_) => return Ok(RuntimeStatus::broken()),
        };
        let Some(pointer) = pointer else {
            return if self.has_any_complete_installation()? {
                Ok(RuntimeStatus::broken())
            } else {
                Ok(RuntimeStatus::not_installed())
            };
        };
        let current =
            self.load_verified_installation_with_state(&pointer.installation_id, &trust_state);
        let Ok(current) = current else {
            return Ok(RuntimeStatus::broken());
        };
        if current.manifest_sha256 != pointer.manifest_sha256 {
            return Ok(RuntimeStatus::broken());
        }
        let can_rollback = self
            .list_verified_installations_with_state(&trust_state)?
            .into_iter()
            .any(|candidate| candidate.installation_id != current.installation_id);
        Ok(RuntimeStatus {
            state: if can_rollback {
                RuntimeState::RollbackAvailable
            } else {
                RuntimeState::Ready
            },
            installation_id: Some(current.installation_id.to_string()),
            release_id: Some(current.manifest.release.release_id.to_string()),
            can_rollback,
            repair_required: false,
        })
    }

    fn load_verified_installation(
        &self,
        installation_id: &SafeIdentifier,
    ) -> Result<VerifiedInstallation, RuntimeError> {
        let trust_state = self.trust_store.load()?;
        self.load_verified_installation_with_state(installation_id, &trust_state)
    }

    fn load_verified_installation_with_state(
        &self,
        installation_id: &SafeIdentifier,
        trust_state: &crate::domain::runtime::RuntimeTrustState,
    ) -> Result<VerifiedInstallation, RuntimeError> {
        let path = self.paths.version_path(installation_id);
        let (manifest, manifest_sha256) = crate::adapters::runtime_installed::read_manifest(&path)?;
        if !trust_state.permits(
            &manifest.release.release_id,
            manifest.release.release_sequence,
            manifest_sha256,
        ) {
            return Err(RuntimeError::Trust(
                "installed manifest is not trusted by persisted runtime state".to_owned(),
            ));
        }
        verify_installation(&self.paths, installation_id, &manifest, manifest_sha256)
    }

    fn list_verified_installations(&self) -> Result<Vec<VerifiedInstallation>, RuntimeError> {
        let trust_state = self.trust_store.load()?;
        self.list_verified_installations_with_state(&trust_state)
    }

    fn list_verified_installations_with_state(
        &self,
        trust_state: &crate::domain::runtime::RuntimeTrustState,
    ) -> Result<Vec<VerifiedInstallation>, RuntimeError> {
        let mut installations = Vec::new();
        for entry in fs::read_dir(self.paths.versions_root())? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Ok(installation_id) = SafeIdentifier::new(name) else {
                continue;
            };
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                continue;
            }
            let Ok((manifest, manifest_sha256)) =
                crate::adapters::runtime_installed::read_manifest(&entry.path())
            else {
                continue;
            };
            if !trust_state.permits(
                &manifest.release.release_id,
                manifest.release.release_sequence,
                manifest_sha256,
            ) {
                continue;
            }
            if let Ok(installation) =
                verify_installation(&self.paths, &installation_id, &manifest, manifest_sha256)
            {
                installations.push(installation);
            }
        }
        Ok(installations)
    }

    fn has_any_complete_installation(&self) -> Result<bool, RuntimeError> {
        for entry in fs::read_dir(self.paths.versions_root())? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if !metadata.file_type().is_symlink()
                && metadata.is_dir()
                && fs::symlink_metadata(
                    entry
                        .path()
                        .join(crate::adapters::runtime_installed::COMPLETE_MARKER_FILE),
                )
                .is_ok()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn has_other_complete_installation(
        &self,
        candidate_id: &SafeIdentifier,
    ) -> Result<bool, RuntimeError> {
        for entry in fs::read_dir(self.paths.versions_root())? {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name == candidate_id.as_str() || SafeIdentifier::new(name).is_err() {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path())?;
            if !metadata.file_type().is_symlink()
                && metadata.is_dir()
                && fs::symlink_metadata(
                    entry
                        .path()
                        .join(crate::adapters::runtime_installed::COMPLETE_MARKER_FILE),
                )
                .is_ok()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn cleanup_staging_locked(&self) -> Result<(), RuntimeError> {
        for entry in fs::read_dir(self.paths.staging_root())? {
            let entry = entry?;
            let path = entry.path();
            if !self.paths.is_owned_staging_path(&path) {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                fs::remove_dir_all(&path)?;
            }
        }
        fsync_directory(self.paths.staging_root())?;
        Ok(())
    }

    fn cleanup_incomplete_versions_locked(&self) -> Result<(), RuntimeError> {
        // Never remove a parseable pointer target during startup recovery, even if its completion
        // marker is missing. A broken active target must remain available for explicit repair.
        let active_id = read_active_pointer(&self.paths)
            .ok()
            .flatten()
            .map(|pointer| pointer.installation_id);
        for entry in fs::read_dir(self.paths.versions_root())? {
            let entry = entry?;
            let path = entry.path();
            if !self.paths.is_owned_version_path(&path) {
                continue;
            }
            let Some(id) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if active_id
                .as_ref()
                .is_some_and(|active| active.as_str() == id)
            {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                continue;
            }
            if fs::symlink_metadata(
                path.join(crate::adapters::runtime_installed::COMPLETE_MARKER_FILE),
            )
            .is_err()
            {
                fs::remove_dir_all(&path)?;
            }
        }
        fsync_directory(self.paths.versions_root())?;
        Ok(())
    }

    fn cleanup_retained_versions_locked(&self) -> Result<(), RuntimeError> {
        let pointer = match read_active_pointer(&self.paths) {
            Ok(pointer) => pointer,
            Err(_) => return Ok(()),
        };
        let Some(pointer) = pointer else {
            // Without an authoritative active pointer, complete installations are never guessed
            // or deleted. Incomplete staging is still safe to clean separately.
            return Ok(());
        };
        let active = match self.load_verified_installation(&pointer.installation_id) {
            Ok(active) if active.manifest_sha256 == pointer.manifest_sha256 => active,
            _ => return Ok(()),
        };
        let trust_state = self.trust_store.load()?;
        let mut verified = self.list_verified_installations_with_state(&trust_state)?;
        verified.retain(|installation| installation.installation_id != active.installation_id);
        verified.sort_by(|left, right| {
            right
                .manifest
                .release
                .release_sequence
                .cmp(&left.manifest.release.release_sequence)
                .then_with(|| left.installation_id.cmp(&right.installation_id))
        });
        let fallback_count = self.retention.max_retained_installations.saturating_sub(1);
        let preserved: std::collections::BTreeSet<_> = verified
            .iter()
            .take(fallback_count)
            .map(|installation| installation.installation_id.clone())
            .collect();
        for installation in verified {
            if !preserved.contains(&installation.installation_id) {
                self.remove_owned_installation(
                    &installation.installation_id,
                    &active.installation_id,
                    &preserved,
                )?;
            }
        }
        Ok(())
    }

    fn remove_owned_installation(
        &self,
        installation_id: &SafeIdentifier,
        active_id: &SafeIdentifier,
        rollback_ids: &std::collections::BTreeSet<SafeIdentifier>,
    ) -> Result<(), RuntimeError> {
        if installation_id == active_id || rollback_ids.contains(installation_id) {
            return Err(RuntimeError::Storage(
                "refusing to delete active or rollback runtime".to_owned(),
            ));
        }
        let path = self.paths.version_path(installation_id);
        if !self.paths.is_owned_version_path(&path) {
            return Err(RuntimeError::Storage(
                "refusing to delete a path outside runtime versions".to_owned(),
            ));
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(RuntimeError::Storage(
                "refusing to delete a non-directory runtime target".to_owned(),
            ));
        }
        fs::remove_dir_all(&path)?;
        fsync_directory(self.paths.versions_root())?;
        Ok(())
    }
}

fn create_real_directory(base: &Path, relative: &Path) -> Result<(), RuntimeError> {
    let mut current = base.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(RuntimeError::Storage(
                "candidate directory path is not relative and normal".to_owned(),
            ));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(RuntimeError::Storage(format!(
                    "candidate directory is not a real directory: {}",
                    current.display()
                )))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)?,
            Err(error) => return Err(RuntimeError::Io(error)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{RetentionPolicy, RuntimeManager, StructuralSmokeValidator};
    use crate::adapters::runtime_archive::RuntimeArchiveExtractor;
    use crate::adapters::runtime_integrity::{sha256_bytes, sha256_file};
    use crate::adapters::runtime_paths::RuntimePaths;
    use crate::adapters::runtime_pointer::read_active_pointer;
    use crate::adapters::runtime_process::StaticManagedProcessInspector;
    use crate::adapters::runtime_source::LocalTrustedReleaseSource;
    use crate::domain::runtime::{
        ArchiveFormat, ComponentKind, InstalledEntry, InstalledEntryType, RelativePath,
        ReleaseChannel, RuntimeArchitecture, RuntimeCompatibility, RuntimeComponent, RuntimeError,
        RuntimeManifest, RuntimePlatform, RuntimeRelease, SafeIdentifier, Sha256Digest,
    };
    use std::collections::BTreeMap;
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[derive(Debug, Clone, Copy)]
    struct SyntheticRuntimeArchiveExtractor;

    impl RuntimeArchiveExtractor for SyntheticRuntimeArchiveExtractor {
        fn extract(
            &self,
            _component: &RuntimeComponent,
            artifact: &std::path::Path,
            destination: &std::path::Path,
            _inventory: &[InstalledEntry],
            _limits: &crate::domain::runtime::ExtractionLimits,
        ) -> Result<(), RuntimeError> {
            let mut archive = tar::Archive::new(File::open(artifact)?);
            let mut found = false;
            for entry in archive.entries()? {
                let mut entry = entry?;
                let path = entry.path()?;
                if path.to_str() != Some("AppRun") {
                    return Err(RuntimeError::Extraction(
                        "synthetic runtime archive contains an unexpected path".to_owned(),
                    ));
                }
                let output_path = destination.join("AppRun");
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(output_path.clone())?;
                std::io::copy(&mut entry, &mut output)?;
                output.flush()?;
                output.sync_all()?;
                fs::set_permissions(output_path, fs::Permissions::from_mode(0o755))?;
                found = true;
            }
            if !found {
                return Err(RuntimeError::Extraction(
                    "synthetic runtime archive has no AppRun".to_owned(),
                ));
            }
            Ok(())
        }
    }

    fn archive_fixture(
        directory: &std::path::Path,
        sequence: u64,
    ) -> (std::path::PathBuf, Vec<u8>) {
        let app_run = format!("#!/bin/sh\nexit {sequence}\n").into_bytes();
        let archive_path = directory.join(format!("runtime-{sequence}.tar"));
        let file = File::create(&archive_path).unwrap();
        let mut builder = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_path("AppRun").unwrap();
        header.set_size(app_run.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, app_run.as_slice()).unwrap();
        builder.finish().unwrap();
        (archive_path, app_run)
    }

    fn fixture_manager(
        app_data: &std::path::Path,
        sequence: u64,
        process_inspector: StaticManagedProcessInspector,
    ) -> RuntimeManager {
        fs::create_dir_all(app_data).unwrap();
        let (archive, app_run) = archive_fixture(app_data, sequence);
        let (archive_size, archive_hash) = sha256_file(&archive).unwrap();
        let app_hash = sha256_bytes(&app_run);
        let manifest = RuntimeManifest {
            schema_version: 1,
            manifest_id: SafeIdentifier::new(format!("manifest-{sequence}")).unwrap(),
            channel: ReleaseChannel::Stable,
            min_retrofrontier_version: "0.1.0".to_owned(),
            release: RuntimeRelease {
                release_id: SafeIdentifier::new(format!("release-{sequence}")).unwrap(),
                release_sequence: sequence,
                retrofrontier_runtime_version: format!("{sequence}"),
                retroarch_version: format!("1.{sequence}"),
                platform: RuntimePlatform::Linux,
                architecture: RuntimeArchitecture::X86_64,
                components: vec![RuntimeComponent {
                    id: SafeIdentifier::new("retroarch").unwrap(),
                    kind: ComponentKind::Runtime,
                    target_name: format!("targets/runtime-{sequence}.tar"),
                    source_id: None,
                    source_url: None,
                    archive_format: ArchiveFormat::AppImage,
                    archive_size_bytes: archive_size,
                    sha256: archive_hash,
                    install_path: RelativePath::new("runtime/app").unwrap(),
                    expected_root: None,
                    payload_filename: None,
                    executable_relative_path: None,
                    display_version: None,
                    source_revision: None,
                    source_pinning: None,
                    license: "GPL-3.0-or-later".to_owned(),
                    systems: Vec::new(),
                }],
                app_run_path: RelativePath::new("runtime/app/AppRun").unwrap(),
                inventory: vec![
                    InstalledEntry {
                        path: RelativePath::new("runtime").unwrap(),
                        entry_type: InstalledEntryType::Directory,
                        size_bytes: 0,
                        sha256: None,
                        executable: false,
                        link_target: None,
                    },
                    InstalledEntry {
                        path: RelativePath::new("runtime/app").unwrap(),
                        entry_type: InstalledEntryType::Directory,
                        size_bytes: 0,
                        sha256: None,
                        executable: false,
                        link_target: None,
                    },
                    InstalledEntry {
                        path: RelativePath::new("runtime/app/AppRun").unwrap(),
                        entry_type: InstalledEntryType::File,
                        size_bytes: app_run.len() as u64,
                        sha256: Some(app_hash),
                        executable: true,
                        link_target: None,
                    },
                ],
                extraction: Default::default(),
            },
            compatibility: RuntimeCompatibility {
                retroarch_core_api: "1".to_owned(),
                save_state_policy: "isolated".to_owned(),
            },
        };
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let mut target_files = BTreeMap::new();
        target_files.insert(format!("targets/runtime-{sequence}.tar"), archive);
        let source = LocalTrustedReleaseSource::from_manifest_bytes(
            format!("manifests/release-{sequence}.json"),
            manifest_bytes,
            target_files,
        )
        .unwrap();
        RuntimeManager::new(
            RuntimePaths::new(app_data),
            std::sync::Arc::new(source),
            std::sync::Arc::new(SyntheticRuntimeArchiveExtractor),
            std::sync::Arc::new(process_inspector),
            std::sync::Arc::new(StructuralSmokeValidator),
            RetentionPolicy::default(),
        )
        .unwrap()
    }

    #[test]
    fn retention_defaults_are_bounded() {
        let policy = RetentionPolicy::default();
        assert_eq!(policy.max_retained_installations, 2);
        assert!(policy.max_storage_bytes > 0);
    }

    #[tokio::test]
    async fn installs_immutable_versions_and_rolls_back_between_them() {
        let directory = tempdir().unwrap();
        let inspector = StaticManagedProcessInspector::default();
        let first = fixture_manager(directory.path(), 1, inspector.clone());
        let first_status = first.install("manifests/release-1.json").await.unwrap();
        assert_eq!(
            first_status.state,
            crate::domain::runtime::RuntimeState::Ready
        );
        let first_pointer = read_active_pointer(first.paths()).unwrap().unwrap();

        let second = fixture_manager(directory.path(), 2, inspector);
        let second_status = second.install("manifests/release-2.json").await.unwrap();
        assert_eq!(
            second_status.state,
            crate::domain::runtime::RuntimeState::RollbackAvailable
        );
        assert_ne!(
            second_status.installation_id.as_deref(),
            Some(first_pointer.installation_id.as_str())
        );

        let rollback_status = second.rollback().unwrap();
        assert_eq!(
            rollback_status.state,
            crate::domain::runtime::RuntimeState::RollbackAvailable
        );
        let rollback_pointer = read_active_pointer(second.paths()).unwrap().unwrap();
        assert_eq!(
            rollback_pointer.installation_id,
            first_pointer.installation_id
        );
    }

    #[tokio::test]
    async fn repair_reconstructs_a_new_installation_without_patching_the_broken_one() {
        let directory = tempdir().unwrap();
        let inspector = StaticManagedProcessInspector::default();
        let initial = fixture_manager(directory.path(), 1, inspector.clone());
        initial.install("manifests/release-1.json").await.unwrap();
        let old_pointer = read_active_pointer(initial.paths()).unwrap().unwrap();
        let old_path = initial.paths().version_path(&old_pointer.installation_id);
        fs::write(old_path.join("runtime/app/AppRun"), b"corrupted!\n").unwrap();
        assert_eq!(
            initial.status().unwrap().state,
            crate::domain::runtime::RuntimeState::Broken
        );

        let repair = fixture_manager(directory.path(), 1, inspector);
        let status = repair.repair("manifests/release-1.json").await.unwrap();
        assert_eq!(status.state, crate::domain::runtime::RuntimeState::Ready);
        let new_pointer = read_active_pointer(repair.paths()).unwrap().unwrap();
        assert_ne!(new_pointer.installation_id, old_pointer.installation_id);
        assert!(old_path.exists());
    }

    #[tokio::test]
    async fn active_game_blocks_activation_and_startup_does_not_infer_a_runtime() {
        let directory = tempdir().unwrap();
        let inspector = StaticManagedProcessInspector::default();
        let manager = fixture_manager(directory.path(), 1, inspector.clone());
        assert_eq!(
            manager.status().unwrap().state,
            crate::domain::runtime::RuntimeState::NotInstalled
        );
        fs::create_dir(manager.paths().staging_root().join("stale-1")).unwrap();
        assert_eq!(
            manager.startup_reconcile().unwrap().state,
            crate::domain::runtime::RuntimeState::NotInstalled
        );
        assert!(!manager.paths().staging_root().join("stale-1").exists());

        inspector.set_active(true);
        assert!(matches!(
            manager.install("manifests/release-1.json").await,
            Err(crate::domain::runtime::RuntimeError::GameActive)
        ));
    }

    #[tokio::test]
    async fn reconciliation_reports_corrupt_pointer_and_missing_target_as_broken() {
        let directory = tempdir().unwrap();
        let manager = fixture_manager(
            directory.path(),
            1,
            StaticManagedProcessInspector::default(),
        );
        manager.install("manifests/release-1.json").await.unwrap();
        fs::write(manager.paths().active_pointer(), b"not json").unwrap();
        assert_eq!(
            manager.status().unwrap().state,
            crate::domain::runtime::RuntimeState::Broken
        );
        assert_eq!(
            manager.startup_reconcile().unwrap().state,
            crate::domain::runtime::RuntimeState::Broken
        );

        let manager = fixture_manager(
            &directory.path().join("missing-target"),
            1,
            StaticManagedProcessInspector::default(),
        );
        manager.install("manifests/release-1.json").await.unwrap();
        let pointer = read_active_pointer(manager.paths()).unwrap().unwrap();
        fs::remove_dir_all(manager.paths().version_path(&pointer.installation_id)).unwrap();
        assert_eq!(
            manager.status().unwrap().state,
            crate::domain::runtime::RuntimeState::Broken
        );
    }

    #[tokio::test]
    async fn activation_requires_the_candidate_digest_and_verified_fallback() {
        let directory = tempdir().unwrap();
        let first = fixture_manager(
            directory.path(),
            1,
            StaticManagedProcessInspector::default(),
        );
        first.install("manifests/release-1.json").await.unwrap();
        let first_pointer = read_active_pointer(first.paths()).unwrap().unwrap();
        assert!(first
            .activate_candidate(
                &first_pointer.installation_id,
                Sha256Digest::from_hex(&"b".repeat(64)).unwrap(),
            )
            .is_err());

        let second = fixture_manager(
            directory.path(),
            2,
            StaticManagedProcessInspector::default(),
        );
        second.install("manifests/release-2.json").await.unwrap();
        fs::write(
            second
                .paths()
                .version_path(&first_pointer.installation_id)
                .join("runtime/app/AppRun"),
            b"damaged\n",
        )
        .unwrap();
        assert!(matches!(
            second.rollback(),
            Err(crate::domain::runtime::RuntimeError::NoRollback)
        ));
    }

    #[test]
    fn cleanup_cannot_cross_the_runtime_ownership_boundary() {
        let directory = tempdir().unwrap();
        let outside = directory.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep.txt"), b"keep").unwrap();
        let manager = fixture_manager(
            directory.path(),
            1,
            StaticManagedProcessInspector::default(),
        );
        manager.cleanup().unwrap();
        assert!(outside.join("keep.txt").exists());
    }
}
