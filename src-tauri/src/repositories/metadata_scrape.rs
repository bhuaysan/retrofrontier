//! Persistence for user-initiated metadata scrape runs.
//!
//! Every write here is short and local. No provider or network operation ever happens inside one of
//! these transactions: the feeder only moves rows between `metadata_scrape_run_items` and
//! `metadata_jobs`, and the M5 worker does the talking afterwards.
//!
//! The feeder deliberately writes both tables in one transaction. Splitting them would open a crash
//! window in which a job exists that no run item is waiting for, or a run item is marked queued
//! with no job behind it; the first wastes provider quota, the second hangs the run. Atomicity
//! across the two tables is the guarantee, so the SQL for it lives together here rather than being
//! split across two repositories for tidiness.

use crate::domain::library::{GameId, UnixTimestamp};
use crate::domain::metadata::{MetadataProviderId, ProviderMatchStatus};
use crate::domain::metadata_scrape::{
    MetadataJobBand, MetadataScrapeItemFacts, MetadataScrapeItemState, MetadataScrapeMode,
    MetadataScrapeProgress, MetadataScrapeRun, MetadataScrapeRunId, MetadataScrapeRunStatus,
};
use crate::error::AppError;
use sqlx::{Row, SqlitePool};

#[derive(Clone)]
pub struct MetadataScrapeRepository {
    pool: SqlitePool,
}

impl MetadataScrapeRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // ------------------------------------------------------------------------------- eligibility

    /// Games a run in `mode` would target if it started now.
    ///
    /// The two predicates are deliberately narrow.
    ///
    /// *Missing* means untouched: no provider relationship and no metadata job of any kind. A
    /// no-match, an ambiguous candidate set, an unsupported shape and a parked failure are all
    /// answers, so a repeated run does not re-ask the provider about them.
    ///
    /// *Refresh* means an accepted match that still names a provider game, which is exactly the
    /// condition the existing per-game refresh action requires before it will refresh rather than
    /// re-identify.
    pub async fn count_eligible_games(
        &self,
        provider_id: MetadataProviderId,
        mode: MetadataScrapeMode,
    ) -> Result<i64, AppError> {
        match mode {
            MetadataScrapeMode::MissingMetadata => sqlx::query_scalar(
                "SELECT COUNT(*) FROM games g \
                 LEFT JOIN provider_matches pm ON pm.game_id = g.id AND pm.provider_id = ? \
                 LEFT JOIN metadata_jobs mj ON mj.game_id = g.id AND mj.provider_id = ? \
                 WHERE pm.id IS NULL AND mj.id IS NULL",
            )
            .bind(provider_id.as_db())
            .bind(provider_id.as_db()),
            MetadataScrapeMode::RefreshMatched => sqlx::query_scalar(
                "SELECT COUNT(*) FROM provider_matches pm WHERE pm.provider_id = ? \
                 AND pm.status = 'matched' AND pm.provider_game_id IS NOT NULL",
            )
            .bind(provider_id.as_db()),
        }
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)
    }

    // ------------------------------------------------------------------------------ run creation

    /// Creates a run and freezes its target set in one transaction.
    ///
    /// Membership is captured once and never appended to. A library scan that discovers new games
    /// while this run is in flight leaves them for a future run, so an active run cannot grow
    /// without bound as the library changes.
    ///
    /// Returns `None` when the provider already has an active run: the partial unique index rejects
    /// the insert, which is what makes the one-active-run rule a database invariant rather than an
    /// application-level check two concurrent callers could both pass.
    pub async fn create_run(
        &self,
        provider_id: MetadataProviderId,
        mode: MetadataScrapeMode,
        now: UnixTimestamp,
    ) -> Result<Option<MetadataScrapeRunId>, AppError> {
        let mut transaction = self.pool.begin().await.map_err(AppError::Database)?;

        let inserted: Option<i64> = sqlx::query_scalar(
            "INSERT INTO metadata_scrape_runs \
             (provider_id, mode, status, created_at, updated_at) \
             VALUES (?, ?, 'preparing', ?, ?) \
             ON CONFLICT DO NOTHING RETURNING id",
        )
        .bind(provider_id.as_db())
        .bind(mode.as_db())
        .bind(now)
        .bind(now)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        let Some(run_id) = inserted else {
            transaction.rollback().await.map_err(AppError::Database)?;
            return Ok(None);
        };
        let run_id = MetadataScrapeRunId(run_id);

        match mode {
            MetadataScrapeMode::MissingMetadata => sqlx::query(
                "INSERT INTO metadata_scrape_run_items (run_id, game_id, state, updated_at) \
                 SELECT ?, g.id, 'pending', ? FROM games g \
                 LEFT JOIN provider_matches pm ON pm.game_id = g.id AND pm.provider_id = ? \
                 LEFT JOIN metadata_jobs mj ON mj.game_id = g.id AND mj.provider_id = ? \
                 WHERE pm.id IS NULL AND mj.id IS NULL",
            )
            .bind(run_id.0)
            .bind(now)
            .bind(provider_id.as_db())
            .bind(provider_id.as_db()),
            MetadataScrapeMode::RefreshMatched => sqlx::query(
                "INSERT INTO metadata_scrape_run_items (run_id, game_id, state, updated_at) \
                 SELECT ?, pm.game_id, 'pending', ? FROM provider_matches pm \
                 WHERE pm.provider_id = ? AND pm.status = 'matched' \
                 AND pm.provider_game_id IS NOT NULL",
            )
            .bind(run_id.0)
            .bind(now)
            .bind(provider_id.as_db()),
        }
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        sqlx::query(
            "UPDATE metadata_scrape_runs SET status = 'running', updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(run_id.0)
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        transaction.commit().await.map_err(AppError::Database)?;
        Ok(Some(run_id))
    }

    // ----------------------------------------------------------------------------------- reading

    pub async fn active_run(
        &self,
        provider_id: MetadataProviderId,
    ) -> Result<Option<MetadataScrapeRun>, AppError> {
        self.load_run_by(
            "SELECT id, provider_id, mode, status, created_at, updated_at, finished_at \
             FROM metadata_scrape_runs \
             WHERE provider_id = ? AND status IN ('preparing', 'running', 'stopping') \
             ORDER BY id DESC LIMIT 1",
            provider_id,
        )
        .await
    }

    /// The active run, or the most recent finished one when nothing is active.
    pub async fn latest_run(
        &self,
        provider_id: MetadataProviderId,
    ) -> Result<Option<MetadataScrapeRun>, AppError> {
        self.load_run_by(
            "SELECT id, provider_id, mode, status, created_at, updated_at, finished_at \
             FROM metadata_scrape_runs WHERE provider_id = ? ORDER BY id DESC LIMIT 1",
            provider_id,
        )
        .await
    }

    pub async fn load_run(
        &self,
        run_id: MetadataScrapeRunId,
    ) -> Result<Option<MetadataScrapeRun>, AppError> {
        let row = sqlx::query(
            "SELECT id, provider_id, mode, status, created_at, updated_at, finished_at \
             FROM metadata_scrape_runs WHERE id = ?",
        )
        .bind(run_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        let Some(row) = row else { return Ok(None) };
        let mut run = run_from_row(&row)?;
        run.progress = self.progress(run.id).await?;
        Ok(Some(run))
    }

    async fn load_run_by(
        &self,
        sql: &'static str,
        provider_id: MetadataProviderId,
    ) -> Result<Option<MetadataScrapeRun>, AppError> {
        let row = sqlx::query(sql)
            .bind(provider_id.as_db())
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::Database)?;

        let Some(row) = row else { return Ok(None) };
        let mut run = run_from_row(&row)?;
        run.progress = self.progress(run.id).await?;
        Ok(Some(run))
    }

    /// Game-count progress, aggregated straight from the item rows.
    ///
    /// Derived rather than kept in counters beside them, so the invariant the progress UI relies on
    /// — processed equals the sum of the five result buckets — cannot drift out of step with the
    /// rows that actually decide it.
    pub async fn progress(
        &self,
        run_id: MetadataScrapeRunId,
    ) -> Result<MetadataScrapeProgress, AppError> {
        let rows = sqlx::query(
            "SELECT state, COUNT(*) AS games FROM metadata_scrape_run_items \
             WHERE run_id = ? GROUP BY state",
        )
        .bind(run_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        let mut progress = MetadataScrapeProgress::default();
        for row in &rows {
            let raw: String = row.try_get("state").map_err(AppError::Database)?;
            let games: i64 = row.try_get("games").map_err(AppError::Database)?;
            let state = MetadataScrapeItemState::from_db(&raw).ok_or_else(|| {
                AppError::Metadata(format!("unknown scrape run item state '{raw}'"))
            })?;

            progress.total_games += games;
            match state {
                MetadataScrapeItemState::Matched => progress.matched += games,
                MetadataScrapeItemState::NeedsReview => progress.needs_review += games,
                MetadataScrapeItemState::NoMatch => progress.no_match += games,
                MetadataScrapeItemState::Unsupported => progress.unsupported += games,
                MetadataScrapeItemState::Failed => progress.failed += games,
                MetadataScrapeItemState::Running => progress.running += games,
                // A target game with no answer yet is waiting, whether or not it has been queued.
                // The distinction between "queued" and "not fed" is a feeder detail, not something
                // the user is asked to reason about.
                MetadataScrapeItemState::Pending | MetadataScrapeItemState::Queued => {
                    progress.waiting += games
                }
            }
        }
        Ok(progress)
    }

    /// Item state for one game, for tests and diagnostics.
    pub async fn item_state(
        &self,
        run_id: MetadataScrapeRunId,
        game_id: GameId,
    ) -> Result<Option<MetadataScrapeItemState>, AppError> {
        let raw: Option<String> = sqlx::query_scalar(
            "SELECT state FROM metadata_scrape_run_items WHERE run_id = ? AND game_id = ?",
        )
        .bind(run_id.0)
        .bind(game_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        raw.map(|raw| {
            MetadataScrapeItemState::from_db(&raw)
                .ok_or_else(|| AppError::Metadata(format!("unknown scrape run item state '{raw}'")))
        })
        .transpose()
    }

    // ------------------------------------------------------------------------------------ feeder

    /// Live provider jobs this run still owns exclusively.
    ///
    /// The feeder tops up against this rather than against the target size, which is what keeps the
    /// active provider queue bounded no matter how large the target set is.
    pub async fn live_owned_jobs(&self, run_id: MetadataScrapeRunId) -> Result<i64, AppError> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM metadata_jobs \
             WHERE bulk_run_id = ? AND state IN ('pending', 'running', 'deferred')",
        )
        .bind(run_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)
    }

    /// Moves up to `limit` pending target games into the M5 queue.
    ///
    /// One transaction covers both the job rows and the item rows, so a crash can never leave a
    /// queued job no run is waiting for, or a queued item with no job behind it. Re-running it is
    /// harmless: the item update is guarded on `state = 'pending'`, and the job insert leans on the
    /// existing `UNIQUE (game_id, provider_id, kind)` constraint rather than on a duplicate check
    /// in Rust.
    ///
    /// Returns the number of games fed.
    pub async fn feed_pending_items(
        &self,
        run_id: MetadataScrapeRunId,
        provider_id: MetadataProviderId,
        mode: MetadataScrapeMode,
        limit: usize,
        now: UnixTimestamp,
    ) -> Result<usize, AppError> {
        if limit == 0 {
            return Ok(0);
        }

        let mut transaction = self.pool.begin().await.map_err(AppError::Database)?;

        let games: Vec<i64> = sqlx::query_scalar(
            "SELECT game_id FROM metadata_scrape_run_items \
             WHERE run_id = ? AND state = 'pending' ORDER BY game_id ASC LIMIT ?",
        )
        .bind(run_id.0)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        if games.is_empty() {
            transaction.rollback().await.map_err(AppError::Database)?;
            return Ok(0);
        }

        for game_id in &games {
            for kind in mode.required_job_kinds() {
                sqlx::query(
                    "INSERT INTO metadata_jobs \
                     (game_id, provider_id, kind, state, priority, attempts, bulk_run_id, \
                      created_at, updated_at) \
                     VALUES (?, ?, ?, 'pending', ?, 0, ?, ?, ?) \
                     ON CONFLICT(game_id, provider_id, kind) DO UPDATE SET \
                     state = CASE WHEN metadata_jobs.state IN ('completed', 'failed') \
                                  THEN 'pending' ELSE metadata_jobs.state END, \
                     attempts = CASE WHEN metadata_jobs.state IN ('completed', 'failed') \
                                     THEN 0 ELSE metadata_jobs.attempts END, \
                     last_failure = CASE WHEN metadata_jobs.state IN ('completed', 'failed') \
                                         THEN NULL ELSE metadata_jobs.last_failure END, \
                     earliest_next_attempt_at = CASE \
                         WHEN metadata_jobs.state IN ('completed', 'failed') THEN NULL \
                         ELSE metadata_jobs.earliest_next_attempt_at END, \
                     priority = CASE WHEN metadata_jobs.state IN ('completed', 'failed') \
                                     THEN excluded.priority ELSE metadata_jobs.priority END, \
                     bulk_run_id = CASE WHEN metadata_jobs.state IN ('completed', 'failed') \
                                        THEN excluded.bulk_run_id \
                                        ELSE metadata_jobs.bulk_run_id END, \
                     updated_at = excluded.updated_at",
                )
                .bind(game_id)
                .bind(provider_id.as_db())
                .bind(kind.as_db())
                .bind(MetadataJobBand::Bulk.priority(*kind))
                .bind(run_id.0)
                .bind(now)
                .bind(now)
                .execute(&mut *transaction)
                .await
                .map_err(AppError::Database)?;
            }

            sqlx::query(
                "UPDATE metadata_scrape_run_items SET state = 'queued', updated_at = ? \
                 WHERE run_id = ? AND game_id = ? AND state = 'pending'",
            )
            .bind(now)
            .bind(run_id.0)
            .bind(game_id)
            .execute(&mut *transaction)
            .await
            .map_err(AppError::Database)?;
        }

        transaction.commit().await.map_err(AppError::Database)?;
        Ok(games.len())
    }

    // ---------------------------------------------------------------------------- reconciliation

    /// Authoritative M5 state for the run's unfinished, already-fed games.
    ///
    /// Bounded by construction: only fed items are examined, and the feeder never lets more of them
    /// exist than the feed window allows.
    pub async fn unfinished_item_facts(
        &self,
        run_id: MetadataScrapeRunId,
        provider_id: MetadataProviderId,
        limit: usize,
    ) -> Result<Vec<MetadataScrapeItemFacts>, AppError> {
        let rows = sqlx::query(
            "SELECT i.game_id AS game_id, \
             EXISTS (SELECT 1 FROM metadata_jobs j WHERE j.game_id = i.game_id \
                     AND j.provider_id = ? \
                     AND j.state IN ('pending', 'running', 'deferred')) AS has_live, \
             EXISTS (SELECT 1 FROM metadata_jobs j WHERE j.game_id = i.game_id \
                     AND j.provider_id = ? AND j.state = 'running') AS has_running, \
             EXISTS (SELECT 1 FROM metadata_jobs j WHERE j.game_id = i.game_id \
                     AND j.provider_id = ? AND j.state = 'failed') AS has_parked, \
             pm.status AS match_status, \
             CASE WHEN pm.unsupported_reason IS NOT NULL THEN 1 ELSE 0 END AS unsupported \
             FROM metadata_scrape_run_items i \
             LEFT JOIN provider_matches pm \
                 ON pm.game_id = i.game_id AND pm.provider_id = ? \
             WHERE i.run_id = ? AND i.state IN ('queued', 'running') \
             ORDER BY i.game_id ASC LIMIT ?",
        )
        .bind(provider_id.as_db())
        .bind(provider_id.as_db())
        .bind(provider_id.as_db())
        .bind(provider_id.as_db())
        .bind(run_id.0)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        rows.iter()
            .map(|row| {
                let match_status: Option<String> =
                    row.try_get("match_status").map_err(AppError::Database)?;
                let match_status = match_status
                    .map(|raw| {
                        ProviderMatchStatus::from_db(&raw).ok_or_else(|| {
                            AppError::Metadata(format!("unknown provider match status '{raw}'"))
                        })
                    })
                    .transpose()?;

                Ok(MetadataScrapeItemFacts {
                    game_id: GameId(row.try_get("game_id").map_err(AppError::Database)?),
                    has_live_job: row
                        .try_get::<i64, _>("has_live")
                        .map_err(AppError::Database)?
                        != 0,
                    has_running_job: row
                        .try_get::<i64, _>("has_running")
                        .map_err(AppError::Database)?
                        != 0,
                    has_parked_job: row
                        .try_get::<i64, _>("has_parked")
                        .map_err(AppError::Database)?
                        != 0,
                    match_status,
                    unsupported: row
                        .try_get::<i64, _>("unsupported")
                        .map_err(AppError::Database)?
                        != 0,
                })
            })
            .collect()
    }

    /// Writes reconciled item states.
    ///
    /// Guarded on the item still being unfinished, so a concurrent stop or a second reconciliation
    /// pass can never move a game back out of a result it already reached.
    pub async fn apply_item_states(
        &self,
        run_id: MetadataScrapeRunId,
        outcomes: &[(GameId, MetadataScrapeItemState)],
        now: UnixTimestamp,
    ) -> Result<usize, AppError> {
        if outcomes.is_empty() {
            return Ok(0);
        }

        let mut transaction = self.pool.begin().await.map_err(AppError::Database)?;
        let mut changed = 0;
        for (game_id, state) in outcomes {
            let result = sqlx::query(
                "UPDATE metadata_scrape_run_items SET state = ?, updated_at = ? \
                 WHERE run_id = ? AND game_id = ? AND state IN ('queued', 'running') \
                 AND state != ?",
            )
            .bind(state.as_db())
            .bind(now)
            .bind(run_id.0)
            .bind(game_id.0)
            .bind(state.as_db())
            .execute(&mut *transaction)
            .await
            .map_err(AppError::Database)?;
            changed += usize::try_from(result.rows_affected()).unwrap_or(0);
        }
        transaction.commit().await.map_err(AppError::Database)?;
        Ok(changed)
    }

    // ------------------------------------------------------------------------------- finalization

    /// Completes a running run once every target game has an answer.
    pub async fn complete_if_finished(
        &self,
        run_id: MetadataScrapeRunId,
        now: UnixTimestamp,
    ) -> Result<bool, AppError> {
        let result = sqlx::query(
            "UPDATE metadata_scrape_runs SET status = 'completed', finished_at = ?, updated_at = ? \
             WHERE id = ? AND status = 'running' AND NOT EXISTS ( \
                 SELECT 1 FROM metadata_scrape_run_items \
                 WHERE run_id = ? AND state IN ('pending', 'queued', 'running'))",
        )
        .bind(now)
        .bind(now)
        .bind(run_id.0)
        .bind(run_id.0)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(result.rows_affected() > 0)
    }

    /// Finishes a stopping run once nothing it fed is outstanding.
    ///
    /// Target games the run never reached stay `pending`, which is what lets a later Missing
    /// Metadata run pick them up: they still have no provider relationship and no job.
    pub async fn stop_if_settled(
        &self,
        run_id: MetadataScrapeRunId,
        now: UnixTimestamp,
    ) -> Result<bool, AppError> {
        let result = sqlx::query(
            "UPDATE metadata_scrape_runs SET status = 'stopped', finished_at = ?, updated_at = ? \
             WHERE id = ? AND status = 'stopping' AND NOT EXISTS ( \
                 SELECT 1 FROM metadata_scrape_run_items \
                 WHERE run_id = ? AND state IN ('queued', 'running'))",
        )
        .bind(now)
        .bind(now)
        .bind(run_id.0)
        .bind(run_id.0)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(result.rows_affected() > 0)
    }

    // -------------------------------------------------------------------------------------- stop

    /// Begins a cooperative stop.
    ///
    /// Stops feeding, then detaches the queued and deferred jobs this run still owns exclusively.
    /// Two things are deliberately left alone: a job an explicit user action promoted out of the run
    /// — its `bulk_run_id` is already `NULL`, so it is not selected — and a job that is currently
    /// running, which is allowed to finish so its result can still be recorded.
    pub async fn begin_stop(
        &self,
        run_id: MetadataScrapeRunId,
        provider_id: MetadataProviderId,
        now: UnixTimestamp,
    ) -> Result<bool, AppError> {
        let mut transaction = self.pool.begin().await.map_err(AppError::Database)?;

        let marked = sqlx::query(
            "UPDATE metadata_scrape_runs SET status = 'stopping', updated_at = ? \
             WHERE id = ? AND status IN ('preparing', 'running')",
        )
        .bind(now)
        .bind(run_id.0)
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        if marked.rows_affected() == 0 {
            transaction.rollback().await.map_err(AppError::Database)?;
            return Ok(false);
        }

        // Return the items whose work is about to disappear to `pending` *before* deleting it, so
        // they are indistinguishable from target games the run never reached. A game whose request
        // is in flight is excluded: it keeps its queued item and is resolved by reconciliation once
        // the request finishes.
        sqlx::query(
            "UPDATE metadata_scrape_run_items SET state = 'pending', updated_at = ? \
             WHERE run_id = ? AND state IN ('queued', 'running') \
             AND game_id IN (SELECT game_id FROM metadata_jobs \
                             WHERE bulk_run_id = ? AND state IN ('pending', 'deferred')) \
             AND game_id NOT IN (SELECT game_id FROM metadata_jobs \
                                 WHERE provider_id = ? AND state = 'running')",
        )
        .bind(now)
        .bind(run_id.0)
        .bind(run_id.0)
        .bind(provider_id.as_db())
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        sqlx::query(
            "DELETE FROM metadata_jobs \
             WHERE bulk_run_id = ? AND state IN ('pending', 'deferred')",
        )
        .bind(run_id.0)
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        transaction.commit().await.map_err(AppError::Database)?;
        Ok(true)
    }

    // -------------------------------------------------------------------------- restart recovery

    /// Resolves a run left mid-start by an unclean shutdown.
    ///
    /// `preparing` is committed-but-unsnapshotted. Creating a run is one transaction, so SQLite's
    /// atomicity should make this state unreachable; it is handled anyway rather than assumed away,
    /// because a run stuck in it would block every future run through the active-run index.
    pub async fn recover_preparing_runs(
        &self,
        provider_id: MetadataProviderId,
        now: UnixTimestamp,
    ) -> Result<u64, AppError> {
        let mut transaction = self.pool.begin().await.map_err(AppError::Database)?;

        let promoted = sqlx::query(
            "UPDATE metadata_scrape_runs SET status = 'running', updated_at = ? \
             WHERE provider_id = ? AND status = 'preparing' AND EXISTS ( \
                 SELECT 1 FROM metadata_scrape_run_items WHERE run_id = metadata_scrape_runs.id)",
        )
        .bind(now)
        .bind(provider_id.as_db())
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        let abandoned = sqlx::query(
            "UPDATE metadata_scrape_runs SET status = 'stopped', finished_at = ?, updated_at = ? \
             WHERE provider_id = ? AND status = 'preparing'",
        )
        .bind(now)
        .bind(now)
        .bind(provider_id.as_db())
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        transaction.commit().await.map_err(AppError::Database)?;
        Ok(promoted.rows_affected() + abandoned.rows_affected())
    }

    /// Test-only access for asserting job attribution.
    #[cfg(test)]
    pub async fn owned_job_count(&self, run_id: MetadataScrapeRunId) -> Result<i64, AppError> {
        sqlx::query_scalar("SELECT COUNT(*) FROM metadata_jobs WHERE bulk_run_id = ?")
            .bind(run_id.0)
            .fetch_one(&self.pool)
            .await
            .map_err(AppError::Database)
    }
}

fn run_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<MetadataScrapeRun, AppError> {
    let provider_raw: String = row.try_get("provider_id").map_err(AppError::Database)?;
    let mode_raw: String = row.try_get("mode").map_err(AppError::Database)?;
    let status_raw: String = row.try_get("status").map_err(AppError::Database)?;

    Ok(MetadataScrapeRun {
        id: MetadataScrapeRunId(row.try_get("id").map_err(AppError::Database)?),
        provider_id: MetadataProviderId::from_db(&provider_raw).ok_or_else(|| {
            AppError::Metadata(format!("unknown metadata provider '{provider_raw}'"))
        })?,
        mode: MetadataScrapeMode::from_db(&mode_raw)
            .ok_or_else(|| AppError::Metadata(format!("unknown scrape mode '{mode_raw}'")))?,
        status: MetadataScrapeRunStatus::from_db(&status_raw).ok_or_else(|| {
            AppError::Metadata(format!("unknown scrape run status '{status_raw}'"))
        })?,
        progress: MetadataScrapeProgress::default(),
        created_at: row.try_get("created_at").map_err(AppError::Database)?,
        updated_at: row.try_get("updated_at").map_err(AppError::Database)?,
        finished_at: row.try_get("finished_at").map_err(AppError::Database)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::database::Database;
    use crate::domain::metadata::MetadataJobKind;
    use crate::domain::metadata_scrape::classify_scrape_item;
    use crate::domain::system::SystemId;
    use tempfile::TempDir;

    const NOW: UnixTimestamp = 1_760_000_000_000;
    const PROVIDER: MetadataProviderId = MetadataProviderId::ScreenScraper;

    struct Fixture {
        _directory: TempDir,
        pool: SqlitePool,
        repository: MetadataScrapeRepository,
    }

    impl Fixture {
        async fn new() -> Self {
            let directory = tempfile::tempdir().expect("temporary directory");
            let database = Database::open(&directory.path().join("retrofrontier.sqlite3"))
                .await
                .expect("database should open");
            let pool = database.pool().clone();
            Self {
                _directory: directory,
                repository: MetadataScrapeRepository::new(pool.clone()),
                pool,
            }
        }

        /// Inserts `count` bare games. Metadata eligibility never reads content units, so the
        /// scrape repository can be exercised without the scanner.
        async fn insert_games(&self, count: usize) -> Vec<GameId> {
            let mut transaction = self.pool.begin().await.expect("transaction");
            let mut ids = Vec::with_capacity(count);
            for index in 0..count {
                let id: i64 = sqlx::query_scalar(
                    "INSERT INTO games (system_id, local_title, availability, created_at, \
                     updated_at) VALUES (?, ?, 'available', ?, ?) RETURNING id",
                )
                .bind(SystemId::Nes.as_str())
                .bind(format!("Game {index:06}"))
                .bind(NOW)
                .bind(NOW)
                .fetch_one(&mut *transaction)
                .await
                .expect("game fixture");
                ids.push(GameId(id));
            }
            transaction.commit().await.expect("commit");
            ids
        }

        async fn insert_match(
            &self,
            game_id: GameId,
            status: &str,
            provider_game_id: Option<&str>,
        ) {
            sqlx::query(
                "INSERT INTO provider_matches \
                 (game_id, provider_id, status, match_type, provider_game_id, created_at, \
                  updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(game_id.0)
            .bind(PROVIDER.as_db())
            .bind(status)
            .bind((status == "matched").then_some("deterministic_sha1"))
            .bind(provider_game_id)
            .bind(NOW)
            .bind(NOW)
            .execute(&self.pool)
            .await
            .expect("match fixture");
        }

        async fn insert_unsupported(&self, game_id: GameId) {
            sqlx::query(
                "INSERT INTO provider_matches \
                 (game_id, provider_id, status, unsupported_reason, created_at, updated_at) \
                 VALUES (?, ?, 'deferred', 'playlist_is_not_identity', ?, ?)",
            )
            .bind(game_id.0)
            .bind(PROVIDER.as_db())
            .bind(NOW)
            .bind(NOW)
            .execute(&self.pool)
            .await
            .expect("unsupported fixture");
        }

        async fn insert_job(&self, game_id: GameId, kind: MetadataJobKind, state: &str) {
            sqlx::query(
                "INSERT INTO metadata_jobs \
                 (game_id, provider_id, kind, state, priority, attempts, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, 100, 0, ?, ?)",
            )
            .bind(game_id.0)
            .bind(PROVIDER.as_db())
            .bind(kind.as_db())
            .bind(state)
            .bind(NOW)
            .bind(NOW)
            .execute(&self.pool)
            .await
            .expect("job fixture");
        }

        async fn job_state(&self, game_id: GameId, kind: MetadataJobKind) -> Option<String> {
            sqlx::query_scalar(
                "SELECT state FROM metadata_jobs WHERE game_id = ? AND provider_id = ? AND kind = ?",
            )
            .bind(game_id.0)
            .bind(PROVIDER.as_db())
            .bind(kind.as_db())
            .fetch_optional(&self.pool)
            .await
            .expect("job state")
        }

        async fn job_priority(&self, game_id: GameId, kind: MetadataJobKind) -> Option<i64> {
            sqlx::query_scalar(
                "SELECT priority FROM metadata_jobs WHERE game_id = ? AND provider_id = ? \
                 AND kind = ?",
            )
            .bind(game_id.0)
            .bind(PROVIDER.as_db())
            .bind(kind.as_db())
            .fetch_optional(&self.pool)
            .await
            .expect("job priority")
        }

        async fn live_job_total(&self) -> i64 {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM metadata_jobs \
                 WHERE state IN ('pending', 'running', 'deferred')",
            )
            .fetch_one(&self.pool)
            .await
            .expect("live job total")
        }

        /// Runs the reconciliation loop the application service performs, without the service.
        async fn reconcile(&self, run_id: MetadataScrapeRunId) -> usize {
            let facts = self
                .repository
                .unfinished_item_facts(run_id, PROVIDER, 4_096)
                .await
                .expect("facts should load");
            let outcomes: Vec<_> = facts
                .iter()
                .map(|facts| (facts.game_id, classify_scrape_item(facts)))
                .collect();
            self.repository
                .apply_item_states(run_id, &outcomes, NOW)
                .await
                .expect("outcomes should apply")
        }
    }

    // ---------------------------------------------------------------------------- eligibility

    #[tokio::test]
    async fn an_untouched_game_is_the_only_missing_metadata_target() {
        let fixture = Fixture::new().await;
        let games = fixture.insert_games(7).await;

        // games[0] stays untouched.
        fixture
            .insert_match(games[1], "matched", Some("7001"))
            .await;
        fixture.insert_match(games[2], "ambiguous", None).await;
        fixture.insert_match(games[3], "no_match", None).await;
        fixture.insert_unsupported(games[4]).await;
        fixture.insert_match(games[5], "stale", Some("7005")).await;
        fixture
            .insert_job(games[6], MetadataJobKind::Identify, "pending")
            .await;

        assert_eq!(
            fixture
                .repository
                .count_eligible_games(PROVIDER, MetadataScrapeMode::MissingMetadata)
                .await
                .expect("preview should count"),
            1
        );

        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");
        assert_eq!(
            fixture
                .repository
                .item_state(run_id, games[0])
                .await
                .expect("item lookup"),
            Some(MetadataScrapeItemState::Pending)
        );
        for excluded in &games[1..] {
            assert_eq!(
                fixture
                    .repository
                    .item_state(run_id, *excluded)
                    .await
                    .expect("item lookup"),
                None,
                "game {excluded} must not be a Missing Metadata target"
            );
        }
    }

    #[tokio::test]
    async fn a_terminal_failure_is_an_answer_and_is_not_retried_by_a_later_missing_run() {
        let fixture = Fixture::new().await;
        let games = fixture.insert_games(1).await;
        fixture.insert_match(games[0], "failed", None).await;
        fixture
            .insert_job(games[0], MetadataJobKind::Identify, "failed")
            .await;

        assert_eq!(
            fixture
                .repository
                .count_eligible_games(PROVIDER, MetadataScrapeMode::MissingMetadata)
                .await
                .expect("preview should count"),
            0
        );
    }

    #[tokio::test]
    async fn only_accepted_matches_are_refresh_targets() {
        let fixture = Fixture::new().await;
        let games = fixture.insert_games(5).await;
        fixture
            .insert_match(games[0], "matched", Some("8001"))
            .await;
        fixture.insert_match(games[1], "ambiguous", None).await;
        fixture.insert_match(games[2], "no_match", None).await;
        fixture.insert_unsupported(games[3]).await;
        // An accepted status with no provider identity is not refreshable: there is nothing to ask
        // the provider for, and the schema's own CHECK would reject it as a trusted relationship.
        fixture.insert_match(games[4], "stale", Some("8004")).await;

        assert_eq!(
            fixture
                .repository
                .count_eligible_games(PROVIDER, MetadataScrapeMode::RefreshMatched)
                .await
                .expect("preview should count"),
            1
        );

        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::RefreshMatched, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");
        assert_eq!(
            fixture
                .repository
                .item_state(run_id, games[0])
                .await
                .expect("item lookup"),
            Some(MetadataScrapeItemState::Pending)
        );
        assert_eq!(
            fixture
                .repository
                .item_state(run_id, games[4])
                .await
                .expect("item lookup"),
            None
        );
    }

    // ----------------------------------------------------------------------- run lifecycle

    #[tokio::test]
    async fn a_provider_may_only_have_one_active_run() {
        let fixture = Fixture::new().await;
        fixture.insert_games(3).await;

        let first = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created");
        assert!(first.is_some());

        let second = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::RefreshMatched, NOW)
            .await
            .expect("a rejected second run is not an error");
        assert_eq!(
            second, None,
            "the active-run index must reject a second run"
        );

        // Finishing the first run releases the provider.
        fixture
            .repository
            .begin_stop(first.expect("first run"), PROVIDER, NOW)
            .await
            .expect("stop should begin");
        fixture
            .repository
            .stop_if_settled(first.expect("first run"), NOW)
            .await
            .expect("stop should settle");

        assert!(fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created")
            .is_some());
    }

    #[tokio::test]
    async fn run_membership_is_fixed_when_the_run_starts() {
        let fixture = Fixture::new().await;
        fixture.insert_games(148).await;

        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");
        assert_eq!(
            fixture
                .repository
                .progress(run_id)
                .await
                .expect("progress")
                .total_games,
            148
        );

        // A later library scan discovers ten more games.
        let discovered = fixture.insert_games(10).await;

        let progress = fixture.repository.progress(run_id).await.expect("progress");
        assert_eq!(progress.total_games, 148, "an active run must not grow");
        for game_id in discovered {
            assert_eq!(
                fixture
                    .repository
                    .item_state(run_id, game_id)
                    .await
                    .expect("item lookup"),
                None
            );
        }

        // They are eligible for the next run instead.
        assert_eq!(
            fixture
                .repository
                .count_eligible_games(PROVIDER, MetadataScrapeMode::MissingMetadata)
                .await
                .expect("preview should count"),
            158
        );
    }

    #[tokio::test]
    async fn a_run_with_no_targets_completes_immediately() {
        let fixture = Fixture::new().await;
        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");

        assert!(fixture
            .repository
            .complete_if_finished(run_id, NOW)
            .await
            .expect("completion check"));
        let run = fixture
            .repository
            .load_run(run_id)
            .await
            .expect("run should load")
            .expect("a run should exist");
        assert_eq!(run.status, MetadataScrapeRunStatus::Completed);
        assert_eq!(run.finished_at, Some(NOW));
    }

    #[tokio::test]
    async fn a_run_does_not_complete_while_a_target_is_unfinished() {
        let fixture = Fixture::new().await;
        fixture.insert_games(2).await;
        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");

        assert!(!fixture
            .repository
            .complete_if_finished(run_id, NOW)
            .await
            .expect("completion check"));
    }

    // ------------------------------------------------------------------------- bounded feeder

    #[tokio::test]
    async fn feeding_creates_one_bulk_job_per_required_kind() {
        let fixture = Fixture::new().await;
        let games = fixture.insert_games(2).await;
        fixture
            .insert_match(games[0], "matched", Some("9001"))
            .await;
        fixture
            .insert_match(games[1], "matched", Some("9002"))
            .await;

        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::RefreshMatched, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");

        let fed = fixture
            .repository
            .feed_pending_items(
                run_id,
                PROVIDER,
                MetadataScrapeMode::RefreshMatched,
                10,
                NOW,
            )
            .await
            .expect("feeding should succeed");

        assert_eq!(fed, 2);
        assert_eq!(
            fixture
                .repository
                .owned_job_count(run_id)
                .await
                .expect("owned jobs"),
            4,
            "a refresh needs both halves for each game"
        );
        assert_eq!(
            fixture
                .repository
                .item_state(run_id, games[0])
                .await
                .expect("item lookup"),
            Some(MetadataScrapeItemState::Queued)
        );
        assert_eq!(
            fixture
                .job_priority(games[0], MetadataJobKind::RefreshMetadata)
                .await,
            Some(MetadataJobBand::Bulk.priority(MetadataJobKind::RefreshMetadata))
        );
    }

    #[tokio::test]
    async fn feeding_is_idempotent_and_never_duplicates_a_job() {
        let fixture = Fixture::new().await;
        fixture.insert_games(3).await;
        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");

        assert_eq!(
            fixture
                .repository
                .feed_pending_items(
                    run_id,
                    PROVIDER,
                    MetadataScrapeMode::MissingMetadata,
                    10,
                    NOW
                )
                .await
                .expect("feeding should succeed"),
            3
        );
        // A second pass finds nothing pending, so nothing is enqueued twice.
        assert_eq!(
            fixture
                .repository
                .feed_pending_items(
                    run_id,
                    PROVIDER,
                    MetadataScrapeMode::MissingMetadata,
                    10,
                    NOW
                )
                .await
                .expect("feeding should succeed"),
            0
        );
        assert_eq!(
            fixture
                .repository
                .owned_job_count(run_id)
                .await
                .expect("owned jobs"),
            3
        );
    }

    #[tokio::test]
    async fn feeding_never_steals_a_live_interactive_job() {
        let fixture = Fixture::new().await;
        let games = fixture.insert_games(1).await;
        fixture
            .insert_match(games[0], "matched", Some("9100"))
            .await;

        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::RefreshMatched, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");

        // The user asks for this game by hand before the feeder reaches it.
        fixture
            .insert_job(games[0], MetadataJobKind::RefreshMetadata, "pending")
            .await;

        fixture
            .repository
            .feed_pending_items(
                run_id,
                PROVIDER,
                MetadataScrapeMode::RefreshMatched,
                10,
                NOW,
            )
            .await
            .expect("feeding should succeed");

        assert_eq!(
            fixture
                .job_priority(games[0], MetadataJobKind::RefreshMetadata)
                .await,
            Some(100),
            "the interactive job keeps its own priority"
        );
        assert_eq!(
            fixture
                .repository
                .owned_job_count(run_id)
                .await
                .expect("owned jobs"),
            1,
            "only the cover half is owned by the run"
        );
    }

    #[tokio::test]
    async fn feeding_revives_a_dead_job_and_takes_ownership_of_it() {
        let fixture = Fixture::new().await;
        let games = fixture.insert_games(1).await;
        fixture
            .insert_match(games[0], "matched", Some("9200"))
            .await;
        // A previous refresh finished, leaving its jobs behind as completed rows.
        fixture
            .insert_job(games[0], MetadataJobKind::RefreshMetadata, "completed")
            .await;
        fixture
            .insert_job(games[0], MetadataJobKind::RefreshCover, "failed")
            .await;

        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::RefreshMatched, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");
        fixture
            .repository
            .feed_pending_items(
                run_id,
                PROVIDER,
                MetadataScrapeMode::RefreshMatched,
                10,
                NOW,
            )
            .await
            .expect("feeding should succeed");

        assert_eq!(
            fixture
                .job_state(games[0], MetadataJobKind::RefreshMetadata)
                .await
                .as_deref(),
            Some("pending")
        );
        assert_eq!(
            fixture
                .job_state(games[0], MetadataJobKind::RefreshCover)
                .await
                .as_deref(),
            Some("pending")
        );
        assert_eq!(
            fixture
                .repository
                .owned_job_count(run_id)
                .await
                .expect("owned jobs"),
            2
        );
    }

    // ------------------------------------------------------------------------ promotion

    #[tokio::test]
    async fn an_explicit_request_promotes_bulk_work_instead_of_duplicating_it() {
        let fixture = Fixture::new().await;
        let games = fixture.insert_games(2).await;
        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");
        fixture
            .repository
            .feed_pending_items(
                run_id,
                PROVIDER,
                MetadataScrapeMode::MissingMetadata,
                10,
                NOW,
            )
            .await
            .expect("feeding should succeed");

        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM metadata_jobs")
            .fetch_one(&fixture.pool)
            .await
            .expect("job total");
        assert_eq!(before, 2);
        assert_eq!(
            fixture
                .job_priority(games[0], MetadataJobKind::Identify)
                .await,
            Some(MetadataJobBand::Bulk.priority(MetadataJobKind::Identify))
        );

        // The user opens this game and asks for its metadata by hand.
        crate::repositories::metadata::MetadataRepository::new(fixture.pool.clone())
            .enqueue_job(games[0], PROVIDER, MetadataJobKind::Identify, NOW + 1)
            .await
            .expect("an explicit request should enqueue");

        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM metadata_jobs")
            .fetch_one(&fixture.pool)
            .await
            .expect("job total");
        assert_eq!(after, 2, "promotion must not create a second job");
        assert_eq!(
            fixture
                .job_priority(games[0], MetadataJobKind::Identify)
                .await,
            Some(MetadataJobBand::Interactive.priority(MetadataJobKind::Identify)),
            "the promoted job must run in the interactive band"
        );
        assert_eq!(
            fixture
                .repository
                .owned_job_count(run_id)
                .await
                .expect("owned jobs"),
            1,
            "the promoted job is no longer owned by the run"
        );

        // The M5 scheduler claims the promoted job before the bulk one.
        let claim_order: Vec<i64> = sqlx::query_scalar(
            "SELECT game_id FROM metadata_jobs WHERE provider_id = ? \
             AND state IN ('pending', 'deferred') ORDER BY priority ASC, id ASC",
        )
        .bind(PROVIDER.as_db())
        .fetch_all(&fixture.pool)
        .await
        .expect("claim ordering");
        assert_eq!(claim_order, vec![games[0].0, games[1].0]);
    }

    #[tokio::test]
    async fn promoting_a_running_job_does_not_restart_it() {
        let fixture = Fixture::new().await;
        let games = fixture.insert_games(1).await;
        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");
        fixture
            .repository
            .feed_pending_items(
                run_id,
                PROVIDER,
                MetadataScrapeMode::MissingMetadata,
                10,
                NOW,
            )
            .await
            .expect("feeding should succeed");
        sqlx::query("UPDATE metadata_jobs SET state = 'running', attempts = 2, claimed_at = ?")
            .bind(NOW)
            .execute(&fixture.pool)
            .await
            .expect("claim");

        crate::repositories::metadata::MetadataRepository::new(fixture.pool.clone())
            .enqueue_job(games[0], PROVIDER, MetadataJobKind::Identify, NOW + 1)
            .await
            .expect("an explicit request should enqueue");

        let (state, attempts): (String, i64) =
            sqlx::query_as("SELECT state, attempts FROM metadata_jobs WHERE game_id = ?")
                .bind(games[0].0)
                .fetch_one(&fixture.pool)
                .await
                .expect("job row");
        assert_eq!(state, "running", "an in-flight request is never restarted");
        assert_eq!(attempts, 2, "its retry budget is untouched");
    }

    // ------------------------------------------------------------------------ reconciliation

    #[tokio::test]
    async fn reconciliation_records_results_and_leaves_live_work_alone() {
        let fixture = Fixture::new().await;
        let games = fixture.insert_games(4).await;
        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");
        fixture
            .repository
            .feed_pending_items(
                run_id,
                PROVIDER,
                MetadataScrapeMode::MissingMetadata,
                10,
                NOW,
            )
            .await
            .expect("feeding should succeed");

        // Three games answered; the fourth is still deferred by the provider.
        for (game_id, status) in [
            (games[0], "matched"),
            (games[1], "ambiguous"),
            (games[2], "no_match"),
        ] {
            sqlx::query("UPDATE metadata_jobs SET state = 'completed' WHERE game_id = ?")
                .bind(game_id.0)
                .execute(&fixture.pool)
                .await
                .expect("job completion");
            fixture
                .insert_match(game_id, status, (status == "matched").then_some("1"))
                .await;
        }
        sqlx::query("UPDATE metadata_jobs SET state = 'deferred' WHERE game_id = ?")
            .bind(games[3].0)
            .execute(&fixture.pool)
            .await
            .expect("job deferral");

        fixture.reconcile(run_id).await;

        let progress = fixture.repository.progress(run_id).await.expect("progress");
        assert_eq!(progress.matched, 1);
        assert_eq!(progress.needs_review, 1);
        assert_eq!(progress.no_match, 1);
        assert_eq!(progress.processed(), 3);
        assert_eq!(progress.waiting, 1, "a deferred game is not processed");
        assert_eq!(
            progress.processed() + progress.running + progress.waiting,
            progress.total_games
        );
        assert!(!fixture
            .repository
            .complete_if_finished(run_id, NOW)
            .await
            .expect("completion check"));
    }

    #[tokio::test]
    async fn a_refresh_game_is_unfinished_until_both_halves_are_done() {
        let fixture = Fixture::new().await;
        let games = fixture.insert_games(1).await;
        fixture
            .insert_match(games[0], "matched", Some("9300"))
            .await;
        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::RefreshMatched, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");
        fixture
            .repository
            .feed_pending_items(
                run_id,
                PROVIDER,
                MetadataScrapeMode::RefreshMatched,
                10,
                NOW,
            )
            .await
            .expect("feeding should succeed");

        sqlx::query("UPDATE metadata_jobs SET state = 'completed' WHERE kind = 'refresh_metadata'")
            .execute(&fixture.pool)
            .await
            .expect("metadata half completes");

        fixture.reconcile(run_id).await;
        assert_eq!(
            fixture
                .repository
                .progress(run_id)
                .await
                .expect("progress")
                .processed(),
            0,
            "the cover half is still queued"
        );

        sqlx::query("UPDATE metadata_jobs SET state = 'completed' WHERE kind = 'refresh_cover'")
            .execute(&fixture.pool)
            .await
            .expect("cover half completes");

        fixture.reconcile(run_id).await;
        assert_eq!(
            fixture
                .repository
                .progress(run_id)
                .await
                .expect("progress")
                .matched,
            1
        );
        assert!(fixture
            .repository
            .complete_if_finished(run_id, NOW)
            .await
            .expect("completion check"));
    }

    // ---------------------------------------------------------------------------------- stop

    #[tokio::test]
    async fn stopping_detaches_owned_work_and_keeps_promoted_work() {
        let fixture = Fixture::new().await;
        let games = fixture.insert_games(4).await;
        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");
        fixture
            .repository
            .feed_pending_items(
                run_id,
                PROVIDER,
                MetadataScrapeMode::MissingMetadata,
                10,
                NOW,
            )
            .await
            .expect("feeding should succeed");

        // games[0] was promoted into explicit interactive ownership; games[1] is in flight.
        sqlx::query(
            "UPDATE metadata_jobs SET bulk_run_id = NULL, priority = 100 WHERE game_id = ?",
        )
        .bind(games[0].0)
        .execute(&fixture.pool)
        .await
        .expect("promotion");
        sqlx::query("UPDATE metadata_jobs SET state = 'running' WHERE game_id = ?")
            .bind(games[1].0)
            .execute(&fixture.pool)
            .await
            .expect("claim");

        assert!(fixture
            .repository
            .begin_stop(run_id, PROVIDER, NOW)
            .await
            .expect("stop should begin"));

        assert!(
            fixture
                .job_state(games[0], MetadataJobKind::Identify)
                .await
                .is_some(),
            "promoted interactive work must survive a stop"
        );
        assert_eq!(
            fixture
                .job_state(games[1], MetadataJobKind::Identify)
                .await
                .as_deref(),
            Some("running"),
            "an in-flight request is allowed to finish"
        );
        assert!(
            fixture
                .job_state(games[2], MetadataJobKind::Identify)
                .await
                .is_none(),
            "queued bulk-only work is detached"
        );
        assert_eq!(
            fixture
                .repository
                .item_state(run_id, games[2])
                .await
                .expect("item lookup"),
            Some(MetadataScrapeItemState::Pending),
            "a detached game returns to untouched so a later run can reach it"
        );

        // The run settles once the in-flight request resolves.
        assert!(!fixture
            .repository
            .stop_if_settled(run_id, NOW)
            .await
            .expect("settle check"));
        sqlx::query("UPDATE metadata_jobs SET state = 'completed' WHERE state = 'running'")
            .execute(&fixture.pool)
            .await
            .expect("request finishes");
        fixture.insert_match(games[1], "matched", Some("1")).await;
        fixture.insert_match(games[0], "matched", Some("2")).await;
        sqlx::query("UPDATE metadata_jobs SET state = 'completed'")
            .execute(&fixture.pool)
            .await
            .expect("all requests finish");
        fixture.reconcile(run_id).await;

        assert!(fixture
            .repository
            .stop_if_settled(run_id, NOW)
            .await
            .expect("settle check"));
        let run = fixture
            .repository
            .load_run(run_id)
            .await
            .expect("run loads")
            .expect("a run exists");
        assert_eq!(run.status, MetadataScrapeRunStatus::Stopped);
        assert_eq!(
            run.progress.matched, 2,
            "results already written are preserved"
        );

        // Games the stopped run never reached are untouched again.
        assert_eq!(
            fixture
                .repository
                .count_eligible_games(PROVIDER, MetadataScrapeMode::MissingMetadata)
                .await
                .expect("preview should count"),
            2
        );
    }

    // ------------------------------------------------------------------------------ recovery

    #[tokio::test]
    async fn a_preparing_run_is_resolved_on_startup() {
        let fixture = Fixture::new().await;
        fixture.insert_games(2).await;
        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");
        sqlx::query("UPDATE metadata_scrape_runs SET status = 'preparing' WHERE id = ?")
            .bind(run_id.0)
            .execute(&fixture.pool)
            .await
            .expect("simulate a crash inside the start transaction");

        assert_eq!(
            fixture
                .repository
                .recover_preparing_runs(PROVIDER, NOW)
                .await
                .expect("recovery"),
            1
        );
        assert_eq!(
            fixture
                .repository
                .load_run(run_id)
                .await
                .expect("run loads")
                .expect("a run exists")
                .status,
            MetadataScrapeRunStatus::Running
        );
    }

    #[tokio::test]
    async fn a_preparing_run_with_no_snapshot_is_abandoned_rather_than_blocking_the_provider() {
        let fixture = Fixture::new().await;
        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");
        sqlx::query("UPDATE metadata_scrape_runs SET status = 'preparing' WHERE id = ?")
            .bind(run_id.0)
            .execute(&fixture.pool)
            .await
            .expect("simulate a crash inside the start transaction");

        fixture
            .repository
            .recover_preparing_runs(PROVIDER, NOW)
            .await
            .expect("recovery");

        assert_eq!(
            fixture
                .repository
                .load_run(run_id)
                .await
                .expect("run loads")
                .expect("a run exists")
                .status,
            MetadataScrapeRunStatus::Stopped
        );
        assert!(
            fixture
                .repository
                .active_run(PROVIDER)
                .await
                .expect("active run lookup")
                .is_none(),
            "an abandoned run must not hold the provider forever"
        );
    }

    // --------------------------------------------------------------------------------- scale

    async fn scale_run(games: usize, window: usize) {
        let fixture = Fixture::new().await;
        fixture.insert_games(games).await;

        let run_id = fixture
            .repository
            .create_run(PROVIDER, MetadataScrapeMode::MissingMetadata, NOW)
            .await
            .expect("run should be created")
            .expect("a run should exist");
        assert_eq!(
            fixture
                .repository
                .progress(run_id)
                .await
                .expect("progress")
                .total_games,
            games as i64
        );

        // Drive the run to completion the way the feeder does, never exceeding the window.
        let mut processed = 0usize;
        let mut rounds = 0usize;
        while processed < games {
            rounds += 1;
            assert!(rounds < games, "the feeder must make progress every round");

            let live = fixture
                .repository
                .live_owned_jobs(run_id)
                .await
                .expect("live job count");
            assert!(
                live <= window as i64,
                "the active provider queue must stay bounded: {live} > {window}"
            );
            let headroom = window.saturating_sub(live as usize);
            fixture
                .repository
                .feed_pending_items(
                    run_id,
                    PROVIDER,
                    MetadataScrapeMode::MissingMetadata,
                    headroom,
                    NOW,
                )
                .await
                .expect("feeding should succeed");

            assert!(
                fixture.live_job_total().await <= window as i64,
                "no more than one window of jobs may be live at once"
            );

            // The worker answers everything it was handed.
            sqlx::query(
                "UPDATE metadata_jobs SET state = 'completed' \
                 WHERE bulk_run_id = ? AND state = 'pending'",
            )
            .bind(run_id.0)
            .execute(&fixture.pool)
            .await
            .expect("worker round");
            sqlx::query(
                "INSERT INTO provider_matches \
                 (game_id, provider_id, status, match_type, provider_game_id, created_at, \
                  updated_at) \
                 SELECT game_id, ?, 'matched', 'deterministic_sha1', '1', ?, ? \
                 FROM metadata_scrape_run_items \
                 WHERE run_id = ? AND state = 'queued' \
                 ON CONFLICT DO NOTHING",
            )
            .bind(PROVIDER.as_db())
            .bind(NOW)
            .bind(NOW)
            .bind(run_id.0)
            .execute(&fixture.pool)
            .await
            .expect("provider answers");

            fixture.reconcile(run_id).await;
            processed = fixture
                .repository
                .progress(run_id)
                .await
                .expect("progress")
                .processed() as usize;
        }

        assert!(fixture
            .repository
            .complete_if_finished(run_id, NOW)
            .await
            .expect("completion check"));
        let progress = fixture.repository.progress(run_id).await.expect("progress");
        assert_eq!(progress.total_games, games as i64);
        assert_eq!(progress.matched, games as i64);
        assert_eq!(progress.processed(), games as i64);
        assert_eq!(progress.waiting, 0);
    }

    #[tokio::test]
    async fn a_five_thousand_game_run_stays_bounded() {
        scale_run(5_000, 200).await;
    }

    #[tokio::test]
    async fn a_twenty_thousand_game_run_stays_bounded() {
        scale_run(20_000, 200).await;
    }
}
