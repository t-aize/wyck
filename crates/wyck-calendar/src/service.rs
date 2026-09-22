//! The self-refreshing cache: a background task that owns the fetch schedule and publishes
//! the latest good calendar through a [`tokio::sync::watch`] channel.
//!
//! Behavior a UI can rely on:
//!
//! - **Never per-frame.** Reading state is a channel borrow; the network is touched at
//!   most once per [`ServiceConfig::refresh_interval`] (plus manual refreshes, which are
//!   throttled).
//! - **Stale beats empty.** A failed refresh is logged and recorded in
//!   [`CalendarState::last_error`]; the previous events stay in place. The
//!   engine's `RefreshFailed` philosophy, applied to the feed.
//! - **Polite backoff.** Failures retry with exponential backoff, and a `429` honors the
//!   server's `Retry-After`. Manual refreshes cannot be used to bypass either.
//! - **Conditional GETs.** `ETag` / `Last-Modified` are replayed so an unchanged feed
//!   costs a `304`.
//! - **Clean shutdown.** The task ends when every [`CalendarHandle`] is dropped.

use std::sync::Arc;
use std::time::Duration;

use time::OffsetDateTime;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use crate::client::{CalendarClient, Fetch, FetchOutcome, Validators};
use crate::error::{CalendarError, Result};
use crate::event::CalendarEvent;

/// Scheduling knobs for [`CalendarService`].
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// Time between successful refreshes. Default 30 minutes: the feed changes at most a
    /// few times a day, and polling it harder invites `429`s. Raised to
    /// `min_refresh_interval` if set below it.
    pub refresh_interval: Duration,
    /// The shortest gap allowed between two fetch attempts of any kind: scheduled,
    /// retry, or a manual [`CalendarHandle::refresh`]. Default 5 minutes. The public feed
    /// allows roughly 2 requests per 5 minutes per IP (see the crate docs), so this keeps
    /// one instance at half that budget and leaves room for a restart or a second
    /// process on the same address.
    pub min_refresh_interval: Duration,
    /// Delay before the first retry after a failure; doubles per consecutive failure and
    /// is never below `min_refresh_interval`. Default 5 minutes.
    pub retry_initial: Duration,
    /// Upper bound for the doubled retry delay (a server `Retry-After` may still exceed
    /// it). Default 30 minutes.
    pub retry_max: Duration,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            refresh_interval: Duration::from_secs(30 * 60),
            min_refresh_interval: Duration::from_secs(5 * 60),
            retry_initial: Duration::from_secs(5 * 60),
            retry_max: Duration::from_secs(30 * 60),
        }
    }
}

/// How long to back off after a `429` that carried no `Retry-After`, or after an HTML
/// block page. Matches the `Retry-After: 300` the feed itself sends.
pub const DEFAULT_RATE_LIMIT_HOLD: Duration = Duration::from_secs(300);

/// How trustworthy [`CalendarState::events`] currently is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// The first fetch has not finished yet; there are no events.
    Loading,
    /// No fetch has ever succeeded; there are no events (see `last_error`).
    Unavailable,
    /// The latest fetch succeeded.
    Fresh,
    /// The latest fetch failed; the events are from an earlier success (see
    /// `fetched_at` for their age and `last_error` for why).
    Stale,
}

/// A snapshot of the calendar and how it got there. Cheap to clone (the events are
/// behind an [`Arc`]).
#[derive(Debug, Clone, Default)]
pub struct CalendarState {
    /// The last good calendar, sorted by time. Empty until the first success.
    pub events: Arc<[CalendarEvent]>,
    /// When the events were last confirmed current (a fresh download or a `304`).
    pub fetched_at: Option<OffsetDateTime>,
    /// The most recent failure, cleared by the next success.
    pub last_error: Option<String>,
    /// Failures since the last success.
    pub consecutive_failures: u32,
    /// Records the latest successful download rejected as malformed (schema-drift
    /// canary; `0` is healthy).
    pub skipped_records: usize,
}

impl CalendarState {
    /// Summarizes the state; see [`Freshness`].
    #[must_use]
    pub fn freshness(&self) -> Freshness {
        match (self.fetched_at, self.consecutive_failures) {
            (None, 0) => Freshness::Loading,
            (None, _) => Freshness::Unavailable,
            (Some(_), 0) => Freshness::Fresh,
            (Some(_), _) => Freshness::Stale,
        }
    }
}

/// A cloneable handle to a running service. The task shuts down when the last handle is
/// dropped.
#[derive(Clone)]
pub struct CalendarHandle {
    state: watch::Receiver<CalendarState>,
    refresh: mpsc::Sender<()>,
}

impl CalendarHandle {
    /// The current state.
    #[must_use]
    pub fn state(&self) -> CalendarState {
        self.state.borrow().clone()
    }

    /// A receiver that wakes whenever the state changes: `await` its `changed()` in a
    /// `select!` alongside terminal input.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<CalendarState> {
        self.state.clone()
    }

    /// Asks for a refresh soon (subject to [`ServiceConfig::min_refresh_interval`] and any
    /// active rate-limit backoff). Never blocks; returns `false` when the request was
    /// dropped because one is already pending or the service has stopped.
    pub fn refresh(&self) -> bool {
        self.refresh.try_send(()).is_ok()
    }
}

/// Entry points for starting the background service.
pub struct CalendarService;

impl CalendarService {
    /// Starts the service against the real feed with default settings.
    ///
    /// Must be called from within a Tokio runtime.
    ///
    /// # Errors
    ///
    /// [`CalendarError::Config`] if the HTTP client cannot be built.
    pub fn spawn_default() -> Result<CalendarHandle> {
        Ok(Self::spawn(
            CalendarClient::with_defaults()?,
            ServiceConfig::default(),
        ))
    }

    /// Starts the service with a chosen [`Fetch`] source and schedule. The first fetch
    /// begins immediately.
    ///
    /// Must be called from within a Tokio runtime.
    pub fn spawn<F: Fetch>(fetcher: F, config: ServiceConfig) -> CalendarHandle {
        let (state_tx, state_rx) = watch::channel(CalendarState::default());
        let (refresh_tx, refresh_rx) = mpsc::channel(1);
        tokio::spawn(run(fetcher, config, state_tx, refresh_rx));
        CalendarHandle {
            state: state_rx,
            refresh: refresh_tx,
        }
    }
}

async fn run<F: Fetch>(
    fetcher: F,
    mut config: ServiceConfig,
    state: watch::Sender<CalendarState>,
    mut refresh_requests: mpsc::Receiver<()>,
) {
    config.refresh_interval = config.refresh_interval.max(config.min_refresh_interval);
    let mut validators: Option<Validators> = None;
    let mut failures: u32 = 0;

    loop {
        let attempted_at = Instant::now();
        let mut min_gap = config.min_refresh_interval;

        let next_delay = match fetcher.fetch(validators.as_ref()).await {
            Ok(FetchOutcome::Modified {
                feed,
                validators: new_validators,
            }) => {
                failures = 0;
                validators = new_validators;
                tracing::debug!(
                    events = feed.events.len(),
                    skipped = feed.skipped,
                    "calendar refreshed"
                );
                state.send_modify(|s| {
                    s.events = Arc::from(feed.events);
                    s.fetched_at = Some(OffsetDateTime::now_utc());
                    s.last_error = None;
                    s.consecutive_failures = 0;
                    s.skipped_records = feed.skipped;
                });
                config.refresh_interval
            }
            Ok(FetchOutcome::NotModified) => {
                failures = 0;
                tracing::debug!("calendar unchanged (304)");
                state.send_modify(|s| {
                    s.fetched_at = Some(OffsetDateTime::now_utc());
                    s.last_error = None;
                    s.consecutive_failures = 0;
                });
                config.refresh_interval
            }
            Err(error) => {
                failures = failures.saturating_add(1);
                tracing::warn!(%error, failures, "calendar refresh failed; keeping last good data");
                let delay = backoff(&config, failures, &error);
                // A manual refresh must obey the same backoff as the scheduled retry.
                min_gap = min_gap.max(delay);
                state.send_modify(|s| {
                    s.last_error = Some(error.to_string());
                    s.consecutive_failures = failures;
                });
                delay
            }
        };

        tokio::select! {
            () = tokio::time::sleep(next_delay) => {}
            request = refresh_requests.recv() => {
                if request.is_none() {
                    tracing::debug!("all calendar handles dropped; stopping");
                    return;
                }
                tokio::time::sleep_until(attempted_at + min_gap).await;
            }
        }
    }
}

/// The minimum time the server has asked us (explicitly, or by blocking us) to stay away.
fn required_hold(error: &CalendarError) -> Option<Duration> {
    match error {
        CalendarError::RateLimited { retry_after } => {
            Some(retry_after.unwrap_or(DEFAULT_RATE_LIMIT_HOLD))
        }
        CalendarError::HtmlResponse => Some(DEFAULT_RATE_LIMIT_HOLD),
        _ => None,
    }
}

/// Exponential backoff, capped at `retry_max`, floored at `min_refresh_interval` (so a
/// retry storm can never burn the feed's request budget), and never shorter than the
/// server's own hold (which may exceed the cap).
fn backoff(config: &ServiceConfig, failures: u32, error: &CalendarError) -> Duration {
    let exponent = failures.saturating_sub(1).min(16);
    let delay = config
        .retry_initial
        .saturating_mul(1u32 << exponent)
        .min(config.retry_max)
        .max(config.min_refresh_interval);
    required_hold(error).map_or(delay, |hold| delay.max(hold))
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;
    use crate::parse::Feed;

    /// A scripted source: pops one canned result per call (repeating the last), and
    /// records the validators each call was made with.
    #[derive(Clone)]
    struct Script(Arc<ScriptInner>);

    struct ScriptInner {
        results: Mutex<VecDeque<Result<FetchOutcome>>>,
        seen_validators: Mutex<Vec<Option<Validators>>>,
        call_times: Mutex<Vec<Instant>>,
    }

    impl Script {
        fn new(results: impl IntoIterator<Item = Result<FetchOutcome>>) -> Self {
            Self(Arc::new(ScriptInner {
                results: Mutex::new(results.into_iter().collect()),
                seen_validators: Mutex::default(),
                call_times: Mutex::default(),
            }))
        }

        fn calls(&self) -> usize {
            self.0.call_times.lock().unwrap().len()
        }

        fn gaps(&self) -> Vec<Duration> {
            let t = self.0.call_times.lock().unwrap();
            t.windows(2).map(|w| w[1] - w[0]).collect()
        }
    }

    impl Fetch for Script {
        async fn fetch(&self, validators: Option<&Validators>) -> Result<FetchOutcome> {
            self.0.call_times.lock().unwrap().push(Instant::now());
            self.0
                .seen_validators
                .lock()
                .unwrap()
                .push(validators.cloned());
            let mut results = self.0.results.lock().unwrap();
            if results.len() > 1 {
                results.pop_front().unwrap()
            } else {
                match results.front().unwrap() {
                    Ok(outcome) => Ok(outcome.clone()),
                    Err(CalendarError::Status { status }) => {
                        Err(CalendarError::Status { status: *status })
                    }
                    Err(CalendarError::RateLimited { retry_after }) => {
                        Err(CalendarError::RateLimited {
                            retry_after: *retry_after,
                        })
                    }
                    Err(_) => unreachable!("unsupported error in test script"),
                }
            }
        }
    }

    fn ok(events: usize) -> Result<FetchOutcome> {
        let events = (0..events)
            .map(|i| {
                crate::event::testing::event(
                    &format!("e{i}"),
                    crate::Scope::Global,
                    time::macros::datetime!(2026-09-14 08:30 -4),
                    crate::Impact::Low,
                )
            })
            .collect();
        Ok(FetchOutcome::Modified {
            feed: Feed { events, skipped: 0 },
            validators: Some(Validators {
                etag: Some("\"v1\"".to_owned()),
                last_modified: None,
            }),
        })
    }

    fn server_error() -> Result<FetchOutcome> {
        Err(CalendarError::Status { status: 503 })
    }

    fn config() -> ServiceConfig {
        ServiceConfig {
            refresh_interval: Duration::from_secs(1800),
            min_refresh_interval: Duration::from_secs(60),
            retry_initial: Duration::from_secs(60),
            retry_max: Duration::from_secs(600),
        }
    }

    async fn next_change(rx: &mut watch::Receiver<CalendarState>) -> CalendarState {
        rx.changed().await.expect("service stopped");
        rx.borrow_and_update().clone()
    }

    #[tokio::test(start_paused = true)]
    async fn publishes_the_first_fetch_then_refreshes_on_the_interval() {
        let script = Script::new([ok(3), ok(5)]);
        let handle = CalendarService::spawn(script.clone(), config());
        let mut rx = handle.subscribe();

        assert_eq!(handle.state().freshness(), Freshness::Loading);
        let first = next_change(&mut rx).await;
        assert_eq!(first.events.len(), 3);
        assert_eq!(first.freshness(), Freshness::Fresh);

        let second = next_change(&mut rx).await;
        assert_eq!(second.events.len(), 5);
        assert_eq!(script.gaps(), [Duration::from_secs(1800)]);
    }

    #[tokio::test(start_paused = true)]
    async fn replays_validators_and_treats_304_as_fresh() {
        let script = Script::new([ok(2), Ok(FetchOutcome::NotModified)]);
        let handle = CalendarService::spawn(script.clone(), config());
        let mut rx = handle.subscribe();

        next_change(&mut rx).await;
        let after_304 = next_change(&mut rx).await;

        assert_eq!(after_304.events.len(), 2, "304 keeps the cached events");
        assert_eq!(after_304.freshness(), Freshness::Fresh);
        let seen = script.0.seen_validators.lock().unwrap();
        assert_eq!(seen[0], None);
        assert_eq!(
            seen[1].as_ref().and_then(|v| v.etag.as_deref()),
            Some("\"v1\"")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_refresh_keeps_stale_events_and_reports_the_error() {
        let script = Script::new([ok(4), server_error()]);
        let handle = CalendarService::spawn(script.clone(), config());
        let mut rx = handle.subscribe();

        next_change(&mut rx).await;
        let failed = next_change(&mut rx).await;

        assert_eq!(failed.events.len(), 4, "stale data is kept, not cleared");
        assert_eq!(failed.freshness(), Freshness::Stale);
        assert_eq!(failed.consecutive_failures, 1);
        assert!(failed.last_error.as_deref().unwrap().contains("503"));
    }

    #[tokio::test(start_paused = true)]
    async fn failure_before_any_success_is_unavailable_and_retries_with_backoff() {
        let script = Script::new([server_error()]);
        let handle = CalendarService::spawn(script.clone(), config());
        let mut rx = handle.subscribe();

        let first = next_change(&mut rx).await;
        assert_eq!(first.freshness(), Freshness::Unavailable);
        assert!(first.events.is_empty());

        for _ in 0..4 {
            next_change(&mut rx).await;
        }
        // 60s, 120s, 240s, 480s: doubling from retry_initial.
        assert_eq!(script.gaps(), [60, 120, 240, 480].map(Duration::from_secs));
    }

    #[tokio::test(start_paused = true)]
    async fn backoff_is_capped_and_recovery_resets_the_failure_count() {
        let mut results: Vec<_> = (0..8).map(|_| server_error()).collect();
        results.push(ok(1));
        let script = Script::new(results);
        let handle = CalendarService::spawn(script.clone(), config());
        let mut rx = handle.subscribe();

        for _ in 0..9 {
            next_change(&mut rx).await;
        }
        let state = handle.state();
        assert_eq!(state.freshness(), Freshness::Fresh);
        assert_eq!(state.consecutive_failures, 0);
        assert!(state.last_error.is_none());
        assert!(script.gaps().iter().all(|g| *g <= Duration::from_secs(600)));
        assert_eq!(
            *script.gaps().iter().max().unwrap(),
            Duration::from_secs(600)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limit_retry_after_is_honored() {
        let script = Script::new([Err(CalendarError::RateLimited {
            retry_after: Some(Duration::from_secs(300)),
        })]);
        let handle = CalendarService::spawn(script.clone(), config());
        let mut rx = handle.subscribe();

        next_change(&mut rx).await;
        next_change(&mut rx).await;
        assert_eq!(script.gaps(), [Duration::from_secs(300)]);
    }

    #[tokio::test(start_paused = true)]
    async fn manual_refresh_is_throttled_to_the_minimum_gap() {
        let script = Script::new([ok(1), ok(2)]);
        let handle = CalendarService::spawn(script.clone(), config());
        let mut rx = handle.subscribe();

        next_change(&mut rx).await;
        assert!(handle.refresh());
        let refreshed = next_change(&mut rx).await;

        assert_eq!(refreshed.events.len(), 2);
        assert_eq!(
            script.gaps(),
            [Duration::from_secs(60)],
            "not immediate, not 30 min"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn manual_refresh_cannot_bypass_a_rate_limit_hold() {
        let script = Script::new([
            Err(CalendarError::RateLimited {
                retry_after: Some(Duration::from_secs(300)),
            }),
            ok(1),
        ]);
        let handle = CalendarService::spawn(script.clone(), config());
        let mut rx = handle.subscribe();

        next_change(&mut rx).await;
        assert!(handle.refresh());
        next_change(&mut rx).await;
        assert_eq!(script.gaps(), [Duration::from_secs(300)]);
    }

    #[tokio::test(start_paused = true)]
    async fn manual_refresh_cannot_bypass_exponential_backoff() {
        let script = Script::new([server_error(), server_error(), ok(1)]);
        let handle = CalendarService::spawn(script.clone(), config());
        let mut rx = handle.subscribe();

        next_change(&mut rx).await;
        next_change(&mut rx).await;
        assert!(handle.refresh());
        next_change(&mut rx).await;
        assert_eq!(script.gaps(), [60, 120].map(Duration::from_secs));
    }

    #[tokio::test(start_paused = true)]
    async fn stops_when_every_handle_is_dropped() {
        let script = Script::new([ok(1)]);
        let handle = CalendarService::spawn(script.clone(), config());
        let mut rx = handle.subscribe();
        next_change(&mut rx).await;

        drop(handle);
        // The task exits, dropping the watch sender: `changed()` then errors.
        assert!(rx.changed().await.is_err());
        assert_eq!(script.calls(), 1);
    }

    #[test]
    fn backoff_never_drops_below_the_minimum_gap() {
        let cfg = ServiceConfig {
            retry_initial: Duration::from_secs(1),
            ..config()
        };
        let plain = CalendarError::Status { status: 500 };
        assert_eq!(backoff(&cfg, 1, &plain), cfg.min_refresh_interval);
    }

    #[test]
    fn rate_limit_without_retry_after_and_html_block_pages_hold_for_five_minutes() {
        let cfg = ServiceConfig {
            min_refresh_interval: Duration::from_secs(1),
            retry_initial: Duration::from_secs(1),
            ..config()
        };
        let no_hint = CalendarError::RateLimited { retry_after: None };
        assert_eq!(backoff(&cfg, 1, &no_hint), DEFAULT_RATE_LIMIT_HOLD);
        assert_eq!(
            backoff(&cfg, 1, &CalendarError::HtmlResponse),
            DEFAULT_RATE_LIMIT_HOLD
        );
        assert_eq!(required_hold(&CalendarError::Status { status: 500 }), None);
    }

    #[test]
    fn backoff_math() {
        let cfg = config();
        let plain = CalendarError::Status { status: 500 };
        let secs = |n| Duration::from_secs(n);
        assert_eq!(backoff(&cfg, 1, &plain), secs(60));
        assert_eq!(backoff(&cfg, 2, &plain), secs(120));
        assert_eq!(backoff(&cfg, 5, &plain), secs(600));
        assert_eq!(backoff(&cfg, u32::MAX, &plain), secs(600));
        let limited = CalendarError::RateLimited {
            retry_after: Some(secs(900)),
        };
        assert_eq!(
            backoff(&cfg, 1, &limited),
            secs(900),
            "Retry-After may exceed the cap"
        );
    }
}
