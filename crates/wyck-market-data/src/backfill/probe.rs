//! Finds the oldest available history for a symbol by walking backward in growing
//! windows. cTrader documents no fixed retention depth (Spotware support: "it usually
//! goes back to the date the broker decided to offer the symbol") so this probes the
//! boundary per symbol rather than assuming one, the same doubling-window trick the
//! app's own live chart loader (`chart/load.rs::ticks_before`) uses to survive a quiet
//! weekend without mistaking it for the start of history.

use wyck_openapi_model::market::Period;

use super::scheduler::{CHUNK_BARS, MAX_TICK_RANGE_MS, RateLimiter, Upstream, fetch_ticks_chunked};
use crate::catalog::Catalog;
use crate::catalog::manifest::TICKS;
use crate::error::{MarketDataError, Result};

/// Empty windows in a row that mean the upstream has no more (older) data.
const MAX_EMPTY_RUN: usize = 6;
/// Steps a tick probe (doubling its window) takes before giving up: a safety cap, not a
/// target. Doubling from one day converges on decades of history well within this many.
const MAX_TICK_STEPS: usize = 20;
/// Steps a bar probe (a fixed span per step) takes before giving up. A bar request is
/// capped by *count*, not time span (unlike ticks), so widening the window does not
/// reduce how many requests are needed to cover a given depth of history: this walks one
/// chunk at a time instead, with a step budget generous enough for years of dense
/// history, since this runs once per symbol/period in the background and is cached in
/// the manifest afterwards.
const MAX_BAR_STEPS: usize = 500;

fn map_upstream_err<E: std::fmt::Display>(err: E) -> MarketDataError {
    MarketDataError::Upstream(err.to_string())
}

/// Walks backward from `before_ms` one server-sized chunk at a time until
/// `MAX_EMPTY_RUN` consecutive empty chunks are seen, records `symbol_id`/`period`'s
/// oldest known bar time in the catalog manifest, and stores every bar fetched along the
/// way (it does not mark any month complete: a probe chunk is not month-aligned, only
/// [`super::scheduler::backfill_bars`] seals a month). Idempotent: safe to call again
/// later, for example after a broker adds older history for a symbol.
pub async fn probe_oldest_bars(
    catalog: &Catalog,
    upstream: &impl Upstream,
    limiter: &RateLimiter,
    symbol_id: i64,
    period: Period,
    before_ms: i64,
    probed_at_ms: i64,
) -> Result<i64> {
    let span = CHUNK_BARS * period.millis();
    let mut to = before_ms;
    let mut oldest = before_ms;
    let mut empty_run = 0;
    for _ in 0..MAX_BAR_STEPS {
        if to < 0 || empty_run >= MAX_EMPTY_RUN {
            break;
        }
        let from = (to - span + 1).max(0);
        limiter.acquire().await;
        let chunk = upstream
            .bars(symbol_id, period, from, to)
            .await
            .map_err(map_upstream_err)?;
        match chunk.iter().map(|bar| bar.time_ms).min() {
            Some(min) => {
                oldest = min;
                empty_run = 0;
                catalog.store_bars(symbol_id, period, &chunk)?;
            }
            None => empty_run += 1,
        }
        to = from - 1;
    }
    catalog
        .manifest()
        .record_oldest_known(symbol_id, period.label(), oldest, probed_at_ms)?;
    Ok(oldest)
}

/// Walks backward from `before_ms` in growing tick windows, see [`probe_oldest_bars`].
pub async fn probe_oldest_ticks(
    catalog: &Catalog,
    upstream: &impl Upstream,
    limiter: &RateLimiter,
    symbol_id: i64,
    before_ms: i64,
    probed_at_ms: i64,
) -> Result<i64> {
    // The window doubles while it finds nothing, capped at the server's own maximum
    // span, so a quiet stretch (a weekend, a symbol with sparse ticks) does not read as
    // the start of history: mirrors `chart/load.rs::ticks_before`.
    let mut window = 86_400_000; // one day
    let mut to = before_ms;
    let mut oldest = before_ms;
    let mut empty_run = 0;
    for _ in 0..MAX_TICK_STEPS {
        if to < 0 || empty_run >= MAX_EMPTY_RUN {
            break;
        }
        let from = (to - window + 1).max(0);
        let chunk = fetch_ticks_chunked(upstream, limiter, symbol_id, from, to).await?;
        match chunk.iter().map(|tick| tick.time_ms).min() {
            Some(min) => {
                oldest = min;
                empty_run = 0;
                catalog.store_ticks(symbol_id, &chunk)?;
            }
            None => empty_run += 1,
        }
        to = from - 1;
        window = (window * 2).min(MAX_TICK_RANGE_MS);
    }
    catalog
        .manifest()
        .record_oldest_known(symbol_id, TICKS, oldest, probed_at_ms)?;
    Ok(oldest)
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Mutex;

    use wyck_openapi_model::market::{Bar, Tick};

    use super::*;
    use crate::catalog::chunks::{month_start_ms, next_month_start_ms};

    struct Fake {
        start: i64,
        end: i64,
        bar_request_count: Mutex<usize>,
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
            *self.bar_request_count.lock().unwrap() += 1;
            let step = period.millis();
            let (start, end) = (self.start, self.end);
            async move {
                let first = from_ms.max(start).div_euclid(step) * step;
                Ok((0..)
                    .map(|i| first + i * step)
                    .take_while(|t| *t <= to_ms.min(end))
                    .filter(|t| *t >= from_ms && *t >= start)
                    .map(|t| Bar {
                        time_ms: t,
                        open: 1,
                        high: 1,
                        low: 1,
                        close: 1,
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
            let (start, end) = (self.start, self.end);
            async move {
                Ok((from_ms.max(start)..=to_ms.min(end))
                    .step_by(1_000)
                    .map(|t| Tick {
                        time_ms: t,
                        price: 1,
                    })
                    .collect())
            }
        }
    }

    fn limiter() -> RateLimiter {
        RateLimiter::new(1_000_000.0)
    }

    #[tokio::test]
    async fn probing_finds_the_true_start_of_history() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        // History exists for the whole of March 2024 only.
        let history_start = month_start_ms(2024, 3);
        let before = next_month_start_ms(2024, 3);
        let fake = Fake {
            start: history_start,
            end: before,
            bar_request_count: Mutex::new(0),
        };

        let oldest = probe_oldest_bars(&catalog, &fake, &limiter(), 1, Period::H1, before, 999)
            .await
            .unwrap();

        assert_eq!(oldest, history_start);
        assert_eq!(
            catalog
                .manifest()
                .oldest_known_ms(1, Period::H1.label())
                .unwrap(),
            Some(history_start)
        );
    }

    #[tokio::test]
    async fn probing_stops_after_a_run_of_empty_windows() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        let history_start = month_start_ms(2024, 6);
        let before = next_month_start_ms(2024, 6);
        let fake = Fake {
            start: history_start,
            end: before,
            bar_request_count: Mutex::new(0),
        };

        let oldest = probe_oldest_bars(&catalog, &fake, &limiter(), 1, Period::M1, before, 1)
            .await
            .unwrap();

        assert_eq!(oldest, history_start);
        // One request per chunk to walk the month, plus a bounded run of empty ones:
        // nowhere near the full MAX_BAR_STEPS budget.
        let month_chunks = 31 * 86_400_000 / (CHUNK_BARS * Period::M1.millis()) + 1;
        assert!(*fake.bar_request_count.lock().unwrap() <= month_chunks as usize + MAX_EMPTY_RUN);
    }

    #[tokio::test]
    async fn ticks_are_probed_and_stored_along_the_way() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        let history_start = month_start_ms(2024, 6);
        let before = history_start + 3_600_000;
        let fake = Fake {
            start: history_start,
            end: before,
            bar_request_count: Mutex::new(0),
        };

        let oldest = probe_oldest_ticks(&catalog, &fake, &limiter(), 1, before, 1)
            .await
            .unwrap();

        assert_eq!(oldest, history_start);
        assert!(
            !catalog
                .load_ticks(1, history_start, before + 1)
                .unwrap()
                .is_empty()
        );
    }
}
