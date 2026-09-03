mod app_info;
pub mod launch;
pub mod library;
pub mod metadata;
pub mod metadata_scrape;
mod runtime;
pub mod runtime_manager;
pub mod save_state;
mod systems;

use crate::repositories::settings::SettingsRepository;

use crate::adapters::runtime_lock::ApplicationInstanceLock;
use crate::services::media_delivery::CachedCoverDelivery;
pub use app_info::AppInfoService;
pub use launch::{LaunchApplicationService, LaunchConfig, TauriLaunchEventSink};
pub use library::{LibraryApplicationService, TauriScanEventSink};
pub use metadata::{
    MetadataApplicationService, MetadataConfig, MetadataWorker, ProviderCredentialState,
    TauriMetadataStateEventSink,
};
pub use metadata_scrape::{
    MetadataScrapeApplicationService, MetadataScrapeConfig, MetadataWorkSignal,
};
pub use runtime::{RuntimeApplicationService, RuntimeInstallResponse, RuntimeInstallState};
pub use runtime_manager::RuntimeManager;
pub use save_state::{SaveStateApplicationService, SaveStateConfig};
use std::sync::Arc;
pub use systems::{SystemsApplicationService, SystemsResponse};

#[derive(Clone)]
pub struct AppState {
    app_info: AppInfoService,
    launch: LaunchApplicationService,
    runtime: RuntimeApplicationService,
    systems: SystemsApplicationService,
    library: LibraryApplicationService,
    metadata: Arc<MetadataApplicationService>,
    metadata_scrape: Arc<MetadataScrapeApplicationService>,
    media_delivery: Arc<CachedCoverDelivery>,
    save_states: Arc<SaveStateApplicationService>,
    _metadata_worker: Arc<MetadataWorker>,
    _instance_lock: Arc<ApplicationInstanceLock>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settings: SettingsRepository,
        runtime: RuntimeApplicationService,
        systems: SystemsApplicationService,
        instance_lock: ApplicationInstanceLock,
        library: LibraryApplicationService,
        launch: LaunchApplicationService,
        metadata: Arc<MetadataApplicationService>,
        metadata_scrape: Arc<MetadataScrapeApplicationService>,
        media_delivery: Arc<CachedCoverDelivery>,
        save_states: Arc<SaveStateApplicationService>,
        metadata_worker: Arc<MetadataWorker>,
    ) -> Self {
        Self {
            app_info: AppInfoService::new(settings),
            launch,
            runtime,
            systems,
            library,
            metadata,
            metadata_scrape,
            media_delivery,
            save_states,
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

    pub fn launch(&self) -> &LaunchApplicationService {
        &self.launch
    }

    pub fn metadata(&self) -> &MetadataApplicationService {
        &self.metadata
    }

    pub fn metadata_scrape(&self) -> &MetadataScrapeApplicationService {
        &self.metadata_scrape
    }

    pub fn media_delivery(&self) -> &CachedCoverDelivery {
        &self.media_delivery
    }

    pub fn save_states(&self) -> &Arc<SaveStateApplicationService> {
        &self.save_states
    }
}
