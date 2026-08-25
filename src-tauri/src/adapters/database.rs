use std::path::PathBuf;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::error::AppError;

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self, AppError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(AppError::Storage)?;
        }

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(AppError::Database)?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(AppError::Migration)?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::Database;
    use tempfile::tempdir;

    #[tokio::test]
    async fn opens_sqlite_and_runs_foundation_migrations() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory
            .path()
            .join("database")
            .join("retrofrontier.sqlite3");

        let database = Database::open(&path).await.expect("database should open");
        let value: String =
            sqlx::query_scalar("SELECT value FROM app_settings WHERE key = 'foundation.ready'")
                .fetch_one(database.pool())
                .await
                .expect("foundation marker should exist");

        assert_eq!(value, "true");
        assert!(path.is_file());
    }
}
