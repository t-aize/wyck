//! Rate-limited, chunked fetching from an [`Upstream`] historical data source, storing
//! whatever comes back into the [`Catalog`].
//!
//! Every request goes through a shared [`RateLimiter`] (cTrader's historical endpoints
//! allow 5 requests/second per connection, see [`RateLimiter::for_ctrader_history`]), and
//! every fetch is split into chunks the server accepts: at most [`CHUNK_BARS`] bars, or
//! [`MAX_TICK_RANGE_MS`] of ticks, per request. A whole calendar month is always fetched
//! and stored as one unit and only then marked complete in the manifest (unless it is the
//! still-open current month), so "complete" always means "we asked the server for this
//! exact month and stored whatever it returned", never a partial-range guess.

use std::future::Future;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;
use wyck_openapi_model::market::{Bar, Period, Tick};

use crate::catalog::Catalog;
use crate::catalog::chunks::{month_of, month_start_ms, months_between, next_month_start_ms};
use crate::error::{MarketDataError, Result};

/// Bars asked for in one upstream request, well under the documented 14,000-per-response
/// hard cap (mirrors the app's own live chart loader, `chart/load.rs::CHUNK_BARS`, which
/// found smaller chunks friendlier to the server in practice).
pub const CHUNK_BARS: i64 = 1_500;

/// The documented maximum span of a single tick request (seven days), with a minute of
/// slack so an off-by-one at the edges is never refused.
pub const MAX_TICK_RANGE_MS: i64 = 7 * 86_400_000 - 60_000;

/// Requests one backfill call may make for a single month: a guard against a runaway
/// loop, not a target.
const MAX_CHUNKS_PER_MONTH: usize = 200;

/// An upstream source of historical bars and ticks: the real cTrader `MarketClient` in
/// the app, or a scriptable fake in tests. A call is expected to resolve its own range
/// completely (including any server-side `hasMore` pagination), the same contract the
/// app's existing `chart::load::History` trait already assumes of `MarketClient`.
pub trait Upstream: Sync {
    /// The upstream's own error type. Only its [`std::fmt::Display`] text is kept (see
    /// [`MarketDataError::Upstream`]), so this crate never has to depend on the
    /// upstream's concrete error type or the (heavier) crate that defines it.
    type Error: std::fmt::Display + Send + 'static;

    /// Every bar in `[from_ms, to_ms]` for `symbol_id`/`period`.
    fn bars(
        &self,
        symbol_id: i64,
        period: Period,
        from_ms: i64,
        to_ms: i64,
    ) -> impl Future<Output = std::result::Result<Vec<Bar>, Self::Error>> + Send;

    /// Every tick in `[from_ms, to_ms]` for `symbol_id`.
    fn ticks(
        &self,
        symbol_id: i64,
        from_ms: i64,
        to_ms: i64,
    ) -> impl Future<Output = std::result::Result<Vec<Tick>, Self::Error>> + Send;
}

/// Enforces a minimum spacing between requests on a shared upstream connection.
pub struct RateLimiter {
    min_interval: Duration,
    last: Mutex<Option<Instant>>,
}

impl RateLimiter {
    /// A limiter allowing at most `requests_per_second` requests, evenly spaced.
    #[must_use]
    pub fn new(requests_per_second: f64) -> Self {
        Self {
            min_interval: Duration::from_secs_f64(1.0 / requests_per_second),
            last: Mutex::new(None),
        }
    }

    /// cTrader's documented limit for historical (bar/tick) requests: 5 per second per
    /// connection.
    #[must_use]
    pub fn for_ctrader_history() -> Self {
        Self::new(5.0)
    }

    /// Waits, if needed, until issuing another request would not exceed the limit.
    pub async fn acquire(&self) {
        let mut last = self.last.lock().await;
        if let Some(previous) = *last {
            let elapsed = previous.elapsed();
            if elapsed < self.min_interval {
                tokio::time::sleep(self.min_interval - elapsed).await;
            }
        }
        *last = Some(Instant::now());
    }
}

fn map_upstream_err<E: std::fmt::Display>(err: E) -> MarketDataError {
    MarketDataError::Upstream(err.to_string())
}

/// Fetches `[from_ms, to_ms]` bars in server-sized chunks, rate-limited, sorted and
/// deduplicated by time.
pub(crate) async fn fetch_bars_chunked(
    upstream: &impl Upstream,
    limiter: &RateLimiter,
    symbol_id: i64,
    period: Period,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<Bar>> {
    let span = CHUNK_BARS * period.millis();
    let mut all = Vec::new();
    let mut from = from_ms;
    for _ in 0..MAX_CHUNKS_PER_MONTH {
        if from > to_ms {
            break;
        }
        let to = (from + span - 1).min(to_ms);
        limiter.acquire().await;
        all.extend(
            upstream
                .bars(symbol_id, period, from, to)
                .await
                .map_err(map_upstream_err)?,
        );
        from = to + 1;
    }
    all.sort_by_key(|bar| bar.time_ms);
    all.dedup_by_key(|bar| bar.time_ms);
    Ok(all)
}

/// Fetches `[from_ms, to_ms]` ticks in server-sized windows, rate-limited, sorted and
/// deduplicated by time.
pub(crate) async fn fetch_ticks_chunked(
    upstream: &impl Upstream,
    limiter: &RateLimiter,
    symbol_id: i64,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<Tick>> {
    let mut all = Vec::new();
    let mut from = from_ms;
    for _ in 0..MAX_CHUNKS_PER_MONTH {
        if from > to_ms {
            break;
        }
        let to = (from + MAX_TICK_RANGE_MS - 1).min(to_ms);
        limiter.acquire().await;
        all.extend(
            upstream
                .ticks(symbol_id, from, to)
                .await
                .map_err(map_upstream_err)?,
        );
        from = to + 1;
    }
    all.sort_by_key(|tick| tick.time_ms);
    all.dedup_by_key(|tick| tick.time_ms);
    Ok(all)
}

/// The range a backfill call should ensure is covered, plus the current wall-clock time
/// (needed only so the still-open month, if any, is fetched but never sealed).
#[derive(Debug, Clone, Copy)]
pub struct BackfillRange {
    /// The start of the range (inclusive).
    pub from_ms: i64,
    /// The end of the range (exclusive).
    pub to_ms: i64,
    /// The current wall-clock time.
    pub now_ms: i64,
}

/// Ensures every whole calendar month overlapping `range` is present in the catalog for
/// `symbol_id`/`period`, fetching and storing any month not already marked complete.
///
/// A closed month (one that has fully elapsed) is fetched and cached in full, once, then
/// sealed. The month `range.now_ms` falls in is different: it keeps receiving new bars,
/// so it is never sealed, and — critically — only the slice of it actually asked for is
/// fetched, not the whole month. Without that distinction, every single call touching
/// "now" (which is every chart load) would re-fetch the entire current month from
/// scratch, since it can never be marked complete: exactly the bug this comment is here
/// to keep someone from reintroducing.
pub async fn backfill_bars(
    catalog: &Catalog,
    upstream: &impl Upstream,
    limiter: &RateLimiter,
    symbol_id: i64,
    period: Period,
    range: BackfillRange,
) -> Result<()> {
    let open_month_start = {
        let (year, month) = month_of(range.now_ms);
        month_start_ms(year, month)
    };
    for (year, month) in months_between(range.from_ms, range.to_ms) {
        let start = month_start_ms(year, month);
        if catalog.bars_month_complete(symbol_id, period, start)? {
            continue;
        }
        if start == open_month_start {
            let month_end = next_month_start_ms(year, month);
            let from = range.from_ms.max(start);
            let to = range.to_ms.min(month_end) - 1;
            if from > to {
                continue;
            }
            let bars = fetch_bars_chunked(upstream, limiter, symbol_id, period, from, to).await?;
            catalog.store_bars(symbol_id, period, &bars)?;
        } else {
            let end = next_month_start_ms(year, month) - 1;
            let bars = fetch_bars_chunked(upstream, limiter, symbol_id, period, start, end).await?;
            catalog.store_bars(symbol_id, period, &bars)?;
            catalog.mark_bars_month_complete(symbol_id, period, start)?;
        }
    }
    Ok(())
}

/// Ensures every whole calendar month overlapping `range` is present in the catalog's
/// tick series for `symbol_id`, see [`backfill_bars`] (including why the still-open month
/// only ever fetches the requested slice, never the whole month).
pub async fn backfill_ticks(
    catalog: &Catalog,
    upstream: &impl Upstream,
    limiter: &RateLimiter,
    symbol_id: i64,
    range: BackfillRange,
) -> Result<()> {
    let open_month_start = {
        let (year, month) = month_of(range.now_ms);
        month_start_ms(year, month)
    };
    for (year, month) in months_between(range.from_ms, range.to_ms) {
        let start = month_start_ms(year, month);
        if catalog.ticks_month_complete(symbol_id, start)? {
            continue;
        }
        if start == open_month_start {
            let month_end = next_month_start_ms(year, month);
            let from = range.from_ms.max(start);
            let to = range.to_ms.min(month_end) - 1;
            if from > to {
                continue;
            }
            let ticks = fetch_ticks_chunked(upstream, limiter, symbol_id, from, to).await?;
            catalog.store_ticks(symbol_id, &ticks)?;
            continue;
        }
        let end = next_month_start_ms(year, month) - 1;
        let ticks = fetch_ticks_chunked(upstream, limiter, symbol_id, start, end).await?;
        catalog.store_ticks(symbol_id, &ticks)?;
        catalog.mark_ticks_month_complete(symbol_id, start)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use super::*;

    /// A fake upstream: bars/ticks evenly spaced from `start` to `end`, and a log of
    /// every request made of it, mirroring `chart/load.rs`'s own test fake.
    struct Fake {
        start: i64,
        end: i64,
        bar_requests: StdMutex<Vec<(i64, i64)>>,
        tick_requests: StdMutex<Vec<(i64, i64)>>,
    }

    impl Fake {
        fn new(start: i64, end: i64) -> Self {
            Self {
                start,
                end,
                bar_requests: StdMutex::new(Vec::new()),
                tick_requests: StdMutex::new(Vec::new()),
            }
        }
    }

    impl Upstream for Fake {
        type Error = std::convert::Infallible;

        fn bars(
            &self,
            _symbol_id: i64,
            period: Period,
            from_ms: i64,
            to_ms: i64,
        ) -> impl Future<Output = std::result::Result<Vec<Bar>, Self::Error>> + Send {
            self.bar_requests.lock().unwrap().push((from_ms, to_ms));
            let step = period.millis();
            let (start, end) = (self.start, self.end);
            async move {
                assert!(
                    (to_ms - from_ms) / step <= CHUNK_BARS,
                    "a request asks for more bars than the server serves"
                );
                let first = from_ms.max(start).div_euclid(step) * step;
                Ok((0..)
                    .map(|i| first + i * step)
                    .take_while(|t| *t <= to_ms.min(end))
                    .filter(|t| *t >= from_ms && *t >= start)
                    .map(|t| Bar {
                        time_ms: t,
                        open: 100,
                        high: 110,
                        low: 90,
                        close: 105,
                        volume: 1,
                    })
                    .collect())
            }
        }

        fn ticks(
            &self,
            _symbol_id: i64,
            from_ms: i64,
            to_ms: i64,
        ) -> impl Future<Output = std::result::Result<Vec<Tick>, Self::Error>> + Send {
            self.tick_requests.lock().unwrap().push((from_ms, to_ms));
            let (start, end) = (self.start, self.end);
            async move {
                assert!(
                    to_ms - from_ms <= MAX_TICK_RANGE_MS,
                    "a tick request is too long"
                );
                let first = (from_ms.max(start) + 999).div_euclid(1_000) * 1_000;
                Ok((0..)
                    .map(|i| first + i * 1_000)
                    .take_while(|t| *t <= to_ms.min(end))
                    .map(|t| Tick {
                        time_ms: t,
                        price: 100,
                    })
                    .collect())
            }
        }
    }

    fn limiter() -> RateLimiter {
        // No real waiting in tests: an effectively unbounded rate.
        RateLimiter::new(1_000_000.0)
    }

    #[tokio::test]
    async fn backfilling_a_range_stores_bars_chunked_within_the_server_limit() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        let feb_start = month_start_ms(2024, 2);
        let mar_end = next_month_start_ms(2024, 3);
        let fake = Fake::new(feb_start, mar_end);
        // `now_ms` is well past both months, so both may be sealed.
        let now = next_month_start_ms(2024, 4);

        backfill_bars(
            &catalog,
            &fake,
            &limiter(),
            1,
            Period::M1,
            BackfillRange {
                from_ms: feb_start,
                to_ms: mar_end,
                now_ms: now,
            },
        )
        .await
        .unwrap();

        let loaded = catalog.load_bars(1, Period::M1, feb_start, mar_end).unwrap();
        assert!(!loaded.is_empty());
        assert!(loaded.windows(2).all(|w| w[0].time_ms < w[1].time_ms));
        assert!(catalog.bars_month_complete(1, Period::M1, feb_start).unwrap());
        assert!(catalog.bars_month_complete(1, Period::M1, month_start_ms(2024, 3)).unwrap());
    }

    #[tokio::test]
    async fn a_second_backfill_skips_months_already_sealed() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        let start = month_start_ms(2024, 5);
        let end = next_month_start_ms(2024, 5);
        let fake = Fake::new(start, end);
        let now = next_month_start_ms(2024, 6);

        let range = BackfillRange {
            from_ms: start,
            to_ms: end,
            now_ms: now,
        };
        backfill_bars(&catalog, &fake, &limiter(), 1, Period::H1, range)
            .await
            .unwrap();
        let requests_after_first = fake.bar_requests.lock().unwrap().len();
        assert!(requests_after_first > 0);

        backfill_bars(&catalog, &fake, &limiter(), 1, Period::H1, range)
            .await
            .unwrap();
        assert_eq!(
            fake.bar_requests.lock().unwrap().len(),
            requests_after_first,
            "a sealed month must not be re-fetched"
        );
    }

    #[tokio::test]
    async fn the_current_open_month_is_stored_but_never_sealed() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        let (year, month) = month_of(1_800_000_000_000);
        let start = month_start_ms(year, month);
        let now = start + 60_000; // "now" is inside this same month
        let fake = Fake::new(start, now);

        backfill_bars(
            &catalog,
            &fake,
            &limiter(),
            1,
            Period::M1,
            BackfillRange {
                from_ms: start,
                to_ms: now,
                now_ms: now,
            },
        )
        .await
        .unwrap();

        assert!(!catalog.load_bars(1, Period::M1, start, now).unwrap().is_empty());
        assert!(!catalog.bars_month_complete(1, Period::M1, start).unwrap());
    }

    /// Regression test: a chart asking for a small recent window (as every normal chart
    /// load does) must never trigger fetching the whole current month, or a load that
    /// used to be instant against the live broker becomes a multi-second (or, on a
    /// dense M1 chart, multi-request) stall every single time, since the open month can
    /// never be marked complete and would otherwise be re-fetched whole on every call.
    #[tokio::test]
    async fn a_small_window_in_the_open_month_never_fetches_the_whole_month() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        let month_start = month_start_ms(2024, 8);
        let now = month_start + 20 * 86_400_000; // 20 days into the month
        let fake = Fake::new(month_start, now);
        let window_start = now - 20 * 60_000; // the last 20 minutes only

        backfill_bars(
            &catalog,
            &fake,
            &limiter(),
            1,
            Period::M1,
            BackfillRange {
                from_ms: window_start,
                to_ms: now,
                now_ms: now,
            },
        )
        .await
        .unwrap();

        for (from, _to) in fake.bar_requests.lock().unwrap().iter() {
            assert!(
                *from >= window_start,
                "asked for {from}, long before the requested window {window_start}: the whole month was fetched"
            );
        }
        assert!(!catalog.bars_month_complete(1, Period::M1, month_start).unwrap());
    }

    #[tokio::test]
    async fn ticks_are_backfilled_in_windows_within_the_server_limit() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        let start = month_start_ms(2024, 6);
        let end = next_month_start_ms(2024, 6);
        let fake = Fake::new(start, end);
        let now = next_month_start_ms(2024, 7);

        backfill_ticks(
            &catalog,
            &fake,
            &limiter(),
            1,
            BackfillRange {
                from_ms: start,
                to_ms: end,
                now_ms: now,
            },
        )
        .await
        .unwrap();

        assert!(!catalog.load_ticks(1, start, end).unwrap().is_empty());
        assert!(catalog.ticks_month_complete(1, start).unwrap());
        assert!(fake.tick_requests.lock().unwrap().len() > 1, "a month is longer than one tick window");
    }

    #[tokio::test]
    async fn rate_limiter_spaces_requests_at_least_the_minimum_interval_apart() {
        tokio::time::pause();
        let limiter = RateLimiter::new(5.0);
        let start = Instant::now();
        limiter.acquire().await;
        limiter.acquire().await;
        // The second acquire must not have returned before at least 1/5s of (virtual)
        // time passed, i.e. it actually waited rather than racing ahead.
        assert!(start.elapsed() >= Duration::from_secs_f64(0.2));
    }
}
