//! Metadata persistence.
//!
//! All metadata SQL lives here. The operations are deliberately targeted — one game, one job, one
//! bounded batch — because metadata processing must never require loading the whole library.
//!
//! Nothing in this module writes to `games`, `content_units`, `content_files`, or
//! `content_unit_files`. Provider state can therefore never change local library identity or
//! availability, even if a provider or a job misbehaves.

use crate::domain::library::ContentUnitKind;
use crate::domain::library::{ContentUnitId, GameId, UnixTimestamp};
use crate::domain::metadata::{
    MatchEvidence, MatchType, MediaAsset, MediaAssetKind, MediaAssetState, MetadataJob,
    MetadataJobId, MetadataJobKind, MetadataJobState, MetadataProvenance, MetadataProviderId,
    NormalizedMetadata, ProviderCandidate, ProviderFailureClass, ProviderMatch, ProviderMatchId,
    ProviderMatchStatus, ProviderMetadataRecord, ProviderQuotaSnapshot, ProviderSchedulerState,
    UnsupportedContentReason, UserAccountState, UserProviderSelection,
};
use crate::domain::system::SystemId;
use crate::error::AppError;
use sqlx::{Row, SqlitePool};

/// Non-secret record of an optional personal provider account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderUserAccountRecord {
    pub vault_reference: String,
    pub state: UserAccountState,
}

/// Everything persisted about one game and provider, loaded in one place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMetadata {
    pub provider_match: Option<ProviderMatch>,
    pub metadata: Option<ProviderMetadataRecord>,
    pub cover: Option<MediaAsset>,
    pub user_selection: Option<UserProviderSelection>,
    pub jobs: Vec<MetadataJob>,
}

/// The values written when a provider relationship is accepted or updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMatchWrite {
    pub game_id: GameId,
    pub provider_id: MetadataProviderId,
    pub status: ProviderMatchStatus,
    pub match_type: Option<MatchType>,
    pub provider_game_id: Option<String>,
    pub provider_rom_id: Option<String>,
    pub unsupported_reason: Option<UnsupportedContentReason>,
    pub last_failure: Option<ProviderFailureClass>,
    pub evidence: Option<MatchEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaAssetWrite {
    pub game_id: GameId,
    pub provider_id: MetadataProviderId,
    pub kind: MediaAssetKind,
    pub state: MediaAssetState,
    pub provider_media_type: Option<String>,
    pub region: Option<String>,
    pub cache_relative_path: Option<String>,
    pub content_type: Option<String>,
    pub size_bytes: Option<u64>,
    pub content_sha256: Option<String>,
    pub provider_crc32: Option<String>,
    pub provider_md5: Option<String>,
    pub provider_sha1: Option<String>,
    pub source_credit: Option<String>,
    pub last_failure: Option<ProviderFailureClass>,
    pub fetched_at: Option<UnixTimestamp>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JobCounts {
    pub pending: i64,
    pub deferred: i64,
    pub failed: i64,
}

#[derive(Clone)]
pub struct MetadataRepository {
    pool: SqlitePool,
}

impl MetadataRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // ----------------------------------------------------------------- provider match & evidence

    pub async fn load_match(
        &self,
        game_id: GameId,
        provider_id: MetadataProviderId,
    ) -> Result<Option<ProviderMatch>, AppError> {
        let Some(row) = sqlx::query(
            "SELECT id, game_id, provider_id, status, match_type, provider_game_id, \
             provider_rom_id, unsupported_reason, last_failure, last_checked_at, last_matched_at, \
             created_at, updated_at FROM provider_matches WHERE game_id = ? AND provider_id = ?",
        )
        .bind(game_id.0)
        .bind(provider_id.as_db())
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?
        else {
            return Ok(None);
        };

        let mut provider_match = provider_match_from_row(&row)?;
        provider_match.evidence = self.load_evidence(provider_match.id).await?;
        provider_match.candidates = self.load_candidates(provider_match.id).await?;
        Ok(Some(provider_match))
    }

    async fn load_evidence(
        &self,
        provider_match_id: ProviderMatchId,
    ) -> Result<Option<MatchEvidence>, AppError> {
        let row = sqlx::query(
            "SELECT game_id, content_unit_id, system_id, content_unit_kind, content_file_id, \
             size_bytes, crc32, md5, sha1, fingerprint, evidence_version \
             FROM provider_match_evidence WHERE provider_match_id = ?",
        )
        .bind(provider_match_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        row.as_ref().map(evidence_from_row).transpose()
    }

    async fn load_candidates(
        &self,
        provider_match_id: ProviderMatchId,
    ) -> Result<Vec<ProviderCandidate>, AppError> {
        let rows = sqlx::query(
            "SELECT provider_game_id, title, release_date FROM provider_match_candidates \
             WHERE provider_match_id = ? ORDER BY ordinal ASC",
        )
        .bind(provider_match_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| ProviderCandidate {
                provider_game_id: row.get("provider_game_id"),
                title: row.get("title"),
                release_date: row.get("release_date"),
            })
            .collect())
    }

    /// Writes the provider relationship and its evidence snapshot in one transaction.
    ///
    /// Evidence is replaced together with the match so a stored match can never be paired with the
    /// evidence of a previous one.
    pub async fn persist_match(
        &self,
        write: &ProviderMatchWrite,
        now: UnixTimestamp,
    ) -> Result<ProviderMatchId, AppError> {
        let matched_at = matches!(write.status, ProviderMatchStatus::Matched).then_some(now);
        let mut transaction = self.pool.begin().await.map_err(AppError::Database)?;

        sqlx::query(
            "INSERT INTO provider_matches (game_id, provider_id, status, match_type, \
             provider_game_id, provider_rom_id, unsupported_reason, last_failure, \
             last_checked_at, last_matched_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(game_id, provider_id) DO UPDATE SET \
             status = excluded.status, match_type = excluded.match_type, \
             provider_game_id = excluded.provider_game_id, \
             provider_rom_id = excluded.provider_rom_id, \
             unsupported_reason = excluded.unsupported_reason, \
             last_failure = excluded.last_failure, \
             last_checked_at = excluded.last_checked_at, \
             last_matched_at = COALESCE(excluded.last_matched_at, provider_matches.last_matched_at), \
             updated_at = excluded.updated_at",
        )
        .bind(write.game_id.0)
        .bind(write.provider_id.as_db())
        .bind(write.status.as_db())
        .bind(write.match_type.map(MatchType::as_db))
        .bind(write.provider_game_id.as_deref())
        .bind(write.provider_rom_id.as_deref())
        .bind(write.unsupported_reason.map(UnsupportedContentReason::as_db))
        .bind(write.last_failure.map(ProviderFailureClass::as_db))
        .bind(now)
        .bind(matched_at)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        let provider_match_id: i64 = sqlx::query_scalar(
            "SELECT id FROM provider_matches WHERE game_id = ? AND provider_id = ?",
        )
        .bind(write.game_id.0)
        .bind(write.provider_id.as_db())
        .fetch_one(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        sqlx::query("DELETE FROM provider_match_evidence WHERE provider_match_id = ?")
            .bind(provider_match_id)
            .execute(&mut *transaction)
            .await
            .map_err(AppError::Database)?;

        if let (Some(evidence), Some(match_type)) = (write.evidence.as_ref(), write.match_type) {
            sqlx::query(
                "INSERT INTO provider_match_evidence (provider_match_id, game_id, \
                 content_unit_id, system_id, content_unit_kind, content_file_id, size_bytes, \
                 crc32, md5, sha1, fingerprint, match_type, evidence_version, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(provider_match_id)
            .bind(evidence.game_id.0)
            .bind(evidence.content_unit_id.0)
            .bind(evidence.system_id.as_str())
            .bind(evidence.content_unit_kind.as_db())
            .bind(evidence.content_file_id.map(|id| id.0))
            .bind(sqlite_size(evidence.size_bytes)?)
            .bind(evidence.crc32.as_deref())
            .bind(evidence.md5.as_deref())
            .bind(evidence.sha1.as_deref())
            .bind(evidence.fingerprint.as_deref())
            .bind(match_type.as_db())
            .bind(evidence.evidence_version)
            .bind(now)
            .execute(&mut *transaction)
            .await
            .map_err(AppError::Database)?;
        }

        transaction.commit().await.map_err(AppError::Database)?;
        Ok(ProviderMatchId(provider_match_id))
    }

    /// Marks a match as needing revalidation without touching cached metadata or media.
    ///
    /// The stored evidence is deliberately kept so the reason for invalidation stays inspectable.
    pub async fn mark_match_stale(
        &self,
        game_id: GameId,
        provider_id: MetadataProviderId,
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE provider_matches SET status = 'stale', updated_at = ? \
             WHERE game_id = ? AND provider_id = ? AND status != 'stale'",
        )
        .bind(now)
        .bind(game_id.0)
        .bind(provider_id.as_db())
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    /// Replaces the heuristic candidate list for one match.
    pub async fn replace_candidates(
        &self,
        provider_match_id: ProviderMatchId,
        candidates: &[ProviderCandidate],
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        let mut transaction = self.pool.begin().await.map_err(AppError::Database)?;
        sqlx::query("DELETE FROM provider_match_candidates WHERE provider_match_id = ?")
            .bind(provider_match_id.0)
            .execute(&mut *transaction)
            .await
            .map_err(AppError::Database)?;
        for (ordinal, candidate) in candidates.iter().enumerate() {
            sqlx::query(
                "INSERT INTO provider_match_candidates (provider_match_id, ordinal, \
                 provider_game_id, title, release_date, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(provider_match_id.0)
            .bind(i64::try_from(ordinal).unwrap_or(i64::MAX))
            .bind(&candidate.provider_game_id)
            .bind(&candidate.title)
            .bind(candidate.release_date.as_deref())
            .bind(now)
            .execute(&mut *transaction)
            .await
            .map_err(AppError::Database)?;
        }
        transaction.commit().await.map_err(AppError::Database)?;
        Ok(())
    }

    // -------------------------------------------------------------------------- normalized data

    pub async fn load_metadata(
        &self,
        game_id: GameId,
        provider_id: MetadataProviderId,
    ) -> Result<Option<ProviderMetadataRecord>, AppError> {
        let row = sqlx::query(
            "SELECT provider_game_id, title, sort_title, synopsis, release_date, developer, \
             publisher, genre, players, region, source_credit, fetched_at \
             FROM provider_metadata WHERE game_id = ? AND provider_id = ?",
        )
        .bind(game_id.0)
        .bind(provider_id.as_db())
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(row.map(|row| ProviderMetadataRecord {
            metadata: NormalizedMetadata {
                title: row.get("title"),
                sort_title: row.get("sort_title"),
                synopsis: row.get("synopsis"),
                release_date: row.get("release_date"),
                developer: row.get("developer"),
                publisher: row.get("publisher"),
                genre: row.get("genre"),
                players: row.get("players"),
                region: row.get("region"),
            },
            provenance: MetadataProvenance {
                provider_id,
                provider_game_id: row.get("provider_game_id"),
                source_credit: row.get("source_credit"),
                fetched_at: row.get("fetched_at"),
            },
        }))
    }

    /// Replaces provider-derived metadata atomically.
    ///
    /// The write is all-or-nothing, so a failed refresh always leaves the previous snapshot intact.
    pub async fn persist_metadata(
        &self,
        game_id: GameId,
        record: &ProviderMetadataRecord,
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO provider_metadata (game_id, provider_id, provider_game_id, title, \
             sort_title, synopsis, release_date, developer, publisher, genre, players, region, \
             source_credit, fetched_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(game_id, provider_id) DO UPDATE SET \
             provider_game_id = excluded.provider_game_id, title = excluded.title, \
             sort_title = excluded.sort_title, synopsis = excluded.synopsis, \
             release_date = excluded.release_date, developer = excluded.developer, \
             publisher = excluded.publisher, genre = excluded.genre, players = excluded.players, \
             region = excluded.region, source_credit = excluded.source_credit, \
             fetched_at = excluded.fetched_at, updated_at = excluded.updated_at",
        )
        .bind(game_id.0)
        .bind(record.provenance.provider_id.as_db())
        .bind(&record.provenance.provider_game_id)
        .bind(&record.metadata.title)
        .bind(record.metadata.sort_title.as_deref())
        .bind(record.metadata.synopsis.as_deref())
        .bind(record.metadata.release_date.as_deref())
        .bind(record.metadata.developer.as_deref())
        .bind(record.metadata.publisher.as_deref())
        .bind(record.metadata.genre.as_deref())
        .bind(record.metadata.players.as_deref())
        .bind(record.metadata.region.as_deref())
        .bind(record.provenance.source_credit.as_deref())
        .bind(record.provenance.fetched_at)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    // ------------------------------------------------------------------------------------ media

    pub async fn load_media_asset(
        &self,
        game_id: GameId,
        provider_id: MetadataProviderId,
        kind: MediaAssetKind,
    ) -> Result<Option<MediaAsset>, AppError> {
        let row = sqlx::query(
            "SELECT game_id, provider_id, kind, state, provider_media_type, region, \
             cache_relative_path, content_type, size_bytes, content_sha256, provider_crc32, \
             provider_md5, provider_sha1, source_credit, last_failure, fetched_at, updated_at \
             FROM provider_media_assets WHERE game_id = ? AND provider_id = ? AND kind = ?",
        )
        .bind(game_id.0)
        .bind(provider_id.as_db())
        .bind(kind.as_db())
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        row.as_ref().map(media_asset_from_row).transpose()
    }

    /// Records a successful cover publication. Only called after the file is safely in place.
    pub async fn persist_media_asset(
        &self,
        write: &MediaAssetWrite,
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO provider_media_assets (game_id, provider_id, kind, state, \
             provider_media_type, region, cache_relative_path, content_type, size_bytes, \
             content_sha256, provider_crc32, provider_md5, provider_sha1, source_credit, \
             last_failure, fetched_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(game_id, provider_id, kind) DO UPDATE SET \
             state = excluded.state, provider_media_type = excluded.provider_media_type, \
             region = excluded.region, cache_relative_path = excluded.cache_relative_path, \
             content_type = excluded.content_type, size_bytes = excluded.size_bytes, \
             content_sha256 = excluded.content_sha256, provider_crc32 = excluded.provider_crc32, \
             provider_md5 = excluded.provider_md5, provider_sha1 = excluded.provider_sha1, \
             source_credit = excluded.source_credit, last_failure = excluded.last_failure, \
             fetched_at = excluded.fetched_at, updated_at = excluded.updated_at",
        )
        .bind(write.game_id.0)
        .bind(write.provider_id.as_db())
        .bind(write.kind.as_db())
        .bind(write.state.as_db())
        .bind(write.provider_media_type.as_deref())
        .bind(write.region.as_deref())
        .bind(write.cache_relative_path.as_deref())
        .bind(write.content_type.as_deref())
        .bind(write.size_bytes.map(sqlite_size).transpose()?)
        .bind(write.content_sha256.as_deref())
        .bind(write.provider_crc32.as_deref())
        .bind(write.provider_md5.as_deref())
        .bind(write.provider_sha1.as_deref())
        .bind(write.source_credit.as_deref())
        .bind(write.last_failure.map(ProviderFailureClass::as_db))
        .bind(write.fetched_at)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    /// Records a media failure while preserving every field of the last-known-good asset.
    ///
    /// A failed refresh must never blank out a cover that is still on disk, so this updates only
    /// the failure marker and, when nothing was ever cached, the state.
    pub async fn record_media_failure(
        &self,
        game_id: GameId,
        provider_id: MetadataProviderId,
        kind: MediaAssetKind,
        failure: ProviderFailureClass,
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO provider_media_assets (game_id, provider_id, kind, state, last_failure, \
             created_at, updated_at) VALUES (?, ?, ?, 'failed', ?, ?, ?) \
             ON CONFLICT(game_id, provider_id, kind) DO UPDATE SET \
             last_failure = excluded.last_failure, updated_at = excluded.updated_at, \
             state = CASE WHEN provider_media_assets.cache_relative_path IS NULL \
                          THEN 'failed' ELSE provider_media_assets.state END",
        )
        .bind(game_id.0)
        .bind(provider_id.as_db())
        .bind(kind.as_db())
        .bind(failure.as_db())
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    // ------------------------------------------------------------------------------------- jobs

    /// Adds work for a game, or re-arms an existing job without losing its identity.
    ///
    /// A completed or failed job of the same kind is reset to pending so a manual request always
    /// has an effect, while a job that is already pending or running is left alone.
    pub async fn enqueue_job(
        &self,
        game_id: GameId,
        provider_id: MetadataProviderId,
        kind: MetadataJobKind,
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO metadata_jobs (game_id, provider_id, kind, state, priority, attempts, \
             created_at, updated_at) VALUES (?, ?, ?, 'pending', ?, 0, ?, ?) \
             ON CONFLICT(game_id, provider_id, kind) DO UPDATE SET \
             state = CASE WHEN metadata_jobs.state IN ('completed', 'failed', 'deferred') \
                          THEN 'pending' ELSE metadata_jobs.state END, \
             attempts = CASE WHEN metadata_jobs.state IN ('completed', 'failed') \
                             THEN 0 ELSE metadata_jobs.attempts END, \
             earliest_next_attempt_at = CASE \
                 WHEN metadata_jobs.state IN ('completed', 'failed') THEN NULL \
                 ELSE metadata_jobs.earliest_next_attempt_at END, \
             updated_at = excluded.updated_at",
        )
        .bind(game_id.0)
        .bind(provider_id.as_db())
        .bind(kind.as_db())
        .bind(kind.default_priority())
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    pub async fn load_jobs_for_game(
        &self,
        game_id: GameId,
        provider_id: MetadataProviderId,
    ) -> Result<Vec<MetadataJob>, AppError> {
        let rows = sqlx::query(
            "SELECT id, game_id, provider_id, kind, state, priority, attempts, last_failure, \
             earliest_next_attempt_at, claimed_at, created_at, updated_at FROM metadata_jobs \
             WHERE game_id = ? AND provider_id = ? ORDER BY priority ASC, id ASC",
        )
        .bind(game_id.0)
        .bind(provider_id.as_db())
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        rows.iter().map(job_from_row).collect()
    }

    /// Atomically claims up to `limit` runnable jobs and marks them running.
    ///
    /// Claiming inside one transaction is what keeps concurrent workers and a restarted process
    /// from processing the same job twice.
    pub async fn claim_ready_jobs(
        &self,
        provider_id: MetadataProviderId,
        now: UnixTimestamp,
        limit: usize,
    ) -> Result<Vec<MetadataJob>, AppError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut transaction = self.pool.begin().await.map_err(AppError::Database)?;
        let rows = sqlx::query(
            "SELECT id, game_id, provider_id, kind, state, priority, attempts, last_failure, \
             earliest_next_attempt_at, claimed_at, created_at, updated_at FROM metadata_jobs \
             WHERE provider_id = ? AND state IN ('pending', 'deferred') \
             AND (earliest_next_attempt_at IS NULL OR earliest_next_attempt_at <= ?) \
             ORDER BY priority ASC, id ASC LIMIT ?",
        )
        .bind(provider_id.as_db())
        .bind(now)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

        let mut jobs = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut job = job_from_row(row)?;
            sqlx::query(
                "UPDATE metadata_jobs SET state = 'running', claimed_at = ?, updated_at = ? \
                 WHERE id = ?",
            )
            .bind(now)
            .bind(now)
            .bind(job.id.0)
            .execute(&mut *transaction)
            .await
            .map_err(AppError::Database)?;
            job.state = MetadataJobState::Running;
            job.claimed_at = Some(now);
            jobs.push(job);
        }
        transaction.commit().await.map_err(AppError::Database)?;
        Ok(jobs)
    }

    /// Returns jobs stuck in `running` to `pending` after an unclean shutdown.
    ///
    /// Called during startup so a crash mid-request can never leave work permanently claimed.
    pub async fn recover_claimed_jobs(&self, now: UnixTimestamp) -> Result<u64, AppError> {
        let result = sqlx::query(
            "UPDATE metadata_jobs SET state = 'pending', claimed_at = NULL, updated_at = ? \
             WHERE state = 'running'",
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(result.rows_affected())
    }

    /// Returns a specific set of claimed jobs to `pending`.
    ///
    /// Used when a scheduling round is abandoned part-way through: the jobs it claimed but never
    /// reached must not stay `running` until the next process start.
    pub async fn release_claimed_jobs(
        &self,
        job_ids: &[MetadataJobId],
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        if job_ids.is_empty() {
            return Ok(());
        }
        let mut transaction = self.pool.begin().await.map_err(AppError::Database)?;
        for job_id in job_ids {
            sqlx::query(
                "UPDATE metadata_jobs SET state = 'pending', claimed_at = NULL, updated_at = ? \
                 WHERE id = ? AND state = 'running'",
            )
            .bind(now)
            .bind(job_id.0)
            .execute(&mut *transaction)
            .await
            .map_err(AppError::Database)?;
        }
        transaction.commit().await.map_err(AppError::Database)?;
        Ok(())
    }

    /// Returns jobs whose claim has outlived its lease to `pending`.
    ///
    /// Startup recovery cannot help a claim that leaks while the process keeps running — for
    /// example when a storage failure abandons a scheduling round — so the worker re-arms expired
    /// claims on every pass as well. The lease is far longer than any legitimate job takes.
    pub async fn recover_expired_claims(
        &self,
        claimed_before: UnixTimestamp,
        now: UnixTimestamp,
    ) -> Result<u64, AppError> {
        let result = sqlx::query(
            "UPDATE metadata_jobs SET state = 'pending', claimed_at = NULL, updated_at = ? \
             WHERE state = 'running' AND (claimed_at IS NULL OR claimed_at <= ?)",
        )
        .bind(now)
        .bind(claimed_before)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(result.rows_affected())
    }

    pub async fn complete_job(
        &self,
        job_id: MetadataJobId,
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE metadata_jobs SET state = 'completed', claimed_at = NULL, last_failure = NULL, \
             earliest_next_attempt_at = NULL, updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(job_id.0)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    /// Defers a job until `earliest_next_attempt_at`, optionally counting a retry attempt.
    ///
    /// Provider quota deferral passes `count_attempt = false` so waiting for capacity never
    /// consumes the bounded retry budget reserved for genuinely transient faults.
    pub async fn defer_job(
        &self,
        job_id: MetadataJobId,
        failure: ProviderFailureClass,
        earliest_next_attempt_at: UnixTimestamp,
        count_attempt: bool,
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE metadata_jobs SET state = 'deferred', claimed_at = NULL, last_failure = ?, \
             attempts = attempts + ?, earliest_next_attempt_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(failure.as_db())
        .bind(i64::from(count_attempt))
        .bind(earliest_next_attempt_at)
        .bind(now)
        .bind(job_id.0)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    /// Marks a job failed. It stays inspectable and can be re-armed by an explicit user request.
    pub async fn fail_job(
        &self,
        job_id: MetadataJobId,
        failure: ProviderFailureClass,
        count_attempt: bool,
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE metadata_jobs SET state = 'failed', claimed_at = NULL, last_failure = ?, \
             attempts = attempts + ?, earliest_next_attempt_at = NULL, updated_at = ? WHERE id = ?",
        )
        .bind(failure.as_db())
        .bind(i64::from(count_attempt))
        .bind(now)
        .bind(job_id.0)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    pub async fn job_counts(&self, provider_id: MetadataProviderId) -> Result<JobCounts, AppError> {
        let row = sqlx::query(
            "SELECT \
             SUM(state IN ('pending', 'running')) AS pending, \
             SUM(state = 'deferred') AS deferred, \
             SUM(state = 'failed') AS failed \
             FROM metadata_jobs WHERE provider_id = ?",
        )
        .bind(provider_id.as_db())
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(JobCounts {
            pending: row
                .try_get::<Option<i64>, _>("pending")
                .unwrap_or(None)
                .unwrap_or_default(),
            deferred: row
                .try_get::<Option<i64>, _>("deferred")
                .unwrap_or(None)
                .unwrap_or_default(),
            failed: row
                .try_get::<Option<i64>, _>("failed")
                .unwrap_or(None)
                .unwrap_or_default(),
        })
    }

    /// Games that have no provider relationship yet, in a bounded batch.
    pub async fn games_needing_metadata(
        &self,
        provider_id: MetadataProviderId,
        limit: usize,
    ) -> Result<Vec<GameId>, AppError> {
        let rows = sqlx::query(
            "SELECT g.id FROM games g \
             LEFT JOIN provider_matches pm ON pm.game_id = g.id AND pm.provider_id = ? \
             LEFT JOIN metadata_jobs mj ON mj.game_id = g.id AND mj.provider_id = ? \
             WHERE pm.id IS NULL AND mj.id IS NULL ORDER BY g.id ASC LIMIT ?",
        )
        .bind(provider_id.as_db())
        .bind(provider_id.as_db())
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(rows.into_iter().map(|row| GameId(row.get("id"))).collect())
    }

    /// Games whose accepted match should be revalidated, in a bounded batch.
    ///
    /// `after_game_id` lets the caller walk the whole set one batch at a time instead of
    /// re-examining the same lowest identifiers on every sweep.
    pub async fn matched_games(
        &self,
        provider_id: MetadataProviderId,
        after_game_id: i64,
        limit: usize,
    ) -> Result<Vec<GameId>, AppError> {
        let rows = sqlx::query(
            "SELECT game_id FROM provider_matches WHERE provider_id = ? AND status = 'matched' \
             AND game_id > ? ORDER BY game_id ASC LIMIT ?",
        )
        .bind(provider_id.as_db())
        .bind(after_game_id)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| GameId(row.get("game_id")))
            .collect())
    }

    // -------------------------------------------------------------------------- scheduler state

    pub async fn load_scheduler_state(
        &self,
        provider_id: MetadataProviderId,
    ) -> Result<ProviderSchedulerState, AppError> {
        let Some(row) = sqlx::query(
            "SELECT max_threads, max_requests_per_minute, max_requests_per_day, \
             max_negative_requests_per_day, requests_today, negative_requests_today, \
             observed_at, deferred_until, defer_reason, consecutive_transport_failures \
             FROM provider_scheduler_state WHERE provider_id = ?",
        )
        .bind(provider_id.as_db())
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?
        else {
            return Ok(ProviderSchedulerState::empty(provider_id));
        };

        Ok(ProviderSchedulerState {
            provider_id,
            quota: ProviderQuotaSnapshot {
                max_threads: row.get("max_threads"),
                max_requests_per_minute: row.get("max_requests_per_minute"),
                max_requests_per_day: row.get("max_requests_per_day"),
                max_negative_requests_per_day: row.get("max_negative_requests_per_day"),
                requests_today: row.get("requests_today"),
                negative_requests_today: row.get("negative_requests_today"),
            },
            observed_at: row.get("observed_at"),
            deferred_until: row.get("deferred_until"),
            defer_reason: row
                .get::<Option<String>, _>("defer_reason")
                .as_deref()
                .and_then(ProviderFailureClass::from_db),
            consecutive_transport_failures: row.get("consecutive_transport_failures"),
        })
    }

    /// Stores the provider's own latest quota numbers.
    pub async fn update_quota(
        &self,
        provider_id: MetadataProviderId,
        quota: &ProviderQuotaSnapshot,
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO provider_scheduler_state (provider_id, max_threads, \
             max_requests_per_minute, max_requests_per_day, max_negative_requests_per_day, \
             requests_today, negative_requests_today, observed_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(provider_id) DO UPDATE SET \
             max_threads = excluded.max_threads, \
             max_requests_per_minute = excluded.max_requests_per_minute, \
             max_requests_per_day = excluded.max_requests_per_day, \
             max_negative_requests_per_day = excluded.max_negative_requests_per_day, \
             requests_today = excluded.requests_today, \
             negative_requests_today = excluded.negative_requests_today, \
             observed_at = excluded.observed_at, updated_at = excluded.updated_at",
        )
        .bind(provider_id.as_db())
        .bind(quota.max_threads)
        .bind(quota.max_requests_per_minute)
        .bind(quota.max_requests_per_day)
        .bind(quota.max_negative_requests_per_day)
        .bind(quota.requests_today)
        .bind(quota.negative_requests_today)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    /// Persists a provider-wide deferral so it survives restart.
    pub async fn set_provider_deferral(
        &self,
        provider_id: MetadataProviderId,
        reason: ProviderFailureClass,
        deferred_until: UnixTimestamp,
        transport_failure: bool,
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO provider_scheduler_state (provider_id, deferred_until, defer_reason, \
             consecutive_transport_failures, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(provider_id) DO UPDATE SET \
             deferred_until = excluded.deferred_until, defer_reason = excluded.defer_reason, \
             consecutive_transport_failures = CASE WHEN ? \
                 THEN provider_scheduler_state.consecutive_transport_failures + 1 ELSE 0 END, \
             updated_at = excluded.updated_at",
        )
        .bind(provider_id.as_db())
        .bind(deferred_until)
        .bind(reason.as_db())
        .bind(i64::from(transport_failure))
        .bind(now)
        .bind(now)
        .bind(transport_failure)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    pub async fn clear_provider_deferral(
        &self,
        provider_id: MetadataProviderId,
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO provider_scheduler_state (provider_id, created_at, updated_at) \
             VALUES (?, ?, ?) ON CONFLICT(provider_id) DO UPDATE SET \
             deferred_until = NULL, defer_reason = NULL, \
             consecutive_transport_failures = 0, updated_at = excluded.updated_at",
        )
        .bind(provider_id.as_db())
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    // ------------------------------------------------------------------ optional personal account

    pub async fn load_user_account(
        &self,
        provider_id: MetadataProviderId,
    ) -> Result<Option<ProviderUserAccountRecord>, AppError> {
        let row = sqlx::query(
            "SELECT vault_reference, state FROM provider_user_accounts WHERE provider_id = ?",
        )
        .bind(provider_id.as_db())
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        row.map(|row| {
            let state = row.get::<String, _>("state");
            Ok(ProviderUserAccountRecord {
                vault_reference: row.get("vault_reference"),
                state: UserAccountState::from_db(&state).ok_or_else(|| {
                    AppError::Metadata(format!("invalid provider account state: {state}"))
                })?,
            })
        })
        .transpose()
    }

    pub async fn persist_user_account(
        &self,
        provider_id: MetadataProviderId,
        vault_reference: &str,
        state: UserAccountState,
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        let state = state.as_db().ok_or_else(|| {
            AppError::Metadata("only a configured or invalid account can be persisted".to_owned())
        })?;
        sqlx::query(
            "INSERT INTO provider_user_accounts (provider_id, vault_reference, state, \
             created_at, updated_at) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(provider_id) DO UPDATE SET \
             vault_reference = excluded.vault_reference, state = excluded.state, \
             updated_at = excluded.updated_at",
        )
        .bind(provider_id.as_db())
        .bind(vault_reference)
        .bind(state)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    pub async fn delete_user_account(
        &self,
        provider_id: MetadataProviderId,
    ) -> Result<(), AppError> {
        sqlx::query("DELETE FROM provider_user_accounts WHERE provider_id = ?")
            .bind(provider_id.as_db())
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;
        Ok(())
    }

    // ------------------------------------------------------------------------ user-owned decisions

    pub async fn load_user_selection(
        &self,
        game_id: GameId,
        provider_id: MetadataProviderId,
    ) -> Result<Option<UserProviderSelection>, AppError> {
        let row = sqlx::query(
            "SELECT game_id, provider_game_id, updated_at FROM user_provider_selections \
             WHERE game_id = ? AND provider_id = ?",
        )
        .bind(game_id.0)
        .bind(provider_id.as_db())
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(row.map(|row| UserProviderSelection {
            game_id: GameId(row.get("game_id")),
            provider_id,
            provider_game_id: row.get("provider_game_id"),
            updated_at: row.get("updated_at"),
        }))
    }

    pub async fn persist_user_selection(
        &self,
        game_id: GameId,
        provider_id: MetadataProviderId,
        provider_game_id: &str,
        now: UnixTimestamp,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO user_provider_selections (game_id, provider_id, provider_game_id, \
             created_at, updated_at) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(game_id, provider_id) DO UPDATE SET \
             provider_game_id = excluded.provider_game_id, updated_at = excluded.updated_at",
        )
        .bind(game_id.0)
        .bind(provider_id.as_db())
        .bind(provider_game_id)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    pub async fn delete_user_selection(
        &self,
        game_id: GameId,
        provider_id: MetadataProviderId,
    ) -> Result<(), AppError> {
        sqlx::query("DELETE FROM user_provider_selections WHERE game_id = ? AND provider_id = ?")
            .bind(game_id.0)
            .bind(provider_id.as_db())
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;
        Ok(())
    }

    // ------------------------------------------------------------------------------- composite read

    /// Loads everything persisted for one game and provider.
    pub async fn load_stored_metadata(
        &self,
        game_id: GameId,
        provider_id: MetadataProviderId,
    ) -> Result<StoredMetadata, AppError> {
        Ok(StoredMetadata {
            provider_match: self.load_match(game_id, provider_id).await?,
            metadata: self.load_metadata(game_id, provider_id).await?,
            cover: self
                .load_media_asset(game_id, provider_id, MediaAssetKind::Cover)
                .await?,
            user_selection: self.load_user_selection(game_id, provider_id).await?,
            jobs: self.load_jobs_for_game(game_id, provider_id).await?,
        })
    }
}

fn provider_match_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ProviderMatch, AppError> {
    let provider = row.get::<String, _>("provider_id");
    let status = row.get::<String, _>("status");
    Ok(ProviderMatch {
        id: ProviderMatchId(row.get("id")),
        game_id: GameId(row.get("game_id")),
        provider_id: MetadataProviderId::from_db(&provider).ok_or_else(|| {
            AppError::Metadata(format!("invalid provider identifier: {provider}"))
        })?,
        status: ProviderMatchStatus::from_db(&status).ok_or_else(|| {
            AppError::Metadata(format!("invalid provider match status: {status}"))
        })?,
        match_type: optional_enum(row, "match_type", MatchType::from_db, "match type")?,
        provider_game_id: row.get("provider_game_id"),
        provider_rom_id: row.get("provider_rom_id"),
        unsupported_reason: optional_enum(
            row,
            "unsupported_reason",
            UnsupportedContentReason::from_db,
            "unsupported reason",
        )?,
        last_failure: optional_enum(
            row,
            "last_failure",
            ProviderFailureClass::from_db,
            "failure class",
        )?,
        last_checked_at: row.get("last_checked_at"),
        last_matched_at: row.get("last_matched_at"),
        evidence: None,
        candidates: Vec::new(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn evidence_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<MatchEvidence, AppError> {
    let system = row.get::<String, _>("system_id");
    let kind = row.get::<String, _>("content_unit_kind");
    Ok(MatchEvidence {
        game_id: GameId(row.get("game_id")),
        content_unit_id: ContentUnitId(row.get("content_unit_id")),
        system_id: SystemId::from_str(&system)
            .ok_or_else(|| AppError::Metadata(format!("invalid system identifier: {system}")))?,
        content_unit_kind: ContentUnitKind::from_db(&kind)
            .ok_or_else(|| AppError::Metadata(format!("invalid content unit kind: {kind}")))?,
        content_file_id: row
            .get::<Option<i64>, _>("content_file_id")
            .map(crate::domain::library::ContentFileId),
        size_bytes: u64_value(row.get("size_bytes"))?,
        crc32: row.get("crc32"),
        md5: row.get("md5"),
        sha1: row.get("sha1"),
        fingerprint: row.get("fingerprint"),
        evidence_version: row.get("evidence_version"),
    })
}

fn media_asset_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<MediaAsset, AppError> {
    let provider = row.get::<String, _>("provider_id");
    let kind = row.get::<String, _>("kind");
    let state = row.get::<String, _>("state");
    Ok(MediaAsset {
        game_id: GameId(row.get("game_id")),
        provider_id: MetadataProviderId::from_db(&provider).ok_or_else(|| {
            AppError::Metadata(format!("invalid provider identifier: {provider}"))
        })?,
        kind: MediaAssetKind::from_db(&kind)
            .ok_or_else(|| AppError::Metadata(format!("invalid media asset kind: {kind}")))?,
        state: MediaAssetState::from_db(&state)
            .ok_or_else(|| AppError::Metadata(format!("invalid media asset state: {state}")))?,
        provider_media_type: row.get("provider_media_type"),
        region: row.get("region"),
        cache_relative_path: row.get("cache_relative_path"),
        content_type: row.get("content_type"),
        size_bytes: row
            .get::<Option<i64>, _>("size_bytes")
            .map(u64_value)
            .transpose()?,
        content_sha256: row.get("content_sha256"),
        provider_crc32: row.get("provider_crc32"),
        provider_md5: row.get("provider_md5"),
        provider_sha1: row.get("provider_sha1"),
        source_credit: row.get("source_credit"),
        last_failure: optional_enum(
            row,
            "last_failure",
            ProviderFailureClass::from_db,
            "failure class",
        )?,
        fetched_at: row.get("fetched_at"),
        updated_at: row.get("updated_at"),
    })
}

fn job_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<MetadataJob, AppError> {
    let provider = row.get::<String, _>("provider_id");
    let kind = row.get::<String, _>("kind");
    let state = row.get::<String, _>("state");
    Ok(MetadataJob {
        id: MetadataJobId(row.get("id")),
        game_id: GameId(row.get("game_id")),
        provider_id: MetadataProviderId::from_db(&provider).ok_or_else(|| {
            AppError::Metadata(format!("invalid provider identifier: {provider}"))
        })?,
        kind: MetadataJobKind::from_db(&kind)
            .ok_or_else(|| AppError::Metadata(format!("invalid metadata job kind: {kind}")))?,
        state: MetadataJobState::from_db(&state)
            .ok_or_else(|| AppError::Metadata(format!("invalid metadata job state: {state}")))?,
        priority: row.get("priority"),
        attempts: row.get("attempts"),
        last_failure: optional_enum(
            row,
            "last_failure",
            ProviderFailureClass::from_db,
            "failure class",
        )?,
        earliest_next_attempt_at: row.get("earliest_next_attempt_at"),
        claimed_at: row.get("claimed_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn optional_enum<T>(
    row: &sqlx::sqlite::SqliteRow,
    column: &str,
    parse: impl Fn(&str) -> Option<T>,
    label: &str,
) -> Result<Option<T>, AppError> {
    row.get::<Option<String>, _>(column)
        .map(|value| {
            parse(&value).ok_or_else(|| AppError::Metadata(format!("invalid {label}: {value}")))
        })
        .transpose()
}

fn sqlite_size(value: u64) -> Result<i64, AppError> {
    i64::try_from(value)
        .map_err(|_| AppError::Metadata("size is too large for SQLite integer storage".to_owned()))
}

fn u64_value(value: i64) -> Result<u64, AppError> {
    u64::try_from(value)
        .map_err(|_| AppError::Metadata("database contains a negative size".to_owned()))
}
