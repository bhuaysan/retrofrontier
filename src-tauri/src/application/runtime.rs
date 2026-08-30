use crate::adapters::runtime_release_source::ConfiguredReleaseSource;
use crate::application::RuntimeManager;
use crate::domain::runtime::{
    RuntimeError, RuntimeSourceOrigin, RuntimeStatus, VerifiedRuntimeSnapshot,
};
use crate::error::AppError;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// A normalized reason a managed runtime installation did not complete.
///
/// These mirror M7's launch-error contract: an anticipated problem is a typed response React can
/// act on, never an IPC error carrying a filesystem path or an OS message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeInstallErrorCode {
    /// This build has no approved managed release source configured at all.
    SourceNotConfigured,
    /// Another installation is already running in this process.
    InstallationInProgress,
    /// A managed RetroArch process is alive, so the runtime must not be mutated.
    GameRunning,
    /// The release could not be authenticated: metadata, signatures, policy, or digests.
    ReleaseNotTrusted,
    /// Trusted metadata or an approved target could not be retrieved.
    DownloadFailed,
    /// Downloaded bytes or the installed tree did not match the authenticated description.
    VerificationFailed,
    /// A verified archive could not be safely extracted.
    ExtractionFailed,
    /// The runtime retention or free-space policy refuses this installation.
    StorageLimit,
    /// The approved release does not target this platform or architecture.
    UnsupportedPlatform,
    /// Anything else, including filesystem and activation failures.
    InstallationFailed,
}

impl RuntimeInstallErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SourceNotConfigured => "sourceNotConfigured",
            Self::InstallationInProgress => "installationInProgress",
            Self::GameRunning => "gameRunning",
            Self::ReleaseNotTrusted => "releaseNotTrusted",
            Self::DownloadFailed => "downloadFailed",
            Self::VerificationFailed => "verificationFailed",
            Self::ExtractionFailed => "extractionFailed",
            Self::StorageLimit => "storageLimit",
            Self::UnsupportedPlatform => "unsupportedPlatform",
            Self::InstallationFailed => "installationFailed",
        }
    }

    /// A fixed RetroFrontier sentence. The underlying error text is logged, never returned: it can
    /// contain absolute paths and OS messages React has no business rendering.
    pub fn message(self) -> &'static str {
        match self {
            Self::SourceNotConfigured => {
                "This build has no approved managed RetroArch release source, so the runtime \
                 cannot be installed yet."
            }
            Self::InstallationInProgress => "A managed runtime installation is already running.",
            Self::GameRunning => {
                "A game is running. Close it before installing or repairing the runtime."
            }
            Self::ReleaseNotTrusted => {
                "The managed RetroArch release could not be authenticated and was refused."
            }
            Self::DownloadFailed => "The managed RetroArch release could not be downloaded.",
            Self::VerificationFailed => {
                "The managed RetroArch release failed integrity verification and was discarded."
            }
            Self::ExtractionFailed => {
                "The managed RetroArch release could not be installed safely."
            }
            Self::StorageLimit => {
                "There is not enough managed runtime storage available for this release."
            }
            Self::UnsupportedPlatform => {
                "The approved managed RetroArch release does not support this system."
            }
            Self::InstallationFailed => "The managed RetroArch runtime could not be installed.",
        }
    }
}

/// Map a runtime error onto the normalized code React sees.
fn classify(error: &RuntimeError) -> RuntimeInstallErrorCode {
    match error {
        RuntimeError::UnsupportedPlatform => RuntimeInstallErrorCode::UnsupportedPlatform,
        RuntimeError::GameActive | RuntimeError::ProcessRecordSchema => {
            RuntimeInstallErrorCode::GameRunning
        }
        RuntimeError::Trust(_) | RuntimeError::Manifest(_) => {
            RuntimeInstallErrorCode::ReleaseNotTrusted
        }
        RuntimeError::Download(_) => RuntimeInstallErrorCode::DownloadFailed,
        RuntimeError::Integrity(_) | RuntimeError::InstalledTree(_) => {
            RuntimeInstallErrorCode::VerificationFailed
        }
        RuntimeError::Extraction(_) => RuntimeInstallErrorCode::ExtractionFailed,
        RuntimeError::StorageLimit | RuntimeError::Storage(_) => {
            RuntimeInstallErrorCode::StorageLimit
        }
        RuntimeError::Lock(_)
        | RuntimeError::Pointer(_)
        | RuntimeError::NoRollback
        | RuntimeError::Io(_) => RuntimeInstallErrorCode::InstallationFailed,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInstallFailure {
    pub code: RuntimeInstallErrorCode,
    pub message: &'static str,
}

/// The result of one installation attempt, always accompanied by the runtime's real status.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInstallResponse {
    pub installed: bool,
    pub status: RuntimeStatus,
    pub error: Option<RuntimeInstallFailure>,
}

/// What Settings needs to render the runtime section truthfully.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInstallState {
    pub status: RuntimeStatus,
    /// Whether an approved managed release source exists in this build at all.
    pub source_configured: bool,
    /// Absent when no source is configured. Never inferred.
    pub source_origin: Option<RuntimeSourceOrigin>,
    /// The single approved release this build installs, for maintainer-visible identification.
    pub release_target: Option<String>,
    pub installing: bool,
}

/// Application-facing runtime boundary. Tauri commands depend on this service rather than on
/// filesystem, archive, or trust adapters.
#[derive(Clone)]
pub struct RuntimeApplicationService {
    manager: RuntimeManager,
    release_source: Option<Arc<ConfiguredReleaseSource>>,
    /// Serializes installation attempts inside this process. `RuntimeMutationLock` already
    /// serializes across processes; this exists so a second click reports a clear reason instead
    /// of blocking the IPC worker on a kernel lock.
    install_guard: Arc<Mutex<()>>,
    installing: Arc<AtomicBool>,
}

impl RuntimeApplicationService {
    pub fn new(manager: RuntimeManager) -> Self {
        Self {
            manager,
            release_source: None,
            install_guard: Arc::new(Mutex::new(())),
            installing: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_release_source(mut self, source: Option<ConfiguredReleaseSource>) -> Self {
        self.release_source = source.map(Arc::new);
        self
    }

    pub async fn get_runtime_status(&self) -> Result<RuntimeStatus, AppError> {
        self.verified_runtime_snapshot()
            .map(|snapshot| snapshot.status)
    }

    pub fn verified_runtime_snapshot(&self) -> Result<VerifiedRuntimeSnapshot, AppError> {
        self.manager.verified_snapshot().map_err(AppError::Runtime)
    }

    pub fn manager(&self) -> &RuntimeManager {
        &self.manager
    }

    pub fn get_install_state(&self) -> Result<RuntimeInstallState, AppError> {
        let status = self
            .manager
            .verified_snapshot()
            .map_err(AppError::Runtime)?;
        Ok(RuntimeInstallState {
            status: status.status,
            source_configured: self.release_source.is_some(),
            source_origin: self.release_source.as_ref().map(|source| source.origin),
            release_target: self
                .release_source
                .as_ref()
                .map(|source| source.manifest_target_name.clone()),
            installing: self.installing.load(Ordering::SeqCst),
        })
    }

    /// Install the single approved managed release this build is configured for.
    pub async fn install_runtime(&self) -> RuntimeInstallResponse {
        self.install_or_repair(false).await
    }

    /// Reconstruct the approved managed release into a fresh immutable installation.
    pub async fn repair_runtime(&self) -> RuntimeInstallResponse {
        self.install_or_repair(true).await
    }

    async fn install_or_repair(&self, repair: bool) -> RuntimeInstallResponse {
        let Some(release) = self.release_source.as_ref() else {
            return self.failure(RuntimeInstallErrorCode::SourceNotConfigured);
        };
        let Ok(guard) = self.install_guard.try_lock() else {
            return self.failure(RuntimeInstallErrorCode::InstallationInProgress);
        };
        self.installing.store(true, Ordering::SeqCst);

        let target = release.manifest_target_name.as_str();
        let result = if repair {
            self.manager.repair(target).await
        } else {
            self.manager.install(target).await
        };

        self.installing.store(false, Ordering::SeqCst);
        drop(guard);

        match result {
            Ok(status) => {
                tracing::info!(
                    repair,
                    state = ?status.state,
                    release_id = status.release_id.as_deref().unwrap_or("unknown"),
                    "managed runtime installation finished"
                );
                RuntimeInstallResponse {
                    installed: true,
                    status,
                    error: None,
                }
            }
            Err(error) => {
                let code = classify(&error);
                // The detail stays in the log. React receives the normalized code only.
                tracing::warn!(repair, code = code.as_str(), error = %error,
                    "managed runtime installation failed");
                self.failure(code)
            }
        }
    }

    fn failure(&self, code: RuntimeInstallErrorCode) -> RuntimeInstallResponse {
        // Report the runtime's real state alongside the failure; a failed install must never make
        // the UI believe the previously installed runtime disappeared.
        let status = self
            .manager
            .verified_snapshot()
            .map(|snapshot| snapshot.status)
            .unwrap_or_else(|_| RuntimeStatus::broken());
        RuntimeInstallResponse {
            installed: false,
            status,
            error: Some(RuntimeInstallFailure {
                code,
                message: code.message(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{classify, RuntimeInstallErrorCode};
    use crate::domain::runtime::RuntimeError;

    #[test]
    fn every_runtime_error_maps_to_a_stable_normalized_code() {
        let cases = [
            (
                RuntimeError::UnsupportedPlatform,
                RuntimeInstallErrorCode::UnsupportedPlatform,
            ),
            (
                RuntimeError::GameActive,
                RuntimeInstallErrorCode::GameRunning,
            ),
            (
                RuntimeError::ProcessRecordSchema,
                RuntimeInstallErrorCode::GameRunning,
            ),
            (
                RuntimeError::Trust("bad signature".to_owned()),
                RuntimeInstallErrorCode::ReleaseNotTrusted,
            ),
            (
                RuntimeError::Manifest("bad schema".to_owned()),
                RuntimeInstallErrorCode::ReleaseNotTrusted,
            ),
            (
                RuntimeError::Download("timeout".to_owned()),
                RuntimeInstallErrorCode::DownloadFailed,
            ),
            (
                RuntimeError::Integrity("hash".to_owned()),
                RuntimeInstallErrorCode::VerificationFailed,
            ),
            (
                RuntimeError::InstalledTree("inventory".to_owned()),
                RuntimeInstallErrorCode::VerificationFailed,
            ),
            (
                RuntimeError::Extraction("traversal".to_owned()),
                RuntimeInstallErrorCode::ExtractionFailed,
            ),
            (
                RuntimeError::StorageLimit,
                RuntimeInstallErrorCode::StorageLimit,
            ),
            (
                RuntimeError::Lock("busy".to_owned()),
                RuntimeInstallErrorCode::InstallationFailed,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(classify(&error), expected, "unexpected code for {error}");
        }
    }

    #[test]
    fn a_running_game_is_never_reported_as_a_generic_installation_failure() {
        // ADR-011 refuses runtime mutation while a managed process is alive. The user needs to be
        // told to close the game, not offered a pointless retry.
        assert_eq!(
            classify(&RuntimeError::GameActive),
            RuntimeInstallErrorCode::GameRunning
        );
    }

    #[test]
    fn normalized_messages_never_contain_a_filesystem_path() {
        for code in [
            RuntimeInstallErrorCode::SourceNotConfigured,
            RuntimeInstallErrorCode::InstallationInProgress,
            RuntimeInstallErrorCode::GameRunning,
            RuntimeInstallErrorCode::ReleaseNotTrusted,
            RuntimeInstallErrorCode::DownloadFailed,
            RuntimeInstallErrorCode::VerificationFailed,
            RuntimeInstallErrorCode::ExtractionFailed,
            RuntimeInstallErrorCode::StorageLimit,
            RuntimeInstallErrorCode::UnsupportedPlatform,
            RuntimeInstallErrorCode::InstallationFailed,
        ] {
            let message = code.message();
            assert!(!message.contains('/'), "{} leaks a path", code.as_str());
            assert!(!message.is_empty());
        }
    }
}
