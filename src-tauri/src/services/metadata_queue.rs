//! Provider-aware scheduling policy.
//!
//! Pure decision logic plus the injectable clock and jitter source that make it testable. Nothing
//! here performs I/O, so retry and deferral behaviour can be asserted without sleeping and without
//! a wall-clock race.
//!
//! Two budget kinds are kept apart on purpose. A *retry* is a bounded response to a transient
//! fault and counts against the job's attempt budget. A *deferral* is the provider telling us to
//! come back later; it must not consume retries, and it must not be probed in a tight loop.

use crate::domain::library::UnixTimestamp;
use crate::domain::metadata::{FailureDisposition, ProviderFailureClass, ProviderSchedulerState};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock source. Injectable so tests are deterministic.
pub trait Clock: Send + Sync {
    /// Milliseconds since the Unix epoch, matching the timestamps used across the schema.
    fn now_ms(&self) -> UnixTimestamp;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> UnixTimestamp {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
            .unwrap_or_default()
    }
}

/// Randomness used to spread retries. Injectable so backoff is exactly reproducible in tests.
pub trait JitterSource: Send + Sync {
    /// A value in `0..=maximum_ms`.
    fn jitter_ms(&self, maximum_ms: i64) -> i64;
}

pub struct RandomJitter;

impl JitterSource for RandomJitter {
    fn jitter_ms(&self, maximum_ms: i64) -> i64 {
        if maximum_ms <= 0 {
            return 0;
        }
        rand::random_range(0..=maximum_ms)
    }
}

/// Deterministic jitter for tests.
#[cfg(test)]
pub struct NoJitter;

#[cfg(test)]
impl JitterSource for NoJitter {
    fn jitter_ms(&self, _maximum_ms: i64) -> i64 {
        0
    }
}

/// Attempts allowed for a genuinely transient failure before a job is parked as failed.
pub const MAX_TRANSIENT_ATTEMPTS: i64 = 5;

const SECOND_MS: i64 = 1_000;
const MINUTE_MS: i64 = 60 * SECOND_MS;
const HOUR_MS: i64 = 60 * MINUTE_MS;

/// Base and cap for each deferral family.
///
/// The provider returns no `Retry-After`, reset timestamp, or next-allowed timestamp, so these are
/// conservative local probes rather than an invented authoritative reset instant.
const TRANSIENT_BASE_MS: i64 = 5 * SECOND_MS;
const TRANSIENT_CAP_MS: i64 = 30 * MINUTE_MS;
const CAPACITY_BASE_MS: i64 = MINUTE_MS;
const CAPACITY_CAP_MS: i64 = 15 * MINUTE_MS;
const UNAVAILABLE_BASE_MS: i64 = 5 * MINUTE_MS;
const UNAVAILABLE_CAP_MS: i64 = HOUR_MS;
const QUOTA_BASE_MS: i64 = 30 * MINUTE_MS;
const QUOTA_CAP_MS: i64 = 6 * HOUR_MS;

/// Fraction of the computed delay that jitter may add, as a divisor.
const JITTER_DIVISOR: i64 = 4;

/// What to do with a job after a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureAction {
    /// Retry after `delay_ms`; the attempt counts against the retry budget.
    Retry { delay_ms: i64 },
    /// Wait for the provider; the attempt does not count against the retry budget.
    Defer { delay_ms: i64 },
    /// Park the job. Only an explicit user request or a configuration change revives it.
    Park,
    /// A definitive negative answer bound to the submitted evidence.
    Negative,
}

/// Decides what happens to a job after a failure.
///
/// `attempts` is the count *before* this failure.
pub fn failure_action(
    failure: ProviderFailureClass,
    attempts: i64,
    jitter: &dyn JitterSource,
) -> FailureAction {
    match failure.disposition() {
        FailureDisposition::Permanent => FailureAction::Park,
        FailureDisposition::NegativeResult => FailureAction::Negative,
        FailureDisposition::RetryWithBackoff => {
            if attempts + 1 >= MAX_TRANSIENT_ATTEMPTS {
                FailureAction::Park
            } else {
                FailureAction::Retry {
                    delay_ms: backoff_delay_ms(
                        TRANSIENT_BASE_MS,
                        TRANSIENT_CAP_MS,
                        attempts,
                        jitter,
                    ),
                }
            }
        }
        FailureDisposition::DeferForProvider => {
            let (base, cap) = match failure {
                ProviderFailureClass::CapacityDeferred => (CAPACITY_BASE_MS, CAPACITY_CAP_MS),
                ProviderFailureClass::DailyQuotaExceeded
                | ProviderFailureClass::NegativeQuotaExceeded => (QUOTA_BASE_MS, QUOTA_CAP_MS),
                _ => (UNAVAILABLE_BASE_MS, UNAVAILABLE_CAP_MS),
            };
            FailureAction::Defer {
                delay_ms: backoff_delay_ms(base, cap, attempts, jitter),
            }
        }
    }
}

/// Bounded exponential backoff with additive jitter.
///
/// The exponent is clamped so a long-lived deferral cannot overflow or produce an absurd delay.
pub fn backoff_delay_ms(
    base_ms: i64,
    cap_ms: i64,
    attempts: i64,
    jitter: &dyn JitterSource,
) -> i64 {
    let exponent = attempts.clamp(0, 16) as u32;
    let delay = base_ms.saturating_mul(1_i64 << exponent).min(cap_ms);
    delay.saturating_add(jitter.jitter_ms(delay / JITTER_DIVISOR))
}

/// Provider-level deferral length for a failure that describes the provider rather than the job.
pub fn provider_deferral_ms(
    failure: ProviderFailureClass,
    consecutive_failures: i64,
    jitter: &dyn JitterSource,
) -> i64 {
    match failure_action(failure, consecutive_failures, jitter) {
        FailureAction::Defer { delay_ms } | FailureAction::Retry { delay_ms } => delay_ms,
        FailureAction::Park | FailureAction::Negative => 0,
    }
}

/// Rolling one-minute request budget.
///
/// The provider reports a per-minute maximum but no reset instant, so the window is tracked locally
/// and the limit is re-read from the provider on every response.
#[derive(Default)]
pub struct MinuteBudget {
    issued: Mutex<VecDeque<UnixTimestamp>>,
}

impl MinuteBudget {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserves one request slot, or reports when the next slot frees up.
    ///
    /// Called immediately before each provider request so the window reflects requests actually
    /// issued rather than scheduling rounds.
    pub fn reserve(&self, now: UnixTimestamp, maximum: Option<i64>) -> Result<(), UnixTimestamp> {
        let mut issued = self
            .issued
            .lock()
            .expect("minute budget mutex is not poisoned");
        Self::evict_expired(&mut issued, now);

        let Some(maximum) = maximum.filter(|maximum| *maximum > 0) else {
            // No advertised limit: record the request for later accounting but do not block.
            issued.push_back(now);
            return Ok(());
        };

        if (issued.len() as i64) < maximum {
            issued.push_back(now);
            return Ok(());
        }

        Err(Self::next_slot(&issued, now))
    }

    /// Reports whether a slot is currently available, without consuming one.
    ///
    /// Used by scheduling decisions so merely deciding to look for work never spends budget.
    pub fn availability(
        &self,
        now: UnixTimestamp,
        maximum: Option<i64>,
    ) -> Result<(), UnixTimestamp> {
        let mut issued = self
            .issued
            .lock()
            .expect("minute budget mutex is not poisoned");
        Self::evict_expired(&mut issued, now);

        match maximum.filter(|maximum| *maximum > 0) {
            Some(maximum) if (issued.len() as i64) >= maximum => Err(Self::next_slot(&issued, now)),
            _ => Ok(()),
        }
    }

    fn evict_expired(issued: &mut VecDeque<UnixTimestamp>, now: UnixTimestamp) {
        while issued
            .front()
            .is_some_and(|issued_at| now.saturating_sub(*issued_at) >= MINUTE_MS)
        {
            issued.pop_front();
        }
    }

    fn next_slot(issued: &VecDeque<UnixTimestamp>, now: UnixTimestamp) -> UnixTimestamp {
        issued
            .front()
            .copied()
            .unwrap_or(now)
            .saturating_add(MINUTE_MS)
    }

    #[cfg(test)]
    pub fn used(&self, now: UnixTimestamp) -> usize {
        let issued = self
            .issued
            .lock()
            .expect("minute budget mutex is not poisoned");
        issued
            .iter()
            .filter(|issued_at| now.saturating_sub(**issued_at) < MINUTE_MS)
            .count()
    }
}

/// The scheduler's answer for one polling round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulingDecision {
    /// Work may proceed with at most this many concurrent provider requests.
    Run { concurrency: usize },
    /// No work may be issued before this timestamp.
    WaitUntil(UnixTimestamp),
}

/// How long to wait before re-probing an exhausted daily budget.
pub const DAILY_BUDGET_PROBE_MS: i64 = QUOTA_BASE_MS;

/// Decides whether provider work may be issued right now.
///
/// The order of checks matters: an explicit provider deferral outranks local budget arithmetic, and
/// a locally observed exhausted daily budget stops work even when the provider has not answered
/// with a quota status yet.
pub fn plan(
    state: &ProviderSchedulerState,
    now: UnixTimestamp,
    configured_concurrency: usize,
    minute_budget: &MinuteBudget,
) -> SchedulingDecision {
    if let Some(deferred_until) = state.deferred_until {
        if deferred_until > now {
            return SchedulingDecision::WaitUntil(deferred_until);
        }
    }
    if state.daily_budget_exhausted() {
        return SchedulingDecision::WaitUntil(now.saturating_add(DAILY_BUDGET_PROBE_MS));
    }
    if let Err(next_slot) = minute_budget.availability(now, state.quota.max_requests_per_minute) {
        return SchedulingDecision::WaitUntil(next_slot);
    }

    SchedulingDecision::Run {
        concurrency: state.permitted_concurrency(configured_concurrency),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::{Clock, JitterSource};
    use crate::domain::library::UnixTimestamp;
    use std::sync::atomic::{AtomicI64, Ordering};

    /// Clock whose value only changes when a test advances it.
    pub struct ManualClock {
        now_ms: AtomicI64,
    }

    impl ManualClock {
        pub fn new(now_ms: UnixTimestamp) -> Self {
            Self {
                now_ms: AtomicI64::new(now_ms),
            }
        }

        pub fn advance(&self, delta_ms: i64) {
            self.now_ms.fetch_add(delta_ms, Ordering::SeqCst);
        }

        pub fn set(&self, now_ms: UnixTimestamp) {
            self.now_ms.store(now_ms, Ordering::SeqCst);
        }
    }

    impl Clock for ManualClock {
        fn now_ms(&self) -> UnixTimestamp {
            self.now_ms.load(Ordering::SeqCst)
        }
    }

    /// Jitter that always returns its configured maximum, so the widest delay is asserted exactly.
    pub struct MaximumJitter;

    impl JitterSource for MaximumJitter {
        fn jitter_ms(&self, maximum_ms: i64) -> i64 {
            maximum_ms
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{ManualClock, MaximumJitter};
    use super::*;
    use crate::domain::metadata::MetadataProviderId;

    fn state() -> ProviderSchedulerState {
        ProviderSchedulerState::empty(MetadataProviderId::ScreenScraper)
    }

    #[test]
    fn transient_failures_use_bounded_exponential_backoff() {
        let delays: Vec<i64> = (0..4)
            .map(|attempts| {
                match failure_action(ProviderFailureClass::Transport, attempts, &NoJitter) {
                    FailureAction::Retry { delay_ms } => delay_ms,
                    other => panic!("expected a retry, got {other:?}"),
                }
            })
            .collect();

        assert_eq!(delays, vec![5_000, 10_000, 20_000, 40_000]);
        assert!(delays.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn the_retry_budget_is_bounded_and_then_the_job_is_parked() {
        assert_eq!(
            failure_action(
                ProviderFailureClass::Transport,
                MAX_TRANSIENT_ATTEMPTS - 1,
                &NoJitter
            ),
            FailureAction::Park
        );
        assert_eq!(
            failure_action(
                ProviderFailureClass::TransientServer,
                MAX_TRANSIENT_ATTEMPTS + 10,
                &NoJitter
            ),
            FailureAction::Park
        );
    }

    #[test]
    fn backoff_is_capped_and_jitter_is_additive_and_bounded() {
        let capped = backoff_delay_ms(TRANSIENT_BASE_MS, TRANSIENT_CAP_MS, 60, &NoJitter);
        assert_eq!(capped, TRANSIENT_CAP_MS);

        let jittered = backoff_delay_ms(TRANSIENT_BASE_MS, TRANSIENT_CAP_MS, 0, &MaximumJitter);
        assert_eq!(
            jittered,
            TRANSIENT_BASE_MS + TRANSIENT_BASE_MS / JITTER_DIVISOR
        );
        assert!(jittered > TRANSIENT_BASE_MS);
    }

    #[test]
    fn each_quota_class_defers_with_its_own_delay_without_spending_retries() {
        let capacity = failure_action(ProviderFailureClass::CapacityDeferred, 0, &NoJitter);
        let daily = failure_action(ProviderFailureClass::DailyQuotaExceeded, 0, &NoJitter);
        let negative = failure_action(ProviderFailureClass::NegativeQuotaExceeded, 0, &NoJitter);
        let unavailable = failure_action(ProviderFailureClass::ProviderUnavailable, 0, &NoJitter);
        let restricted = failure_action(ProviderFailureClass::ProviderRestricted, 0, &NoJitter);

        assert_eq!(
            capacity,
            FailureAction::Defer {
                delay_ms: MINUTE_MS
            }
        );
        assert_eq!(
            daily,
            FailureAction::Defer {
                delay_ms: QUOTA_BASE_MS
            }
        );
        assert_eq!(
            negative, daily,
            "negative quota is deferred like daily quota"
        );
        assert_eq!(
            unavailable,
            FailureAction::Defer {
                delay_ms: UNAVAILABLE_BASE_MS
            }
        );
        assert_eq!(restricted, unavailable);

        // A long-running deferral never becomes a park, so work resumes when the provider allows it.
        assert!(matches!(
            failure_action(ProviderFailureClass::DailyQuotaExceeded, 50, &NoJitter),
            FailureAction::Defer { .. }
        ));
    }

    #[test]
    fn permanent_failures_are_parked_and_no_match_is_a_negative_result() {
        for failure in [
            ProviderFailureClass::InvalidRequest,
            ProviderFailureClass::DeveloperAuthenticationFailed,
            ProviderFailureClass::UserAuthenticationFailed,
            ProviderFailureClass::ClientRejected,
            ProviderFailureClass::CredentialsUnavailable,
        ] {
            assert_eq!(
                failure_action(failure, 0, &NoJitter),
                FailureAction::Park,
                "{failure:?} must not be retried automatically"
            );
        }
        assert_eq!(
            failure_action(ProviderFailureClass::NoMatch, 0, &NoJitter),
            FailureAction::Negative
        );
    }

    #[test]
    fn a_persisted_provider_deferral_blocks_all_work_until_it_expires() {
        let mut state = state();
        state.deferred_until = Some(10_000);
        state.defer_reason = Some(ProviderFailureClass::DailyQuotaExceeded);
        let budget = MinuteBudget::new();

        assert_eq!(
            plan(&state, 5_000, 4, &budget),
            SchedulingDecision::WaitUntil(10_000)
        );
        assert_eq!(
            budget.used(5_000),
            0,
            "a deferred provider must not consume a request slot"
        );
        assert_eq!(
            plan(&state, 10_001, 4, &budget),
            SchedulingDecision::Run { concurrency: 1 }
        );
    }

    #[test]
    fn concurrency_never_exceeds_the_advertised_thread_count() {
        let mut state = state();
        state.quota.max_threads = Some(3);

        assert_eq!(
            plan(&state, 0, 8, &MinuteBudget::new()),
            SchedulingDecision::Run { concurrency: 3 }
        );
        assert_eq!(
            plan(&state, 0, 2, &MinuteBudget::new()),
            SchedulingDecision::Run { concurrency: 2 }
        );

        let unknown = ProviderSchedulerState::empty(MetadataProviderId::ScreenScraper);
        assert_eq!(
            plan(&unknown, 0, 8, &MinuteBudget::new()),
            SchedulingDecision::Run { concurrency: 1 },
            "an unknown quota must stay conservative"
        );
    }

    #[test]
    fn the_rolling_minute_budget_defers_instead_of_spinning() {
        let mut state = state();
        state.quota.max_requests_per_minute = Some(2);
        let budget = MinuteBudget::new();

        // Planning peeks, so repeated scheduling rounds never spend the budget themselves.
        assert!(matches!(
            plan(&state, 1_000, 4, &budget),
            SchedulingDecision::Run { .. }
        ));
        assert!(matches!(
            plan(&state, 1_000, 4, &budget),
            SchedulingDecision::Run { .. }
        ));
        assert_eq!(budget.used(1_000), 0);

        // Issuing requests does.
        assert!(budget.reserve(1_000, Some(2)).is_ok());
        assert!(budget.reserve(1_500, Some(2)).is_ok());
        assert_eq!(
            plan(&state, 2_000, 4, &budget),
            SchedulingDecision::WaitUntil(61_000),
            "the third request in one minute waits for the oldest slot to expire"
        );
        assert_eq!(budget.reserve(2_000, Some(2)), Err(61_000));

        // Once the window rolls forward, work resumes.
        assert!(matches!(
            plan(&state, 61_001, 4, &budget),
            SchedulingDecision::Run { .. }
        ));
    }

    #[test]
    fn an_exhausted_daily_budget_defers_without_a_tight_loop() {
        let mut state = state();
        state.quota.requests_today = Some(10_000);
        state.quota.max_requests_per_day = Some(10_000);

        let decision = plan(&state, 1_000, 4, &MinuteBudget::new());

        assert_eq!(
            decision,
            SchedulingDecision::WaitUntil(1_000 + DAILY_BUDGET_PROBE_MS)
        );
    }

    #[test]
    fn an_unlimited_minute_budget_does_not_block() {
        let budget = MinuteBudget::new();
        for offset in 0..100 {
            assert!(budget.reserve(offset, None).is_ok());
        }
        assert_eq!(budget.used(0), 100);
    }

    #[test]
    fn a_reduced_provider_limit_is_honoured_immediately() {
        let budget = MinuteBudget::new();
        assert!(budget.reserve(0, Some(5)).is_ok());
        assert!(budget.reserve(1, Some(5)).is_ok());

        // The provider now advertises a lower ceiling than we have already used.
        assert_eq!(budget.reserve(2, Some(1)), Err(MINUTE_MS));
    }

    #[test]
    fn the_manual_clock_only_moves_when_a_test_moves_it() {
        let clock = ManualClock::new(1_000);
        assert_eq!(clock.now_ms(), 1_000);
        clock.advance(500);
        assert_eq!(clock.now_ms(), 1_500);
        clock.set(42);
        assert_eq!(clock.now_ms(), 42);
    }

    #[test]
    fn random_jitter_stays_within_its_bound() {
        for maximum in [0, 1, 1_000] {
            for _ in 0..32 {
                let jitter = RandomJitter.jitter_ms(maximum);
                assert!((0..=maximum).contains(&jitter));
            }
        }
        assert_eq!(RandomJitter.jitter_ms(-5), 0);
    }
}

#[cfg(test)]
mod concurrency_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Two schedulers racing for the final slot: exactly one may consume it.
    ///
    /// `MinuteBudget::reserve` takes the lock, evicts, checks, and pushes as one critical section,
    /// so a peek/consume interleaving cannot let both callers observe the same last slot.
    #[test]
    fn only_one_of_two_concurrent_operations_can_take_the_last_minute_slot() {
        for _ in 0..200 {
            let budget = Arc::new(MinuteBudget::new());
            let now: UnixTimestamp = 1_700_000_000_000;
            // Consume everything but one slot of a three-request minute.
            budget.reserve(now, Some(3)).expect("first slot");
            budget.reserve(now, Some(3)).expect("second slot");

            // Both threads see a slot available if they only peek...
            assert!(budget.availability(now, Some(3)).is_ok());

            let granted = Arc::new(AtomicUsize::new(0));
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let handles: Vec<_> = (0..2)
                .map(|_| {
                    let budget = budget.clone();
                    let granted = granted.clone();
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        barrier.wait();
                        if budget.reserve(now, Some(3)).is_ok() {
                            granted.fetch_add(1, Ordering::SeqCst);
                        }
                    })
                })
                .collect();
            for handle in handles {
                handle.join().expect("worker thread should not panic");
            }

            assert_eq!(
                granted.load(Ordering::SeqCst),
                1,
                "two concurrent operations must not both consume the final minute slot"
            );
            assert_eq!(
                budget.used(now),
                3,
                "the window must never exceed the maximum"
            );
        }
    }

    /// A provider-wide deferral appearing mid-round wins over local budget arithmetic.
    #[test]
    fn a_provider_deferral_outranks_an_available_minute_slot() {
        let budget = MinuteBudget::new();
        let now: UnixTimestamp = 1_700_000_000_000;
        let mut state = ProviderSchedulerState::empty(
            crate::domain::metadata::MetadataProviderId::ScreenScraper,
        );
        state.quota.max_requests_per_minute = Some(10);
        assert!(matches!(
            plan(&state, now, 4, &budget),
            SchedulingDecision::Run { .. }
        ));

        state.deferred_until = Some(now + 60_000);
        assert_eq!(
            plan(&state, now, 4, &budget),
            SchedulingDecision::WaitUntil(now + 60_000),
            "a deferral that appears while scheduling must stop the round"
        );
    }

    /// A shrinking advertised maximum takes effect against slots already spent in the window.
    #[test]
    fn a_lowered_maximum_applies_to_slots_already_consumed() {
        let budget = MinuteBudget::new();
        let now: UnixTimestamp = 1_700_000_000_000;
        for _ in 0..5 {
            budget
                .reserve(now, Some(5))
                .expect("slot within the old maximum");
        }
        assert!(
            budget.reserve(now, Some(2)).is_err(),
            "a reduced maximum must be honoured immediately, not from the next window"
        );
    }
}
