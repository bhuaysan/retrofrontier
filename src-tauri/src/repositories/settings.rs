use sqlx::{Row, SqlitePool};

use crate::error::AppError;

#[derive(Clone)]
pub struct SettingsRepository {
    pool: SqlitePool,
}

impl SettingsRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>, AppError> {
        sqlx::query("SELECT value FROM app_settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map(|row| row.map(|row| row.get::<String, _>("value")))
            .map_err(AppError::Database)
    }

    pub async fn set(&self, key: &str, value: &str) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO app_settings (key, value) VALUES (?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(AppError::Database)
    }
}

#[cfg(test)]
mod tests {
    use super::SettingsRepository;
    use crate::adapters::database::Database;
    use tempfile::tempdir;

    #[tokio::test]
    async fn reads_and_updates_generic_foundation_settings() {
        let directory = tempdir().expect("temporary directory should be created");
        let database = Database::open(directory.path().join("settings.sqlite3"))
            .await
            .expect("database should initialize");
        let repository = SettingsRepository::new(database.pool().clone());

        assert_eq!(
            repository
                .get("foundation.ready")
                .await
                .expect("setting should be readable"),
            Some("true".to_owned())
        );
        assert_eq!(
            repository
                .get("ui.theme")
                .await
                .expect("missing setting should be readable"),
            None
        );

        repository
            .set("ui.theme", "dark")
            .await
            .expect("setting should be writable");
        assert_eq!(
            repository
                .get("ui.theme")
                .await
                .expect("setting should be readable after write"),
            Some("dark".to_owned())
        );
    }
}
