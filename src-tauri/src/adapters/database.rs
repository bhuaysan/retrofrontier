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
    use sqlx::migrate::Migrator;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::fs;
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

    #[tokio::test]
    async fn upgrades_an_existing_m3_database_through_the_m4_migration() {
        let directory = tempdir().expect("temporary database directory should be created");
        let path = directory.path().join("retrofrontier.sqlite3");
        let m3_migrations = tempdir().expect("temporary M3 migration directory should be created");
        fs::write(
            m3_migrations
                .path()
                .join("20260825000000_foundation_settings.up.sql"),
            include_str!("../../migrations/20260825000000_foundation_settings.up.sql"),
        )
        .expect("M3 up migration should be copied into the fixture");
        fs::write(
            m3_migrations
                .path()
                .join("20260825000000_foundation_settings.down.sql"),
            include_str!("../../migrations/20260825000000_foundation_settings.down.sql"),
        )
        .expect("M3 down migration should be copied into the fixture");

        let m3_database = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .create_if_missing(true)
                    .foreign_keys(true),
            )
            .await
            .expect("M3 database should open");
        let m3_migrator = Migrator::new(m3_migrations.path())
            .await
            .expect("M3 migration fixture should load");
        m3_migrator
            .run(&m3_database)
            .await
            .expect("M3 migration should apply");
        sqlx::query("INSERT INTO app_settings (key, value) VALUES ('legacy.setting', 'kept')")
            .execute(&m3_database)
            .await
            .expect("representative M3 settings data should be inserted");
        m3_database.close().await;

        let database = Database::open(&path)
            .await
            .expect("normal migration path should upgrade the M3 database");
        let legacy_value: String =
            sqlx::query_scalar("SELECT value FROM app_settings WHERE key = 'legacy.setting'")
                .fetch_one(database.pool())
                .await
                .expect("M3 settings data should survive the upgrade");
        assert_eq!(legacy_value, "kept");
        let m4_table_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN \
             ('content_roots', 'games', 'content_units', 'content_files', \
              'content_unit_files', 'scan_runs', 'scan_issues')",
        )
        .fetch_one(database.pool())
        .await
        .expect("M4 tables should exist");
        assert_eq!(m4_table_count, 7);
        let migration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(database.pool())
            .await
            .expect("migration history should be available");
        assert_eq!(migration_count, 2);
        database.pool().close().await;

        let reopened = Database::open(&path)
            .await
            .expect("opening an already upgraded database should be idempotent");
        let reopened_value: String =
            sqlx::query_scalar("SELECT value FROM app_settings WHERE key = 'legacy.setting'")
                .fetch_one(reopened.pool())
                .await
                .expect("legacy settings data should survive a second open");
        assert_eq!(reopened_value, "kept");
        let reopened_migration_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
                .fetch_one(reopened.pool())
                .await
                .expect("migration history should remain stable");
        assert_eq!(reopened_migration_count, 2);

        sqlx::migrate!("./migrations")
            .undo(reopened.pool(), 20260825000000)
            .await
            .expect("M4 down migration should revert only M4 schema");
        let m4_table_count_after_down: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN \
             ('content_roots', 'games', 'content_units', 'content_files', \
              'content_unit_files', 'scan_runs', 'scan_issues')",
        )
        .fetch_one(reopened.pool())
        .await
        .expect("schema should remain queryable after M4 down migration");
        assert_eq!(m4_table_count_after_down, 0);
        let legacy_after_down: String =
            sqlx::query_scalar("SELECT value FROM app_settings WHERE key = 'legacy.setting'")
                .fetch_one(reopened.pool())
                .await
                .expect("M3 settings should survive M4 down migration");
        assert_eq!(legacy_after_down, "kept");
        reopened.pool().close().await;

        let migrated_again = Database::open(&path)
            .await
            .expect("M4 should apply again after its down migration");
        let m4_table_count_after_reapply: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN \
             ('content_roots', 'games', 'content_units', 'content_files', \
              'content_unit_files', 'scan_runs', 'scan_issues')",
        )
        .fetch_one(migrated_again.pool())
        .await
        .expect("M4 tables should be restored");
        assert_eq!(m4_table_count_after_reapply, 7);
        let final_legacy_value: String =
            sqlx::query_scalar("SELECT value FROM app_settings WHERE key = 'legacy.setting'")
                .fetch_one(migrated_again.pool())
                .await
                .expect("M3 settings should survive reapplying M4");
        assert_eq!(final_legacy_value, "kept");
    }
}
