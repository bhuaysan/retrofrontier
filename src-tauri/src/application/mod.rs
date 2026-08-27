mod app_info;
pub mod library;
pub mod metadata;
mod runtime;
pub mod runtime_manager;
mod systems;

use crate::repositories::settings::SettingsRepository;

use crate::adapters::runtime_lock::ApplicationInstanceLock;
pub use app_info::AppInfoService;
pub use library::{LibraryApplicationService, TauriScanEventSink};
pub use metadata::{
    MetadataApplicationService, MetadataConfig, MetadataWorker, ProviderCredentialState,
};
pub use runtime::RuntimeApplicationService;
pub use runtime_manager::RuntimeManager;
use std::sync::Arc;
pub use systems::{SystemsApplicationService, SystemsResponse};

#[derive(Clone)]
pub struct AppState {
    app_info: AppInfoService,
    runtime: RuntimeApplicationService,
    systems: SystemsApplicationService,
    library: LibraryApplicationService,
    metadata: Arc<MetadataApplicationService>,
    _metadata_worker: Arc<MetadataWorker>,
    _instance_lock: Arc<ApplicationInstanceLock>,
}

impl AppState {
    pub fn new(
        settings: SettingsRepository,
        runtime: RuntimeApplicationService,
        systems: SystemsApplicationService,
        instance_lock: ApplicationInstanceLock,
        library: LibraryApplicationService,
        metadata: Arc<MetadataApplicationService>,
        metadata_worker: Arc<MetadataWorker>,
    ) -> Self {
        Self {
            app_info: AppInfoService::new(settings),
            runtime,
            systems,
            library,
            metadata,
            _metadata_worker: metadata_worker,
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

    pub fn library(&self) -> &LibraryApplicationService {
        &self.library
    }

    pub fn metadata(&self) -> &MetadataApplicationService {
        &self.metadata
    }
}
