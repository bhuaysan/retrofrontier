use crate::domain::AppInfo;
use crate::error::AppError;
use crate::repositories::settings::SettingsRepository;

#[derive(Clone)]
pub struct AppInfoService {
    settings: SettingsRepository,
}

impl AppInfoService {
    pub fn new(settings: SettingsRepository) -> Self {
        Self { settings }
    }

    pub async fn get_app_info(&self) -> Result<AppInfo, AppError> {
        let database_ready = self
            .settings
            .get("foundation.ready")
            .await?
            .is_some_and(|value| value == "true");

        Ok(AppInfo {
            app_name: "RetroFrontier",
            version: env!("CARGO_PKG_VERSION"),
            platform: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
            database_ready,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AppInfoService;
    use crate::adapters::database::Database;
    use crate::repositories::settings::SettingsRepository;
    use tempfile::tempdir;

    #[tokio::test]
    async fn service_returns_platform_info_and_database_health() {
        let directory = tempdir().expect("temporary directory should be created");
        let database = Database::open(directory.path().join("foundation.sqlite3"))
            .await
            .expect("database should initialize");
        let service = AppInfoService::new(SettingsRepository::new(database.pool().clone()));

        let info = service.get_app_info().await.expect("app info should load");

        assert_eq!(info.app_name, "RetroFrontier");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.platform, std::env::consts::OS);
        assert_eq!(info.architecture, std::env::consts::ARCH);
        assert!(info.database_ready);
    }
}
