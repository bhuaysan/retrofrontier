//! Metadata application service.
//!
//! This is the only place that combines the provider, the match policy, the persistent queue, the
//! scheduler, and the cover cache. Three rules govern everything below:
//!
//! 1. Local library identity is authoritative and read-only here. No code path writes to `games`,
//!    `content_units`, `content_files`, or `content_unit_files`.
//! 2. A provider answer becomes an accepted match only when returned evidence agrees with the
//!    current M4 evidence, and only while it keeps agreeing.
//! 3. A failure of any kind may change provider state only. Cached metadata and the cached cover
//!    survive every failed refresh.

use crate::adapters::credentials::{
    CredentialVault, CredentialVaultError, DeveloperCredentials, ProviderCredentialSource,
    SecretString, UserCredentials,
};
use crate::adapters::metadata_paths::MetadataPaths;
use crate::domain::library::{ContentUnit, GameId, UnixTimestamp};
use crate::domain::metadata::{
    evidence_for_unit, GameMetadataState, MatchEvidence, MatchType, MediaAssetKind,
    MediaAssetState, MetadataJob, MetadataJobKind, MetadataProviderId, MetadataProviderStatus,
    ProviderCandidate, ProviderFailureClass, ProviderMatchStatus, ProviderMetadataRecord,
    ProviderQuotaSnapshot, UnsupportedContentReason, UserAccountState,
};
use crate::error::AppError;
use crate::repositories::library::LibraryRepository;
use crate::repositories::metadata::{MediaAssetWrite, MetadataRepository, ProviderMatchWrite};
use crate::services::metadata_matching::{classify_deterministic_match, DeterministicOutcome};
use crate::services::metadata_media::CoverCache;
use crate::services::metadata_provider::{
    CandidateSearchRequest, ContentIdentificationRequest, MetadataProvider,
    ProviderCoverDescriptor, ProviderGameRecord,
};
use crate::services::metadata_queue::{
    failure_action, plan, provider_deferral_ms, Clock, FailureAction, JitterSource, MinuteBudget,
    SchedulingDecision,
};
use std::sync::Arc;
use std::sync::Mutex;

/// Vault key for the optional personal provider account.
///
/// Stable and non-secret: SQLite stores only this reference, never the credential itself.
const USER_CREDENTIAL_VAULT_REFERENCE: &str = "screenscraper-user";

/// Vault service name used for OS keychain entries.
pub const CREDENTIAL_VAULT_SERVICE: &str = "RetroFrontier";

#[derive(Debug, Clone, Copy)]
pub struct MetadataConfig {
    /// Upper bound on in-flight provider requests, further reduced by the provider's own limit.
    pub max_concurrency: usize,
    /// Bounded batch size for background sweeps.
    pub batch_size: usize,
}

impl Default for MetadataConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 2,
            batch_size: 50,
        }
    }
}

/// Result of one scheduling round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcessedJobs {
    pub completed: usize,
    pub deferred: usize,
    pub failed: usize,
    /// Earliest time work may be attempted again, when the round was blocked.
    pub wait_until: Option<UnixTimestamp>,
}

impl ProcessedJobs {
    pub fn total(&self) -> usize {
        self.completed + self.deferred + self.failed
    }
}

/// Outcome of one job, distinguishing provider failures from local storage failures.
enum JobOutcome {
    Done,
    ProviderFailure(ProviderFailureClass),
}

/// Live credential state shared by the provider adapter and the application service.
///
/// The adapter needs a credential source at construction time while the personal account can only
/// be loaded once the database is open, so this type is created first and filled in afterwards.
/// It holds the values in memory only; persistence is the vault's job.
pub struct ProviderCredentialState {
    developer: Option<DeveloperCredentials>,
    user: Mutex<Option<UserCredentials>>,
}

impl ProviderCredentialState {
    pub fn new(developer: Option<DeveloperCredentials>) -> Self {
        Self {
            developer,
            user: Mutex::new(None),
        }
    }

    fn set_user(&self, user: Option<UserCredentials>) {
        *self
            .user
            .lock()
            .expect("provider credential mutex is not poisoned") = user;
    }
}

impl ProviderCredentialSource for ProviderCredentialState {
    fn developer(&self) -> Option<DeveloperCredentials> {
        self.developer.clone()
    }

    fn user(&self) -> Option<UserCredentials> {
        self.user
            .lock()
            .expect("provider credential mutex is not poisoned")
            .clone()
    }
}

pub struct MetadataApplicationService {
    repository: MetadataRepository,
    library: LibraryRepository,
    provider: Arc<dyn MetadataProvider>,
    covers: Arc<CoverCache>,
    vault: Arc<dyn CredentialVault>,
    credentials: Arc<ProviderCredentialState>,
    clock: Arc<dyn Clock>,
    jitter: Arc<dyn JitterSource>,
    minute_budget: MinuteBudget,
    config: MetadataConfig,
    provider_id: MetadataProviderId,
    developer_credentials_configured: bool,
}

impl MetadataApplicationService {
    /// Builds the service and recovers persisted state.
    ///
    /// Recovery is part of construction so a crash mid-request cannot leave jobs claimed forever
    /// and cannot leave partial cover downloads on disk.
    #[allow(clippy::too_many_arguments)]
    pub async fn initialize(
        pool: sqlx::SqlitePool,
        provider: Arc<dyn MetadataProvider>,
        vault: Arc<dyn CredentialVault>,
        credentials: Arc<ProviderCredentialState>,
        paths: MetadataPaths,
        clock: Arc<dyn Clock>,
        jitter: Arc<dyn JitterSource>,
        config: MetadataConfig,
    ) -> Result<Self, AppError> {
        let repository = MetadataRepository::new(pool.clone());
        let library = LibraryRepository::new(pool);
        let provider_id = provider.provider_id();
        let covers = Arc::new(CoverCache::new(paths));
        covers.clean_partial_downloads();

        // A vault failure must never stop the application: personal credentials are optional.
        let stored_account = repository.load_user_account(provider_id).await?;
        if let Some(account) = stored_account.as_ref() {
            match vault.load(&account.vault_reference) {
                Ok(user) => credentials.set_user(user),
                Err(error) => tracing::warn!(
                    error = %error,
                    "optional provider account could not be read from the credential vault"
                ),
            }
        }

        let developer_credentials_configured = credentials.developer().is_some();

        let recovered = repository.recover_claimed_jobs(clock.now_ms()).await?;
        if recovered > 0 {
            tracing::info!(
                jobs = recovered,
                "recovered claimed metadata jobs after restart"
            );
        }

        Ok(Self {
            repository,
            library,
            provider,
            covers,
            vault,
            credentials,
            clock,
            jitter,
            minute_budget: MinuteBudget::new(),
            config,
            provider_id,
            developer_credentials_configured,
        })
    }

    pub fn provider_id(&self) -> MetadataProviderId {
        self.provider_id
    }

    pub fn covers(&self) -> &CoverCache {
        &self.covers
    }

    // ------------------------------------------------------------------------------------ reads

    /// Everything the UI needs for one game.
    ///
    /// This never writes and never calls the provider, so it works identically offline. A match
    /// whose stored evidence no longer agrees with current content is reported as stale here even
    /// before the revalidation sweep persists that, so a read can never overstate a match.
    pub async fn get_metadata_state(&self, game_id: GameId) -> Result<GameMetadataState, AppError> {
        let stored = self
            .repository
            .load_stored_metadata(game_id, self.provider_id)
            .await?;
        let current_evidence = self.current_evidence(game_id).await?;

        let mut status = stored
            .provider_match
            .as_ref()
            .map_or(ProviderMatchStatus::Pending, |provider_match| {
                provider_match.status
            });
        let match_type = stored
            .provider_match
            .as_ref()
            .and_then(|provider_match| provider_match.match_type);
        let evidence_is_current = stored
            .provider_match
            .as_ref()
            .and_then(|provider_match| provider_match.evidence.as_ref())
            .zip(current_evidence.as_ref())
            .is_some_and(|(stored_evidence, current)| stored_evidence.agrees_with(current));

        if status == ProviderMatchStatus::Matched && !evidence_is_current {
            status = ProviderMatchStatus::Stale;
        }
        let deterministic = status == ProviderMatchStatus::Matched
            && match_type.is_some_and(MatchType::is_deterministic);

        Ok(GameMetadataState {
            game_id,
            provider_id: self.provider_id,
            status,
            match_type,
            deterministic,
            provider_game_id: stored
                .provider_match
                .as_ref()
                .and_then(|provider_match| provider_match.provider_game_id.clone()),
            provider_rom_id: stored
                .provider_match
                .as_ref()
                .and_then(|provider_match| provider_match.provider_rom_id.clone()),
            unsupported_reason: stored
                .provider_match
                .as_ref()
                .and_then(|provider_match| provider_match.unsupported_reason),
            last_failure: stored
                .provider_match
                .as_ref()
                .and_then(|provider_match| provider_match.last_failure),
            last_checked_at: stored
                .provider_match
                .as_ref()
                .and_then(|provider_match| provider_match.last_checked_at),
            metadata: stored.metadata,
            cover: self.readable_cover(stored.cover.clone()),
            candidates: stored
                .provider_match
                .as_ref()
                .map(|provider_match| provider_match.candidates.clone())
                .unwrap_or_default(),
            user_selection: stored.user_selection,
            jobs: stored.jobs,
        })
    }

    /// Provider-wide status for diagnostics and settings surfaces.
    pub async fn provider_status(&self) -> Result<MetadataProviderStatus, AppError> {
        let state = self
            .repository
            .load_scheduler_state(self.provider_id)
            .await?;
        let counts = self.repository.job_counts(self.provider_id).await?;
        let (user_account, user_account_name) = self.user_account_state().await?;
        let now = self.clock.now_ms();

        Ok(MetadataProviderStatus {
            provider_id: self.provider_id,
            credentials_configured: self.developer_credentials_configured,
            user_account,
            user_account_name,
            quota: state.quota.clone(),
            quota_observed_at: state.observed_at,
            deferred_until: state.deferred_until,
            defer_reason: state.defer_reason,
            // "Offline" is the observable form of repeated transport failure.
            offline: state.consecutive_transport_failures > 0
                && state.deferred_until.is_some_and(|until| until > now),
            pending_jobs: counts.pending,
            deferred_jobs: counts.deferred,
            failed_jobs: counts.failed,
        })
    }

    /// Reports the personal account state without ever returning the password.
    pub async fn user_account_state(&self) -> Result<(UserAccountState, Option<String>), AppError> {
        let Some(account) = self.repository.load_user_account(self.provider_id).await? else {
            return Ok((UserAccountState::NotConfigured, None));
        };
        match self.vault.load(&account.vault_reference) {
            Ok(Some(user)) => Ok((account.state, Some(user.username))),
            // A record without a vault entry is a configuration that can no longer be used.
            Ok(None) => Ok((UserAccountState::NotConfigured, None)),
            Err(CredentialVaultError::Unavailable) | Err(CredentialVaultError::Malformed) => {
                Ok((UserAccountState::VaultUnavailable, None))
            }
        }
    }

    // ----------------------------------------------------------------------------- user requests

    /// Enqueues identification for one game.
    pub async fn request_enrichment(&self, game_id: GameId) -> Result<(), AppError> {
        self.ensure_game_exists(game_id).await?;
        self.repository
            .enqueue_job(
                game_id,
                self.provider_id,
                MetadataJobKind::Identify,
                self.clock.now_ms(),
            )
            .await
    }

    /// Enqueues a refresh.
    ///
    /// An accepted match refreshes metadata and cover; anything else needs identification first,
    /// because refreshing a provider identity whose evidence no longer holds would re-trust it.
    pub async fn request_refresh(&self, game_id: GameId) -> Result<(), AppError> {
        self.ensure_game_exists(game_id).await?;
        let now = self.clock.now_ms();
        let provider_match = self
            .repository
            .load_match(game_id, self.provider_id)
            .await?;
        let refreshable = provider_match.as_ref().is_some_and(|provider_match| {
            provider_match.status == ProviderMatchStatus::Matched
                && provider_match.provider_game_id.is_some()
        });

        if refreshable {
            self.repository
                .enqueue_job(
                    game_id,
                    self.provider_id,
                    MetadataJobKind::RefreshMetadata,
                    now,
                )
                .await?;
            self.repository
                .enqueue_job(
                    game_id,
                    self.provider_id,
                    MetadataJobKind::RefreshCover,
                    now,
                )
                .await?;
        } else {
            self.repository
                .enqueue_job(game_id, self.provider_id, MetadataJobKind::Identify, now)
                .await?;
        }
        Ok(())
    }

    /// Records a user's manual provider choice.
    ///
    /// Stored as user-owned state in its own table. A provider refresh replaces normalized metadata
    /// and media but never touches this row.
    pub async fn select_provider_candidate(
        &self,
        game_id: GameId,
        provider_game_id: &str,
    ) -> Result<(), AppError> {
        self.ensure_game_exists(game_id).await?;
        if provider_game_id.trim().is_empty() {
            return Err(AppError::Metadata(
                "a provider game identifier is required".to_owned(),
            ));
        }
        let now = self.clock.now_ms();
        self.repository
            .persist_user_selection(game_id, self.provider_id, provider_game_id.trim(), now)
            .await?;
        self.repository
            .enqueue_job(game_id, self.provider_id, MetadataJobKind::Identify, now)
            .await
    }

    pub async fn clear_provider_candidate(&self, game_id: GameId) -> Result<(), AppError> {
        self.repository
            .delete_user_selection(game_id, self.provider_id)
            .await
    }

    /// Stores the optional personal account. The password only ever reaches the OS vault.
    pub async fn set_user_credentials(
        &self,
        username: &str,
        password: SecretString,
    ) -> Result<(), AppError> {
        let username = username.trim();
        if username.is_empty() {
            return Err(AppError::Metadata(
                "a provider username is required".to_owned(),
            ));
        }
        if username.contains('\n') || username.contains('\r') {
            return Err(AppError::Metadata(
                "a provider username must be a single line".to_owned(),
            ));
        }
        if password.is_empty() {
            return Err(AppError::Metadata(
                "a provider password is required".to_owned(),
            ));
        }

        let credentials = UserCredentials {
            username: username.to_owned(),
            password,
        };
        self.vault
            .store(USER_CREDENTIAL_VAULT_REFERENCE, &credentials)
            .map_err(|error| AppError::Metadata(error.to_string()))?;
        self.repository
            .persist_user_account(
                self.provider_id,
                USER_CREDENTIAL_VAULT_REFERENCE,
                UserAccountState::Configured,
                self.clock.now_ms(),
            )
            .await?;
        self.credentials.set_user(Some(credentials));
        Ok(())
    }

    pub async fn clear_user_credentials(&self) -> Result<(), AppError> {
        self.vault
            .delete(USER_CREDENTIAL_VAULT_REFERENCE)
            .map_err(|error| AppError::Metadata(error.to_string()))?;
        self.repository
            .delete_user_account(self.provider_id)
            .await?;
        self.credentials.set_user(None);
        Ok(())
    }

    /// Enqueues identification for games that have no provider relationship yet.
    pub async fn enqueue_missing_metadata(&self) -> Result<usize, AppError> {
        let now = self.clock.now_ms();
        let games = self
            .repository
            .games_needing_metadata(self.provider_id, self.config.batch_size)
            .await?;
        for game_id in &games {
            self.repository
                .enqueue_job(*game_id, self.provider_id, MetadataJobKind::Identify, now)
                .await?;
        }
        Ok(games.len())
    }

    /// Marks accepted matches whose evidence no longer holds and schedules re-identification.
    ///
    /// M4 keeps `GameId`, `ContentUnitId`, and `ContentFileId` stable across same-path byte
    /// replacement, so this sweep is the only thing that notices replaced content. It never deletes
    /// cached metadata or the cover: the last-known-good data stays readable while the match is
    /// untrusted.
    pub async fn revalidate_matches(&self) -> Result<usize, AppError> {
        let now = self.clock.now_ms();
        let games = self
            .repository
            .matched_games(self.provider_id, self.config.batch_size)
            .await?;
        let mut stale = 0;
        for game_id in games {
            let Some(provider_match) = self
                .repository
                .load_match(game_id, self.provider_id)
                .await?
            else {
                continue;
            };
            let Some(stored_evidence) = provider_match.evidence.as_ref() else {
                continue;
            };
            let current = self.current_evidence(game_id).await?;
            let agrees = current
                .as_ref()
                .is_some_and(|current| stored_evidence.agrees_with(current));
            if !agrees {
                self.repository
                    .mark_match_stale(game_id, self.provider_id, now)
                    .await?;
                self.repository
                    .enqueue_job(game_id, self.provider_id, MetadataJobKind::Identify, now)
                    .await?;
                stale += 1;
            }
        }
        Ok(stale)
    }

    // -------------------------------------------------------------------------- job processing

    /// Runs one scheduling round.
    ///
    /// Returns without issuing any request when the provider is deferred or a budget is exhausted,
    /// reporting when to come back. Nothing here sleeps, so the caller controls pacing.
    pub async fn process_ready_jobs(&self, max_jobs: usize) -> Result<ProcessedJobs, AppError> {
        let now = self.clock.now_ms();
        let state = self
            .repository
            .load_scheduler_state(self.provider_id)
            .await?;

        let concurrency = match plan(
            &state,
            now,
            self.config.max_concurrency,
            &self.minute_budget,
        ) {
            SchedulingDecision::WaitUntil(wait_until) => {
                return Ok(ProcessedJobs {
                    wait_until: Some(wait_until),
                    ..ProcessedJobs::default()
                })
            }
            SchedulingDecision::Run { concurrency } => concurrency,
        };

        let limit = max_jobs.min(concurrency);
        let jobs = self
            .repository
            .claim_ready_jobs(self.provider_id, now, limit)
            .await?;
        let mut processed = ProcessedJobs::default();

        for job in jobs {
            // Each provider request must fit the rolling minute budget.
            if let Err(next_slot) = self
                .minute_budget
                .reserve(self.clock.now_ms(), state.quota.max_requests_per_minute)
            {
                self.repository
                    .defer_job(
                        job.id,
                        ProviderFailureClass::CapacityDeferred,
                        next_slot,
                        false,
                        self.clock.now_ms(),
                    )
                    .await?;
                processed.deferred += 1;
                processed.wait_until = Some(next_slot);
                continue;
            }

            match self.run_job(&job).await? {
                JobOutcome::Done => {
                    self.repository
                        .complete_job(job.id, self.clock.now_ms())
                        .await?;
                    processed.completed += 1;
                }
                JobOutcome::ProviderFailure(failure) => {
                    match self.apply_failure(&job, failure).await? {
                        FailureAction::Retry { .. } | FailureAction::Defer { .. } => {
                            processed.deferred += 1
                        }
                        FailureAction::Park => processed.failed += 1,
                        FailureAction::Negative => processed.completed += 1,
                    }
                }
            }
        }

        Ok(processed)
    }

    async fn run_job(&self, job: &MetadataJob) -> Result<JobOutcome, AppError> {
        match job.kind {
            MetadataJobKind::Identify => self.identify(job.game_id).await,
            MetadataJobKind::RefreshMetadata => self.refresh_metadata(job.game_id).await,
            MetadataJobKind::RefreshCover => self.refresh_cover(job.game_id).await,
        }
    }

    /// Applies retry, deferral, and parking policy, and records provider-visible state.
    async fn apply_failure(
        &self,
        job: &MetadataJob,
        failure: ProviderFailureClass,
    ) -> Result<FailureAction, AppError> {
        let now = self.clock.now_ms();
        let action = failure_action(failure, job.attempts, self.jitter.as_ref());

        // A failure that describes the provider rather than this job stops the whole provider for a
        // while, which is what keeps an offline or quota-exhausted client from spinning.
        let transport = matches!(failure, ProviderFailureClass::Transport);
        if failure.defers_provider() || transport {
            let state = self
                .repository
                .load_scheduler_state(self.provider_id)
                .await?;
            let delay = provider_deferral_ms(
                failure,
                state.consecutive_transport_failures,
                self.jitter.as_ref(),
            )
            .max(1);
            self.repository
                .set_provider_deferral(
                    self.provider_id,
                    failure,
                    now.saturating_add(delay),
                    transport,
                    now,
                )
                .await?;
        }

        match action {
            FailureAction::Retry { delay_ms } => {
                self.repository
                    .defer_job(job.id, failure, now.saturating_add(delay_ms), true, now)
                    .await?;
            }
            FailureAction::Defer { delay_ms } => {
                self.repository
                    .defer_job(job.id, failure, now.saturating_add(delay_ms), false, now)
                    .await?;
            }
            FailureAction::Park => {
                self.repository.fail_job(job.id, failure, true, now).await?;
            }
            FailureAction::Negative => {
                // A deterministic no-match is an answer, so the job is finished.
                self.repository.complete_job(job.id, now).await?;
            }
        }

        match job.kind {
            MetadataJobKind::Identify => {
                self.record_identification_failure(job.game_id, failure, action)
                    .await?
            }
            MetadataJobKind::RefreshMetadata => {
                self.record_refresh_failure(job.game_id, failure).await?
            }
            MetadataJobKind::RefreshCover => {
                self.repository
                    .record_media_failure(
                        job.game_id,
                        self.provider_id,
                        MediaAssetKind::Cover,
                        failure,
                        now,
                    )
                    .await?
            }
        }

        Ok(action)
    }

    /// Records a failed identification without ever discarding cached data.
    async fn record_identification_failure(
        &self,
        game_id: GameId,
        failure: ProviderFailureClass,
        action: FailureAction,
    ) -> Result<(), AppError> {
        let evidence = self.current_evidence(game_id).await?;
        let existing = self
            .repository
            .load_match(game_id, self.provider_id)
            .await?;

        // An accepted match is not demoted by a transient provider failure; only its failure marker
        // and check timestamp move. A no-match answer replaces the relationship.
        let status = match (action, existing.as_ref().map(|existing| existing.status)) {
            (FailureAction::Negative, _) => ProviderMatchStatus::NoMatch,
            (_, Some(ProviderMatchStatus::Matched)) => ProviderMatchStatus::Matched,
            (FailureAction::Park, _) => ProviderMatchStatus::Failed,
            _ => ProviderMatchStatus::Deferred,
        };
        let keep_identity = status == ProviderMatchStatus::Matched;

        self.repository
            .persist_match(
                &ProviderMatchWrite {
                    game_id,
                    provider_id: self.provider_id,
                    status,
                    match_type: keep_identity
                        .then(|| existing.as_ref().and_then(|existing| existing.match_type))
                        .flatten(),
                    provider_game_id: keep_identity
                        .then(|| {
                            existing
                                .as_ref()
                                .and_then(|existing| existing.provider_game_id.clone())
                        })
                        .flatten(),
                    provider_rom_id: keep_identity
                        .then(|| {
                            existing
                                .as_ref()
                                .and_then(|existing| existing.provider_rom_id.clone())
                        })
                        .flatten(),
                    unsupported_reason: None,
                    last_failure: Some(failure),
                    evidence: if keep_identity {
                        existing
                            .as_ref()
                            .and_then(|existing| existing.evidence.clone())
                    } else if status == ProviderMatchStatus::NoMatch {
                        // Bind the negative answer to the exact evidence that produced it.
                        evidence
                    } else {
                        None
                    },
                },
                self.clock.now_ms(),
            )
            .await?;
        Ok(())
    }

    /// Records a failed metadata refresh. The stored snapshot is deliberately untouched.
    async fn record_refresh_failure(
        &self,
        game_id: GameId,
        failure: ProviderFailureClass,
    ) -> Result<(), AppError> {
        let Some(existing) = self
            .repository
            .load_match(game_id, self.provider_id)
            .await?
        else {
            return Ok(());
        };
        self.repository
            .persist_match(
                &ProviderMatchWrite {
                    game_id,
                    provider_id: self.provider_id,
                    status: existing.status,
                    match_type: existing.match_type,
                    provider_game_id: existing.provider_game_id.clone(),
                    provider_rom_id: existing.provider_rom_id.clone(),
                    unsupported_reason: existing.unsupported_reason,
                    last_failure: Some(failure),
                    evidence: existing.evidence.clone(),
                },
                self.clock.now_ms(),
            )
            .await?;
        Ok(())
    }

    // ------------------------------------------------------------------------- matching pipeline

    /// Stage 1 to 3 of the matching pipeline for one game.
    async fn identify(&self, game_id: GameId) -> Result<JobOutcome, AppError> {
        let Some(game) = self.library.game(game_id).await? else {
            // The local library moved on. Nothing to enrich, and nothing to change locally.
            return Ok(JobOutcome::Done);
        };

        // Stage 1: system gate. An unmapped system is never searched globally.
        if !self.provider.supports_system(game.system_id) {
            self.persist_unsupported(game_id, UnsupportedContentReason::SystemNotMapped, &[])
                .await?;
            return Ok(JobOutcome::Done);
        }

        // A user-pinned provider game short-circuits provider matching entirely.
        if let Some(selection) = self
            .repository
            .load_user_selection(game_id, self.provider_id)
            .await?
        {
            return self
                .attach_user_confirmed(game_id, game.system_id, &selection.provider_game_id)
                .await;
        }

        let units = self.library.game_content_units(game_id).await?;
        let (evidence, basename) = match self.deterministic_candidate(&units) {
            Ok(candidate) => candidate,
            Err(reason) => {
                // Stage 3 only: an unsupported representation may still produce suggestions, but
                // never an automatic attachment.
                let candidates = match self
                    .search_candidates(game.system_id, &game.local_title)
                    .await
                {
                    Ok(candidates) => candidates,
                    Err(failure) => return Ok(JobOutcome::ProviderFailure(failure)),
                };
                self.persist_unsupported(game_id, reason, &candidates)
                    .await?;
                return Ok(JobOutcome::Done);
            }
        };

        // Stage 2: deterministic content lookup.
        let record = match self
            .provider
            .identify_content(&ContentIdentificationRequest {
                system_id: game.system_id,
                evidence: evidence.clone(),
                file_basename: basename,
            })
            .await
        {
            Ok(response) => {
                self.record_quota(response.quota).await?;
                response.value
            }
            Err(failure) => return Ok(JobOutcome::ProviderFailure(failure)),
        };

        match classify_deterministic_match(&evidence, &record) {
            DeterministicOutcome::Accepted {
                match_type,
                provider_rom_id,
            } => {
                self.attach_match(game_id, &record, match_type, provider_rom_id, &evidence)
                    .await?;
                Ok(JobOutcome::Done)
            }
            DeterministicOutcome::Conflicting(conflict) => {
                // Conflicting provider evidence is never resolved by preference or ordering.
                tracing::info!(
                    game_id = %game_id,
                    conflict = ?conflict,
                    "provider content evidence conflicted with local content evidence"
                );
                self.persist_ambiguous(game_id, &[]).await?;
                Ok(JobOutcome::Done)
            }
            DeterministicOutcome::Insufficient(reason) => {
                tracing::debug!(
                    game_id = %game_id,
                    reason = ?reason,
                    "provider returned no comparable content evidence"
                );
                // Stage 3: offer suggestions, still without attaching anything.
                let candidates = match self
                    .search_candidates(game.system_id, &game.local_title)
                    .await
                {
                    Ok(candidates) => candidates,
                    Err(failure) => return Ok(JobOutcome::ProviderFailure(failure)),
                };
                self.persist_ambiguous(game_id, &candidates).await?;
                Ok(JobOutcome::Done)
            }
        }
    }

    /// Picks the first content unit that may take part in automatic deterministic matching.
    fn deterministic_candidate(
        &self,
        units: &[ContentUnit],
    ) -> Result<(MatchEvidence, String), UnsupportedContentReason> {
        let mut first_reason = None;
        for unit in units {
            match evidence_for_unit(unit) {
                Ok(evidence) => {
                    let basename = unit
                        .files
                        .iter()
                        .min_by_key(|membership| membership.ordinal)
                        .map(|membership| basename_of(&membership.file.relative_path))
                        .unwrap_or_default();
                    return Ok((evidence, basename));
                }
                Err(reason) => first_reason = first_reason.or(Some(reason)),
            }
        }
        Err(first_reason.unwrap_or(UnsupportedContentReason::NoPrimaryContentFile))
    }

    async fn search_candidates(
        &self,
        system_id: crate::domain::system::SystemId,
        title: &str,
    ) -> Result<Vec<ProviderCandidate>, ProviderFailureClass> {
        if title.trim().is_empty() {
            return Ok(Vec::new());
        }
        if let Err(next_slot) = self.minute_budget.reserve(self.clock.now_ms(), None) {
            tracing::debug!(next_slot, "heuristic search postponed by the minute budget");
            return Ok(Vec::new());
        }
        let response = self
            .provider
            .search_candidates(&CandidateSearchRequest {
                system_id,
                title: title.to_owned(),
            })
            .await?;
        // Quota is recorded on a best-effort basis; a storage failure here must not lose the search.
        if let Some(quota) = response.quota {
            let _ = self
                .repository
                .update_quota(self.provider_id, &quota, self.clock.now_ms())
                .await;
        }
        Ok(response.value)
    }

    /// Persists an accepted match together with its metadata and cover.
    async fn attach_match(
        &self,
        game_id: GameId,
        record: &ProviderGameRecord,
        match_type: MatchType,
        provider_rom_id: Option<String>,
        evidence: &MatchEvidence,
    ) -> Result<(), AppError> {
        let now = self.clock.now_ms();
        self.repository
            .persist_match(
                &ProviderMatchWrite {
                    game_id,
                    provider_id: self.provider_id,
                    status: ProviderMatchStatus::Matched,
                    match_type: Some(match_type),
                    provider_game_id: Some(record.provider_game_id.clone()),
                    provider_rom_id: provider_rom_id.or_else(|| record.provider_rom_id.clone()),
                    unsupported_reason: None,
                    last_failure: None,
                    evidence: Some(evidence.clone()),
                },
                now,
            )
            .await?;
        self.persist_normalized(game_id, record).await?;
        // A cover failure must not undo a successful metadata attachment.
        if let Err(failure) = self.store_cover(game_id, record).await {
            self.repository
                .record_media_failure(
                    game_id,
                    self.provider_id,
                    MediaAssetKind::Cover,
                    failure,
                    self.clock.now_ms(),
                )
                .await?;
        }
        Ok(())
    }

    /// Attaches a user-pinned provider game. Recorded as user-confirmed, never as hash-exact.
    async fn attach_user_confirmed(
        &self,
        game_id: GameId,
        system_id: crate::domain::system::SystemId,
        provider_game_id: &str,
    ) -> Result<JobOutcome, AppError> {
        let record = match self.provider.fetch_game(system_id, provider_game_id).await {
            Ok(response) => {
                self.record_quota(response.quota).await?;
                response.value
            }
            Err(failure) => return Ok(JobOutcome::ProviderFailure(failure)),
        };

        // Evidence is still stored so a later content replacement invalidates the pin as well.
        let units = self.library.game_content_units(game_id).await?;
        let evidence = units.iter().find_map(|unit| evidence_for_unit(unit).ok());

        self.repository
            .persist_match(
                &ProviderMatchWrite {
                    game_id,
                    provider_id: self.provider_id,
                    status: ProviderMatchStatus::Matched,
                    match_type: Some(MatchType::HeuristicUserConfirmed),
                    provider_game_id: Some(record.provider_game_id.clone()),
                    provider_rom_id: record.provider_rom_id.clone(),
                    unsupported_reason: None,
                    last_failure: None,
                    evidence,
                },
                self.clock.now_ms(),
            )
            .await?;
        self.persist_normalized(game_id, &record).await?;
        if let Err(failure) = self.store_cover(game_id, &record).await {
            self.repository
                .record_media_failure(
                    game_id,
                    self.provider_id,
                    MediaAssetKind::Cover,
                    failure,
                    self.clock.now_ms(),
                )
                .await?;
        }
        Ok(JobOutcome::Done)
    }

    async fn persist_unsupported(
        &self,
        game_id: GameId,
        reason: UnsupportedContentReason,
        candidates: &[ProviderCandidate],
    ) -> Result<(), AppError> {
        let now = self.clock.now_ms();
        let provider_match_id = self
            .repository
            .persist_match(
                &ProviderMatchWrite {
                    game_id,
                    provider_id: self.provider_id,
                    status: ProviderMatchStatus::Deferred,
                    match_type: None,
                    provider_game_id: None,
                    provider_rom_id: None,
                    unsupported_reason: Some(reason),
                    last_failure: None,
                    evidence: None,
                },
                now,
            )
            .await?;
        self.repository
            .replace_candidates(provider_match_id, candidates, now)
            .await
    }

    async fn persist_ambiguous(
        &self,
        game_id: GameId,
        candidates: &[ProviderCandidate],
    ) -> Result<(), AppError> {
        let now = self.clock.now_ms();
        let provider_match_id = self
            .repository
            .persist_match(
                &ProviderMatchWrite {
                    game_id,
                    provider_id: self.provider_id,
                    status: ProviderMatchStatus::Ambiguous,
                    match_type: None,
                    provider_game_id: None,
                    provider_rom_id: None,
                    unsupported_reason: None,
                    last_failure: None,
                    evidence: None,
                },
                now,
            )
            .await?;
        self.repository
            .replace_candidates(provider_match_id, candidates, now)
            .await
    }

    // ---------------------------------------------------------------------------------- refresh

    async fn refresh_metadata(&self, game_id: GameId) -> Result<JobOutcome, AppError> {
        let Some(provider_match) = self
            .repository
            .load_match(game_id, self.provider_id)
            .await?
        else {
            return Ok(JobOutcome::Done);
        };
        let Some(provider_game_id) = provider_match.provider_game_id.clone() else {
            return Ok(JobOutcome::Done);
        };
        let Some(game) = self.library.game(game_id).await? else {
            return Ok(JobOutcome::Done);
        };

        // Refreshing a provider identity whose evidence no longer holds would silently re-trust it,
        // so this becomes a re-identification instead.
        if let Some(stored_evidence) = provider_match.evidence.as_ref() {
            let current = self.current_evidence(game_id).await?;
            let agrees = current
                .as_ref()
                .is_some_and(|current| stored_evidence.agrees_with(current));
            if !agrees {
                let now = self.clock.now_ms();
                self.repository
                    .mark_match_stale(game_id, self.provider_id, now)
                    .await?;
                self.repository
                    .enqueue_job(game_id, self.provider_id, MetadataJobKind::Identify, now)
                    .await?;
                return Ok(JobOutcome::Done);
            }
        }

        let record = match self
            .provider
            .fetch_game(game.system_id, &provider_game_id)
            .await
        {
            Ok(response) => {
                self.record_quota(response.quota).await?;
                response.value
            }
            Err(failure) => return Ok(JobOutcome::ProviderFailure(failure)),
        };

        self.persist_normalized(game_id, &record).await?;
        // Clear the previous failure marker now that a valid snapshot has replaced it.
        self.repository
            .persist_match(
                &ProviderMatchWrite {
                    game_id,
                    provider_id: self.provider_id,
                    status: provider_match.status,
                    match_type: provider_match.match_type,
                    provider_game_id: Some(provider_game_id),
                    provider_rom_id: provider_match.provider_rom_id.clone(),
                    unsupported_reason: provider_match.unsupported_reason,
                    last_failure: None,
                    evidence: provider_match.evidence.clone(),
                },
                self.clock.now_ms(),
            )
            .await?;
        Ok(JobOutcome::Done)
    }

    async fn refresh_cover(&self, game_id: GameId) -> Result<JobOutcome, AppError> {
        let Some(provider_match) = self
            .repository
            .load_match(game_id, self.provider_id)
            .await?
        else {
            return Ok(JobOutcome::Done);
        };
        let Some(provider_game_id) = provider_match.provider_game_id.clone() else {
            return Ok(JobOutcome::Done);
        };
        let Some(game) = self.library.game(game_id).await? else {
            return Ok(JobOutcome::Done);
        };

        let record = match self
            .provider
            .fetch_game(game.system_id, &provider_game_id)
            .await
        {
            Ok(response) => {
                self.record_quota(response.quota).await?;
                response.value
            }
            Err(failure) => return Ok(JobOutcome::ProviderFailure(failure)),
        };

        match self.store_cover(game_id, &record).await {
            Ok(()) => Ok(JobOutcome::Done),
            Err(failure) => Ok(JobOutcome::ProviderFailure(failure)),
        }
    }

    // -------------------------------------------------------------------------------- persistence

    async fn persist_normalized(
        &self,
        game_id: GameId,
        record: &ProviderGameRecord,
    ) -> Result<(), AppError> {
        let now = self.clock.now_ms();
        self.repository
            .persist_metadata(
                game_id,
                &ProviderMetadataRecord {
                    metadata: record.metadata.clone(),
                    provenance: crate::domain::metadata::MetadataProvenance {
                        provider_id: self.provider_id,
                        provider_game_id: record.provider_game_id.clone(),
                        source_credit: record.source_credit.clone(),
                        fetched_at: now,
                    },
                },
                now,
            )
            .await
    }

    /// Downloads and publishes the primary cover, keeping the previous one on any failure.
    async fn store_cover(
        &self,
        game_id: GameId,
        record: &ProviderGameRecord,
    ) -> Result<(), ProviderFailureClass> {
        let Some(cover) = record.primary_cover.as_ref() else {
            return Err(ProviderFailureClass::MediaUnavailable);
        };

        // A cached cover whose provider checksums are unchanged does not need to be fetched again.
        if self.cover_is_unchanged(game_id, cover).await {
            return Ok(());
        }

        if self
            .minute_budget
            .reserve(self.clock.now_ms(), None)
            .is_err()
        {
            return Err(ProviderFailureClass::CapacityDeferred);
        }
        let media = self.provider.download_media(&cover.locator).await?.value;
        let published = self
            .covers
            .publish(game_id, self.provider_id, &media)
            .map_err(|error| {
                tracing::info!(error = %error, game_id = %game_id, "provider cover was rejected");
                ProviderFailureClass::MediaUnavailable
            })?;

        let previous = self
            .repository
            .load_media_asset(game_id, self.provider_id, MediaAssetKind::Cover)
            .await
            .ok()
            .flatten();

        self.repository
            .persist_media_asset(
                &MediaAssetWrite {
                    game_id,
                    provider_id: self.provider_id,
                    kind: MediaAssetKind::Cover,
                    state: MediaAssetState::Cached,
                    provider_media_type: Some(cover.provider_media_type.clone()),
                    region: cover.region.clone(),
                    cache_relative_path: Some(published.relative_path.clone()),
                    content_type: Some(published.content_type.clone()),
                    size_bytes: Some(published.size_bytes),
                    content_sha256: Some(published.content_sha256.clone()),
                    provider_crc32: cover.crc32.clone(),
                    provider_md5: cover.md5.clone(),
                    provider_sha1: cover.sha1.clone(),
                    source_credit: cover.source_credit.clone(),
                    last_failure: None,
                    fetched_at: Some(self.clock.now_ms()),
                },
                self.clock.now_ms(),
            )
            .await
            .map_err(|error| {
                tracing::error!(error = %error, "cover metadata could not be persisted");
                ProviderFailureClass::MediaUnavailable
            })?;

        // Only remove a superseded file once the new row is committed.
        if let Some(previous_path) = previous
            .and_then(|previous| previous.cache_relative_path)
            .filter(|path| *path != published.relative_path)
        {
            self.covers.remove(&previous_path);
        }
        Ok(())
    }

    /// True when the cached cover already matches the provider's advertised checksums.
    async fn cover_is_unchanged(&self, game_id: GameId, cover: &ProviderCoverDescriptor) -> bool {
        let Ok(Some(existing)) = self
            .repository
            .load_media_asset(game_id, self.provider_id, MediaAssetKind::Cover)
            .await
        else {
            return false;
        };
        if existing.state != MediaAssetState::Cached {
            return false;
        }
        let Some(path) = existing.cache_relative_path.as_deref() else {
            return false;
        };
        if !self.covers.is_cached(path) {
            return false;
        }
        // At least one provider checksum must be present on both sides and agree.
        let pairs = [
            (existing.provider_sha1.as_deref(), cover.sha1.as_deref()),
            (existing.provider_md5.as_deref(), cover.md5.as_deref()),
            (existing.provider_crc32.as_deref(), cover.crc32.as_deref()),
        ];
        pairs.iter().any(|(stored, advertised)| {
            matches!((stored, advertised), (Some(stored), Some(advertised))
                if stored.eq_ignore_ascii_case(advertised))
        })
    }

    async fn record_quota(&self, quota: Option<ProviderQuotaSnapshot>) -> Result<(), AppError> {
        let now = self.clock.now_ms();
        if let Some(quota) = quota {
            self.repository
                .update_quota(self.provider_id, &quota, now)
                .await?;
        }
        // A successful call proves the provider is reachable again.
        self.repository
            .clear_provider_deferral(self.provider_id, now)
            .await
    }

    // ----------------------------------------------------------------------------------- helpers

    /// Current M4 evidence for one game, or `None` when no unit qualifies.
    async fn current_evidence(&self, game_id: GameId) -> Result<Option<MatchEvidence>, AppError> {
        let units = self.library.game_content_units(game_id).await?;
        Ok(units.iter().find_map(|unit| evidence_for_unit(unit).ok()))
    }

    async fn ensure_game_exists(&self, game_id: GameId) -> Result<(), AppError> {
        self.library
            .game(game_id)
            .await?
            .map(|_| ())
            .ok_or_else(|| AppError::Metadata("the requested game does not exist".to_owned()))
    }

    /// Hides a recorded cover whose file is no longer readable, without changing stored state.
    fn readable_cover(
        &self,
        cover: Option<crate::domain::metadata::MediaAsset>,
    ) -> Option<crate::domain::metadata::MediaAsset> {
        let cover = cover?;
        match cover.cache_relative_path.as_deref() {
            Some(path) if self.covers.is_cached(path) => Some(cover),
            Some(_) => Some(crate::domain::metadata::MediaAsset {
                state: MediaAssetState::Missing,
                cache_relative_path: None,
                ..cover
            }),
            None => Some(cover),
        }
    }
}

fn basename_of(relative_path: &str) -> String {
    relative_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(relative_path)
        .to_owned()
}

/// Background worker lifecycle.
///
/// One worker owns the whole provider pipeline. It is started once, never duplicated, and always
/// runs on the async runtime rather than the UI thread. Progress needs no frontend listener: state
/// lives in SQLite, so the UI simply reads it whenever it wants.
pub struct MetadataWorker {
    service: Arc<MetadataApplicationService>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    started: std::sync::atomic::AtomicBool,
}

/// Shortest pause between rounds, so a busy queue still yields.
const WORKER_MIN_PAUSE_MS: u64 = 250;
/// Pause when there is nothing to do.
const WORKER_IDLE_PAUSE_MS: u64 = 60_000;
/// Longest pause. A longer persisted deferral is still honoured by the scheduler itself; this only
/// bounds how long the worker sleeps between checks.
const WORKER_MAX_PAUSE_MS: u64 = 300_000;

impl MetadataWorker {
    pub fn new(service: Arc<MetadataApplicationService>) -> Self {
        Self {
            service,
            shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            started: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Starts the worker exactly once. Repeated calls are ignored.
    pub fn start(&self) {
        use std::sync::atomic::Ordering;
        if self
            .started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            tracing::debug!("metadata worker is already running");
            return;
        }

        let service = self.service.clone();
        let shutdown = self.shutdown.clone();
        let batch = service.config.batch_size.max(1);
        tauri::async_runtime::spawn(async move {
            tracing::info!("metadata worker started");
            while !shutdown.load(Ordering::SeqCst) {
                let pause = match run_worker_round(&service, batch).await {
                    Ok(pause) => pause,
                    Err(error) => {
                        error.log();
                        WORKER_IDLE_PAUSE_MS
                    }
                };
                tokio::time::sleep(std::time::Duration::from_millis(pause)).await;
            }
            tracing::info!("metadata worker stopped");
        });
    }

    /// Requests a clean stop. The worker finishes its current round first.
    pub fn shutdown(&self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// One worker round: revalidate, top up the queue, then process what is runnable.
async fn run_worker_round(
    service: &MetadataApplicationService,
    batch: usize,
) -> Result<u64, AppError> {
    service.revalidate_matches().await?;
    service.enqueue_missing_metadata().await?;
    let processed = service.process_ready_jobs(batch).await?;

    if let Some(wait_until) = processed.wait_until {
        let delay = wait_until.saturating_sub(service.clock.now_ms()).max(0) as u64;
        return Ok(delay.clamp(WORKER_MIN_PAUSE_MS, WORKER_MAX_PAUSE_MS));
    }
    Ok(if processed.total() == 0 {
        WORKER_IDLE_PAUSE_MS
    } else {
        WORKER_MIN_PAUSE_MS
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::credentials::InMemoryCredentialVault;
    use crate::adapters::database::Database;
    use crate::domain::library::ContentUnitKind;
    use crate::domain::metadata::{MetadataJobState, NormalizedMetadata};
    use crate::domain::system::SystemId;
    use crate::services::metadata_provider::{
        CandidateSearchRequest, DownloadedMedia, ProviderMediaLocator, ProviderResponse,
        ProviderResult, ProviderRomRecord,
    };
    use crate::services::metadata_queue::{test_support::ManualClock, NoJitter};
    use async_trait::async_trait;
    use sqlx::{Row, SqlitePool};
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use tempfile::TempDir;

    const SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
    const MD5: &str = "d41d8cd98f00b204e9800998ecf8427e";
    const CRC32: &str = "AABBCCDD";
    const SIZE: u64 = 524_288;
    const START_MS: i64 = 1_700_000_000_000;

    // ------------------------------------------------------------------------------ fake provider

    /// Programmable provider.
    ///
    /// Every response is queued by the test, and every call is recorded, so offline behaviour can
    /// be asserted as "zero provider calls" rather than inferred.
    struct FakeProvider {
        supported_systems: Vec<SystemId>,
        identify: Mutex<VecDeque<ProviderResult<ProviderGameRecord>>>,
        search: Mutex<VecDeque<ProviderResult<Vec<ProviderCandidate>>>>,
        fetch: Mutex<VecDeque<ProviderResult<ProviderGameRecord>>>,
        media: Mutex<VecDeque<ProviderResult<DownloadedMedia>>>,
        calls: Mutex<Vec<&'static str>>,
    }

    impl FakeProvider {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                supported_systems: SystemId::ALL_V1.to_vec(),
                identify: Mutex::new(VecDeque::new()),
                search: Mutex::new(VecDeque::new()),
                fetch: Mutex::new(VecDeque::new()),
                media: Mutex::new(VecDeque::new()),
                calls: Mutex::new(Vec::new()),
            })
        }

        fn without_systems() -> Arc<Self> {
            Arc::new(Self {
                supported_systems: Vec::new(),
                identify: Mutex::new(VecDeque::new()),
                search: Mutex::new(VecDeque::new()),
                fetch: Mutex::new(VecDeque::new()),
                media: Mutex::new(VecDeque::new()),
                calls: Mutex::new(Vec::new()),
            })
        }

        fn queue_identify(&self, result: ProviderResult<ProviderGameRecord>) {
            self.identify.lock().unwrap().push_back(result);
        }

        fn queue_search(&self, result: ProviderResult<Vec<ProviderCandidate>>) {
            self.search.lock().unwrap().push_back(result);
        }

        fn queue_fetch(&self, result: ProviderResult<ProviderGameRecord>) {
            self.fetch.lock().unwrap().push_back(result);
        }

        fn queue_media(&self, result: ProviderResult<DownloadedMedia>) {
            self.media.lock().unwrap().push_back(result);
        }

        /// Drops queued responses so a test can switch the provider's behaviour mid-scenario.
        fn clear_queues(&self) {
            self.identify.lock().unwrap().clear();
            self.search.lock().unwrap().clear();
            self.fetch.lock().unwrap().clear();
            self.media.lock().unwrap().clear();
        }

        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }

        fn reset_calls(&self) {
            self.calls.lock().unwrap().clear();
        }

        /// Takes the next queued response, repeating the last one when the queue runs dry.
        fn next<T: Clone>(
            &self,
            queue: &Mutex<VecDeque<ProviderResult<T>>>,
            label: &'static str,
        ) -> ProviderResult<T> {
            self.calls.lock().unwrap().push(label);
            let mut queue = queue.lock().unwrap();
            if queue.len() > 1 {
                queue.pop_front().expect("queue is not empty")
            } else {
                queue
                    .front()
                    .cloned()
                    .unwrap_or(Err(ProviderFailureClass::NoMatch))
            }
        }
    }

    #[async_trait]
    impl MetadataProvider for FakeProvider {
        fn provider_id(&self) -> MetadataProviderId {
            MetadataProviderId::ScreenScraper
        }

        fn supports_system(&self, system: SystemId) -> bool {
            self.supported_systems.contains(&system)
        }

        async fn identify_content(
            &self,
            _request: &ContentIdentificationRequest,
        ) -> ProviderResult<ProviderGameRecord> {
            self.next(&self.identify, "identify")
        }

        async fn search_candidates(
            &self,
            _request: &CandidateSearchRequest,
        ) -> ProviderResult<Vec<ProviderCandidate>> {
            self.next(&self.search, "search")
        }

        async fn fetch_game(
            &self,
            _system: SystemId,
            _provider_game_id: &str,
        ) -> ProviderResult<ProviderGameRecord> {
            self.next(&self.fetch, "fetch")
        }

        async fn download_media(
            &self,
            _locator: &ProviderMediaLocator,
        ) -> ProviderResult<DownloadedMedia> {
            self.next(&self.media, "media")
        }
    }

    // -------------------------------------------------------------------------------- fixtures

    fn matched_rom() -> ProviderRomRecord {
        ProviderRomRecord {
            provider_rom_id: Some("101".to_owned()),
            filename: Some("Example Quest (USA).sfc".to_owned()),
            size_bytes: Some(SIZE),
            crc32: Some(CRC32.to_owned()),
            md5: Some(MD5.to_owned()),
            sha1: Some(SHA1.to_owned()),
            support_number: Some(1),
            support_count: Some(1),
        }
    }

    fn synthetic_cover() -> ProviderCoverDescriptor {
        ProviderCoverDescriptor {
            provider_media_type: "box-2D".to_owned(),
            region: Some("us".to_owned()),
            crc32: Some("1A2B3C4D".to_owned()),
            md5: None,
            sha1: None,
            source_credit: Some("Example Media Source".to_owned()),
            locator: ProviderMediaLocator::new("https://provider.invalid/media/cover"),
        }
    }

    /// Synthetic PNG-signature bytes. Never real provider artwork.
    fn synthetic_png(marker: &str) -> DownloadedMedia {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(format!("synthetic-cover-{marker}").as_bytes());
        DownloadedMedia {
            content_type: Some("image/png".to_owned()),
            bytes,
        }
    }

    fn game_record(matched: Option<ProviderRomRecord>, title: &str) -> ProviderGameRecord {
        let roms = matched.clone().map(|rom| vec![rom]).unwrap_or_default();
        ProviderGameRecord {
            provider_game_id: "3".to_owned(),
            provider_rom_id: Some("77".to_owned()),
            matched_rom: matched,
            roms,
            metadata: NormalizedMetadata {
                title: title.to_owned(),
                sort_title: Some(title.to_owned()),
                synopsis: Some("A synthetic synopsis.".to_owned()),
                release_date: Some("1990-09-01".to_owned()),
                developer: Some("Example Studio".to_owned()),
                publisher: Some("Example Publisher".to_owned()),
                genre: Some("Action".to_owned()),
                players: Some("1-2".to_owned()),
                region: Some("us".to_owned()),
            },
            source_credit: Some("Example Contributor".to_owned()),
            primary_cover: Some(synthetic_cover()),
        }
    }

    fn quota(max_threads: i64) -> ProviderQuotaSnapshot {
        ProviderQuotaSnapshot {
            max_threads: Some(max_threads),
            max_requests_per_minute: Some(3072),
            max_requests_per_day: Some(10_000),
            max_negative_requests_per_day: Some(1_000),
            requests_today: Some(7),
            negative_requests_today: Some(0),
        }
    }

    // --------------------------------------------------------------------------------- harness

    struct Harness {
        _directory: TempDir,
        database_path: PathBuf,
        app_data: PathBuf,
        pool: SqlitePool,
        service: Arc<MetadataApplicationService>,
        provider: Arc<FakeProvider>,
        clock: Arc<ManualClock>,
        vault: Arc<InMemoryCredentialVault>,
    }

    impl Harness {
        async fn new() -> Self {
            Self::with_provider(FakeProvider::new()).await
        }

        async fn with_provider(provider: Arc<FakeProvider>) -> Self {
            let directory = tempfile::tempdir().expect("temporary directory");
            let app_data = directory.path().join("app-data");
            let database_path = app_data.join("database").join("retrofrontier.sqlite3");
            let database = Database::open(&database_path)
                .await
                .expect("database should open");
            let pool = database.pool().clone();
            let clock = Arc::new(ManualClock::new(START_MS));
            let vault = Arc::new(InMemoryCredentialVault::new());
            let service = build_service(
                pool.clone(),
                provider.clone(),
                vault.clone(),
                clock.clone(),
                &app_data,
            )
            .await;

            Self {
                _directory: directory,
                database_path,
                app_data,
                pool,
                service,
                provider,
                clock,
                vault,
            }
        }

        /// Simulates an application restart on the same database and cache directory.
        async fn restart(&mut self) {
            self.pool.close().await;
            let database = Database::open(&self.database_path)
                .await
                .expect("database should reopen");
            self.pool = database.pool().clone();
            self.service = build_service(
                self.pool.clone(),
                self.provider.clone(),
                self.vault.clone(),
                self.clock.clone(),
                &self.app_data,
            )
            .await;
        }

        /// Runs scheduling rounds until nothing more happens or the round budget is exhausted.
        async fn drain(&self, rounds: usize) -> ProcessedJobs {
            let mut total = ProcessedJobs::default();
            for _ in 0..rounds {
                let processed = self
                    .service
                    .process_ready_jobs(8)
                    .await
                    .expect("a scheduling round should not fail");
                total.completed += processed.completed;
                total.deferred += processed.deferred;
                total.failed += processed.failed;
                total.wait_until = processed.wait_until.or(total.wait_until);
                if processed.total() == 0 {
                    break;
                }
            }
            total
        }

        async fn job(&self, game_id: GameId, kind: MetadataJobKind) -> Option<MetadataJob> {
            MetadataRepository::new(self.pool.clone())
                .load_jobs_for_game(game_id, MetadataProviderId::ScreenScraper)
                .await
                .expect("jobs should load")
                .into_iter()
                .find(|job| job.kind == kind)
        }

        async fn state(&self, game_id: GameId) -> GameMetadataState {
            self.service
                .get_metadata_state(game_id)
                .await
                .expect("metadata state should load")
        }

        async fn scheduler_state(&self) -> crate::domain::metadata::ProviderSchedulerState {
            MetadataRepository::new(self.pool.clone())
                .load_scheduler_state(MetadataProviderId::ScreenScraper)
                .await
                .expect("scheduler state should load")
        }
    }

    async fn build_service(
        pool: SqlitePool,
        provider: Arc<FakeProvider>,
        vault: Arc<InMemoryCredentialVault>,
        clock: Arc<ManualClock>,
        app_data: &std::path::Path,
    ) -> Arc<MetadataApplicationService> {
        let credentials = Arc::new(ProviderCredentialState::new(Some(DeveloperCredentials {
            developer_id: SecretString::new("fake-dev-id"),
            developer_password: SecretString::new("fake-dev-password"),
        })));
        Arc::new(
            MetadataApplicationService::initialize(
                pool,
                provider,
                vault,
                credentials,
                MetadataPaths::new(app_data),
                clock,
                Arc::new(NoJitter),
                MetadataConfig {
                    max_concurrency: 4,
                    batch_size: 25,
                },
            )
            .await
            .expect("metadata service should initialize"),
        )
    }

    /// Inserts an M4 single-file game directly, so metadata tests do not depend on the scanner.
    #[allow(clippy::too_many_arguments)]
    async fn insert_game(
        pool: &SqlitePool,
        system: SystemId,
        kind: ContentUnitKind,
        relative_path: &str,
        sha1: Option<&str>,
        md5: Option<&str>,
        crc32: Option<&str>,
        fingerprint: &str,
    ) -> GameId {
        let now = START_MS;
        let root_id: i64 = sqlx::query_scalar(
            "INSERT INTO content_roots (path, kind, enabled, availability, created_at, updated_at) \
             VALUES ('/library/ROMs', 'managed', 1, 'available', ?, ?) \
             ON CONFLICT(path) DO UPDATE SET updated_at = excluded.updated_at RETURNING id",
        )
        .bind(now)
        .bind(now)
        .fetch_one(pool)
        .await
        .expect("content root fixture");

        let game_id: i64 = sqlx::query_scalar(
            "INSERT INTO games (system_id, local_title, availability, created_at, updated_at) \
             VALUES (?, 'Example Quest', 'available', ?, ?) RETURNING id",
        )
        .bind(system.as_str())
        .bind(now)
        .bind(now)
        .fetch_one(pool)
        .await
        .expect("game fixture");

        // Paths are unique per root in M4, so each fixture game gets its own subdirectory.
        let relative_path = format!("g{game_id}/{relative_path}");

        let unit_id: i64 = sqlx::query_scalar(
            "INSERT INTO content_units (game_id, root_id, system_id, kind, local_title, \
             primary_relative_path, fingerprint, availability, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 'Example Quest', ?, ?, 'available', ?, ?) RETURNING id",
        )
        .bind(game_id)
        .bind(root_id)
        .bind(system.as_str())
        .bind(kind.as_db())
        .bind(&relative_path)
        .bind(fingerprint)
        .bind(now)
        .bind(now)
        .fetch_one(pool)
        .await
        .expect("content unit fixture");

        let file_id: i64 = sqlx::query_scalar(
            "INSERT INTO content_files (root_id, relative_path, size_bytes, modified_at, crc32, \
             md5, sha1, availability, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, 'available', ?, ?) RETURNING id",
        )
        .bind(root_id)
        .bind(&relative_path)
        .bind(SIZE as i64)
        .bind(now)
        .bind(crc32)
        .bind(md5)
        .bind(sha1)
        .bind(now)
        .bind(now)
        .fetch_one(pool)
        .await
        .expect("content file fixture");

        sqlx::query(
            "INSERT INTO content_unit_files (content_unit_id, content_file_id, ordinal, role) \
             VALUES (?, ?, 0, 'standalone')",
        )
        .bind(unit_id)
        .bind(file_id)
        .execute(pool)
        .await
        .expect("membership fixture");

        GameId(game_id)
    }

    async fn insert_single_file_game(pool: &SqlitePool) -> GameId {
        insert_game(
            pool,
            SystemId::Snes,
            ContentUnitKind::SingleFile,
            "SNES/Example Quest (USA).sfc",
            Some(SHA1),
            Some(MD5),
            Some(CRC32),
            "fingerprint-1",
        )
        .await
    }

    /// Snapshot of the local library rows a provider must never change.
    #[derive(Debug, PartialEq, Eq)]
    struct LocalLibrarySnapshot {
        games: Vec<(i64, String, String)>,
        units: Vec<(i64, i64, String, Option<String>)>,
        files: Vec<(i64, String, Option<String>, String)>,
    }

    async fn local_snapshot(pool: &SqlitePool) -> LocalLibrarySnapshot {
        let games = sqlx::query("SELECT id, system_id, availability FROM games ORDER BY id")
            .fetch_all(pool)
            .await
            .expect("games should load")
            .into_iter()
            .map(|row| (row.get("id"), row.get("system_id"), row.get("availability")))
            .collect();
        let units = sqlx::query(
            "SELECT id, game_id, availability, fingerprint FROM content_units ORDER BY id",
        )
        .fetch_all(pool)
        .await
        .expect("units should load")
        .into_iter()
        .map(|row| {
            (
                row.get("id"),
                row.get("game_id"),
                row.get("availability"),
                row.get("fingerprint"),
            )
        })
        .collect();
        let files = sqlx::query(
            "SELECT id, relative_path, sha1, availability FROM content_files ORDER BY id",
        )
        .fetch_all(pool)
        .await
        .expect("files should load")
        .into_iter()
        .map(|row| {
            (
                row.get("id"),
                row.get("relative_path"),
                row.get("sha1"),
                row.get("availability"),
            )
        })
        .collect();
        LocalLibrarySnapshot {
            games,
            units,
            files,
        }
    }

    // ------------------------------------------------------------------------- matching pipeline

    #[tokio::test]
    async fn an_exact_sha1_match_attaches_metadata_provider_identity_and_a_cover() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "The Example Quest"),
            Some(quota(1)),
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("a"), None)));

        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        let state = harness.state(game_id).await;
        assert_eq!(state.status, ProviderMatchStatus::Matched);
        assert_eq!(state.match_type, Some(MatchType::DeterministicSha1));
        assert!(state.deterministic);
        assert_eq!(state.provider_game_id.as_deref(), Some("3"));
        assert_eq!(state.provider_rom_id.as_deref(), Some("101"));

        let metadata = state
            .metadata
            .expect("normalized metadata should be stored");
        assert_eq!(metadata.metadata.title, "The Example Quest");
        assert_eq!(
            metadata.metadata.developer.as_deref(),
            Some("Example Studio")
        );
        assert_eq!(
            metadata.provenance.provider_id,
            MetadataProviderId::ScreenScraper
        );
        assert_eq!(metadata.provenance.provider_game_id, "3");
        assert_eq!(
            metadata.provenance.source_credit.as_deref(),
            Some("Example Contributor")
        );

        let cover = state.cover.expect("a cover should be cached");
        assert_eq!(cover.state, MediaAssetState::Cached);
        assert_eq!(
            cover.cache_relative_path.as_deref(),
            Some("covers/screenscraper/1.png")
        );
        assert_eq!(cover.source_credit.as_deref(), Some("Example Media Source"));
        assert!(harness
            .service
            .covers()
            .is_cached(cover.cache_relative_path.as_deref().unwrap()));
    }

    #[tokio::test]
    async fn an_exact_md5_match_attaches_when_sha1_is_unavailable() {
        let harness = Harness::new().await;
        let game_id = insert_game(
            &harness.pool,
            SystemId::Snes,
            ContentUnitKind::SingleFile,
            "SNES/game.sfc",
            None,
            Some(MD5),
            Some(CRC32),
            "fingerprint-md5",
        )
        .await;
        let mut rom = matched_rom();
        rom.sha1 = None;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(rom), "Example"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("md5"), None)));

        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        let state = harness.state(game_id).await;
        assert_eq!(state.status, ProviderMatchStatus::Matched);
        assert_eq!(state.match_type, Some(MatchType::DeterministicMd5));
        assert!(state.deterministic);
    }

    #[tokio::test]
    async fn a_crc32_and_size_match_is_recorded_as_the_weakest_deterministic_class() {
        let harness = Harness::new().await;
        let game_id = insert_game(
            &harness.pool,
            SystemId::Snes,
            ContentUnitKind::SingleFile,
            "SNES/game.sfc",
            None,
            None,
            Some(CRC32),
            "fingerprint-crc",
        )
        .await;
        let mut rom = matched_rom();
        rom.sha1 = None;
        rom.md5 = None;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(rom), "Example"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("crc"), None)));

        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        let state = harness.state(game_id).await;
        assert_eq!(state.match_type, Some(MatchType::DeterministicCrc32));
        assert!(state.deterministic);
    }

    #[tokio::test]
    async fn conflicting_returned_hash_evidence_is_ambiguous_and_never_attaches() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        let mut rom = matched_rom();
        rom.sha1 = Some("0000000000000000000000000000000000000000".to_owned());
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(rom), "Different Game"),
            None,
        )));

        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        let state = harness.state(game_id).await;
        assert_eq!(state.status, ProviderMatchStatus::Ambiguous);
        assert_eq!(state.match_type, None);
        assert!(!state.deterministic);
        assert_eq!(state.provider_game_id, None);
        assert!(
            state.metadata.is_none(),
            "conflicting evidence stores no metadata"
        );
        assert!(
            !harness.provider.calls().contains(&"media"),
            "no cover is fetched for an unattached game"
        );
    }

    #[tokio::test]
    async fn an_unmapped_system_is_deferred_instead_of_searched_globally() {
        let harness = Harness::with_provider(FakeProvider::without_systems()).await;
        let game_id = insert_single_file_game(&harness.pool).await;

        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        let state = harness.state(game_id).await;
        assert_eq!(state.status, ProviderMatchStatus::Deferred);
        assert_eq!(
            state.unsupported_reason,
            Some(UnsupportedContentReason::SystemNotMapped)
        );
        assert_eq!(
            harness.provider.call_count(),
            0,
            "an unmapped system must not produce any provider request"
        );
    }

    #[tokio::test]
    async fn a_response_without_a_provider_content_record_becomes_a_candidate_state() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(None, "Example Quest"),
            None,
        )));
        harness.provider.queue_search(Ok(ProviderResponse::new(
            vec![
                ProviderCandidate {
                    provider_game_id: "11".to_owned(),
                    title: "Example Quest".to_owned(),
                    release_date: Some("1990".to_owned()),
                },
                ProviderCandidate {
                    provider_game_id: "12".to_owned(),
                    title: "Example Quest 2".to_owned(),
                    release_date: None,
                },
            ],
            None,
        )));

        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        let state = harness.state(game_id).await;
        assert_eq!(state.status, ProviderMatchStatus::Ambiguous);
        assert_eq!(state.match_type, None);
        assert_eq!(state.provider_game_id, None);
        assert_eq!(state.candidates.len(), 2);
        assert_eq!(state.candidates[0].provider_game_id, "11");
        assert_eq!(state.candidates[1].provider_game_id, "12");
        assert!(
            state.metadata.is_none(),
            "a heuristic candidate must never silently become a match"
        );
    }

    #[tokio::test]
    async fn a_single_heuristic_candidate_still_does_not_attach() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(None, "Example Quest"),
            None,
        )));
        harness.provider.queue_search(Ok(ProviderResponse::new(
            vec![ProviderCandidate {
                provider_game_id: "11".to_owned(),
                title: "Example Quest".to_owned(),
                release_date: None,
            }],
            None,
        )));

        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        let state = harness.state(game_id).await;
        assert_eq!(state.status, ProviderMatchStatus::Ambiguous);
        assert_eq!(state.candidates.len(), 1);
        assert!(state.provider_game_id.is_none());
        assert!(!state.deterministic);
    }

    #[tokio::test]
    async fn a_deterministic_no_match_is_bound_to_the_submitted_evidence() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness
            .provider
            .queue_identify(Err(ProviderFailureClass::NoMatch));

        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        let state = harness.state(game_id).await;
        assert_eq!(state.status, ProviderMatchStatus::NoMatch);
        assert_eq!(state.last_failure, Some(ProviderFailureClass::NoMatch));
        assert_eq!(
            harness.provider.calls(),
            vec!["identify"],
            "a hash miss must not be turned into a title search automatically"
        );

        let stored = MetadataRepository::new(harness.pool.clone())
            .load_match(game_id, MetadataProviderId::ScreenScraper)
            .await
            .unwrap()
            .expect("a no-match relationship should be recorded");
        assert!(
            stored.evidence.is_none(),
            "a negative answer is stored without a trusted match type"
        );

        let job = harness
            .job(game_id, MetadataJobKind::Identify)
            .await
            .unwrap();
        assert_eq!(job.state, MetadataJobState::Completed);
        assert_eq!(job.attempts, 0, "a definitive answer costs no retry budget");
    }

    #[tokio::test]
    async fn every_unsupported_container_format_refuses_automatic_matching() {
        let cases = [
            (
                ContentUnitKind::Chd,
                SystemId::PlayStation,
                "PS/game.chd",
                UnsupportedContentReason::ChdRepresentationUndefined,
            ),
            (
                ContentUnitKind::CueBin,
                SystemId::PlayStation,
                "PS/game.cue",
                UnsupportedContentReason::CueBinRepresentationUndefined,
            ),
            (
                ContentUnitKind::Gdi,
                SystemId::SegaDreamcast,
                "DC/game.gdi",
                UnsupportedContentReason::GdiRepresentationUndefined,
            ),
            (
                ContentUnitKind::M3u,
                SystemId::PlayStation,
                "PS/game.m3u",
                UnsupportedContentReason::PlaylistIsNotIdentity,
            ),
            (
                ContentUnitKind::SingleFile,
                SystemId::NintendoGameCube,
                "GC/game.rvz",
                UnsupportedContentReason::ContainerRepresentationUndefined,
            ),
        ];

        for (kind, system, path, expected) in cases {
            let harness = Harness::new().await;
            let game_id = insert_game(
                &harness.pool,
                system,
                kind,
                path,
                Some(SHA1),
                Some(MD5),
                Some(CRC32),
                "fingerprint-container",
            )
            .await;
            harness.provider.queue_search(Ok(ProviderResponse::new(
                vec![ProviderCandidate {
                    provider_game_id: "11".to_owned(),
                    title: "Example Quest".to_owned(),
                    release_date: None,
                }],
                None,
            )));

            harness.service.request_enrichment(game_id).await.unwrap();
            harness.drain(4).await;

            let state = harness.state(game_id).await;
            assert_eq!(
                state.status,
                ProviderMatchStatus::Deferred,
                "{path} must be deferred"
            );
            assert_eq!(state.unsupported_reason, Some(expected), "{path}");
            assert_eq!(state.match_type, None, "{path} must not attach");
            assert_eq!(state.provider_game_id, None, "{path} must not attach");
            assert_eq!(
                state.candidates.len(),
                1,
                "{path} may still offer heuristic candidates"
            );
            assert_eq!(
                harness.provider.calls(),
                vec!["search"],
                "{path} must never submit content evidence for identification"
            );
        }
    }

    #[tokio::test]
    async fn an_m3u_playlist_is_never_submitted_as_provider_identity() {
        let harness = Harness::new().await;
        let game_id = insert_game(
            &harness.pool,
            SystemId::PlayStation,
            ContentUnitKind::M3u,
            "PS/Example (Disc 1-2).m3u",
            Some(SHA1),
            Some(MD5),
            Some(CRC32),
            "fingerprint-m3u",
        )
        .await;
        harness
            .provider
            .queue_search(Ok(ProviderResponse::new(Vec::new(), None)));

        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        assert!(
            !harness.provider.calls().contains(&"identify"),
            "playlist bytes must never be submitted as identification evidence"
        );
        let state = harness.state(game_id).await;
        assert_eq!(
            state.unsupported_reason,
            Some(UnsupportedContentReason::PlaylistIsNotIdentity)
        );
    }

    // ---------------------------------------------------------------------------- stale evidence

    #[tokio::test]
    async fn same_path_content_replacement_marks_the_match_stale_and_keeps_cached_data() {
        let mut harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "The Example Quest"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("a"), None)));
        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;
        assert!(harness.state(game_id).await.deterministic);

        // M4 keeps every local identifier stable while replacing the bytes in place.
        let before = local_snapshot(&harness.pool).await;
        sqlx::query(
            "UPDATE content_files SET sha1 = '1111111111111111111111111111111111111111', \
             md5 = '11111111111111111111111111111111', crc32 = '11111111'",
        )
        .execute(&harness.pool)
        .await
        .unwrap();
        sqlx::query("UPDATE content_units SET fingerprint = 'fingerprint-2'")
            .execute(&harness.pool)
            .await
            .unwrap();

        // A read must stop claiming determinism immediately, before any sweep runs.
        let read_state = harness.state(game_id).await;
        assert_eq!(read_state.status, ProviderMatchStatus::Stale);
        assert!(!read_state.deterministic);

        let stale = harness.service.revalidate_matches().await.unwrap();
        assert_eq!(stale, 1);

        let state = harness.state(game_id).await;
        assert_eq!(state.status, ProviderMatchStatus::Stale);
        assert!(!state.deterministic);
        assert!(
            state.metadata.is_some(),
            "last-known-good metadata stays readable while the match is untrusted"
        );
        assert_eq!(
            state.cover.as_ref().map(|cover| cover.state),
            Some(MediaAssetState::Cached),
            "the cached cover stays readable"
        );
        let identify = harness
            .job(game_id, MetadataJobKind::Identify)
            .await
            .unwrap();
        assert_eq!(
            identify.state,
            MetadataJobState::Pending,
            "re-identification is scheduled"
        );

        // Local identity is untouched apart from the hashes M4 itself rewrote.
        let after = local_snapshot(&harness.pool).await;
        assert_eq!(
            before.games, after.games,
            "provider staleness must not change local games"
        );
        assert_eq!(before.units.len(), after.units.len());
        assert_eq!(before.files.len(), after.files.len());
        assert_eq!(before.units[0].0, after.units[0].0);
        assert_eq!(before.files[0].0, after.files[0].0);

        // Stale state survives a restart.
        harness.restart().await;
        let restarted = harness.state(game_id).await;
        assert_eq!(restarted.status, ProviderMatchStatus::Stale);
        assert!(!restarted.deterministic);
        assert!(restarted.metadata.is_some());
    }

    #[tokio::test]
    async fn a_changed_content_unit_fingerprint_alone_marks_the_match_stale() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "The Example Quest"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("a"), None)));
        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        sqlx::query("UPDATE content_units SET fingerprint = 'fingerprint-changed'")
            .execute(&harness.pool)
            .await
            .unwrap();

        assert_eq!(harness.service.revalidate_matches().await.unwrap(), 1);
        assert_eq!(
            harness.state(game_id).await.status,
            ProviderMatchStatus::Stale
        );
    }

    #[tokio::test]
    async fn a_refresh_on_stale_evidence_re_identifies_instead_of_re_trusting_the_provider_id() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "The Example Quest"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("a"), None)));
        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        harness.service.request_refresh(game_id).await.unwrap();
        sqlx::query("UPDATE content_units SET fingerprint = 'fingerprint-replaced'")
            .execute(&harness.pool)
            .await
            .unwrap();
        harness.provider.reset_calls();
        harness.drain(1).await;

        assert!(
            !harness.provider.calls().contains(&"fetch"),
            "a refresh must not re-fetch a provider identity whose evidence no longer holds"
        );
        assert_eq!(
            harness.state(game_id).await.status,
            ProviderMatchStatus::Stale
        );
    }

    // ------------------------------------------------------------------------------ persistence

    #[tokio::test]
    async fn provider_identity_metadata_and_provenance_survive_a_restart() {
        let mut harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "The Example Quest"),
            Some(quota(2)),
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("a"), None)));
        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        harness.restart().await;

        let state = harness.state(game_id).await;
        assert_eq!(state.status, ProviderMatchStatus::Matched);
        assert_eq!(state.match_type, Some(MatchType::DeterministicSha1));
        assert!(state.deterministic);
        assert_eq!(state.provider_game_id.as_deref(), Some("3"));
        assert_eq!(state.provider_rom_id.as_deref(), Some("101"));
        let metadata = state.metadata.expect("metadata should survive restart");
        assert_eq!(metadata.metadata.title, "The Example Quest");
        assert_eq!(metadata.provenance.provider_game_id, "3");
        assert_eq!(
            metadata.provenance.source_credit.as_deref(),
            Some("Example Contributor")
        );
        let cover = state.cover.expect("cover row should survive restart");
        assert_eq!(cover.state, MediaAssetState::Cached);
        assert!(harness
            .service
            .covers()
            .is_cached(cover.cache_relative_path.as_deref().unwrap()));
        assert_eq!(harness.scheduler_state().await.quota.max_threads, Some(2));
    }

    #[tokio::test]
    async fn a_successful_refresh_replaces_provider_derived_metadata() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "Old Title"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("a"), None)));
        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        let mut refreshed = game_record(Some(matched_rom()), "New Title");
        refreshed.metadata.publisher = Some("New Publisher".to_owned());
        // Advertise a changed cover so the refresh replaces it too.
        refreshed.primary_cover = Some(ProviderCoverDescriptor {
            crc32: Some("99999999".to_owned()),
            ..synthetic_cover()
        });
        harness
            .provider
            .queue_fetch(Ok(ProviderResponse::new(refreshed, None)));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("refreshed"), None)));

        harness.service.request_refresh(game_id).await.unwrap();
        harness.drain(6).await;

        let state = harness.state(game_id).await;
        let metadata = state.metadata.expect("metadata should be present");
        assert_eq!(metadata.metadata.title, "New Title");
        assert_eq!(
            metadata.metadata.publisher.as_deref(),
            Some("New Publisher")
        );
        assert_eq!(state.status, ProviderMatchStatus::Matched);
        assert_eq!(
            state
                .cover
                .and_then(|cover| cover.provider_crc32)
                .as_deref(),
            Some("99999999")
        );
    }

    #[tokio::test]
    async fn a_failed_refresh_retains_the_last_known_good_metadata_and_cover() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "Good Title"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("good"), None)));
        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;
        let before = harness.state(game_id).await;
        let cover_path = before
            .cover
            .as_ref()
            .and_then(|cover| cover.cache_relative_path.clone())
            .expect("a cover should be cached");
        let cover_bytes = std::fs::read(
            harness
                .service
                .covers()
                .absolute_path(&cover_path)
                .expect("cover path"),
        )
        .expect("cover should be readable");

        harness
            .provider
            .queue_fetch(Err(ProviderFailureClass::TransientServer));
        harness.service.request_refresh(game_id).await.unwrap();
        harness.drain(6).await;

        let after = harness.state(game_id).await;
        assert_eq!(
            after
                .metadata
                .as_ref()
                .map(|record| record.metadata.title.clone()),
            Some("Good Title".to_owned()),
            "a failed refresh must not replace the stored snapshot"
        );
        assert_eq!(after.status, ProviderMatchStatus::Matched);
        assert_eq!(
            after.last_failure,
            Some(ProviderFailureClass::TransientServer)
        );
        assert!(
            after.deterministic,
            "a transient failure does not untrust a match"
        );
        assert_eq!(
            std::fs::read(
                harness
                    .service
                    .covers()
                    .absolute_path(&cover_path)
                    .expect("cover path")
            )
            .expect("cover should still be readable"),
            cover_bytes
        );
    }

    // -------------------------------------------------------------------------------- media

    #[tokio::test]
    async fn an_unchanged_cover_is_served_from_the_cache_without_a_download() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "Example"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("a"), None)));
        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        // The refresh advertises the same provider checksum, so the cached file is reused.
        harness.provider.queue_fetch(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "Example"),
            None,
        )));
        harness.provider.reset_calls();
        harness.service.request_refresh(game_id).await.unwrap();
        harness.drain(6).await;

        assert!(
            !harness.provider.calls().contains(&"media"),
            "an unchanged cover must not be downloaded again"
        );
        assert_eq!(
            harness.state(game_id).await.cover.map(|cover| cover.state),
            Some(MediaAssetState::Cached)
        );
    }

    #[tokio::test]
    async fn a_game_without_provider_media_keeps_its_metadata() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        let mut record = game_record(Some(matched_rom()), "No Cover");
        record.primary_cover = None;
        harness
            .provider
            .queue_identify(Ok(ProviderResponse::new(record, None)));

        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        let state = harness.state(game_id).await;
        assert_eq!(state.status, ProviderMatchStatus::Matched);
        assert!(
            state.metadata.is_some(),
            "missing media must not lose metadata"
        );
        let cover = state.cover.expect("a media failure marker should exist");
        assert_eq!(cover.state, MediaAssetState::Failed);
        assert_eq!(
            cover.last_failure,
            Some(ProviderFailureClass::MediaUnavailable)
        );
        assert!(cover.cache_relative_path.is_none());
    }

    #[tokio::test]
    async fn a_rejected_cover_download_keeps_the_previous_file_and_records_the_failure() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "Example"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("good"), None)));
        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;
        let cover_path = harness
            .state(game_id)
            .await
            .cover
            .and_then(|cover| cover.cache_relative_path)
            .expect("a cover should be cached");
        let original =
            std::fs::read(harness.service.covers().absolute_path(&cover_path).unwrap()).unwrap();

        // A changed checksum forces a download, which then returns unusable content.
        harness.provider.clear_queues();
        harness.provider.queue_fetch(Ok(ProviderResponse::new(
            ProviderGameRecord {
                primary_cover: Some(ProviderCoverDescriptor {
                    crc32: Some("55555555".to_owned()),
                    ..synthetic_cover()
                }),
                ..game_record(Some(matched_rom()), "Example")
            },
            None,
        )));
        harness.provider.queue_media(Ok(ProviderResponse::new(
            DownloadedMedia {
                content_type: Some("text/plain".to_owned()),
                bytes: b"NOMEDIA".to_vec(),
            },
            None,
        )));
        harness.service.request_refresh(game_id).await.unwrap();
        harness.drain(6).await;

        assert_eq!(
            std::fs::read(harness.service.covers().absolute_path(&cover_path).unwrap()).unwrap(),
            original,
            "a rejected download must never replace a valid cover"
        );
        let cover = harness.state(game_id).await.cover.expect("cover row");
        assert_eq!(cover.state, MediaAssetState::Cached);
        assert_eq!(cover.cache_relative_path.as_deref(), Some(&cover_path[..]));
        assert_eq!(
            cover.last_failure,
            Some(ProviderFailureClass::MediaUnavailable)
        );
    }

    #[tokio::test]
    async fn a_cover_file_removed_from_the_cache_is_reported_as_missing() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "Example"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("a"), None)));
        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;
        let cover_path = harness
            .state(game_id)
            .await
            .cover
            .and_then(|cover| cover.cache_relative_path)
            .unwrap();

        std::fs::remove_file(harness.service.covers().absolute_path(&cover_path).unwrap()).unwrap();

        let cover = harness.state(game_id).await.cover.expect("cover row");
        assert_eq!(cover.state, MediaAssetState::Missing);
        assert!(cover.cache_relative_path.is_none());
        assert_eq!(
            harness.state(game_id).await.status,
            ProviderMatchStatus::Matched,
            "a missing cover file must not affect the match"
        );
    }

    // -------------------------------------------------------------------- queue, quota and retry

    #[tokio::test]
    async fn a_persisted_job_is_claimed_run_and_completed() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "Example"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("a"), None)));

        harness.service.request_enrichment(game_id).await.unwrap();
        let queued = harness
            .job(game_id, MetadataJobKind::Identify)
            .await
            .unwrap();
        assert_eq!(queued.state, MetadataJobState::Pending);
        assert_eq!(queued.attempts, 0);

        let processed = harness.drain(4).await;

        assert_eq!(processed.completed, 1);
        assert_eq!(
            harness
                .job(game_id, MetadataJobKind::Identify)
                .await
                .unwrap()
                .state,
            MetadataJobState::Completed
        );
    }

    #[tokio::test]
    async fn a_job_claimed_when_the_application_crashed_is_recovered_on_restart() {
        let mut harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.service.request_enrichment(game_id).await.unwrap();

        // Simulate a crash while the job was in flight.
        sqlx::query("UPDATE metadata_jobs SET state = 'running', claimed_at = ?")
            .bind(START_MS)
            .execute(&harness.pool)
            .await
            .unwrap();
        assert_eq!(
            harness
                .job(game_id, MetadataJobKind::Identify)
                .await
                .unwrap()
                .state,
            MetadataJobState::Running
        );

        harness.restart().await;

        let recovered = harness
            .job(game_id, MetadataJobKind::Identify)
            .await
            .unwrap();
        assert_eq!(
            recovered.state,
            MetadataJobState::Pending,
            "a job must never stay claimed after a crash"
        );
        assert_eq!(recovered.claimed_at, None);
    }

    #[tokio::test]
    async fn concurrency_is_capped_by_the_provider_advertised_thread_count() {
        let harness = Harness::new().await;
        for _ in 0..4 {
            let game_id = insert_single_file_game(&harness.pool).await;
            harness.service.request_enrichment(game_id).await.unwrap();
        }
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "Example"),
            Some(quota(1)),
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("a"), None)));

        // With one advertised thread each round may claim exactly one job.
        let first = harness.service.process_ready_jobs(8).await.unwrap();
        assert_eq!(first.total(), 1);

        MetadataRepository::new(harness.pool.clone())
            .update_quota(MetadataProviderId::ScreenScraper, &quota(3), START_MS)
            .await
            .unwrap();
        let second = harness.service.process_ready_jobs(8).await.unwrap();
        assert_eq!(
            second.total(),
            3,
            "a raised thread count is honoured immediately"
        );
    }

    #[tokio::test]
    async fn quota_counters_from_a_response_are_persisted() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "Example"),
            Some(quota(2)),
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("a"), None)));

        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        let state = harness.scheduler_state().await;
        assert_eq!(state.quota.max_threads, Some(2));
        assert_eq!(state.quota.max_requests_per_minute, Some(3072));
        assert_eq!(state.quota.max_requests_per_day, Some(10_000));
        assert_eq!(state.quota.max_negative_requests_per_day, Some(1_000));
        assert_eq!(state.quota.requests_today, Some(7));
        assert!(state.observed_at.is_some());
    }

    #[tokio::test]
    async fn each_quota_class_produces_its_own_deferral_and_stops_further_requests() {
        let cases = [
            ProviderFailureClass::CapacityDeferred,
            ProviderFailureClass::DailyQuotaExceeded,
            ProviderFailureClass::NegativeQuotaExceeded,
            ProviderFailureClass::ProviderUnavailable,
            ProviderFailureClass::ProviderRestricted,
        ];

        for failure in cases {
            let harness = Harness::new().await;
            let first = insert_single_file_game(&harness.pool).await;
            let second = insert_single_file_game(&harness.pool).await;
            harness.provider.queue_identify(Err(failure));
            harness.service.request_enrichment(first).await.unwrap();
            harness.service.request_enrichment(second).await.unwrap();

            harness.service.process_ready_jobs(8).await.unwrap();
            let calls_after_first_round = harness.provider.call_count();

            let job = harness.job(first, MetadataJobKind::Identify).await.unwrap();
            assert_eq!(job.state, MetadataJobState::Deferred, "{failure:?}");
            assert_eq!(
                job.attempts, 0,
                "{failure:?} is provider backpressure and must not spend the retry budget"
            );
            assert_eq!(job.last_failure, Some(failure), "{failure:?}");
            assert!(
                job.earliest_next_attempt_at.is_some_and(|at| at > START_MS),
                "{failure:?} must persist a future attempt time"
            );

            let scheduler = harness.scheduler_state().await;
            assert_eq!(scheduler.defer_reason, Some(failure), "{failure:?}");
            assert!(scheduler.deferred_until.is_some_and(|at| at > START_MS));

            // Further rounds must issue no requests at all while the provider is deferred.
            for _ in 0..5 {
                let processed = harness.service.process_ready_jobs(8).await.unwrap();
                assert_eq!(processed.total(), 0, "{failure:?}");
                assert!(processed.wait_until.is_some(), "{failure:?}");
            }
            assert_eq!(
                harness.provider.call_count(),
                calls_after_first_round,
                "{failure:?} must not be probed in a tight loop"
            );
        }
    }

    #[tokio::test]
    async fn a_deferral_expires_and_work_resumes() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness
            .provider
            .queue_identify(Err(ProviderFailureClass::CapacityDeferred));
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "Example"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("a"), None)));
        harness.service.request_enrichment(game_id).await.unwrap();

        harness.service.process_ready_jobs(8).await.unwrap();
        assert_eq!(
            harness.service.process_ready_jobs(8).await.unwrap().total(),
            0
        );

        // Advance past the persisted deferral.
        harness.clock.advance(2 * 60 * 1_000);
        harness.drain(4).await;

        assert_eq!(
            harness.state(game_id).await.status,
            ProviderMatchStatus::Matched
        );
    }

    #[tokio::test]
    async fn transient_failures_retry_with_backoff_and_are_bounded() {
        for failure in [
            ProviderFailureClass::Transport,
            ProviderFailureClass::TransientServer,
            ProviderFailureClass::MalformedResponse,
        ] {
            let harness = Harness::new().await;
            let game_id = insert_single_file_game(&harness.pool).await;
            harness.provider.queue_identify(Err(failure));
            harness.service.request_enrichment(game_id).await.unwrap();

            let mut attempt_times = Vec::new();
            for _ in 0..8 {
                harness.service.process_ready_jobs(8).await.unwrap();
                let job = harness
                    .job(game_id, MetadataJobKind::Identify)
                    .await
                    .unwrap();
                if job.state == MetadataJobState::Failed {
                    break;
                }
                let next = job
                    .earliest_next_attempt_at
                    .expect("a retry must have a next attempt time");
                attempt_times.push(next - harness.clock.now_ms());
                harness.clock.set(next);
            }

            let job = harness
                .job(game_id, MetadataJobKind::Identify)
                .await
                .unwrap();
            assert_eq!(
                job.state,
                MetadataJobState::Failed,
                "{failure:?} must stop retrying eventually"
            );
            assert_eq!(
                job.attempts, 5,
                "{failure:?} must use a bounded attempt budget"
            );
            assert!(
                attempt_times.windows(2).all(|pair| pair[0] < pair[1]),
                "{failure:?} delays must grow: {attempt_times:?}"
            );

            // A parked job never mutates local library state and can be re-armed by the user.
            harness.service.request_enrichment(game_id).await.unwrap();
            let rearmed = harness
                .job(game_id, MetadataJobKind::Identify)
                .await
                .unwrap();
            assert_eq!(rearmed.state, MetadataJobState::Pending);
            assert_eq!(rearmed.attempts, 0);
        }
    }

    #[tokio::test]
    async fn permanent_failures_are_never_retried_automatically() {
        for failure in [
            ProviderFailureClass::InvalidRequest,
            ProviderFailureClass::DeveloperAuthenticationFailed,
            ProviderFailureClass::UserAuthenticationFailed,
            ProviderFailureClass::ClientRejected,
            ProviderFailureClass::CredentialsUnavailable,
        ] {
            let harness = Harness::new().await;
            let game_id = insert_single_file_game(&harness.pool).await;
            harness.provider.queue_identify(Err(failure));
            harness.service.request_enrichment(game_id).await.unwrap();

            harness.service.process_ready_jobs(8).await.unwrap();

            let job = harness
                .job(game_id, MetadataJobKind::Identify)
                .await
                .unwrap();
            assert_eq!(job.state, MetadataJobState::Failed, "{failure:?}");
            assert_eq!(job.last_failure, Some(failure));
            assert_eq!(job.earliest_next_attempt_at, None);

            let calls = harness.provider.call_count();
            harness.clock.advance(24 * 60 * 60 * 1_000);
            for _ in 0..3 {
                harness.service.process_ready_jobs(8).await.unwrap();
            }
            assert_eq!(
                harness.provider.call_count(),
                calls,
                "{failure:?} must not be retried until configuration changes"
            );
            assert_eq!(
                harness.state(game_id).await.status,
                ProviderMatchStatus::Failed
            );
        }
    }

    #[tokio::test]
    async fn a_deferred_job_keeps_its_state_across_a_restart() {
        let mut harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness
            .provider
            .queue_identify(Err(ProviderFailureClass::DailyQuotaExceeded));
        harness.service.request_enrichment(game_id).await.unwrap();
        harness.service.process_ready_jobs(8).await.unwrap();
        let before = harness
            .job(game_id, MetadataJobKind::Identify)
            .await
            .unwrap();

        harness.restart().await;

        let after = harness
            .job(game_id, MetadataJobKind::Identify)
            .await
            .unwrap();
        assert_eq!(after.state, MetadataJobState::Deferred);
        assert_eq!(
            after.earliest_next_attempt_at,
            before.earliest_next_attempt_at
        );
        assert_eq!(
            after.last_failure,
            Some(ProviderFailureClass::DailyQuotaExceeded)
        );
        let scheduler = harness.scheduler_state().await;
        assert_eq!(
            scheduler.defer_reason,
            Some(ProviderFailureClass::DailyQuotaExceeded)
        );
        assert_eq!(
            harness.service.process_ready_jobs(8).await.unwrap().total(),
            0,
            "the persisted deferral is still honoured after a restart"
        );
    }

    #[tokio::test]
    async fn an_exhausted_daily_budget_blocks_work_before_any_request_is_issued() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.service.request_enrichment(game_id).await.unwrap();
        MetadataRepository::new(harness.pool.clone())
            .update_quota(
                MetadataProviderId::ScreenScraper,
                &ProviderQuotaSnapshot {
                    max_threads: Some(4),
                    max_requests_per_day: Some(10_000),
                    requests_today: Some(10_000),
                    ..ProviderQuotaSnapshot::default()
                },
                START_MS,
            )
            .await
            .unwrap();

        let processed = harness.service.process_ready_jobs(8).await.unwrap();

        assert_eq!(processed.total(), 0);
        assert!(processed.wait_until.is_some());
        assert_eq!(harness.provider.call_count(), 0);
    }

    #[tokio::test]
    async fn the_rolling_minute_budget_defers_a_job_without_spending_a_retry() {
        let harness = Harness::new().await;
        MetadataRepository::new(harness.pool.clone())
            .update_quota(
                MetadataProviderId::ScreenScraper,
                &ProviderQuotaSnapshot {
                    max_threads: Some(4),
                    max_requests_per_minute: Some(1),
                    ..ProviderQuotaSnapshot::default()
                },
                START_MS,
            )
            .await
            .unwrap();
        let first = insert_single_file_game(&harness.pool).await;
        let second = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(None, "Example"),
            None,
        )));
        harness
            .provider
            .queue_search(Ok(ProviderResponse::new(Vec::new(), None)));
        harness.service.request_enrichment(first).await.unwrap();
        harness.service.request_enrichment(second).await.unwrap();

        let processed = harness.service.process_ready_jobs(8).await.unwrap();

        assert!(
            processed.deferred >= 1,
            "the minute budget must defer the surplus"
        );
        let deferred = harness
            .job(second, MetadataJobKind::Identify)
            .await
            .unwrap();
        assert_eq!(deferred.state, MetadataJobState::Deferred);
        assert_eq!(deferred.attempts, 0);
        assert_eq!(
            deferred.last_failure,
            Some(ProviderFailureClass::CapacityDeferred)
        );
    }

    // ---------------------------------------------------------------------------------- offline

    #[tokio::test]
    async fn offline_operation_keeps_cached_data_readable_and_defers_new_work() {
        let mut harness = Harness::new().await;
        let cached_game = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(Some(matched_rom()), "Cached Title"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("cached"), None)));
        harness
            .service
            .request_enrichment(cached_game)
            .await
            .unwrap();
        harness.drain(4).await;

        // The provider is now unreachable, as it would be with no network.
        let new_game = insert_single_file_game(&harness.pool).await;
        let before = local_snapshot(&harness.pool).await;
        harness.provider.clear_queues();
        harness
            .provider
            .queue_identify(Err(ProviderFailureClass::Transport));
        harness
            .provider
            .queue_fetch(Err(ProviderFailureClass::Transport));
        harness.provider.reset_calls();

        harness.service.request_enrichment(new_game).await.unwrap();
        harness.service.request_refresh(cached_game).await.unwrap();
        harness.drain(3).await;

        // One attempt each, then the provider itself is deferred and nothing more is issued.
        let calls_after_failure = harness.provider.call_count();
        assert!(calls_after_failure >= 1);
        for _ in 0..10 {
            let processed = harness.service.process_ready_jobs(8).await.unwrap();
            assert_eq!(processed.total(), 0);
            assert!(processed.wait_until.is_some());
        }
        assert_eq!(
            harness.provider.call_count(),
            calls_after_failure,
            "an offline client must not retry in a busy loop"
        );

        // Cached data stays fully readable.
        let cached = harness.state(cached_game).await;
        assert_eq!(cached.status, ProviderMatchStatus::Matched);
        assert!(cached.deterministic);
        assert_eq!(
            cached
                .metadata
                .as_ref()
                .map(|record| record.metadata.title.clone()),
            Some("Cached Title".to_owned())
        );
        let cover = cached.cover.expect("cover should remain readable offline");
        assert_eq!(cover.state, MediaAssetState::Cached);
        assert!(harness
            .service
            .covers()
            .is_cached(cover.cache_relative_path.as_deref().unwrap()));

        // The provider status reports offline rather than pretending everything is fine.
        let status = harness.service.provider_status().await.unwrap();
        assert!(status.offline);
        assert_eq!(status.defer_reason, Some(ProviderFailureClass::Transport));
        assert!(status.deferred_jobs >= 1);

        // Local library rows are untouched.
        assert_eq!(before.games, local_snapshot(&harness.pool).await.games);

        // Restarting offline is consistent and still issues nothing.
        harness.restart().await;
        let restarted = harness.state(cached_game).await;
        assert_eq!(restarted.status, ProviderMatchStatus::Matched);
        assert!(restarted.metadata.is_some());
        assert_eq!(
            harness.service.process_ready_jobs(8).await.unwrap().total(),
            0
        );
    }

    #[tokio::test]
    async fn repeated_transport_failures_lengthen_the_provider_deferral() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness
            .provider
            .queue_identify(Err(ProviderFailureClass::Transport));
        harness.service.request_enrichment(game_id).await.unwrap();

        let mut deferrals = Vec::new();
        for _ in 0..3 {
            harness.service.process_ready_jobs(8).await.unwrap();
            let scheduler = harness.scheduler_state().await;
            let until = scheduler.deferred_until.expect("a deferral should be set");
            deferrals.push(until - harness.clock.now_ms());
            harness.clock.set(
                harness
                    .job(game_id, MetadataJobKind::Identify)
                    .await
                    .unwrap()
                    .earliest_next_attempt_at
                    .unwrap()
                    .max(until),
            );
        }

        assert!(
            deferrals.windows(2).all(|pair| pair[0] < pair[1]),
            "consecutive transport failures must back off further: {deferrals:?}"
        );
    }

    // ------------------------------------------------------------------- local failure isolation

    #[tokio::test]
    async fn no_provider_failure_can_change_local_library_identity_or_availability() {
        let failures = [
            ProviderFailureClass::DeveloperAuthenticationFailed,
            ProviderFailureClass::UserAuthenticationFailed,
            ProviderFailureClass::Transport,
            ProviderFailureClass::ProviderUnavailable,
            ProviderFailureClass::DailyQuotaExceeded,
            ProviderFailureClass::NegativeQuotaExceeded,
            ProviderFailureClass::CapacityDeferred,
            ProviderFailureClass::NoMatch,
            ProviderFailureClass::MalformedResponse,
            ProviderFailureClass::InvalidRequest,
            ProviderFailureClass::ClientRejected,
            ProviderFailureClass::CredentialsUnavailable,
            ProviderFailureClass::MediaUnavailable,
        ];

        for failure in failures {
            let harness = Harness::new().await;
            let game_id = insert_single_file_game(&harness.pool).await;
            let library_root = harness.app_data.join("library-content");
            std::fs::create_dir_all(&library_root).unwrap();
            let user_file = library_root.join("Example Quest (USA).sfc");
            std::fs::write(&user_file, b"synthetic-rom-bytes").unwrap();
            let before = local_snapshot(&harness.pool).await;

            harness.provider.queue_identify(Err(failure));
            harness.provider.queue_search(Err(failure));
            harness.service.request_enrichment(game_id).await.unwrap();
            harness.drain(3).await;

            let after = local_snapshot(&harness.pool).await;
            assert_eq!(before, after, "{failure:?} changed local library state");
            assert_eq!(
                after.games.len(),
                1,
                "{failure:?} must never delete or hide a game"
            );
            assert_eq!(after.games[0].2, "available", "{failure:?}");
            assert_eq!(after.units.len(), 1, "{failure:?}");
            assert_eq!(after.files.len(), 1, "{failure:?}");
            assert_eq!(
                std::fs::read(&user_file).unwrap(),
                b"synthetic-rom-bytes",
                "{failure:?} must never touch user content on disk"
            );

            // The game remains readable through the metadata surface too.
            let state = harness.state(game_id).await;
            assert_eq!(state.game_id, game_id);
            assert_ne!(state.status, ProviderMatchStatus::Matched, "{failure:?}");
        }
    }

    #[tokio::test]
    async fn an_ambiguous_candidate_state_leaves_the_local_game_intact() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        let before = local_snapshot(&harness.pool).await;
        harness.provider.queue_identify(Ok(ProviderResponse::new(
            game_record(None, "Example"),
            None,
        )));
        harness.provider.queue_search(Ok(ProviderResponse::new(
            vec![ProviderCandidate {
                provider_game_id: "11".to_owned(),
                title: "Example".to_owned(),
                release_date: None,
            }],
            None,
        )));

        harness.service.request_enrichment(game_id).await.unwrap();
        harness.drain(4).await;

        assert_eq!(before, local_snapshot(&harness.pool).await);
        assert_eq!(
            harness.state(game_id).await.status,
            ProviderMatchStatus::Ambiguous
        );
    }

    // ------------------------------------------------------------------------ user-owned state

    #[tokio::test]
    async fn a_user_selected_candidate_attaches_as_user_confirmed_and_survives_refresh() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.provider.queue_fetch(Ok(ProviderResponse::new(
            game_record(None, "User Chosen Title"),
            None,
        )));
        harness
            .provider
            .queue_media(Ok(ProviderResponse::new(synthetic_png("user"), None)));

        harness
            .service
            .select_provider_candidate(game_id, "11")
            .await
            .unwrap();
        harness.drain(4).await;

        let state = harness.state(game_id).await;
        assert_eq!(state.status, ProviderMatchStatus::Matched);
        assert_eq!(state.match_type, Some(MatchType::HeuristicUserConfirmed));
        assert!(
            !state.deterministic,
            "a user-confirmed match is never reported as hash-exact"
        );
        assert_eq!(
            state
                .user_selection
                .as_ref()
                .map(|selection| selection.provider_game_id.clone()),
            Some("11".to_owned())
        );
        assert_eq!(
            state.metadata.map(|record| record.metadata.title),
            Some("User Chosen Title".to_owned())
        );
        assert_eq!(
            harness.provider.calls(),
            vec!["fetch", "media"],
            "a pinned provider game skips content identification"
        );

        // A provider refresh replaces provider-derived data but never the user's decision.
        harness.provider.queue_fetch(Ok(ProviderResponse::new(
            game_record(None, "Refreshed Title"),
            None,
        )));
        harness.service.request_refresh(game_id).await.unwrap();
        harness.drain(6).await;
        let refreshed = harness.state(game_id).await;
        assert_eq!(
            refreshed
                .user_selection
                .map(|selection| selection.provider_game_id),
            Some("11".to_owned())
        );

        harness
            .service
            .clear_provider_candidate(game_id)
            .await
            .unwrap();
        assert!(harness.state(game_id).await.user_selection.is_none());
    }

    // -------------------------------------------------------------------- credential boundary

    #[tokio::test]
    async fn personal_credentials_are_stored_only_in_the_vault_and_never_returned() {
        let harness = Harness::new().await;

        assert_eq!(
            harness.service.user_account_state().await.unwrap(),
            (UserAccountState::NotConfigured, None)
        );

        harness
            .service
            .set_user_credentials("fake-account", SecretString::new("fake-user-password"))
            .await
            .unwrap();

        let (state, username) = harness.service.user_account_state().await.unwrap();
        assert_eq!(state, UserAccountState::Configured);
        assert_eq!(username.as_deref(), Some("fake-account"));

        // No table stores the password, and nothing stores it as a value at all.
        let dump = dump_metadata_tables(&harness.pool).await;
        assert!(
            !dump.contains("fake-user-password"),
            "SQLite must not hold the password"
        );
        let account: String = sqlx::query_scalar(
            "SELECT vault_reference FROM provider_user_accounts WHERE provider_id = 'screenscraper'",
        )
        .fetch_one(&harness.pool)
        .await
        .unwrap();
        assert_eq!(account, "screenscraper-user");

        // The vault holds it, and the status surface never exposes it.
        let stored = harness.vault.load("screenscraper-user").unwrap().unwrap();
        assert_eq!(stored.password.expose(), "fake-user-password");
        let status = harness.service.provider_status().await.unwrap();
        let rendered = format!("{status:?}");
        assert!(!rendered.contains("fake-user-password"));
        assert_eq!(status.user_account, UserAccountState::Configured);
        assert_eq!(status.user_account_name.as_deref(), Some("fake-account"));

        harness.service.clear_user_credentials().await.unwrap();
        assert_eq!(
            harness.service.user_account_state().await.unwrap(),
            (UserAccountState::NotConfigured, None)
        );
        assert!(harness.vault.load("screenscraper-user").unwrap().is_none());
    }

    #[tokio::test]
    async fn stored_personal_credentials_are_reloaded_into_the_provider_after_a_restart() {
        let mut harness = Harness::new().await;
        harness
            .service
            .set_user_credentials("fake-account", SecretString::new("fake-user-password"))
            .await
            .unwrap();

        harness.restart().await;

        let (state, username) = harness.service.user_account_state().await.unwrap();
        assert_eq!(state, UserAccountState::Configured);
        assert_eq!(username.as_deref(), Some("fake-account"));
    }

    #[tokio::test]
    async fn an_unavailable_vault_is_reported_without_breaking_the_application() {
        let mut harness = Harness::new().await;
        harness
            .service
            .set_user_credentials("fake-account", SecretString::new("fake-user-password"))
            .await
            .unwrap();

        harness.vault.set_available(false);
        let (state, username) = harness.service.user_account_state().await.unwrap();
        assert_eq!(state, UserAccountState::VaultUnavailable);
        assert_eq!(username, None);

        // Restarting with a broken vault still starts, and metadata work is unaffected locally.
        harness.restart().await;
        let game_id = insert_single_file_game(&harness.pool).await;
        harness.service.request_enrichment(game_id).await.unwrap();
        assert_eq!(
            harness.state(game_id).await.status,
            ProviderMatchStatus::Pending
        );
    }

    #[tokio::test]
    async fn invalid_credential_input_is_rejected_before_it_reaches_the_vault() {
        let harness = Harness::new().await;

        assert!(harness
            .service
            .set_user_credentials("   ", SecretString::new("fake"))
            .await
            .is_err());
        assert!(harness
            .service
            .set_user_credentials("account\nwith-newline", SecretString::new("fake"))
            .await
            .is_err());
        assert!(harness
            .service
            .set_user_credentials("account", SecretString::new(""))
            .await
            .is_err());
        assert_eq!(
            harness.service.user_account_state().await.unwrap(),
            (UserAccountState::NotConfigured, None)
        );
    }

    // -------------------------------------------------------------------------------- scheduling

    #[tokio::test]
    async fn games_without_a_provider_relationship_are_enqueued_in_bounded_batches() {
        let harness = Harness::new().await;
        for _ in 0..3 {
            insert_single_file_game(&harness.pool).await;
        }

        let enqueued = harness.service.enqueue_missing_metadata().await.unwrap();
        assert_eq!(enqueued, 3);

        // Running it again must not duplicate work.
        assert_eq!(harness.service.enqueue_missing_metadata().await.unwrap(), 0);
        let job_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM metadata_jobs")
            .fetch_one(&harness.pool)
            .await
            .unwrap();
        assert_eq!(job_count, 3);
    }

    #[tokio::test]
    async fn requesting_metadata_for_an_unknown_game_is_rejected() {
        let harness = Harness::new().await;

        assert!(harness
            .service
            .request_enrichment(GameId(999))
            .await
            .is_err());
        assert!(harness.service.request_refresh(GameId(999)).await.is_err());
        assert!(harness
            .service
            .select_provider_candidate(GameId(999), "1")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn a_pending_game_reports_a_usable_empty_state() {
        let harness = Harness::new().await;
        let game_id = insert_single_file_game(&harness.pool).await;

        let state = harness.state(game_id).await;

        assert_eq!(state.status, ProviderMatchStatus::Pending);
        assert_eq!(state.match_type, None);
        assert!(!state.deterministic);
        assert!(state.metadata.is_none());
        assert!(state.cover.is_none());
        assert!(state.candidates.is_empty());
        assert!(state.jobs.is_empty());
    }

    /// Concatenates every text value in the metadata tables, so a test can assert that a secret
    /// appears nowhere in SQLite.
    async fn dump_metadata_tables(pool: &SqlitePool) -> String {
        let mut dump = String::new();
        for table in [
            "SELECT * FROM provider_matches",
            "SELECT * FROM provider_match_evidence",
            "SELECT * FROM provider_match_candidates",
            "SELECT * FROM provider_metadata",
            "SELECT * FROM provider_media_assets",
            "SELECT * FROM metadata_jobs",
            "SELECT * FROM provider_scheduler_state",
            "SELECT * FROM provider_user_accounts",
            "SELECT * FROM user_provider_selections",
        ] {
            let rows = sqlx::query(table)
                .fetch_all(pool)
                .await
                .expect("table should be readable");
            for row in rows {
                for index in 0..row.len() {
                    if let Ok(Some(value)) = row.try_get::<Option<String>, _>(index) {
                        dump.push_str(&value);
                        dump.push('\n');
                    }
                }
            }
        }
        dump
    }
}
