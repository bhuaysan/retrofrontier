use crate::application::RuntimeManager;
use crate::domain::runtime::RuntimeStatus;
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
        self.manager.status().map_err(AppError::Runtime)
    }

    pub fn manager(&self) -> &RuntimeManager {
        &self.manager
    }
}
