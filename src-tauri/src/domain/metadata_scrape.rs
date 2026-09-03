//! User-initiated metadata scrape runs.
//!
//! A scrape run answers a different question from a metadata job, and the two must not be
//! conflated:
//!
//! * `MetadataScrapeRun` — *which user-initiated batch operation is in progress?* It owns the mode,
//!   a fixed target set, progress, stop semantics, and restart semantics.
//! * `MetadataJob` (M5) — *which concrete provider operation must the existing pipeline execute?*
//!   It owns provider requests, quota, deferral, retry, matching, and persistence.
//!
//! Nothing in this module performs I/O or knows about ScreenScraper. It carries the state types and
//! the one classification rule that turns authoritative M5 state back into run progress, so that
//! rule can be asserted exhaustively without a database or a provider.

use crate::domain::library::GameId;
use crate::domain::metadata::{MetadataJobKind, MetadataProviderId, ProviderMatchStatus};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Identifier of one persistent scrape run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MetadataScrapeRunId(pub i64);

impl fmt::Display for MetadataScrapeRunId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// The two whole-library scraper modes offered in V1.
///
/// Both are deliberately whole-library. Scrape-by-system, scrape-by-filter, and scrape-by-selection
/// are explicit non-goals: they multiply the eligibility surface without changing what the pipeline
/// below actually does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetadataScrapeMode {
    /// Games that have never had a meaningful provider attempt.
    ///
    /// Deliberately *not* "games without a cover": a definitive no-match, an ambiguous candidate
    /// set, an unsupported content shape, and a parked failure are all answers, and re-asking the
    /// provider for them on every run would burn quota to learn nothing.
    MissingMetadata,
    /// Accepted provider matches whose metadata and cover should be refetched.
    RefreshMatched,
}

impl MetadataScrapeMode {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::MissingMetadata => "missing_metadata",
            Self::RefreshMatched => "refresh_matched",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "missing_metadata" => Self::MissingMetadata,
            "refresh_matched" => Self::RefreshMatched,
            _ => return None,
        })
    }

    /// Provider work one game of this mode requires.
    ///
    /// Refresh needs both halves of an M5 refresh. A run item is therefore never terminal merely
    /// because the first of them finished.
    pub fn required_job_kinds(self) -> &'static [MetadataJobKind] {
        match self {
            Self::MissingMetadata => &[MetadataJobKind::Identify],
            Self::RefreshMatched => &[
                MetadataJobKind::RefreshMetadata,
                MetadataJobKind::RefreshCover,
            ],
        }
    }
}

/// Lifecycle of one run.
///
/// `Preparing` exists only for the window in which a run row is committed but its target snapshot
/// is not. Startup recovery resolves it; it is never a resting state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetadataScrapeRunStatus {
    Preparing,
    Running,
    Stopping,
    Completed,
    Stopped,
}

impl MetadataScrapeRunStatus {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Completed => "completed",
            Self::Stopped => "stopped",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "preparing" => Self::Preparing,
            "running" => Self::Running,
            "stopping" => Self::Stopping,
            "completed" => Self::Completed,
            "stopped" => Self::Stopped,
            _ => return None,
        })
    }

    /// True while the run still owns the provider for bulk work.
    ///
    /// This is the predicate behind the one-active-run-per-provider invariant, which is enforced by
    /// a partial unique index rather than by Rust alone.
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Preparing | Self::Running | Self::Stopping)
    }
}

/// Per-game state inside a run.
///
/// The five terminal variants are the only ones that count as processed. Provider backoff, a
/// retryable transient error, a queued job, and an in-flight request are all explicitly *not*
/// terminal: reporting a game as processed while RetroFrontier is still waiting for the provider
/// would be a false progress claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetadataScrapeItemState {
    /// In the target set, not yet fed into the M5 queue.
    Pending,
    /// Fed into the M5 queue and waiting there.
    Queued,
    /// A provider request for this game is in flight.
    Running,
    Matched,
    NeedsReview,
    NoMatch,
    Unsupported,
    Failed,
}

impl MetadataScrapeItemState {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Matched => "matched",
            Self::NeedsReview => "needs_review",
            Self::NoMatch => "no_match",
            Self::Unsupported => "unsupported",
            Self::Failed => "failed",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => Self::Pending,
            "queued" => Self::Queued,
            "running" => Self::Running,
            "matched" => Self::Matched,
            "needs_review" => Self::NeedsReview,
            "no_match" => Self::NoMatch,
            "unsupported" => Self::Unsupported,
            "failed" => Self::Failed,
            _ => return None,
        })
    }

    /// True when this game has a final answer and must never be re-examined by this run.
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Matched | Self::NeedsReview | Self::NoMatch | Self::Unsupported | Self::Failed
        )
    }
}

/// Scheduling band for a metadata job.
///
/// Lower numbers run first, so a band is an offset added to the job kind's own ordering. Bulk work
/// must never delay something the user just asked for by hand, and expressing that as a band keeps
/// the relationship between the two explicit instead of scattering magic numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataJobBand {
    /// Explicit per-game user actions, and the automatic evidence-integrity sweep.
    Interactive,
    /// Whole-library scrape-run work.
    Bulk,
}

/// Distance between the two bands.
///
/// Larger than the span of every job-kind priority, so the lowest-priority interactive job still
/// outranks the highest-priority bulk job.
pub const METADATA_JOB_BAND_SPAN: i64 = 1_000;

impl MetadataJobBand {
    pub const fn offset(self) -> i64 {
        match self {
            Self::Interactive => 0,
            Self::Bulk => METADATA_JOB_BAND_SPAN,
        }
    }

    /// Priority for `kind` in this band.
    pub const fn priority(self, kind: MetadataJobKind) -> i64 {
        kind.default_priority() + self.offset()
    }
}

/// Authoritative M5 state for one game, as far as run progress is concerned.
///
/// Gathered by the repository in one bounded query. Keeping it a plain value is what makes the
/// classification rule below testable without a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataScrapeItemFacts {
    pub game_id: GameId,
    /// A metadata job for this game and provider is pending, deferred, or running.
    pub has_live_job: bool,
    /// A provider request for this game is claimed right now.
    pub has_running_job: bool,
    /// A metadata job for this game and provider is parked as failed.
    pub has_parked_job: bool,
    pub match_status: Option<ProviderMatchStatus>,
    /// The provider relationship records a reason why this content shape cannot be matched.
    pub unsupported: bool,
}

/// Maps authoritative M5 state onto one run-item state.
///
/// The rule is deliberately "no live job of any kind for this game" rather than "the job I fed has
/// finished". M5 legitimately turns one piece of work into another — a refresh whose stored
/// evidence no longer holds becomes a re-identification rather than silently re-trusting a stale
/// provider identity — and a run must keep following that game until it has a real answer instead
/// of declaring victory on the job it happened to enqueue.
pub fn classify_scrape_item(facts: &MetadataScrapeItemFacts) -> MetadataScrapeItemState {
    if facts.has_live_job {
        return if facts.has_running_job {
            MetadataScrapeItemState::Running
        } else {
            MetadataScrapeItemState::Queued
        };
    }

    // A parked job is work this run drove that will not be retried without a configuration or
    // content change. It outranks the match status because a game whose cover download was parked
    // has not been scraped cleanly even though its match is still accepted.
    if facts.has_parked_job {
        return MetadataScrapeItemState::Failed;
    }

    match facts.match_status {
        Some(ProviderMatchStatus::Matched) => MetadataScrapeItemState::Matched,
        Some(ProviderMatchStatus::Ambiguous) => MetadataScrapeItemState::NeedsReview,
        Some(ProviderMatchStatus::NoMatch) => MetadataScrapeItemState::NoMatch,
        Some(ProviderMatchStatus::Failed) => MetadataScrapeItemState::Failed,
        // M5 persists "this content shape has no documented provider representation" as a deferred
        // relationship carrying a reason. A deferral without one is an ordinary provider wait.
        _ if facts.unsupported => MetadataScrapeItemState::Unsupported,
        // No live job, nothing parked, and no answer: the work is over and produced nothing usable.
        // Reporting it as a failure keeps the run finishable and keeps the processed total honest,
        // rather than leaving a game suspended in progress forever.
        _ => MetadataScrapeItemState::Failed,
    }
}

/// Game-count progress for one run.
///
/// The user-facing unit is always games. Refresh needs more than one provider operation per game,
/// so raw job counts would neither add up nor mean anything to a person reading the screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataScrapeProgress {
    /// Fixed target size, decided when the run started.
    pub total_games: i64,
    pub matched: i64,
    pub needs_review: i64,
    pub no_match: i64,
    pub unsupported: i64,
    pub failed: i64,
    /// Provider requests in flight. Not processed.
    pub running: i64,
    /// Target games with no final answer yet, whether or not they have been queued. Not processed.
    pub waiting: i64,
}

impl MetadataScrapeProgress {
    /// Games with a final answer.
    ///
    /// Deliberately the sum of the terminal buckets rather than a separately maintained counter, so
    /// the invariant the UI relies on cannot drift.
    pub const fn processed(&self) -> i64 {
        self.matched + self.needs_review + self.no_match + self.unsupported + self.failed
    }
}

/// One persistent run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataScrapeRun {
    pub id: MetadataScrapeRunId,
    pub provider_id: MetadataProviderId,
    pub mode: MetadataScrapeMode,
    pub status: MetadataScrapeRunStatus,
    pub progress: MetadataScrapeProgress,
    pub created_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
}

/// What the Settings scraper surface needs in one bounded read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataScrapeStatus {
    pub provider_id: MetadataProviderId,
    /// The active run, or the most recent finished one when nothing is active.
    pub run: Option<MetadataScrapeRun>,
    /// True while `run` still owns the provider for bulk work.
    pub active: bool,
}

/// How many games a mode would target if it started now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataScrapePreview {
    pub mode: MetadataScrapeMode,
    pub eligible_games: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> MetadataScrapeItemFacts {
        MetadataScrapeItemFacts {
            game_id: GameId(1),
            has_live_job: false,
            has_running_job: false,
            has_parked_job: false,
            match_status: None,
            unsupported: false,
        }
    }

    #[test]
    fn database_encodings_round_trip() {
        for mode in [
            MetadataScrapeMode::MissingMetadata,
            MetadataScrapeMode::RefreshMatched,
        ] {
            assert_eq!(MetadataScrapeMode::from_db(mode.as_db()), Some(mode));
        }
        for status in [
            MetadataScrapeRunStatus::Preparing,
            MetadataScrapeRunStatus::Running,
            MetadataScrapeRunStatus::Stopping,
            MetadataScrapeRunStatus::Completed,
            MetadataScrapeRunStatus::Stopped,
        ] {
            assert_eq!(
                MetadataScrapeRunStatus::from_db(status.as_db()),
                Some(status)
            );
        }
        for state in [
            MetadataScrapeItemState::Pending,
            MetadataScrapeItemState::Queued,
            MetadataScrapeItemState::Running,
            MetadataScrapeItemState::Matched,
            MetadataScrapeItemState::NeedsReview,
            MetadataScrapeItemState::NoMatch,
            MetadataScrapeItemState::Unsupported,
            MetadataScrapeItemState::Failed,
        ] {
            assert_eq!(MetadataScrapeItemState::from_db(state.as_db()), Some(state));
        }
        assert_eq!(MetadataScrapeMode::from_db("everything"), None);
        assert_eq!(MetadataScrapeRunStatus::from_db("paused"), None);
        assert_eq!(MetadataScrapeItemState::from_db("reviewed"), None);
    }

    #[test]
    fn exactly_the_five_result_states_are_terminal() {
        let terminal = [
            MetadataScrapeItemState::Matched,
            MetadataScrapeItemState::NeedsReview,
            MetadataScrapeItemState::NoMatch,
            MetadataScrapeItemState::Unsupported,
            MetadataScrapeItemState::Failed,
        ];
        for state in terminal {
            assert!(state.is_terminal(), "{state:?} should be terminal");
        }
        for state in [
            MetadataScrapeItemState::Pending,
            MetadataScrapeItemState::Queued,
            MetadataScrapeItemState::Running,
        ] {
            assert!(!state.is_terminal(), "{state:?} must not be terminal");
        }
    }

    #[test]
    fn only_preparing_running_and_stopping_hold_the_provider() {
        assert!(MetadataScrapeRunStatus::Preparing.is_active());
        assert!(MetadataScrapeRunStatus::Running.is_active());
        assert!(MetadataScrapeRunStatus::Stopping.is_active());
        assert!(!MetadataScrapeRunStatus::Completed.is_active());
        assert!(!MetadataScrapeRunStatus::Stopped.is_active());
    }

    #[test]
    fn refresh_requires_both_halves_of_an_m5_refresh() {
        assert_eq!(
            MetadataScrapeMode::MissingMetadata.required_job_kinds(),
            &[MetadataJobKind::Identify]
        );
        assert_eq!(
            MetadataScrapeMode::RefreshMatched.required_job_kinds(),
            &[
                MetadataJobKind::RefreshMetadata,
                MetadataJobKind::RefreshCover
            ]
        );
    }

    #[test]
    fn every_interactive_job_outranks_every_bulk_job() {
        let kinds = [
            MetadataJobKind::Identify,
            MetadataJobKind::RefreshMetadata,
            MetadataJobKind::RefreshCover,
        ];
        let slowest_interactive = kinds
            .iter()
            .map(|kind| MetadataJobBand::Interactive.priority(*kind))
            .max()
            .expect("at least one job kind");
        let fastest_bulk = kinds
            .iter()
            .map(|kind| MetadataJobBand::Bulk.priority(*kind))
            .min()
            .expect("at least one job kind");

        assert!(
            slowest_interactive < fastest_bulk,
            "interactive {slowest_interactive} must run before bulk {fastest_bulk}"
        );
    }

    #[test]
    fn the_interactive_band_keeps_the_existing_default_priorities() {
        for kind in [
            MetadataJobKind::Identify,
            MetadataJobKind::RefreshMetadata,
            MetadataJobKind::RefreshCover,
        ] {
            assert_eq!(
                MetadataJobBand::Interactive.priority(kind),
                kind.default_priority()
            );
        }
    }

    #[test]
    fn a_live_job_is_never_a_result() {
        let queued = MetadataScrapeItemFacts {
            has_live_job: true,
            ..facts()
        };
        assert_eq!(
            classify_scrape_item(&queued),
            MetadataScrapeItemState::Queued
        );

        let running = MetadataScrapeItemFacts {
            has_live_job: true,
            has_running_job: true,
            ..facts()
        };
        assert_eq!(
            classify_scrape_item(&running),
            MetadataScrapeItemState::Running
        );
    }

    #[test]
    fn a_provider_deferral_is_not_a_result() {
        // A deferred job is still live, and a deferred relationship without a recorded reason is a
        // provider wait rather than an answer.
        let deferred = MetadataScrapeItemFacts {
            has_live_job: true,
            match_status: Some(ProviderMatchStatus::Deferred),
            ..facts()
        };
        assert_eq!(
            classify_scrape_item(&deferred),
            MetadataScrapeItemState::Queued
        );
        assert!(!classify_scrape_item(&deferred).is_terminal());
    }

    #[test]
    fn a_still_live_second_refresh_job_keeps_the_game_unfinished() {
        // The metadata half completed and left an accepted match behind; the cover half is still
        // queued. The game is not processed yet.
        let half_done = MetadataScrapeItemFacts {
            has_live_job: true,
            match_status: Some(ProviderMatchStatus::Matched),
            ..facts()
        };
        assert_eq!(
            classify_scrape_item(&half_done),
            MetadataScrapeItemState::Queued
        );
    }

    #[test]
    fn provider_answers_map_onto_result_states() {
        for (status, expected) in [
            (
                ProviderMatchStatus::Matched,
                MetadataScrapeItemState::Matched,
            ),
            (
                ProviderMatchStatus::Ambiguous,
                MetadataScrapeItemState::NeedsReview,
            ),
            (
                ProviderMatchStatus::NoMatch,
                MetadataScrapeItemState::NoMatch,
            ),
            (ProviderMatchStatus::Failed, MetadataScrapeItemState::Failed),
        ] {
            let resolved = MetadataScrapeItemFacts {
                match_status: Some(status),
                ..facts()
            };
            assert_eq!(classify_scrape_item(&resolved), expected);
        }
    }

    #[test]
    fn an_unsupported_content_shape_is_its_own_result() {
        let unsupported = MetadataScrapeItemFacts {
            match_status: Some(ProviderMatchStatus::Deferred),
            unsupported: true,
            ..facts()
        };
        assert_eq!(
            classify_scrape_item(&unsupported),
            MetadataScrapeItemState::Unsupported
        );
    }

    #[test]
    fn a_parked_job_fails_the_game_even_when_its_match_still_stands() {
        // A permanently failed cover download does not make the accepted match untrue, but it does
        // mean this run did not scrape the game cleanly.
        let parked = MetadataScrapeItemFacts {
            has_parked_job: true,
            match_status: Some(ProviderMatchStatus::Matched),
            ..facts()
        };
        assert_eq!(
            classify_scrape_item(&parked),
            MetadataScrapeItemState::Failed
        );
    }

    #[test]
    fn finished_work_that_produced_no_answer_still_terminates() {
        // Nothing is live, nothing is parked, and M5 recorded no relationship — for instance
        // because the local game disappeared underneath the request. The run must not hang.
        assert_eq!(
            classify_scrape_item(&facts()),
            MetadataScrapeItemState::Failed
        );
        assert!(classify_scrape_item(&facts()).is_terminal());
    }

    #[test]
    fn processed_is_the_sum_of_the_result_buckets() {
        let progress = MetadataScrapeProgress {
            total_games: 148,
            matched: 31,
            needs_review: 6,
            no_match: 5,
            unsupported: 2,
            failed: 3,
            running: 2,
            waiting: 99,
        };

        assert_eq!(progress.processed(), 47);
        assert_eq!(
            progress.processed() + progress.running + progress.waiting,
            progress.total_games
        );
    }
}
