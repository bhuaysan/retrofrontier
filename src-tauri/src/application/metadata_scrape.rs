//! User-initiated metadata scrape runs.
//!
//! This is an orchestration layer *above* the M5 metadata queue, not a second one beside it. It
//! owns which user-initiated batch operation is in progress, what its fixed target set is, how much
//! of it may be in the provider queue at any moment, and when it is finished or stopped. Everything
//! below the queue — provider requests, quota, deferral, retry, deterministic matching, ambiguous
//! candidates, accepted-match persistence, stale evidence, covers — stays exactly where M5 put it.
//!
//! Nothing here talks to a provider, and no provider call ever happens inside one of its
//! transactions.

use crate::domain::metadata::MetadataProviderId;
use crate::domain::metadata_scrape::{
    classify_scrape_item, MetadataScrapeMode, MetadataScrapePreview, MetadataScrapeRunId,
    MetadataScrapeRunStatus, MetadataScrapeStatus,
};
use crate::error::AppError;
use crate::repositories::metadata_scrape::MetadataScrapeRepository;
use crate::services::metadata_queue::Clock;
use std::sync::Arc;

/// Wakes the metadata worker when explicit new work appears.
///
/// The worker otherwise sleeps for up to a minute when idle, which would make pressing START
/// SCRAPER look broken for that minute. A wake-up only shortens the sleep: the scheduler still
/// re-checks provider deferral, quota, the rolling minute budget, and per-job retry timing before
/// it issues anything, so this can bring work forward but never past a wait the provider imposed.
#[derive(Clone, Default)]
pub struct MetadataWorkSignal(Arc<tokio::sync::Notify>);

impl MetadataWorkSignal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Signals that work worth waking for exists.
    ///
    /// A signal raised while the worker is mid-round is remembered rather than lost, so work is
    /// never left sitting until the next idle timeout.
    pub fn notify(&self) {
        self.0.notify_one();
    }

    pub async fn notified(&self) {
        self.0.notified().await;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MetadataScrapeConfig {
    /// Live provider jobs one run may own at once.
    ///
    /// This is what keeps the active queue bounded: a 20,000-game target set lives in
    /// `metadata_scrape_run_items`, and only a window of it is ever a `metadata_job`. Sized as a
    /// small multiple of the worker's own batch so the scheduler always has more ready work than it
    /// can claim in a round — which is what lets priority ordering matter — without the queue
    /// growing with the library.
    pub feed_window: usize,
    /// Games moved into the queue in one feeding transaction.
    pub feed_batch: usize,
    /// Fed games examined in one reconciliation pass.
    ///
    /// Only fed items are examined and the feeder bounds how many of those exist, so this is a
    /// safety margin rather than the real limit.
    pub reconcile_limit: usize,
}

impl Default for MetadataScrapeConfig {
    fn default() -> Self {
        Self {
            feed_window: 200,
            feed_batch: 50,
            reconcile_limit: 1_000,
        }
    }
}

/// What one orchestration round did. Returned so the worker can pace itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetadataScrapeAdvance {
    pub reconciled: usize,
    pub fed: usize,
    pub finished: bool,
}

impl MetadataScrapeAdvance {
    pub fn did_work(&self) -> bool {
        self.reconciled > 0 || self.fed > 0 || self.finished
    }
}

pub struct MetadataScrapeApplicationService {
    repository: MetadataScrapeRepository,
    clock: Arc<dyn Clock>,
    signal: MetadataWorkSignal,
    provider_id: MetadataProviderId,
    config: MetadataScrapeConfig,
}

impl MetadataScrapeApplicationService {
    /// Builds the service and resolves anything an unclean shutdown left behind.
    ///
    /// A run that is still active survives a restart untouched: its target set, its item states and
    /// its queued jobs are all in SQLite, so the next worker round simply carries on feeding it. No
    /// in-memory progress object has to survive, and the Settings screen does not have to be open.
    pub async fn initialize(
        pool: sqlx::SqlitePool,
        clock: Arc<dyn Clock>,
        signal: MetadataWorkSignal,
        provider_id: MetadataProviderId,
        config: MetadataScrapeConfig,
    ) -> Result<Self, AppError> {
        let repository = MetadataScrapeRepository::new(pool);
        let recovered = repository
            .recover_preparing_runs(provider_id, clock.now_ms())
            .await?;
        if recovered > 0 {
            tracing::info!(
                runs = recovered,
                "resolved interrupted metadata scrape runs"
            );
        }

        Ok(Self {
            repository,
            clock,
            signal,
            provider_id,
            config,
        })
    }

    /// How many games a mode would target if it started now.
    pub async fn preview(
        &self,
        mode: MetadataScrapeMode,
    ) -> Result<MetadataScrapePreview, AppError> {
        Ok(MetadataScrapePreview {
            mode,
            eligible_games: self
                .repository
                .count_eligible_games(self.provider_id, mode)
                .await?,
        })
    }

    /// The active run, or the most recent finished one.
    pub async fn status(&self) -> Result<MetadataScrapeStatus, AppError> {
        let run = self.repository.latest_run(self.provider_id).await?;
        Ok(MetadataScrapeStatus {
            provider_id: self.provider_id,
            active: run.as_ref().is_some_and(|run| run.status.is_active()),
            run,
        })
    }

    /// Starts a run and feeds its first batch.
    ///
    /// A second concurrent start does not fail: it returns the run that is already in progress, so
    /// the UI shows what is actually happening rather than an error about a race the user did not
    /// cause.
    pub async fn start(&self, mode: MetadataScrapeMode) -> Result<MetadataScrapeStatus, AppError> {
        let now = self.clock.now_ms();
        let Some(run_id) = self
            .repository
            .create_run(self.provider_id, mode, now)
            .await?
        else {
            tracing::info!("a metadata scrape run is already active for this provider");
            return self.status().await;
        };

        tracing::info!(run = %run_id, mode = ?mode, "metadata scrape run started");
        // Fill the window immediately so the first provider request does not wait for a worker
        // round, then wake the worker so it does not wait out its idle sleep either.
        self.advance().await?;
        self.signal.notify();
        self.status().await
    }

    /// Begins a cooperative stop.
    ///
    /// Feeding stops at once and the queued work this run still owns is detached. A request already
    /// in flight is left to finish so its result can still be recorded, which is why the run passes
    /// through `Stopping` rather than ending immediately.
    pub async fn stop(&self) -> Result<MetadataScrapeStatus, AppError> {
        let Some(run) = self.repository.active_run(self.provider_id).await? else {
            return self.status().await;
        };

        if run.status != MetadataScrapeRunStatus::Stopping {
            let now = self.clock.now_ms();
            self.repository
                .begin_stop(run.id, self.provider_id, now)
                .await?;
            tracing::info!(run = %run.id, "metadata scrape run stopping");
        }
        // Settle immediately when nothing was in flight.
        self.advance().await?;
        self.status().await
    }

    /// One orchestration round: reconcile results, finish the run if it is done, then top up.
    ///
    /// Reconciliation runs first so finished games release window headroom before more work is fed,
    /// and finalization runs before feeding so a completed run does not enqueue one last batch on
    /// its way out.
    pub async fn advance(&self) -> Result<MetadataScrapeAdvance, AppError> {
        let Some(run) = self.repository.active_run(self.provider_id).await? else {
            return Ok(MetadataScrapeAdvance::default());
        };

        let mut advance = MetadataScrapeAdvance {
            reconciled: self.reconcile(run.id).await?,
            ..MetadataScrapeAdvance::default()
        };

        let now = self.clock.now_ms();
        advance.finished = match run.status {
            MetadataScrapeRunStatus::Stopping => {
                self.repository.stop_if_settled(run.id, now).await?
            }
            _ => self.repository.complete_if_finished(run.id, now).await?,
        };
        if advance.finished {
            tracing::info!(run = %run.id, "metadata scrape run finished");
            return Ok(advance);
        }
        if run.status == MetadataScrapeRunStatus::Stopping {
            return Ok(advance);
        }

        advance.fed = self.feed(run.id, run.mode).await?;
        Ok(advance)
    }

    /// Maps authoritative M5 state back onto run progress.
    ///
    /// The run never decides what a provider answer means; it reads what M5 recorded and translates
    /// it into game-level progress. That direction is deliberate: a push from inside job processing
    /// would have to be undone on every crash, whereas re-deriving from the authority is idempotent
    /// and survives a restart with no extra bookkeeping.
    async fn reconcile(&self, run_id: MetadataScrapeRunId) -> Result<usize, AppError> {
        let facts = self
            .repository
            .unfinished_item_facts(run_id, self.provider_id, self.config.reconcile_limit)
            .await?;
        if facts.is_empty() {
            return Ok(0);
        }

        let outcomes: Vec<_> = facts
            .iter()
            .map(|facts| (facts.game_id, classify_scrape_item(facts)))
            .collect();
        self.repository
            .apply_item_states(run_id, &outcomes, self.clock.now_ms())
            .await
    }

    /// Tops the provider queue up to the feed window.
    async fn feed(
        &self,
        run_id: MetadataScrapeRunId,
        mode: MetadataScrapeMode,
    ) -> Result<usize, AppError> {
        let live = self.repository.live_owned_jobs(run_id).await?;
        let headroom = self
            .config
            .feed_window
            .saturating_sub(usize::try_from(live.max(0)).unwrap_or(usize::MAX))
            .min(self.config.feed_batch);
        if headroom == 0 {
            return Ok(0);
        }

        self.repository
            .feed_pending_items(
                run_id,
                self.provider_id,
                mode,
                headroom,
                self.clock.now_ms(),
            )
            .await
    }

    #[cfg(test)]
    pub fn repository(&self) -> &MetadataScrapeRepository {
        &self.repository
    }
}
