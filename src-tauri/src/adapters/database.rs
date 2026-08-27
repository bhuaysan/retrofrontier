use std::path::PathBuf;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::error::AppError;

/// How long a writer waits for a competing write before failing.
///
/// M5 adds background metadata writes alongside interactive library operations, so a write that
/// arrives during another transaction must wait instead of returning "database is locked" to the
/// user. The value is generous relative to the short transactions this application issues.
const BUSY_TIMEOUT: Duration = Duration::from_secs(10);

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

        // Write-ahead logging lets the interactive reads that dominate the UI proceed while a
        // background metadata transaction is committing. SQLite still allows only one writer at a
        // time, so `BUSY_TIMEOUT` makes the loser wait rather than fail. `Normal` synchronous mode
        // is the documented safe pairing for WAL: it can lose the most recent commits after an OS
        // or power failure but never corrupts the database, and provider metadata is refetchable.
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(BUSY_TIMEOUT);
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
    use super::{Database, BUSY_TIMEOUT};
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
        assert_eq!(migration_count, 3);
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
        assert_eq!(reopened_migration_count, 3);

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

    /// Builds a database at the pre-M5 (M4) schema level using only the first two migrations.
    async fn open_pre_m5_database(path: &std::path::Path) -> sqlx::SqlitePool {
        let migrations = tempdir().expect("temporary migration directory should be created");
        for name in [
            "20260825000000_foundation_settings",
            "20260826000000_library_scanner",
        ] {
            for direction in ["up", "down"] {
                let contents = match (name, direction) {
                    ("20260825000000_foundation_settings", "up") => {
                        include_str!("../../migrations/20260825000000_foundation_settings.up.sql")
                    }
                    ("20260825000000_foundation_settings", "down") => {
                        include_str!("../../migrations/20260825000000_foundation_settings.down.sql")
                    }
                    ("20260826000000_library_scanner", "up") => {
                        include_str!("../../migrations/20260826000000_library_scanner.up.sql")
                    }
                    _ => include_str!("../../migrations/20260826000000_library_scanner.down.sql"),
                };
                fs::write(
                    migrations.path().join(format!("{name}.{direction}.sql")),
                    contents,
                )
                .expect("pre-M5 migration fixture should be written");
            }
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true)
                    .foreign_keys(true),
            )
            .await
            .expect("pre-M5 database should open");
        Migrator::new(migrations.path())
            .await
            .expect("pre-M5 migration fixture should load")
            .run(&pool)
            .await
            .expect("pre-M5 migrations should apply");
        pool
    }

    /// Representative M4 library content: a root, a game, a unit, a file, and reconciliation history.
    async fn populate_m4_library(pool: &sqlx::SqlitePool) {
        sqlx::query(
            "INSERT INTO content_roots (id, path, kind, enabled, availability, created_at, \
             updated_at) VALUES (1, '/library/ROMs', 'managed', 1, 'available', 10, 10)",
        )
        .execute(pool)
        .await
        .expect("content root fixture");
        sqlx::query(
            "INSERT INTO games (id, system_id, local_title, availability, created_at, updated_at) \
             VALUES (41, 'snes', 'Example Quest', 'available', 10, 10)",
        )
        .execute(pool)
        .await
        .expect("game fixture");
        sqlx::query(
            "INSERT INTO content_units (id, game_id, root_id, system_id, kind, local_title, \
             primary_relative_path, fingerprint, availability, created_at, updated_at) \
             VALUES (42, 41, 1, 'snes', 'single_file', 'Example Quest', \
             'SNES/Example Quest (USA).sfc', 'fingerprint-1', 'available', 10, 10)",
        )
        .execute(pool)
        .await
        .expect("content unit fixture");
        sqlx::query(
            "INSERT INTO content_files (id, root_id, relative_path, size_bytes, modified_at, \
             crc32, md5, sha1, availability, created_at, updated_at) \
             VALUES (43, 1, 'SNES/Example Quest (USA).sfc', 524288, 10, 'AABBCCDD', \
             'd41d8cd98f00b204e9800998ecf8427e', \
             'da39a3ee5e6b4b0d3255bfef95601890afd80709', 'available', 10, 10)",
        )
        .execute(pool)
        .await
        .expect("content file fixture");
        sqlx::query(
            "INSERT INTO content_unit_files (content_unit_id, content_file_id, ordinal, role) \
             VALUES (42, 43, 0, 'standalone')",
        )
        .execute(pool)
        .await
        .expect("membership fixture");
        sqlx::query(
            "INSERT INTO scan_runs (id, state, started_at, completed_at) \
             VALUES (5, 'completed', 10, 20)",
        )
        .execute(pool)
        .await
        .expect("scan run fixture");
        sqlx::query(
            "INSERT INTO scan_issues (id, scan_run_id, root_id, kind, relative_path, created_at) \
             VALUES (6, 5, 1, 'duplicate_content', 'SNES/copy.sfc', 15)",
        )
        .execute(pool)
        .await
        .expect("scan issue fixture");
    }

    #[tokio::test]
    async fn upgrades_a_populated_m4_database_to_m5_without_touching_local_identity() {
        let directory = tempdir().expect("temporary database directory should be created");
        let path = directory.path().join("retrofrontier.sqlite3");
        let pre_m5 = open_pre_m5_database(&path).await;
        populate_m4_library(&pre_m5).await;
        pre_m5.close().await;

        let database = Database::open(&path)
            .await
            .expect("the M5 migration should upgrade a populated M4 database");
        let pool = database.pool();

        // Every local identifier is unchanged.
        let game: (i64, String, String) =
            sqlx::query_as("SELECT id, system_id, availability FROM games")
                .fetch_one(pool)
                .await
                .expect("the game should survive the upgrade");
        assert_eq!(game, (41, "snes".to_owned(), "available".to_owned()));
        let unit: (i64, i64, String, Option<String>) =
            sqlx::query_as("SELECT id, game_id, availability, fingerprint FROM content_units")
                .fetch_one(pool)
                .await
                .expect("the content unit should survive the upgrade");
        assert_eq!(
            unit,
            (
                42,
                41,
                "available".to_owned(),
                Some("fingerprint-1".to_owned())
            )
        );
        let file: (i64, String, Option<String>) =
            sqlx::query_as("SELECT id, relative_path, sha1 FROM content_files")
                .fetch_one(pool)
                .await
                .expect("the content file should survive the upgrade");
        assert_eq!(file.0, 43);
        assert_eq!(file.1, "SNES/Example Quest (USA).sfc");
        assert_eq!(
            file.2.as_deref(),
            Some("da39a3ee5e6b4b0d3255bfef95601890afd80709")
        );
        let membership: (i64, i64, i64) = sqlx::query_as(
            "SELECT content_unit_id, content_file_id, ordinal FROM content_unit_files",
        )
        .fetch_one(pool)
        .await
        .expect("membership should survive the upgrade");
        assert_eq!(membership, (42, 43, 0));
        let issues: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scan_issues")
            .fetch_one(pool)
            .await
            .expect("reconciliation history should survive the upgrade");
        assert_eq!(issues, 1);

        // Every new metadata table exists and starts empty.
        let metadata_tables = [
            "SELECT COUNT(*) FROM provider_matches",
            "SELECT COUNT(*) FROM provider_match_evidence",
            "SELECT COUNT(*) FROM provider_match_candidates",
            "SELECT COUNT(*) FROM provider_metadata",
            "SELECT COUNT(*) FROM provider_media_assets",
            "SELECT COUNT(*) FROM metadata_jobs",
            "SELECT COUNT(*) FROM provider_scheduler_state",
            "SELECT COUNT(*) FROM provider_user_accounts",
            "SELECT COUNT(*) FROM user_provider_selections",
        ];
        for query in metadata_tables {
            let count: i64 = sqlx::query_scalar(query)
                .fetch_one(pool)
                .await
                .unwrap_or_else(|error| panic!("{query} should succeed: {error}"));
            assert_eq!(count, 0, "{query} must start empty");
        }

        // Foreign keys are intact and the new tables really reference local identity.
        let violations: Vec<(String,)> = sqlx::query_as("PRAGMA foreign_key_check")
            .fetch_all(pool)
            .await
            .expect("foreign key check should run");
        assert!(
            violations.is_empty(),
            "migrated schema has foreign key violations"
        );
        sqlx::query(
            "INSERT INTO provider_matches (game_id, provider_id, status, created_at, updated_at) \
             VALUES (41, 'screenscraper', 'pending', 30, 30)",
        )
        .execute(pool)
        .await
        .expect("a provider match should attach to the preserved game");
        sqlx::query(
            "INSERT INTO provider_matches (game_id, provider_id, status, created_at, updated_at) \
             VALUES (999, 'screenscraper', 'pending', 30, 30)",
        )
        .execute(pool)
        .await
        .expect_err("a provider match must not reference an unknown game");
        pool.close().await;

        // The application can restart on the migrated database.
        let reopened = Database::open(&path)
            .await
            .expect("the migrated database should reopen");
        let migration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(reopened.pool())
            .await
            .expect("migration history should be available");
        assert_eq!(migration_count, 3);
        let preserved: i64 = sqlx::query_scalar("SELECT id FROM games")
            .fetch_one(reopened.pool())
            .await
            .expect("the game should still be there after a restart");
        assert_eq!(preserved, 41);
        let provider_matches: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM provider_matches")
            .fetch_one(reopened.pool())
            .await
            .expect("provider state should persist across a restart");
        assert_eq!(provider_matches, 1);
    }

    #[tokio::test]
    async fn the_m5_migration_can_be_reverted_without_losing_local_library_data() {
        let directory = tempdir().expect("temporary database directory should be created");
        let path = directory.path().join("retrofrontier.sqlite3");
        let pre_m5 = open_pre_m5_database(&path).await;
        populate_m4_library(&pre_m5).await;
        pre_m5.close().await;

        let database = Database::open(&path).await.expect("M5 migration applies");
        sqlx::query(
            "INSERT INTO provider_matches (game_id, provider_id, status, match_type, \
             provider_game_id, created_at, updated_at) \
             VALUES (41, 'screenscraper', 'matched', 'deterministic_sha1', '3', 30, 30)",
        )
        .execute(database.pool())
        .await
        .expect("provider state fixture");

        sqlx::migrate!("./migrations")
            .undo(database.pool(), 20260826000000)
            .await
            .expect("the M5 down migration should revert only metadata schema");

        let metadata_tables: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN \
             ('provider_matches', 'provider_match_evidence', 'provider_match_candidates', \
              'provider_metadata', 'provider_media_assets', 'metadata_jobs', \
              'provider_scheduler_state', 'provider_user_accounts', 'user_provider_selections')",
        )
        .fetch_one(database.pool())
        .await
        .expect("schema should remain queryable after the down migration");
        assert_eq!(metadata_tables, 0);
        let game: i64 = sqlx::query_scalar("SELECT id FROM games")
            .fetch_one(database.pool())
            .await
            .expect("local library data must survive the down migration");
        assert_eq!(game, 41);
        database.pool().close().await;

        let reapplied = Database::open(&path)
            .await
            .expect("M5 should apply again after being reverted");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM provider_matches")
            .fetch_one(reapplied.pool())
            .await
            .expect("metadata tables should be restored");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn write_concurrency_uses_wal_with_a_busy_timeout() {
        let directory = tempdir().expect("temporary database directory should be created");
        let database = Database::open(directory.path().join("retrofrontier.sqlite3"))
            .await
            .expect("database should open");

        let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(database.pool())
            .await
            .expect("journal mode should be readable");
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");

        let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
            .fetch_one(database.pool())
            .await
            .expect("synchronous mode should be readable");
        assert_eq!(synchronous, 1, "WAL is paired with NORMAL synchronous mode");

        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(database.pool())
            .await
            .expect("foreign key enforcement should be readable");
        assert_eq!(foreign_keys, 1);
    }

    /// ADR-013 asserts the pragmas by test rather than assumption, so the assertion has to cover
    /// the pool, not one arbitrary connection.
    ///
    /// `synchronous`, `foreign_keys`, and `busy_timeout` are connection-local settings: they are
    /// supplied through `SqliteConnectOptions` and therefore applied every time the pool opens a
    /// new connection. This holds every connection open simultaneously so each one is checked.
    #[tokio::test]
    async fn every_pooled_connection_carries_the_required_pragmas() {
        let directory = tempdir().expect("temporary database directory should be created");
        let database = Database::open(directory.path().join("retrofrontier.sqlite3"))
            .await
            .expect("database should open");
        let pool = database.pool();

        // Hold every connection at once so the checks cannot all land on the same one.
        let mut connections = Vec::new();
        for _ in 0..pool.options().get_max_connections() {
            connections.push(pool.acquire().await.expect("a pooled connection"));
        }
        assert!(
            connections.len() > 1,
            "the pool must actually be multi-connection for this test to mean anything"
        );

        for (index, connection) in connections.iter_mut().enumerate() {
            let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
                .fetch_one(&mut **connection)
                .await
                .expect("journal mode should be readable");
            assert_eq!(
                journal_mode.to_ascii_lowercase(),
                "wal",
                "connection {index} is not in WAL mode"
            );

            let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
                .fetch_one(&mut **connection)
                .await
                .expect("synchronous mode should be readable");
            assert_eq!(
                synchronous, 1,
                "connection {index} is not synchronous=NORMAL"
            );

            let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
                .fetch_one(&mut **connection)
                .await
                .expect("foreign key enforcement should be readable");
            assert_eq!(
                foreign_keys, 1,
                "connection {index} does not enforce foreign keys"
            );

            let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
                .fetch_one(&mut **connection)
                .await
                .expect("busy timeout should be readable");
            assert_eq!(
                busy_timeout,
                BUSY_TIMEOUT.as_millis() as i64,
                "connection {index} does not carry the busy timeout"
            );
        }
    }

    #[tokio::test]
    async fn background_metadata_writes_do_not_block_ordinary_library_operations() {
        let directory = tempdir().expect("temporary database directory should be created");
        let database = Database::open(directory.path().join("retrofrontier.sqlite3"))
            .await
            .expect("database should open");
        let pool = database.pool().clone();
        sqlx::query(
            "INSERT INTO content_roots (id, path, kind, enabled, availability, created_at, \
             updated_at) VALUES (1, '/library/ROMs', 'managed', 1, 'available', 10, 10)",
        )
        .execute(&pool)
        .await
        .expect("content root fixture");
        for index in 0..20 {
            sqlx::query(
                "INSERT INTO games (system_id, local_title, availability, created_at, updated_at) \
                 VALUES ('snes', ?, 'available', 10, 10)",
            )
            .bind(format!("Game {index}"))
            .execute(&pool)
            .await
            .expect("game fixture");
        }

        // A background writer hammering the metadata tables in short transactions.
        let writer_pool = pool.clone();
        let writer = tokio::spawn(async move {
            for round in 0..60 {
                let mut transaction = writer_pool
                    .begin()
                    .await
                    .expect("a background transaction should start");
                sqlx::query(
                    "INSERT INTO provider_matches (game_id, provider_id, status, created_at, \
                     updated_at) VALUES (?, 'screenscraper', 'pending', ?, ?) \
                     ON CONFLICT(game_id, provider_id) DO UPDATE SET updated_at = excluded.updated_at",
                )
                .bind((round % 20) + 1)
                .bind(round)
                .bind(round)
                .execute(&mut *transaction)
                .await
                .expect("a background metadata write should succeed");
                transaction
                    .commit()
                    .await
                    .expect("a background metadata write should commit");
            }
        });

        // Interactive library reads and writes must keep succeeding meanwhile.
        for round in 0..60 {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM games")
                .fetch_one(&pool)
                .await
                .expect("an interactive library read must not fail during background writes");
            assert_eq!(count, 20);
            sqlx::query("UPDATE content_roots SET last_scan_at = ? WHERE id = 1")
                .bind(round)
                .execute(&pool)
                .await
                .expect("an interactive library write must not fail during background writes");
        }

        writer.await.expect("the background writer should finish");
        let written: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM provider_matches")
            .fetch_one(&pool)
            .await
            .expect("provider rows should be readable");
        assert_eq!(written, 20);
    }
}
