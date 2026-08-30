use crate::domain::core::CoreId;
use crate::domain::launch::{
    GameLaunchOverride, PlaySession, PlaySessionId, PlaySessionOutcome, RunningGameSession,
};
use crate::domain::library::{ContentUnitId, GameId, UnixTimestamp};
use crate::error::AppError;
use sqlx::{Row, SqlitePool};
use std::time::{SystemTime, UNIX_EPOCH};

/// Persistence for the two user/product-owned launch tables.
///
/// This repository writes no scanner-owned and no provider-owned table, so a rescan or a metadata
/// refresh can never overwrite a per-game core override or delete play history.
#[derive(Clone)]
pub struct LaunchRepository {
    pool: SqlitePool,
}

/// Everything one launch needs to persist about the runtime it resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPlaySession {
    pub game_id: GameId,
    pub content_unit_id: ContentUnitId,
    pub core_id: CoreId,
    pub runtime_installation_id: String,
    pub runtime_release_id: String,
}

impl LaunchRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn core_override(
        &self,
        game_id: GameId,
    ) -> Result<Option<GameLaunchOverride>, AppError> {
        let row = sqlx::query(
            "SELECT game_id, core_id, updated_at FROM game_launch_overrides WHERE game_id = ?",
        )
        .bind(game_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        row.map(|row| {
            Ok(GameLaunchOverride {
                game_id: GameId(row.get("game_id")),
                core_id: core_id(&row.get::<String, _>("core_id"))?,
                updated_at: row.get("updated_at"),
            })
        })
        .transpose()
    }

    /// Records the user's core choice for one game.
    ///
    /// The core identifier is validated as an approved policy decision by the launch service; this
    /// method only refuses a value that is not a well-formed `CoreId`.
    pub async fn set_core_override(
        &self,
        game_id: GameId,
        core_id: &CoreId,
    ) -> Result<GameLaunchOverride, AppError> {
        let now = now_timestamp();
        sqlx::query(
            "INSERT INTO game_launch_overrides (game_id, core_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(game_id) DO UPDATE SET core_id = excluded.core_id, \
                                                updated_at = excluded.updated_at",
        )
        .bind(game_id.0)
        .bind(core_id.as_str())
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(GameLaunchOverride {
            game_id,
            core_id: core_id.clone(),
            updated_at: now,
        })
    }

    pub async fn clear_core_override(&self, game_id: GameId) -> Result<(), AppError> {
        sqlx::query("DELETE FROM game_launch_overrides WHERE game_id = ?")
            .bind(game_id.0)
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;
        Ok(())
    }

    /// Opens a play session before the managed process is spawned.
    ///
    /// The row exists so the durable process record can name the session it belongs to. It is
    /// history, never the answer to "is a managed process alive?".
    pub async fn start_session(&self, session: &NewPlaySession) -> Result<PlaySession, AppError> {
        let now = now_timestamp();
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO play_sessions \
             (game_id, content_unit_id, core_id, runtime_installation_id, runtime_release_id, \
              started_at, ended_at, exit_code, outcome, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, 'running', ?, ?) RETURNING id",
        )
        .bind(session.game_id.0)
        .bind(session.content_unit_id.0)
        .bind(session.core_id.as_str())
        .bind(&session.runtime_installation_id)
        .bind(&session.runtime_release_id)
        .bind(now)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(PlaySession {
            id: PlaySessionId(id),
            game_id: session.game_id,
            content_unit_id: session.content_unit_id,
            core_id: session.core_id.clone(),
            runtime_installation_id: session.runtime_installation_id.clone(),
            runtime_release_id: session.runtime_release_id.clone(),
            started_at: now,
            ended_at: None,
            exit_code: None,
            outcome: PlaySessionOutcome::Running,
        })
    }

    /// Closes one open session. A session that is already closed is left untouched, so a monitor
    /// and a restart reconciliation can never overwrite each other's verdict.
    pub async fn complete_session(
        &self,
        session_id: PlaySessionId,
        outcome: PlaySessionOutcome,
        exit_code: Option<i64>,
    ) -> Result<bool, AppError> {
        if outcome.is_open() {
            return Err(AppError::Library(
                "a play session cannot be closed as running".to_owned(),
            ));
        }
        let now = now_timestamp();
        let result = sqlx::query(
            "UPDATE play_sessions \
             SET outcome = ?, exit_code = ?, ended_at = ?, updated_at = ? \
             WHERE id = ? AND outcome = 'running'",
        )
        .bind(outcome.as_db())
        .bind(exit_code)
        .bind(now)
        .bind(now)
        .bind(session_id.0)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn open_sessions(&self) -> Result<Vec<PlaySession>, AppError> {
        let rows = sqlx::query(
            "SELECT id, game_id, content_unit_id, core_id, runtime_installation_id, \
                    runtime_release_id, started_at, ended_at, exit_code, outcome \
             FROM play_sessions WHERE outcome = 'running' ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        rows.iter().map(session_from_row).collect()
    }

    pub async fn session(
        &self,
        session_id: PlaySessionId,
    ) -> Result<Option<PlaySession>, AppError> {
        let row = sqlx::query(
            "SELECT id, game_id, content_unit_id, core_id, runtime_installation_id, \
                    runtime_release_id, started_at, ended_at, exit_code, outcome \
             FROM play_sessions WHERE id = ?",
        )
        .bind(session_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        row.as_ref().map(session_from_row).transpose()
    }

    /// Closes every session that is still open. Used by startup reconciliation once the durable
    /// process record has proven that no managed process survived.
    pub async fn interrupt_open_sessions(&self) -> Result<u64, AppError> {
        let now = now_timestamp();
        let result = sqlx::query(
            "UPDATE play_sessions SET outcome = 'interrupted', ended_at = ?, updated_at = ? \
             WHERE outcome = 'running'",
        )
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(result.rows_affected())
    }
}

pub fn running_session(session: &PlaySession) -> RunningGameSession {
    RunningGameSession {
        session_id: session.id,
        game_id: session.game_id,
        content_unit_id: session.content_unit_id,
        core_id: session.core_id.clone(),
        started_at: session.started_at,
    }
}

fn session_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<PlaySession, AppError> {
    let outcome = row.get::<String, _>("outcome");
    Ok(PlaySession {
        id: PlaySessionId(row.get("id")),
        game_id: GameId(row.get("game_id")),
        content_unit_id: ContentUnitId(row.get("content_unit_id")),
        core_id: core_id(&row.get::<String, _>("core_id"))?,
        runtime_installation_id: row.get("runtime_installation_id"),
        runtime_release_id: row.get("runtime_release_id"),
        started_at: row.get("started_at"),
        ended_at: row.get("ended_at"),
        exit_code: row.get("exit_code"),
        outcome: PlaySessionOutcome::from_db(&outcome).ok_or_else(|| {
            AppError::Library("a persisted play session has an unknown outcome".to_owned())
        })?,
    })
}

fn core_id(value: &str) -> Result<CoreId, AppError> {
    CoreId::new(value)
        .map_err(|_| AppError::Library("a persisted core identifier is invalid".to_owned()))
}

fn now_timestamp() -> UnixTimestamp {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{LaunchRepository, NewPlaySession};
    use crate::adapters::database::Database;
    use crate::domain::core::CoreId;
    use crate::domain::launch::{PlaySessionId, PlaySessionOutcome};
    use crate::domain::library::{ContentUnitId, GameId};
    use sqlx::SqlitePool;
    use tempfile::TempDir;

    const TEST_TIME: i64 = 1_756_000_000_000;

    async fn fixture() -> (TempDir, SqlitePool) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let database = Database::open(directory.path().join("launch.sqlite3"))
            .await
            .expect("database should open");
        let pool = database.pool().clone();
        sqlx::query(
            "INSERT INTO content_roots \
             (id, path, kind, enabled, availability, created_at, updated_at) \
             VALUES (1, '/synthetic/library', 'managed', 1, 'available', ?, ?)",
        )
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .execute(&pool)
        .await
        .expect("synthetic content root");
        sqlx::query(
            "INSERT INTO games (id, system_id, local_title, availability, created_at, updated_at) \
             VALUES (1, 'nes', 'Synthetic', 'available', ?, ?)",
        )
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .execute(&pool)
        .await
        .expect("synthetic game");
        sqlx::query(
            "INSERT INTO content_units \
             (id, game_id, root_id, system_id, kind, local_title, primary_relative_path, \
              fingerprint, availability, created_at, updated_at) \
             VALUES (1, 1, 1, 'nes', 'single_file', 'Synthetic', 'NES/synthetic.nes', NULL, \
                     'available', ?, ?)",
        )
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .execute(&pool)
        .await
        .expect("synthetic content unit");
        (directory, pool)
    }

    fn new_session() -> NewPlaySession {
        NewPlaySession {
            game_id: GameId(1),
            content_unit_id: ContentUnitId(1),
            core_id: CoreId::new("nestopia").unwrap(),
            runtime_installation_id: "install-1".to_owned(),
            runtime_release_id: "release-1".to_owned(),
        }
    }

    #[tokio::test]
    async fn the_launch_migration_creates_both_tables_with_restrictive_foreign_keys() {
        let (_directory, pool) = fixture().await;

        for (table, expected) in [("game_launch_overrides", 1), ("play_sessions", 2)] {
            let keys: Vec<String> =
                sqlx::query_scalar("SELECT \"on_delete\" FROM pragma_foreign_key_list(?)")
                    .bind(table)
                    .fetch_all(&pool)
                    .await
                    .expect("foreign key list");
            assert_eq!(keys.len(), expected, "{table} foreign keys");
            assert!(
                keys.iter().all(|action| action == "RESTRICT"),
                "{table} must not cascade deletes"
            );
        }
    }

    #[tokio::test]
    async fn an_open_session_must_have_no_end_and_a_closed_session_must_have_one() {
        let (_directory, pool) = fixture().await;

        let invalid = sqlx::query(
            "INSERT INTO play_sessions \
             (game_id, content_unit_id, core_id, runtime_installation_id, runtime_release_id, \
              started_at, ended_at, exit_code, outcome, created_at, updated_at) \
             VALUES (1, 1, 'nestopia', 'install-1', 'release-1', 1, 2, 0, 'running', 1, 1)",
        )
        .execute(&pool)
        .await;
        assert!(invalid.is_err(), "an open session cannot carry an end time");

        let invalid = sqlx::query(
            "INSERT INTO play_sessions \
             (game_id, content_unit_id, core_id, runtime_installation_id, runtime_release_id, \
              started_at, ended_at, exit_code, outcome, created_at, updated_at) \
             VALUES (1, 1, 'nestopia', 'install-1', 'release-1', 1, NULL, 0, 'completed', 1, 1)",
        )
        .execute(&pool)
        .await;
        assert!(invalid.is_err(), "a closed session needs an end time");
    }

    #[tokio::test]
    async fn a_core_override_is_upserted_and_cleared() {
        let (_directory, pool) = fixture().await;
        let repository = LaunchRepository::new(pool);

        assert!(repository.core_override(GameId(1)).await.unwrap().is_none());

        let nestopia = CoreId::new("nestopia").unwrap();
        let stored = repository
            .set_core_override(GameId(1), &nestopia)
            .await
            .unwrap();
        assert_eq!(stored.core_id, nestopia);
        assert_eq!(
            repository
                .core_override(GameId(1))
                .await
                .unwrap()
                .map(|value| value.core_id),
            Some(nestopia)
        );

        let other = CoreId::new("beetle-psx").unwrap();
        repository
            .set_core_override(GameId(1), &other)
            .await
            .unwrap();
        assert_eq!(
            repository
                .core_override(GameId(1))
                .await
                .unwrap()
                .map(|value| value.core_id),
            Some(other)
        );

        repository.clear_core_override(GameId(1)).await.unwrap();
        assert!(repository.core_override(GameId(1)).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_session_is_opened_once_and_closed_once() {
        let (_directory, pool) = fixture().await;
        let repository = LaunchRepository::new(pool);

        let session = repository.start_session(&new_session()).await.unwrap();
        assert_eq!(session.outcome, PlaySessionOutcome::Running);
        assert!(session.ended_at.is_none());
        assert_eq!(repository.open_sessions().await.unwrap().len(), 1);

        assert!(repository
            .complete_session(session.id, PlaySessionOutcome::Completed, Some(0))
            .await
            .unwrap());
        let closed = repository.session(session.id).await.unwrap().unwrap();
        assert_eq!(closed.outcome, PlaySessionOutcome::Completed);
        assert_eq!(closed.exit_code, Some(0));
        assert!(closed.ended_at.is_some());
        assert!(repository.open_sessions().await.unwrap().is_empty());

        // A second verdict must not overwrite the first one.
        assert!(!repository
            .complete_session(session.id, PlaySessionOutcome::Interrupted, None)
            .await
            .unwrap());
        assert_eq!(
            repository
                .session(session.id)
                .await
                .unwrap()
                .unwrap()
                .outcome,
            PlaySessionOutcome::Completed
        );
        assert!(repository
            .complete_session(session.id, PlaySessionOutcome::Running, None)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn restart_reconciliation_interrupts_every_open_session() {
        let (_directory, pool) = fixture().await;
        let repository = LaunchRepository::new(pool);
        let first = repository.start_session(&new_session()).await.unwrap();
        let second = repository.start_session(&new_session()).await.unwrap();
        repository
            .complete_session(first.id, PlaySessionOutcome::Completed, Some(0))
            .await
            .unwrap();

        assert_eq!(repository.interrupt_open_sessions().await.unwrap(), 1);

        assert_eq!(
            repository
                .session(second.id)
                .await
                .unwrap()
                .unwrap()
                .outcome,
            PlaySessionOutcome::Interrupted
        );
        assert_eq!(
            repository.session(first.id).await.unwrap().unwrap().outcome,
            PlaySessionOutcome::Completed
        );
        assert!(repository.open_sessions().await.unwrap().is_empty());
        assert!(repository
            .session(PlaySessionId(9_999))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn a_scan_style_content_rewrite_keeps_user_overrides_and_session_history() {
        let (_directory, pool) = fixture().await;
        let repository = LaunchRepository::new(pool.clone());
        repository
            .set_core_override(GameId(1), &CoreId::new("nestopia").unwrap())
            .await
            .unwrap();
        let session = repository.start_session(&new_session()).await.unwrap();
        repository
            .complete_session(session.id, PlaySessionOutcome::Completed, Some(0))
            .await
            .unwrap();

        // The scanner rewrites its own rows; it never touches the launch tables.
        sqlx::query(
            "UPDATE games SET local_title = 'Rescanned', availability = 'unavailable', \
             updated_at = ? WHERE id = 1",
        )
        .bind(TEST_TIME + 1)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE content_units SET availability = 'missing' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();

        assert!(repository.core_override(GameId(1)).await.unwrap().is_some());
        assert_eq!(
            repository
                .session(session.id)
                .await
                .unwrap()
                .unwrap()
                .outcome,
            PlaySessionOutcome::Completed
        );

        // Restrictive deletes keep history even when the scanner would remove local content.
        assert!(sqlx::query("DELETE FROM games WHERE id = 1")
            .execute(&pool)
            .await
            .is_err());
    }
}
