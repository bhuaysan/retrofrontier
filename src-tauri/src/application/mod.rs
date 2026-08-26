mod app_info;
mod runtime;
pub mod runtime_manager;
mod systems;

use crate::repositories::settings::SettingsRepository;

use crate::adapters::runtime_lock::ApplicationInstanceLock;
pub use app_info::AppInfoService;
pub use runtime::RuntimeApplicationService;
pub use runtime_manager::RuntimeManager;
use std::sync::Arc;
pub use systems::{SystemsApplicationService, SystemsResponse};

#[derive(Clone)]
pub struct AppState {
    app_info: AppInfoService,
    runtime: RuntimeApplicationService,
    systems: SystemsApplicationService,
    _instance_lock: Arc<ApplicationInstanceLock>,
}

impl AppState {
    pub fn new(
        settings: SettingsRepository,
        runtime: RuntimeApplicationService,
        systems: SystemsApplicationService,
        instance_lock: ApplicationInstanceLock,
    ) -> Self {
        Self {
            app_info: AppInfoService::new(settings),
            runtime,
            systems,
            _instance_lock: Arc::new(instance_lock),
        }
    }

    pub fn app_info(&self) -> &AppInfoService {
        &self.app_info
    }

    pub fn runtime(&self) -> &RuntimeApplicationService {
        &self.runtime
    }

    pub fn systems(&self) -> &SystemsApplicationService {
        &self.systems
    }
}
