//! History for a replay, one chunk per call, so the replay decides what to fetch next.
//!
//! A chunk stays inside what the server answers in one request. The range it covers is reported
//! even when it holds no bar: a weekend is covered, and the replay must not ask for it again.

use super::load::{CHUNK_BARS, History};
use crate::broker::Result;
use wyck_market::{Bar, Period};

/// Tuesday 2000-01-04 12:00 UTC: a day the markets are open, a week at a time from here.
const PROBE_BASE_MS: i64 = 946_987_200_000;
const WEEK_MS: i64 = 7 * 86_400_000;
const HOUR_MS: i64 = 3_600_000;
/// Chunks the last step of [`earliest`] may read to find the first bar of a week.
const SCAN_CHUNKS: usize = 10;

/// Bars of a range, oldest first, and the range asked for (both ends included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Chunk {
    pub(crate) bars: Vec<Bar>,
    pub(crate) from_ms: i64,
    pub(crate) to_ms: i64,
}

/// The chunk that ends just before `before_ms`.
pub(crate) async fn chunk_before(
    market: &impl History,
    symbol_id: i64,
    period: Period,
    before_ms: i64,
) -> Result<Chunk> {
    let to = before_ms - 1;
    let from = (to - CHUNK_BARS * period.millis() + 1).max(0);
    range(market, symbol_id, period, from, to).await
}

/// The chunk that starts at `from_ms` and stops at `limit_ms` at the latest.
pub(crate) async fn chunk_forward(
    market: &impl History,
    symbol_id: i64,
    period: Period,
    from_ms: i64,
    limit_ms: i64,
) -> Result<Chunk> {
    let to = (from_ms + CHUNK_BARS * period.millis() - 1).min(limit_ms);
    if to < from_ms {
        return Ok(Chunk {
            bars: Vec::new(),
            from_ms,
            to_ms: from_ms - 1,
        });
    }
    range(market, symbol_id, period, from_ms, to).await
}

async fn range(
    market: &impl History,
    symbol_id: i64,
    period: Period,
    from_ms: i64,
    to_ms: i64,
) -> Result<Chunk> {
    let mut bars = market.bars(symbol_id, period, from_ms, to_ms).await?;
    bars.sort_by_key(|b| b.time_ms);
    bars.dedup_by_key(|b| b.time_ms);
    Ok(Chunk {
        bars,
        from_ms,
        to_ms,
    })
}

/// The opening time of the oldest bar the server has for a symbol and period, or `None` when it
/// has none in the last weeks.
///
/// It probes one window a week, always from a Tuesday noon, and bisects on whether the probe
/// finds a bar: about eleven requests, then a few chunks to land on the first bar. A holiday on
/// the probe day can make it start a week late, which is of no consequence for a replay.
pub(crate) async fn earliest(
    market: &impl History,
    symbol_id: i64,
    period: Period,
    now_ms: i64,
) -> Result<Option<i64>> {
    let window = (CHUNK_BARS * period.millis()).min((2 * period.millis()).max(36 * HOUR_MS));
    let probe = async |week: i64| -> Result<Vec<Bar>> {
        let from = PROBE_BASE_MS + week * WEEK_MS;
        market
            .bars(symbol_id, period, from, from + window - 1)
            .await
    };
    let last_week = (now_ms - PROBE_BASE_MS).div_euclid(WEEK_MS);
    let mut high = last_week;
    if probe(high).await?.is_empty() {
        high -= 1;
        if high < 0 || probe(high).await?.is_empty() {
            return Ok(None);
        }
    }
    let mut low = 0;
    while low < high {
        let mid = low + (high - low) / 2;
        if probe(mid).await?.is_empty() {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    let end = PROBE_BASE_MS + low * WEEK_MS + window - 1;
    let mut from = if low == 0 {
        0
    } else {
        PROBE_BASE_MS + (low - 1) * WEEK_MS + 1
    };
    for _ in 0..SCAN_CHUNKS {
        if from > end {
            break;
        }
        let chunk = chunk_forward(market, symbol_id, period, from, end).await?;
        if let Some(first) = chunk.bars.first() {
            return Ok(Some(first.time_ms));
        }
        from = chunk.to_ms + 1;
    }
    Ok(probe(low).await?.first().map(|b| b.time_ms))
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Mutex;

    use super::*;
    use crate::broker::Error as ApiError;
    use wyck_market::Tick;

    const MINUTE: i64 = 60_000;
    /// 2026-01-05 00:00 UTC.
    const NOW: i64 = 1_767_571_200_000;

    /// A server with a bar every period from `start` to `end`, and a log of the requests.
    struct Fake {
        start: i64,
        end: i64,
        requests: Mutex<Vec<(i64, i64)>>,
    }

    impl Fake {
        fn new(start: i64, end: i64) -> Self {
            Self {
                start,
                end,
                requests: Mutex::new(Vec::new()),
            }
        }

        fn count(&self) -> usize {
            self.requests.lock().expect("log").len()
        }
    }

    impl History for Fake {
        fn bars(
            &self,
            _symbol_id: i64,
            period: Period,
            from_ms: i64,
            to_ms: i64,
        ) -> impl Future<Output = Result<Vec<Bar>>> + Send {
            self.requests.lock().expect("log").push((from_ms, to_ms));
            let step = period.millis();
            let (start, end) = (self.start, self.end);
            async move {
                assert!(
                    (to_ms - from_ms) / step <= CHUNK_BARS,
                    "a request asks for more bars than the server serves"
                );
                let first = from_ms.max(start).div_euclid(step) * step;
                Ok(std::iter::successors(Some(first), |t| t.checked_add(step))
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
            _from_ms: i64,
            _to_ms: i64,
        ) -> impl Future<Output = Result<Vec<Tick>>> + Send {
            std::future::ready(Err(ApiError::Closed))
        }
    }

    fn run<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime")
            .block_on(future)
    }

    #[test]
    fn a_chunk_before_ends_just_before_the_time_and_stays_inside_the_server_limit() {
        let fake = Fake::new(0, NOW);
        let chunk = run(chunk_before(&fake, 1, Period::M1, NOW)).expect("a chunk");
        assert_eq!(chunk.to_ms, NOW - 1);
        assert_eq!(chunk.bars.len(), 1_500);
        assert_eq!(chunk.bars.last().map(|b| b.time_ms), Some(NOW - MINUTE));
    }

    #[test]
    fn a_chunk_before_the_start_of_time_stops_at_zero() {
        let fake = Fake::new(0, NOW);
        let chunk = run(chunk_before(&fake, 1, Period::M1, 10 * MINUTE)).expect("a chunk");
        assert_eq!((chunk.from_ms, chunk.bars.len()), (0, 10));
    }

    #[test]
    fn a_chunk_forward_stops_at_the_limit_and_reports_the_range_even_when_empty() {
        let fake = Fake::new(NOW + 10 * MINUTE, NOW + 20 * MINUTE);
        let chunk = run(chunk_forward(&fake, 1, Period::M1, NOW, NOW + 5 * MINUTE)).expect("chunk");
        assert_eq!(chunk.bars.len(), 0);
        assert_eq!((chunk.from_ms, chunk.to_ms), (NOW, NOW + 5 * MINUTE));
    }

    #[test]
    fn a_chunk_forward_past_its_limit_asks_nothing() {
        let fake = Fake::new(0, NOW);
        let chunk = run(chunk_forward(&fake, 1, Period::M1, NOW, NOW - 1)).expect("a chunk");
        assert_eq!(chunk.bars.len(), 0);
        assert_eq!(chunk.to_ms, NOW - 1);
        assert_eq!(fake.count(), 0);
    }

    #[test]
    fn the_earliest_bar_is_found_with_a_few_requests() {
        for (start, period) in [
            (NOW - 400 * 86_400_000, Period::M1),
            (NOW - 3_000 * 86_400_000 + 7 * MINUTE, Period::M1),
            (NOW - 900 * 86_400_000, Period::H1),
            (NOW - 5_000 * 86_400_000, Period::D1),
        ] {
            let fake = Fake::new(start, NOW);
            let found = run(earliest(&fake, 1, period, NOW)).expect("a search");
            let expected = start.div_euclid(period.millis()) * period.millis();
            let expected = if expected < start {
                expected + period.millis()
            } else {
                expected
            };
            assert_eq!(found, Some(expected), "{period:?}");
            assert!(
                fake.count() <= 30,
                "{} requests for {period:?}",
                fake.count()
            );
        }
    }

    #[test]
    fn a_symbol_without_recent_bars_has_no_earliest() {
        let fake = Fake::new(NOW + 86_400_000, NOW + 2 * 86_400_000);
        assert_eq!(
            run(earliest(&fake, 1, Period::M1, NOW)).expect("a search"),
            None
        );
    }
}
