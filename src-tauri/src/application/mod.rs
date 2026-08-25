mod app_info;

use crate::repositories::settings::SettingsRepository;

pub use app_info::AppInfoService;

#[derive(Clone)]
pub struct AppState {
    app_info: AppInfoService,
}

impl AppState {
    pub fn new(settings: SettingsRepository) -> Self {
        Self {
            app_info: AppInfoService::new(settings),
        }
    }

    pub fn app_info(&self) -> &AppInfoService {
        &self.app_info
    }
}
