//! Persistence for proved Save-State provenance and durable launch baselines.
//!
//! This repository is the SQLite half of the M9 authority split: it owns provenance, lifecycle
//! history, identity, and the *registered* file identity. It never decides filesystem safety, never
//! parses a RetroArch filename, and never reads or writes the filesystem. Those belong to
//! `crate::services::save_state_fs`.
//!
//! Two invariants are enforced structurally rather than by convention:
//!
//! - **Core-binary provenance is immutable.** No method here updates `core_binary_sha256` on an
//!   existing row. A proved change of core binary at the same physical path supersedes the old row
//!   and inserts a new one.
//! - **A closed lifecycle value is never reopened.** Every transition is conditioned on the row
//!   still being `available`, so two reconciliations or a reconciliation racing a delete cannot
//!   overwrite each other's verdict.

use crate::domain::core::CoreId;
use crate::domain::launch::PlaySessionId;
use crate::domain::library::{ContentUnitId, GameId, UnixTimestamp};
use crate::domain::runtime::{RelativePath, SafeIdentifier, Sha256Digest};
use crate::domain::save_state::{
    LaunchStateBaseline, LaunchStateBaselineEntry, SaveState, SaveStateFileIdentity, SaveStateId,
    SaveStateProvenance, SaveStateSlot, SaveStateStatus, SaveStateThumbnailIdentity,
};
use crate::error::AppError;
use sqlx::{Row, SqlitePool};
use std::time::{SystemTime, UNIX_EPOCH};

/// One projection of a `save_states` row, so every read shares a single column list.
///
/// `sqlx` 0.9 accepts only `&'static str` queries, which is a deliberate injection guard. Building
/// the literals through `concat!` keeps that guarantee *and* keeps the column list in one place.
macro_rules! select_save_states {
    ($predicate:literal) => {
        concat!(
            "SELECT id, game_id, content_unit_id, play_session_id, core_id, core_component_id, \
             core_binary_sha256, core_display_version, core_source_revision, \
             originating_runtime_release_id, slot, state_relative_path, state_sha256, \
             state_size, thumbnail_relative_path, thumbnail_sha256, thumbnail_size, status, \
             created_at, updated_at \
             FROM save_states ",
            $predicate
        )
    };
}

/// Everything one reconciliation proved about one new Save State.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSaveState {
    pub provenance: SaveStateProvenance,
    pub slot: SaveStateSlot,
    pub state: SaveStateFileIdentity,
    pub thumbnail: Option<SaveStateThumbnailIdentity>,
}

/// The proved physical content a state row is being refreshed to.
///
/// Used only when the *same* core binary overwrote its own slot: identity and immutable core
/// provenance are untouched, and only the physical facts plus the producing session move on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshedSaveState {
    pub play_session_id: PlaySessionId,
    pub state: SaveStateFileIdentity,
    pub thumbnail: Option<SaveStateThumbnailIdentity>,
}

#[derive(Clone)]
pub struct SaveStateRepository {
    pool: SqlitePool,
}

impl SaveStateRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The Save States Game Detail lists: `available` only, most recently updated first.
    ///
    /// `missing`, `superseded`, and `deleted` rows are history and never appear here, which is why
    /// the frontend needs no lifecycle filter of its own.
    pub async fn save_states_for_game(&self, game_id: GameId) -> Result<Vec<SaveState>, AppError> {
        let rows = sqlx::query(select_save_states!(
            "WHERE game_id = ? AND status = 'available' ORDER BY updated_at DESC, id DESC"
        ))
        .bind(game_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        rows.iter().map(save_state_from_row).collect()
    }

    pub async fn save_state(&self, id: SaveStateId) -> Result<Option<SaveState>, AppError> {
        let row = sqlx::query(select_save_states!("WHERE id = ?"))
            .bind(id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::Database)?;

        row.as_ref().map(save_state_from_row).transpose()
    }

    /// Every `available` row, whatever game it belongs to.
    ///
    /// Used only by reconciliation, and only after a *complete* state-tree enumeration, to prove
    /// that a registered file is really gone.
    pub async fn available_states(&self) -> Result<Vec<SaveState>, AppError> {
        let rows = sqlx::query(select_save_states!(
            "WHERE status = 'available' ORDER BY id ASC"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        rows.iter().map(save_state_from_row).collect()
    }

    /// The single `available` row claiming one physical path, if any.
    pub async fn available_state_at_path(
        &self,
        relative_path: &RelativePath,
    ) -> Result<Option<SaveState>, AppError> {
        let row = sqlx::query(select_save_states!(
            "WHERE state_relative_path = ? AND status = 'available'"
        ))
        .bind(relative_path.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        row.as_ref().map(save_state_from_row).transpose()
    }

    /// Register one newly proved Save State.
    ///
    /// Idempotent by construction: the unique `(play_session_id, state_relative_path, state_sha256)`
    /// index means a replayed reconciliation finds the row it already wrote and returns it
    /// unchanged, including its `updated_at`. Nothing about a replay is a second registration.
    pub async fn register_state(&self, new: &NewSaveState) -> Result<SaveState, AppError> {
        if let Some(existing) = self.state_by_session_identity(new).await? {
            return Ok(existing);
        }

        let now = now_timestamp();
        let inserted = sqlx::query(
            "INSERT INTO save_states \
             (game_id, content_unit_id, play_session_id, core_id, core_component_id, \
              core_binary_sha256, core_display_version, core_source_revision, \
              originating_runtime_release_id, slot, state_relative_path, state_sha256, \
              state_size, thumbnail_relative_path, thumbnail_sha256, thumbnail_size, \
              status, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'available', ?, ?) \
             RETURNING id",
        )
        .bind(new.provenance.game_id.0)
        .bind(new.provenance.content_unit_id.0)
        .bind(new.provenance.play_session_id.0)
        .bind(new.provenance.core_id.as_str())
        .bind(new.provenance.core_component_id.as_str())
        .bind(new.provenance.core_binary_sha256.to_hex())
        .bind(new.provenance.core_display_version.as_deref())
        .bind(new.provenance.core_source_revision.as_deref())
        .bind(new.provenance.originating_runtime_release_id.as_str())
        .bind(i64::from(new.slot.get()))
        .bind(new.state.relative_path.as_str())
        .bind(new.state.sha256.to_hex())
        .bind(new.state.size_bytes as i64)
        .bind(new.thumbnail.as_ref().map(|t| t.relative_path.as_str()))
        .bind(new.thumbnail.as_ref().map(|t| t.sha256.to_hex()))
        .bind(new.thumbnail.as_ref().map(|t| t.size_bytes as i64))
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await;

        match inserted {
            Ok(row) => Ok(SaveState {
                id: SaveStateId(row.get("id")),
                provenance: new.provenance.clone(),
                slot: new.slot,
                state: new.state.clone(),
                thumbnail: new.thumbnail.clone(),
                created_at: now,
                updated_at: now,
                status: SaveStateStatus::Available,
            }),
            // A concurrent reconciliation of the same session won the race. The row it wrote is
            // the same proved fact, so adopting it is the honest outcome.
            Err(error) if is_unique_violation(&error) => self
                .state_by_session_identity(new)
                .await?
                .ok_or_else(|| AppError::Database(error)),
            Err(error) => Err(AppError::Database(error)),
        }
    }

    /// Move one existing state onto newly proved physical content.
    ///
    /// This is the "the same core binary overwrote its own slot" case. Identity, slot, and every
    /// immutable core-provenance column stay exactly as they were — the SQL statement does not
    /// mention `core_binary_sha256`, `core_component_id`, `core_id`, or
    /// `originating_runtime_release_id` at all — and only the physical facts and the session that
    /// produced them move on.
    ///
    /// Conditioned on `status = 'available'`, so it can never resurrect a deleted or superseded row.
    pub async fn refresh_state(
        &self,
        id: SaveStateId,
        refreshed: &RefreshedSaveState,
    ) -> Result<Option<SaveState>, AppError> {
        let now = now_timestamp();
        let affected = sqlx::query(
            "UPDATE save_states SET \
                play_session_id = ?, state_sha256 = ?, state_size = ?, \
                thumbnail_relative_path = ?, thumbnail_sha256 = ?, thumbnail_size = ?, \
                updated_at = ? \
             WHERE id = ? AND status = 'available'",
        )
        .bind(refreshed.play_session_id.0)
        .bind(refreshed.state.sha256.to_hex())
        .bind(refreshed.state.size_bytes as i64)
        .bind(
            refreshed
                .thumbnail
                .as_ref()
                .map(|t| t.relative_path.as_str()),
        )
        .bind(refreshed.thumbnail.as_ref().map(|t| t.sha256.to_hex()))
        .bind(refreshed.thumbnail.as_ref().map(|t| t.size_bytes as i64))
        .bind(now)
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;

        if affected.rows_affected() == 0 {
            return Ok(None);
        }
        self.save_state(id).await
    }

    /// The registered physical content is provably gone, or no longer matches what was registered.
    pub async fn mark_missing(&self, id: SaveStateId) -> Result<bool, AppError> {
        self.close_lifecycle(id, SaveStateStatus::Missing).await
    }

    /// A controlled session proved the same physical path now carries content from a different
    /// core binary. The old object keeps its own provenance untouched.
    pub async fn mark_superseded(&self, id: SaveStateId) -> Result<bool, AppError> {
        self.close_lifecycle(id, SaveStateStatus::Superseded).await
    }

    /// RetroFrontier safely deleted the registered file after explicit user confirmation.
    ///
    /// `thumbnail_removed` records whether the thumbnail was *also* safely deleted. When it was
    /// not — the state deleted safely but the thumbnail could no longer be verified — the
    /// thumbnail identity is retained, because a file RetroFrontier deliberately left on disk
    /// must not be recorded as gone.
    pub async fn mark_deleted(
        &self,
        id: SaveStateId,
        thumbnail_removed: bool,
    ) -> Result<bool, AppError> {
        let now = now_timestamp();
        let affected = if thumbnail_removed {
            sqlx::query(
                "UPDATE save_states SET status = 'deleted', updated_at = ?, \
                    thumbnail_relative_path = NULL, thumbnail_sha256 = NULL, \
                    thumbnail_size = NULL \
                 WHERE id = ? AND status = 'available'",
            )
        } else {
            sqlx::query(
                "UPDATE save_states SET status = 'deleted', updated_at = ? \
                 WHERE id = ? AND status = 'available'",
            )
        }
        .bind(now)
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(affected.rows_affected() > 0)
    }

    async fn close_lifecycle(
        &self,
        id: SaveStateId,
        status: SaveStateStatus,
    ) -> Result<bool, AppError> {
        if status.is_available() {
            return Err(AppError::Library(
                "a save state cannot be closed as available".to_owned(),
            ));
        }
        let affected = sqlx::query(
            "UPDATE save_states SET status = ?, updated_at = ? \
             WHERE id = ? AND status = 'available'",
        )
        .bind(status.as_db())
        .bind(now_timestamp())
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(affected.rows_affected() > 0)
    }

    async fn state_by_session_identity(
        &self,
        new: &NewSaveState,
    ) -> Result<Option<SaveState>, AppError> {
        let row = sqlx::query(select_save_states!(
            "WHERE play_session_id = ? AND state_relative_path = ? AND state_sha256 = ?"
        ))
        .bind(new.provenance.play_session_id.0)
        .bind(new.state.relative_path.as_str())
        .bind(new.state.sha256.to_hex())
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        row.as_ref().map(save_state_from_row).transpose()
    }

    /// Persist one pre-launch baseline durably, replacing any leftover baseline for that session.
    ///
    /// One transaction, so a crash leaves either no baseline or a complete one. A half-written
    /// baseline would be indistinguishable from a state tree that really was smaller, which is
    /// exactly the false-attribution this exists to prevent.
    pub async fn put_baseline(
        &self,
        session_id: PlaySessionId,
        baseline: &LaunchStateBaseline,
    ) -> Result<(), AppError> {
        let mut transaction = self.pool.begin().await.map_err(AppError::Database)?;

        sqlx::query("DELETE FROM launch_state_baseline_entries WHERE play_session_id = ?")
            .bind(session_id.0)
            .execute(&mut *transaction)
            .await
            .map_err(AppError::Database)?;
        sqlx::query("DELETE FROM launch_state_baselines WHERE play_session_id = ?")
            .bind(session_id.0)
            .execute(&mut *transaction)
            .await
            .map_err(AppError::Database)?;

        sqlx::query(
            "INSERT INTO launch_state_baselines \
             (play_session_id, game_id, content_unit_id, core_id, core_component_id, \
              core_binary_sha256, core_display_version, core_source_revision, \
              runtime_installation_id, runtime_release_id, entry_count, attempts, captured_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(session_id.0)
        .bind(baseline.provenance.game_id.0)
        .bind(baseline.provenance.content_unit_id.0)
        .bind(baseline.provenance.core_id.as_str())
        .bind(baseline.provenance.core_component_id.as_str())
        .bind(baseline.provenance.core_binary_sha256.to_hex())
        .bind(baseline.provenance.core_display_version.as_deref())
        .bind(baseline.provenance.core_source_revision.as_deref())
        .bind(baseline.runtime_installation_id.as_str())
        .bind(baseline.provenance.originating_runtime_release_id.as_str())
        .bind(baseline.entries.len() as i64)
        .bind(i64::from(baseline.attempts))
        .bind(baseline.captured_at)
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        for entry in &baseline.entries {
            sqlx::query(
                "INSERT INTO launch_state_baseline_entries \
                 (play_session_id, relative_path, size_bytes, mtime_nanos, inode) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(session_id.0)
            .bind(entry.relative_path.as_str())
            .bind(entry.size_bytes as i64)
            // Stored as an exact decimal string: a modification time in nanoseconds does not fit
            // an `i64` for every conceivable filesystem timestamp, and only equality is ever
            // compared, so a lexical column is sufficient and lossless.
            .bind(entry.mtime_nanos.to_string())
            .bind(entry.inode as i64)
            .execute(&mut *transaction)
            .await
            .map_err(AppError::Database)?;
        }

        transaction.commit().await.map_err(AppError::Database)
    }

    pub async fn baseline(
        &self,
        session_id: PlaySessionId,
    ) -> Result<Option<LaunchStateBaseline>, AppError> {
        let Some(header) = sqlx::query(
            "SELECT game_id, content_unit_id, core_id, core_component_id, core_binary_sha256, \
                    core_display_version, core_source_revision, runtime_installation_id, \
                    runtime_release_id, attempts, captured_at \
             FROM launch_state_baselines WHERE play_session_id = ?",
        )
        .bind(session_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?
        else {
            return Ok(None);
        };

        let rows = sqlx::query(
            "SELECT relative_path, size_bytes, mtime_nanos, inode \
             FROM launch_state_baseline_entries WHERE play_session_id = ? \
             ORDER BY relative_path ASC",
        )
        .bind(session_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in &rows {
            entries.push(LaunchStateBaselineEntry {
                relative_path: relative_path(&row.get::<String, _>("relative_path"))?,
                size_bytes: row.get::<i64, _>("size_bytes") as u64,
                mtime_nanos: row
                    .get::<String, _>("mtime_nanos")
                    .parse::<i128>()
                    .map_err(|_| {
                        AppError::Library(
                            "a persisted baseline modification time is invalid".to_owned(),
                        )
                    })?,
                inode: row.get::<i64, _>("inode") as u64,
            });
        }

        Ok(Some(LaunchStateBaseline {
            provenance: SaveStateProvenance {
                game_id: GameId(header.get("game_id")),
                content_unit_id: ContentUnitId(header.get("content_unit_id")),
                play_session_id: session_id,
                core_id: core_id(&header.get::<String, _>("core_id"))?,
                core_component_id: safe_identifier(&header.get::<String, _>("core_component_id"))?,
                core_binary_sha256: digest(&header.get::<String, _>("core_binary_sha256"))?,
                core_display_version: header.get("core_display_version"),
                core_source_revision: header.get("core_source_revision"),
                originating_runtime_release_id: safe_identifier(
                    &header.get::<String, _>("runtime_release_id"),
                )?,
            },
            runtime_installation_id: safe_identifier(
                &header.get::<String, _>("runtime_installation_id"),
            )?,
            captured_at: header.get("captured_at"),
            attempts: header.get::<i64, _>("attempts").max(0) as u32,
            entries,
        }))
    }

    /// Every persisted baseline whose play session has already *certainly ended*.
    ///
    /// An open session is deliberately excluded: while a session is open the process may still be
    /// alive or of uncertain identity, and M9 performs no attribution in either case. This is the
    /// query startup reconciliation drives, which is what makes a baseline survive a
    /// RetroFrontier crash mid-session.
    pub async fn baselines_awaiting_reconciliation(&self) -> Result<Vec<PlaySessionId>, AppError> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT baselines.play_session_id \
             FROM launch_state_baselines AS baselines \
             JOIN play_sessions AS sessions ON sessions.id = baselines.play_session_id \
             WHERE sessions.outcome <> 'running' \
             ORDER BY baselines.play_session_id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(ids.into_iter().map(PlaySessionId).collect())
    }

    /// Whether any play session that could actually have changed the managed state tree started
    /// *after* this one.
    ///
    /// Launches are mutually exclusive, so a higher session id means another session was opened in
    /// between. That matters because the baseline delta model is only sound while the session that
    /// captured the baseline is the *only* thing that touched the state tree since: once a later
    /// session has written to it, a file absent from this baseline may equally have come from that
    /// later session, and the delta can no longer prove whose it is.
    ///
    /// A later session that never reached a managed process at all — `failed_to_start` with no
    /// `exit_code`, which is exactly the shape a durable baseline-capture failure, a process-record
    /// failure, or a spawn failure that never produced a child all leave — could not have written
    /// anything, so it does not count. Every other later session does: a `failed_to_start` session
    /// *with* an `exit_code` means a process really was created and reaped before its identity
    /// could be confirmed, and it may still have written a state in that brief window; `running`,
    /// `completed`, `crashed`, and `interrupted` all mean a process existed. This keeps an older
    /// baseline usable across a sibling launch attempt that never got anywhere near the state tree,
    /// without weakening the fail-closed rule for every session that plausibly could have.
    pub async fn session_was_superseded(
        &self,
        session_id: PlaySessionId,
    ) -> Result<bool, AppError> {
        let later: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM play_sessions \
             WHERE id > ? AND NOT (outcome = 'failed_to_start' AND exit_code IS NULL)",
        )
        .bind(session_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(later > 0)
    }

    pub async fn increment_baseline_attempts(
        &self,
        session_id: PlaySessionId,
    ) -> Result<u32, AppError> {
        let attempts: Option<i64> = sqlx::query_scalar(
            "UPDATE launch_state_baselines SET attempts = attempts + 1 \
             WHERE play_session_id = ? RETURNING attempts",
        )
        .bind(session_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(attempts.unwrap_or_default().max(0) as u32)
    }

    /// Remove a baseline once reconciliation reached a deterministic outcome.
    ///
    /// Entries are deleted before the header inside one transaction, so the project-wide
    /// no-cascade convention holds with no exception for M9's own aggregate.
    pub async fn delete_baseline(&self, session_id: PlaySessionId) -> Result<(), AppError> {
        let mut transaction = self.pool.begin().await.map_err(AppError::Database)?;
        sqlx::query("DELETE FROM launch_state_baseline_entries WHERE play_session_id = ?")
            .bind(session_id.0)
            .execute(&mut *transaction)
            .await
            .map_err(AppError::Database)?;
        sqlx::query("DELETE FROM launch_state_baselines WHERE play_session_id = ?")
            .bind(session_id.0)
            .execute(&mut *transaction)
            .await
            .map_err(AppError::Database)?;
        transaction.commit().await.map_err(AppError::Database)
    }
}

fn save_state_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<SaveState, AppError> {
    let thumbnail_path: Option<String> = row.get("thumbnail_relative_path");
    let thumbnail_sha256: Option<String> = row.get("thumbnail_sha256");
    let thumbnail_size: Option<i64> = row.get("thumbnail_size");
    let thumbnail = match (thumbnail_path, thumbnail_sha256, thumbnail_size) {
        (Some(path), Some(sha256), Some(size)) => Some(SaveStateThumbnailIdentity {
            relative_path: relative_path(&path)?,
            sha256: digest(&sha256)?,
            size_bytes: size as u64,
        }),
        (None, None, None) => None,
        // The database check constraint makes this unreachable; refusing rather than guessing
        // keeps a partially proved thumbnail from ever being presented as proved.
        _ => {
            return Err(AppError::Library(
                "a persisted save-state thumbnail identity is incomplete".to_owned(),
            ))
        }
    };

    let slot = u16::try_from(row.get::<i64, _>("slot"))
        .ok()
        .and_then(|slot| SaveStateSlot::new(slot).ok())
        .ok_or_else(|| {
            AppError::Library("a persisted save state has an unmanaged slot".to_owned())
        })?;
    let status = SaveStateStatus::from_db(&row.get::<String, _>("status")).ok_or_else(|| {
        AppError::Library("a persisted save state has an unknown status".to_owned())
    })?;

    Ok(SaveState {
        id: SaveStateId(row.get("id")),
        provenance: SaveStateProvenance {
            game_id: GameId(row.get("game_id")),
            content_unit_id: ContentUnitId(row.get("content_unit_id")),
            play_session_id: PlaySessionId(row.get("play_session_id")),
            core_id: core_id(&row.get::<String, _>("core_id"))?,
            core_component_id: safe_identifier(&row.get::<String, _>("core_component_id"))?,
            core_binary_sha256: digest(&row.get::<String, _>("core_binary_sha256"))?,
            core_display_version: row.get("core_display_version"),
            core_source_revision: row.get("core_source_revision"),
            originating_runtime_release_id: safe_identifier(
                &row.get::<String, _>("originating_runtime_release_id"),
            )?,
        },
        slot,
        state: SaveStateFileIdentity {
            relative_path: relative_path(&row.get::<String, _>("state_relative_path"))?,
            sha256: digest(&row.get::<String, _>("state_sha256"))?,
            size_bytes: row.get::<i64, _>("state_size") as u64,
        },
        thumbnail,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        status,
    })
}

/// A persisted path is re-validated on the way out, so a row written by an older or tampered
/// process cannot hand an unsafe path to the filesystem adapter.
fn relative_path(value: &str) -> Result<RelativePath, AppError> {
    RelativePath::new(value).map_err(|_| {
        AppError::Library("a persisted save-state path is not a safe relative path".to_owned())
    })
}

fn digest(value: &str) -> Result<Sha256Digest, AppError> {
    Sha256Digest::from_hex(value)
        .map_err(|_| AppError::Library("a persisted save-state digest is invalid".to_owned()))
}

fn core_id(value: &str) -> Result<CoreId, AppError> {
    CoreId::new(value)
        .map_err(|_| AppError::Library("a persisted core identifier is invalid".to_owned()))
}

fn safe_identifier(value: &str) -> Result<SafeIdentifier, AppError> {
    SafeIdentifier::new(value)
        .map_err(|_| AppError::Library("a persisted managed identifier is invalid".to_owned()))
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(database) if database.is_unique_violation())
}

fn now_timestamp() -> UnixTimestamp {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{NewSaveState, RefreshedSaveState, SaveStateRepository};
    use crate::adapters::database::Database;
    use crate::domain::core::CoreId;
    use crate::domain::launch::{PlaySessionId, PlaySessionOutcome};
    use crate::domain::library::{ContentUnitId, GameId};
    use crate::domain::runtime::{RelativePath, SafeIdentifier, Sha256Digest};
    use crate::domain::save_state::{
        LaunchStateBaseline, LaunchStateBaselineEntry, SaveStateFileIdentity, SaveStateId,
        SaveStateProvenance, SaveStateSlot, SaveStateStatus, SaveStateThumbnailIdentity,
    };
    use crate::repositories::launch::{LaunchRepository, NewPlaySession};
    use sqlx::SqlitePool;
    use tempfile::TempDir;

    const TEST_TIME: i64 = 1_756_900_000_000;

    /// Two content units on one game, so multi-disc provenance is exercised, plus a second game.
    async fn fixture() -> (TempDir, SqlitePool) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let database = Database::open(directory.path().join("save-states.sqlite3"))
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
        for (id, system, title) in [
            (1_i64, "playstation", "Synthetic Disc Set"),
            (2, "nes", "Other"),
        ] {
            sqlx::query(
                "INSERT INTO games (id, system_id, local_title, availability, created_at, \
                 updated_at) VALUES (?, ?, ?, 'available', ?, ?)",
            )
            .bind(id)
            .bind(system)
            .bind(title)
            .bind(TEST_TIME)
            .bind(TEST_TIME)
            .execute(&pool)
            .await
            .expect("synthetic game");
        }
        for (id, game_id, system, title, path) in [
            (1_i64, 1_i64, "playstation", "Disc 1", "PSX/disc1.cue"),
            (2, 1, "playstation", "Disc 2", "PSX/disc2.cue"),
            (3, 2, "nes", "Other", "NES/other.nes"),
        ] {
            sqlx::query(
                "INSERT INTO content_units \
                 (id, game_id, root_id, system_id, kind, local_title, primary_relative_path, \
                  fingerprint, availability, created_at, updated_at) \
                 VALUES (?, ?, 1, ?, 'cue_bin', ?, ?, NULL, 'available', ?, ?)",
            )
            .bind(id)
            .bind(game_id)
            .bind(system)
            .bind(title)
            .bind(path)
            .bind(TEST_TIME)
            .bind(TEST_TIME)
            .execute(&pool)
            .await
            .expect("synthetic content unit");
        }
        (directory, pool)
    }

    async fn session(pool: &SqlitePool, content_unit_id: i64) -> PlaySessionId {
        LaunchRepository::new(pool.clone())
            .start_session(&NewPlaySession {
                game_id: GameId(if content_unit_id == 3 { 2 } else { 1 }),
                content_unit_id: ContentUnitId(content_unit_id),
                core_id: CoreId::new("beetle-psx").unwrap(),
                runtime_installation_id: "install-1".to_owned(),
                runtime_release_id: "release-2".to_owned(),
            })
            .await
            .expect("synthetic play session")
            .id
    }

    async fn close(pool: &SqlitePool, id: PlaySessionId, outcome: PlaySessionOutcome) {
        LaunchRepository::new(pool.clone())
            .complete_session(id, outcome, Some(0))
            .await
            .expect("session should close");
    }

    fn digest(seed: char) -> Sha256Digest {
        Sha256Digest::from_hex(&seed.to_string().repeat(64)).unwrap()
    }

    fn provenance(session_id: PlaySessionId, core_binary: char) -> SaveStateProvenance {
        SaveStateProvenance {
            game_id: GameId(1),
            content_unit_id: ContentUnitId(1),
            play_session_id: session_id,
            core_id: CoreId::new("beetle-psx").unwrap(),
            core_component_id: SafeIdentifier::new("beetle-psx").unwrap(),
            core_binary_sha256: digest(core_binary),
            core_display_version: Some("0.9.44.1".to_owned()),
            core_source_revision: Some("abc1234".to_owned()),
            originating_runtime_release_id: SafeIdentifier::new("release-2").unwrap(),
        }
    }

    fn new_state(session_id: PlaySessionId, slot: u16, path: &str, content: char) -> NewSaveState {
        NewSaveState {
            provenance: provenance(session_id, 'a'),
            slot: SaveStateSlot::new(slot).unwrap(),
            state: SaveStateFileIdentity {
                relative_path: RelativePath::new(path).unwrap(),
                sha256: digest(content),
                size_bytes: 4096,
            },
            thumbnail: None,
        }
    }

    // ---------------------------------------------------------------- migration

    #[tokio::test]
    async fn the_migration_creates_every_table_with_restrictive_foreign_keys_and_no_cascade() {
        let (_directory, pool) = fixture().await;

        for (table, expected) in [
            ("save_states", 3),
            ("launch_state_baselines", 3),
            ("launch_state_baseline_entries", 1),
        ] {
            let actions: Vec<String> =
                sqlx::query_scalar("SELECT \"on_delete\" FROM pragma_foreign_key_list(?)")
                    .bind(table)
                    .fetch_all(&pool)
                    .await
                    .expect("foreign key list");
            assert_eq!(actions.len(), expected, "{table} foreign keys");
            assert!(
                actions.iter().all(|action| action == "RESTRICT"),
                "{table} must not cascade deletes: {actions:?}"
            );
        }
    }

    #[tokio::test]
    async fn only_manual_slots_between_one_and_nine_hundred_and_ninety_nine_can_be_persisted() {
        let (_directory, pool) = fixture().await;
        let session_id = session(&pool, 1).await;

        for slot in [0_i64, 1000, -1] {
            let refused =
                insert_raw(&pool, session_id, slot, "PSX/disc1.state1", "available").await;
            assert!(
                refused.is_err(),
                "slot {slot} must be refused by the database"
            );
        }
        for slot in [1_i64, 999] {
            let path = format!("PSX/disc1.state{slot}");
            insert_raw(&pool, session_id, slot, &path, "available")
                .await
                .unwrap_or_else(|error| panic!("slot {slot} must be accepted: {error}"));
        }
    }

    #[tokio::test]
    async fn the_database_refuses_an_unknown_status_and_a_partially_proved_thumbnail() {
        let (_directory, pool) = fixture().await;
        let session_id = session(&pool, 1).await;

        assert!(
            insert_raw(&pool, session_id, 1, "PSX/disc1.state1", "corrupt")
                .await
                .is_err(),
            "there is no `corrupt` lifecycle value"
        );

        // A thumbnail is proved as a whole or not at all.
        for (path, sha, size) in [
            (Some("PSX/disc1.state1.png"), None, None),
            (None, Some(digest('b').to_hex()), None),
            (
                Some("PSX/disc1.state1.png"),
                Some(digest('b').to_hex()),
                None,
            ),
            (Some("PSX/disc1.state1.png"), None, Some(9_i64)),
        ] {
            let refused = sqlx::query(
                "INSERT INTO save_states \
                 (game_id, content_unit_id, play_session_id, core_id, core_component_id, \
                  core_binary_sha256, originating_runtime_release_id, slot, state_relative_path, \
                  state_sha256, state_size, thumbnail_relative_path, thumbnail_sha256, \
                  thumbnail_size, status, created_at, updated_at) \
                 VALUES (1, 1, ?, 'beetle-psx', 'beetle-psx', ?, 'release-2', 1, \
                         'PSX/disc1.state1', ?, 4096, ?, ?, ?, 'available', ?, ?)",
            )
            .bind(session_id.0)
            .bind(digest('a').to_hex())
            .bind(digest('c').to_hex())
            .bind(path)
            .bind(sha)
            .bind(size)
            .bind(TEST_TIME)
            .bind(TEST_TIME)
            .execute(&pool)
            .await;
            assert!(
                refused.is_err(),
                "a partial thumbnail identity must be refused"
            );
        }
    }

    #[tokio::test]
    async fn one_session_may_register_one_exact_physical_identity_only_once() {
        let (_directory, pool) = fixture().await;
        let session_id = session(&pool, 1).await;
        let repository = SaveStateRepository::new(pool.clone());
        let state = new_state(session_id, 1, "PSX/disc1.state1", 'c');

        let first = repository.register_state(&state).await.unwrap();
        // Replaying a completed reconciliation is a no-op, not a second registration.
        let replayed = repository.register_state(&state).await.unwrap();
        assert_eq!(first, replayed);

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM save_states")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    /// LOW-2 regression: a later session that never reached a managed process at all — the exact
    /// shape a baseline-capture failure, a process-record failure, or a spawn failure that never
    /// produced a child all leave — must not make an older, still-indeterminate baseline
    /// non-attributable. It never touched the state tree, so there is nothing for it to have
    /// superseded.
    #[tokio::test]
    async fn a_later_session_that_never_spawned_does_not_supersede_an_older_baseline() {
        let (_directory, pool) = fixture().await;
        let repository = SaveStateRepository::new(pool.clone());
        let older = session(&pool, 1).await;
        let never_spawned = session(&pool, 1).await;
        assert!(
            never_spawned.0 > older.0,
            "the later session has a higher id"
        );

        sqlx::query(
            "UPDATE play_sessions SET outcome = 'failed_to_start', exit_code = NULL, \
             ended_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .bind(never_spawned.0)
        .execute(&pool)
        .await
        .expect("the never-spawned session should close");

        assert!(!repository
            .session_was_superseded(older)
            .await
            .expect("the query should succeed"));
    }

    /// The converse: every later session that could plausibly have touched the state tree still
    /// supersedes, including the one case that also ends `failed_to_start` — a process that really
    /// was spawned and reaped, however briefly, before its identity could be confirmed.
    #[tokio::test]
    async fn every_later_session_that_could_have_touched_the_tree_still_supersedes() {
        for (outcome, exit_code) in [
            (PlaySessionOutcome::Completed, Some(0_i64)),
            (PlaySessionOutcome::Crashed, Some(1)),
            (PlaySessionOutcome::Interrupted, None),
            // Spawned and reaped before identity capture could confirm it "started" — still a
            // real process that may have written a state before it died.
            (PlaySessionOutcome::FailedToStart, Some(0)),
        ] {
            let (_directory, pool) = fixture().await;
            let repository = SaveStateRepository::new(pool.clone());
            let older = session(&pool, 1).await;
            let later = session(&pool, 1).await;

            sqlx::query(
                "UPDATE play_sessions SET outcome = ?, exit_code = ?, ended_at = ?, \
                 updated_at = ? WHERE id = ?",
            )
            .bind(outcome.as_db())
            .bind(exit_code)
            .bind(TEST_TIME)
            .bind(TEST_TIME)
            .bind(later.0)
            .execute(&pool)
            .await
            .expect("the later session should close");

            assert!(
                repository
                    .session_was_superseded(older)
                    .await
                    .expect("the query should succeed"),
                "{outcome:?} with exit_code {exit_code:?} must still supersede"
            );
        }
    }

    #[tokio::test]
    async fn only_one_available_row_may_claim_one_physical_path() {
        let (_directory, pool) = fixture().await;
        let first_session = session(&pool, 1).await;
        let second_session = session(&pool, 1).await;
        let repository = SaveStateRepository::new(pool.clone());
        let path = "PSX/disc1.state1";

        let first = repository
            .register_state(&new_state(first_session, 1, path, 'c'))
            .await
            .unwrap();

        // A second *available* claim on the same file is refused outright.
        let refused = repository
            .register_state(&new_state(second_session, 1, path, 'd'))
            .await;
        assert!(refused.is_err());

        // Superseding the predecessor is what makes room, and both rows then coexist as history.
        assert!(repository.mark_superseded(first.id).await.unwrap());
        let second = repository
            .register_state(&new_state(second_session, 1, path, 'd'))
            .await
            .unwrap();

        assert_ne!(first.id, second.id);
        assert_eq!(
            repository
                .save_state(first.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            SaveStateStatus::Superseded
        );
        assert_eq!(
            repository
                .save_state(second.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            SaveStateStatus::Available
        );
    }

    // ---------------------------------------------------------------- provenance

    #[tokio::test]
    async fn complete_provenance_survives_a_round_trip_exactly() {
        let (_directory, pool) = fixture().await;
        let session_id = session(&pool, 2).await;
        let repository = SaveStateRepository::new(pool);

        let mut state = new_state(session_id, 7, "beetle-psx/Synthetic Disc Set.state7", 'e');
        state.provenance.content_unit_id = ContentUnitId(2);
        state.thumbnail = Some(SaveStateThumbnailIdentity {
            relative_path: RelativePath::new("beetle-psx/Synthetic Disc Set.state7.png").unwrap(),
            sha256: digest('f'),
            size_bytes: 12_345,
        });

        let registered = repository.register_state(&state).await.unwrap();
        let reloaded = repository
            .save_state(registered.id)
            .await
            .unwrap()
            .expect("the row should be readable");

        assert_eq!(reloaded, registered);
        assert_eq!(reloaded.provenance, state.provenance);
        assert_eq!(reloaded.slot.get(), 7);
        assert_eq!(reloaded.state, state.state);
        assert_eq!(reloaded.thumbnail, state.thumbnail);
        assert_eq!(reloaded.status, SaveStateStatus::Available);
        // The exact content unit is bound, so a Disc 1 state can never be offered as Disc 2.
        assert_eq!(reloaded.provenance.content_unit_id, ContentUnitId(2));
    }

    #[tokio::test]
    async fn no_repository_method_rewrites_immutable_core_binary_provenance() {
        // The statements themselves are the guarantee: nothing that runs UPDATE mentions the
        // immutable provenance columns.
        let source = include_str!("save_state.rs");
        let production = source.split_once("#[cfg(test)]").unwrap().0;
        for statement in production.split("UPDATE save_states").skip(1) {
            let statement = statement.split("WHERE").next().unwrap_or_default();
            for immutable in [
                "core_binary_sha256",
                "core_component_id",
                "core_id",
                "originating_runtime_release_id",
                "slot",
                "game_id",
                "content_unit_id",
                "state_relative_path",
                "created_at",
            ] {
                assert!(
                    !statement.contains(immutable),
                    "an UPDATE must never write {immutable}"
                );
            }
        }
    }

    // ---------------------------------------------------------------- lifecycle

    #[tokio::test]
    async fn every_lifecycle_transition_leaves_available_exactly_once() {
        let (_directory, pool) = fixture().await;
        let repository = SaveStateRepository::new(pool.clone());

        for (index, transition) in [
            SaveStateStatus::Missing,
            SaveStateStatus::Superseded,
            SaveStateStatus::Deleted,
        ]
        .into_iter()
        .enumerate()
        {
            let session_id = session(&pool, 1).await;
            let slot = (index + 1) as u16;
            let path = format!("beetle-psx/Synthetic.state{slot}");
            let state = repository
                .register_state(&new_state(session_id, slot, &path, 'c'))
                .await
                .unwrap();

            let applied = match transition {
                SaveStateStatus::Missing => repository.mark_missing(state.id).await.unwrap(),
                SaveStateStatus::Superseded => repository.mark_superseded(state.id).await.unwrap(),
                SaveStateStatus::Deleted => repository.mark_deleted(state.id, true).await.unwrap(),
                SaveStateStatus::Available => unreachable!(),
            };
            assert!(applied, "{transition:?} should apply to an available row");
            assert_eq!(
                repository
                    .save_state(state.id)
                    .await
                    .unwrap()
                    .unwrap()
                    .status,
                transition
            );

            // A closed lifecycle value is never reopened, and no second verdict overwrites the
            // first — whichever order two reconciliations happen to arrive in.
            assert!(!repository.mark_missing(state.id).await.unwrap());
            assert!(!repository.mark_superseded(state.id).await.unwrap());
            assert!(!repository.mark_deleted(state.id, true).await.unwrap());
            assert!(repository
                .refresh_state(
                    state.id,
                    &RefreshedSaveState {
                        play_session_id: session_id,
                        state: SaveStateFileIdentity {
                            relative_path: RelativePath::new(&path).unwrap(),
                            sha256: digest('d'),
                            size_bytes: 1,
                        },
                        thumbnail: None,
                    },
                )
                .await
                .unwrap()
                .is_none());
            assert_eq!(
                repository
                    .save_state(state.id)
                    .await
                    .unwrap()
                    .unwrap()
                    .status,
                transition
            );
        }
    }

    #[tokio::test]
    async fn a_refresh_keeps_identity_and_immutable_provenance_and_moves_only_physical_facts() {
        let (_directory, pool) = fixture().await;
        let first_session = session(&pool, 1).await;
        let second_session = session(&pool, 1).await;
        let repository = SaveStateRepository::new(pool);
        let path = "beetle-psx/Synthetic.state1";

        let original = repository
            .register_state(&new_state(first_session, 1, path, 'c'))
            .await
            .unwrap();

        let refreshed = repository
            .refresh_state(
                original.id,
                &RefreshedSaveState {
                    play_session_id: second_session,
                    state: SaveStateFileIdentity {
                        relative_path: RelativePath::new(path).unwrap(),
                        sha256: digest('d'),
                        size_bytes: 8192,
                    },
                    thumbnail: Some(SaveStateThumbnailIdentity {
                        relative_path: RelativePath::new("beetle-psx/Synthetic.state1.png")
                            .unwrap(),
                        sha256: digest('e'),
                        size_bytes: 77,
                    }),
                },
            )
            .await
            .unwrap()
            .expect("an available row should refresh");

        assert_eq!(refreshed.id, original.id);
        assert_eq!(refreshed.slot, original.slot);
        assert_eq!(refreshed.created_at, original.created_at);
        assert_eq!(
            refreshed.provenance.core_binary_sha256,
            original.provenance.core_binary_sha256
        );
        assert_eq!(refreshed.provenance.core_id, original.provenance.core_id);
        assert_eq!(
            refreshed.provenance.originating_runtime_release_id,
            original.provenance.originating_runtime_release_id
        );
        // Only the physical facts and the session that produced them moved.
        assert_eq!(refreshed.state.sha256, digest('d'));
        assert_eq!(refreshed.state.size_bytes, 8192);
        assert_eq!(refreshed.provenance.play_session_id, second_session);
        assert!(refreshed.updated_at >= original.updated_at);
        assert!(refreshed.thumbnail.is_some());
    }

    #[tokio::test]
    async fn deleting_a_state_keeps_a_thumbnail_that_was_deliberately_left_on_disk() {
        let (_directory, pool) = fixture().await;
        let session_id = session(&pool, 1).await;
        let repository = SaveStateRepository::new(pool);
        let mut state = new_state(session_id, 1, "beetle-psx/Synthetic.state1", 'c');
        state.thumbnail = Some(SaveStateThumbnailIdentity {
            relative_path: RelativePath::new("beetle-psx/Synthetic.state1.png").unwrap(),
            sha256: digest('e'),
            size_bytes: 77,
        });
        let registered = repository.register_state(&state).await.unwrap();

        // The thumbnail could not be safely verified, so RetroFrontier left it alone. Recording it
        // as gone would be a lie about a file that is still there.
        assert!(repository.mark_deleted(registered.id, false).await.unwrap());
        let reloaded = repository.save_state(registered.id).await.unwrap().unwrap();
        assert_eq!(reloaded.status, SaveStateStatus::Deleted);
        assert_eq!(reloaded.thumbnail, state.thumbnail);
    }

    #[tokio::test]
    async fn the_game_listing_returns_only_available_rows_most_recently_updated_first() {
        let (_directory, pool) = fixture().await;
        let repository = SaveStateRepository::new(pool.clone());

        let mut registered = Vec::new();
        for slot in 1_u16..=4 {
            let session_id = session(&pool, 1).await;
            let path = format!("beetle-psx/Synthetic.state{slot}");
            registered.push(
                repository
                    .register_state(&new_state(session_id, slot, &path, 'c'))
                    .await
                    .unwrap(),
            );
        }
        // Deliberately not in insertion order, so ordering cannot come from the id by accident.
        for (state, updated_at) in registered.iter().zip([30_i64, 10, 40, 20]) {
            sqlx::query("UPDATE save_states SET updated_at = ? WHERE id = ?")
                .bind(TEST_TIME + updated_at)
                .bind(state.id.0)
                .execute(&pool)
                .await
                .unwrap();
        }
        repository.mark_missing(registered[2].id).await.unwrap();

        let listed = repository.save_states_for_game(GameId(1)).await.unwrap();

        assert_eq!(
            listed
                .iter()
                .map(|state| state.slot.get())
                .collect::<Vec<_>>(),
            vec![1, 4, 2],
            "available rows only, updated_at DESC"
        );
        // Another game's states never appear.
        assert!(repository
            .save_states_for_game(GameId(2))
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_state_can_be_found_by_the_physical_path_it_currently_claims() {
        let (_directory, pool) = fixture().await;
        let session_id = session(&pool, 1).await;
        let repository = SaveStateRepository::new(pool);
        let path = RelativePath::new("beetle-psx/Synthetic.state1").unwrap();

        assert!(repository
            .available_state_at_path(&path)
            .await
            .unwrap()
            .is_none());
        let registered = repository
            .register_state(&new_state(session_id, 1, path.as_str(), 'c'))
            .await
            .unwrap();
        assert_eq!(
            repository
                .available_state_at_path(&path)
                .await
                .unwrap()
                .map(|state| state.id),
            Some(registered.id)
        );
        assert_eq!(repository.available_states().await.unwrap().len(), 1);

        // Once it is history, it no longer claims the file.
        repository.mark_missing(registered.id).await.unwrap();
        assert!(repository
            .available_state_at_path(&path)
            .await
            .unwrap()
            .is_none());
        assert!(repository.available_states().await.unwrap().is_empty());
    }

    // ---------------------------------------------------------------- baselines

    fn baseline(
        session_id: PlaySessionId,
        entries: Vec<(&str, u64, i128, u64)>,
    ) -> LaunchStateBaseline {
        LaunchStateBaseline {
            provenance: provenance(session_id, 'a'),
            runtime_installation_id: SafeIdentifier::new("install-1").unwrap(),
            captured_at: TEST_TIME,
            attempts: 0,
            entries: entries
                .into_iter()
                .map(
                    |(path, size_bytes, mtime_nanos, inode)| LaunchStateBaselineEntry {
                        relative_path: RelativePath::new(path).unwrap(),
                        size_bytes,
                        mtime_nanos,
                        inode,
                    },
                )
                .collect(),
        }
    }

    #[tokio::test]
    async fn a_baseline_round_trips_and_survives_a_fresh_repository_over_the_same_file() {
        let (directory, pool) = fixture().await;
        let session_id = session(&pool, 1).await;
        let repository = SaveStateRepository::new(pool.clone());
        let original = baseline(
            session_id,
            vec![
                (
                    "beetle-psx/Synthetic.state1",
                    4096,
                    1_756_900_000_123_456_789,
                    4242,
                ),
                (
                    "beetle-psx/Synthetic.state2",
                    8192,
                    1_756_900_000_987_654_321,
                    4243,
                ),
            ],
        );

        repository
            .put_baseline(session_id, &original)
            .await
            .unwrap();
        assert_eq!(
            repository.baseline(session_id).await.unwrap(),
            Some(original.clone())
        );

        // A RetroFrontier restart: a brand-new pool over the same database file still finds it.
        drop(pool);
        let reopened = Database::open(directory.path().join("save-states.sqlite3"))
            .await
            .expect("database should reopen");
        let restarted = SaveStateRepository::new(reopened.pool().clone());
        assert_eq!(
            restarted.baseline(session_id).await.unwrap(),
            Some(original)
        );
    }

    #[tokio::test]
    async fn putting_a_baseline_twice_replaces_it_rather_than_merging_two_state_trees() {
        let (_directory, pool) = fixture().await;
        let session_id = session(&pool, 1).await;
        let repository = SaveStateRepository::new(pool);

        repository
            .put_baseline(
                session_id,
                &baseline(
                    session_id,
                    vec![("a/x.state1", 1, 1, 1), ("a/y.state2", 2, 2, 2)],
                ),
            )
            .await
            .unwrap();
        let replacement = baseline(session_id, vec![("a/z.state3", 3, 3, 3)]);
        repository
            .put_baseline(session_id, &replacement)
            .await
            .unwrap();

        assert_eq!(
            repository.baseline(session_id).await.unwrap(),
            Some(replacement)
        );
    }

    #[tokio::test]
    async fn only_a_baseline_whose_session_certainly_ended_awaits_reconciliation() {
        let (_directory, pool) = fixture().await;
        let repository = SaveStateRepository::new(pool.clone());
        let open = session(&pool, 1).await;
        let completed = session(&pool, 1).await;
        let crashed = session(&pool, 1).await;
        let interrupted = session(&pool, 1).await;
        for id in [open, completed, crashed, interrupted] {
            repository
                .put_baseline(id, &baseline(id, vec![("a/x.state1", 1, 1, 1)]))
                .await
                .unwrap();
        }

        // An open session may still have a live or uncertain process, so it is never reconciled.
        close(&pool, completed, PlaySessionOutcome::Completed).await;
        // A RetroArch crash is not by itself a reason to discard the delta.
        close(&pool, crashed, PlaySessionOutcome::Crashed).await;
        // Neither is a process that was only proven absent after a restart.
        close(&pool, interrupted, PlaySessionOutcome::Interrupted).await;

        assert_eq!(
            repository
                .baselines_awaiting_reconciliation()
                .await
                .unwrap(),
            vec![completed, crashed, interrupted]
        );
    }

    #[tokio::test]
    async fn baseline_attempts_are_bounded_and_a_baseline_can_be_removed() {
        let (_directory, pool) = fixture().await;
        let session_id = session(&pool, 1).await;
        let repository = SaveStateRepository::new(pool);
        repository
            .put_baseline(
                session_id,
                &baseline(session_id, vec![("a/x.state1", 1, 1, 1)]),
            )
            .await
            .unwrap();

        assert_eq!(
            repository
                .increment_baseline_attempts(session_id)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            repository
                .increment_baseline_attempts(session_id)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            repository
                .baseline(session_id)
                .await
                .unwrap()
                .unwrap()
                .attempts,
            2
        );

        repository.delete_baseline(session_id).await.unwrap();
        assert!(repository.baseline(session_id).await.unwrap().is_none());
        // Deleting an absent baseline is a no-op, so a retried reconciliation is safe.
        repository.delete_baseline(session_id).await.unwrap();
        assert_eq!(
            repository
                .increment_baseline_attempts(session_id)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn a_persisted_row_with_an_unsafe_path_or_slot_is_refused_rather_than_handed_onward() {
        let (_directory, pool) = fixture().await;
        let session_id = session(&pool, 1).await;
        let repository = SaveStateRepository::new(pool.clone());

        // The database check cannot express a path rule, so the read boundary re-validates. A row
        // written by an older or tampered process must never hand an unsafe path to the adapter.
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO save_states \
             (game_id, content_unit_id, play_session_id, core_id, core_component_id, \
              core_binary_sha256, originating_runtime_release_id, slot, state_relative_path, \
              state_sha256, state_size, status, created_at, updated_at) \
             VALUES (1, 1, ?, 'beetle-psx', 'beetle-psx', ?, 'release-2', 1, \
                     '../escape.state1', ?, 4096, 'available', ?, ?) RETURNING id",
        )
        .bind(session_id.0)
        .bind(digest('a').to_hex())
        .bind(digest('c').to_hex())
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(repository.save_state(SaveStateId(id)).await.is_err());
        assert!(repository.save_states_for_game(GameId(1)).await.is_err());
        assert!(repository.available_states().await.is_err());
    }

    #[tokio::test]
    async fn save_state_history_survives_a_scan_that_would_remove_local_content() {
        let (_directory, pool) = fixture().await;
        let session_id = session(&pool, 1).await;
        let repository = SaveStateRepository::new(pool.clone());
        repository
            .register_state(&new_state(
                session_id,
                1,
                "beetle-psx/Synthetic.state1",
                'c',
            ))
            .await
            .unwrap();

        sqlx::query("UPDATE games SET availability = 'unavailable' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE content_units SET availability = 'missing' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(repository.available_states().await.unwrap().len(), 1);
        // Restrictive deletes keep provenance even when the scanner would remove local content.
        for statement in [
            "DELETE FROM games WHERE id = 1",
            "DELETE FROM content_units WHERE id = 1",
            "DELETE FROM play_sessions WHERE id = 1",
        ] {
            assert!(
                sqlx::query(statement).execute(&pool).await.is_err(),
                "{statement} must be refused"
            );
        }
    }

    async fn insert_raw(
        pool: &SqlitePool,
        session_id: PlaySessionId,
        slot: i64,
        path: &str,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO save_states \
             (game_id, content_unit_id, play_session_id, core_id, core_component_id, \
              core_binary_sha256, originating_runtime_release_id, slot, state_relative_path, \
              state_sha256, state_size, status, created_at, updated_at) \
             VALUES (1, 1, ?, 'beetle-psx', 'beetle-psx', ?, 'release-2', ?, ?, ?, 4096, ?, ?, ?)",
        )
        .bind(session_id.0)
        .bind(digest('a').to_hex())
        .bind(slot)
        .bind(path)
        .bind(digest('c').to_hex())
        .bind(status)
        .bind(TEST_TIME)
        .bind(TEST_TIME)
        .execute(pool)
        .await
        .map(|_| ())
    }
}
