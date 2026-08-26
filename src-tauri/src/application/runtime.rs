use crate::application::RuntimeManager;
use crate::domain::runtime::{RuntimeStatus, VerifiedRuntimeSnapshot};
use crate::error::AppError;

/// Application-facing runtime boundary. Tauri commands depend on this service rather than on
/// filesystem, archive, or trust adapters.
#[derive(Clone)]
pub struct RuntimeApplicationService {
    manager: RuntimeManager,
}

impl RuntimeApplicationService {
    pub fn new(manager: RuntimeManager) -> Self {
        Self { manager }
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
}
