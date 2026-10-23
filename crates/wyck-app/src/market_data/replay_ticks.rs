//! Ticks for a replay, one chunk per call. A chunk is a stretch of time, not a number of ticks: how
//! many ticks it holds depends on the symbol and the hour, so the length of the next one is worked
//! out from the last (`next_span_ms`) to keep each request near the same size.
//!
//! The range a chunk covers is reported even when it holds no tick, so a weekend is not asked
//! for twice.

use super::load::History;
use crate::broker::Result;
use wyck_domain::market::Tick;

/// What a chunk aims for: big enough to be few requests, small enough to be quick and to waste
/// little when the replay moves on before the answer comes.
pub(crate) const TARGET_TICKS: usize = 50_000;
/// The first chunk of a symbol, before its tick rate is known.
pub(crate) const FIRST_SPAN_MS: i64 = 30 * 60_000;
const MIN_SPAN_MS: i64 = 5 * 60_000;
const MAX_SPAN_MS: i64 = 6 * 3_600_000;

/// Ticks of a range, oldest first, and the range asked for (both ends included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TickChunk {
    pub(crate) ticks: Vec<Tick>,
    pub(crate) from_ms: i64,
    pub(crate) to_ms: i64,
}

/// The length of the next chunk, from the one just read: as long as it takes to hold about
/// [`TARGET_TICKS`] at the same rate, twice as long after an empty one (a closed market).
pub(crate) fn next_span_ms(span_ms: i64, count: usize) -> i64 {
    let next = if count == 0 {
        span_ms.saturating_mul(2)
    } else {
        let scaled = i128::from(span_ms) * i128::try_from(TARGET_TICKS).unwrap_or(i128::MAX)
            / i128::try_from(count).unwrap_or(i128::MAX);
        i64::try_from(scaled).unwrap_or(MAX_SPAN_MS)
    };
    next.clamp(MIN_SPAN_MS, MAX_SPAN_MS)
}

/// The chunk of `span_ms` that ends just before `before_ms`.
pub(crate) async fn tick_chunk_before(
    market: &impl History,
    symbol_id: i64,
    before_ms: i64,
    span_ms: i64,
) -> Result<TickChunk> {
    let to = before_ms - 1;
    let from = (to - span_ms + 1).max(0);
    range(market, symbol_id, from, to).await
}

/// The chunk of `span_ms` that starts at `from_ms`, and stops at `limit_ms` at the latest.
pub(crate) async fn tick_chunk_forward(
    market: &impl History,
    symbol_id: i64,
    from_ms: i64,
    limit_ms: i64,
    span_ms: i64,
) -> Result<TickChunk> {
    let to = (from_ms + span_ms - 1).min(limit_ms);
    if to < from_ms {
        return Ok(TickChunk {
            ticks: Vec::new(),
            from_ms,
            to_ms: from_ms - 1,
        });
    }
    range(market, symbol_id, from_ms, to).await
}

async fn range(
    market: &impl History,
    symbol_id: i64,
    from_ms: i64,
    to_ms: i64,
) -> Result<TickChunk> {
    let mut ticks = market.ticks(symbol_id, from_ms, to_ms).await?;
    ticks.sort_by_key(|t| t.time_ms);
    Ok(TickChunk {
        ticks,
        from_ms,
        to_ms,
    })
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Mutex;

    use super::*;
    use crate::broker::Error as ApiError;
    use wyck_domain::market::{Bar, Period};

    /// A server with a tick every `every_ms` from `start` to `end`, and a log of the requests.
    struct Fake {
        start: i64,
        end: i64,
        every_ms: i64,
        requests: Mutex<Vec<(i64, i64)>>,
    }

    impl History for Fake {
        fn bars(
            &self,
            _symbol_id: i64,
            _period: Period,
            _from_ms: i64,
            _to_ms: i64,
        ) -> impl Future<Output = Result<Vec<Bar>>> + Send {
            std::future::ready(Err(ApiError::Closed))
        }

        fn ticks(
            &self,
            _symbol_id: i64,
            from_ms: i64,
            to_ms: i64,
        ) -> impl Future<Output = Result<Vec<Tick>>> + Send {
            self.requests.lock().expect("log").push((from_ms, to_ms));
            let (start, end, every) = (self.start, self.end, self.every_ms);
            async move {
                let first = (from_ms.max(start) + every - 1).div_euclid(every) * every;
                Ok(std::iter::successors(Some(first), |t| t.checked_add(every))
                    .take_while(|t| *t <= to_ms.min(end))
                    .map(|t| Tick {
                        time_ms: t,
                        price: 100 + (t / every) % 5,
                    })
                    .collect())
            }
        }
    }

    fn run<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime")
            .block_on(future)
    }

    fn fake(start: i64, end: i64) -> Fake {
        Fake {
            start,
            end,
            every_ms: 1_000,
            requests: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn the_next_chunk_is_as_long_as_it_takes_to_hold_the_target() {
        assert_eq!(next_span_ms(FIRST_SPAN_MS, TARGET_TICKS), FIRST_SPAN_MS);
        assert_eq!(
            next_span_ms(FIRST_SPAN_MS, 4 * TARGET_TICKS),
            FIRST_SPAN_MS / 4
        );
        assert_eq!(
            next_span_ms(FIRST_SPAN_MS, TARGET_TICKS / 2),
            FIRST_SPAN_MS * 2
        );
    }

    #[test]
    fn a_chunk_is_never_too_short_or_too_long() {
        assert_eq!(next_span_ms(FIRST_SPAN_MS, 100 * TARGET_TICKS), MIN_SPAN_MS);
        assert_eq!(next_span_ms(FIRST_SPAN_MS, 1), MAX_SPAN_MS);
        assert_eq!(
            next_span_ms(MAX_SPAN_MS, 0),
            MAX_SPAN_MS,
            "an empty one doubles, within bounds"
        );
        assert_eq!(next_span_ms(MIN_SPAN_MS, 0), 2 * MIN_SPAN_MS);
    }

    #[test]
    fn a_chunk_before_ends_just_before_the_time() {
        let server = fake(0, 10_000_000);
        let chunk = run(tick_chunk_before(&server, 1, 1_000_000, 600_000)).expect("a chunk");
        assert_eq!((chunk.from_ms, chunk.to_ms), (400_000, 999_999));
        assert_eq!(chunk.ticks.first().map(|t| t.time_ms), Some(400_000));
        assert_eq!(chunk.ticks.last().map(|t| t.time_ms), Some(999_000));
    }

    #[test]
    fn a_chunk_forward_stops_at_the_limit_and_covers_a_range_without_ticks() {
        let server = fake(5_000_000, 6_000_000);
        let chunk = run(tick_chunk_forward(&server, 1, 0, 100_000, 600_000)).expect("a chunk");
        assert_eq!(chunk.ticks.len(), 0);
        assert_eq!((chunk.from_ms, chunk.to_ms), (0, 100_000));
    }

    #[test]
    fn a_chunk_past_its_limit_asks_nothing() {
        let server = fake(0, 10_000_000);
        let chunk = run(tick_chunk_forward(&server, 1, 500, 100, 600_000)).expect("a chunk");
        assert_eq!(chunk.ticks.len(), 0);
        assert_eq!(chunk.to_ms, 499);
        assert_eq!(server.requests.lock().expect("log").len(), 0);
    }

    #[test]
    fn consecutive_chunks_neither_overlap_nor_leave_a_gap() {
        let server = fake(0, 10_000_000);
        let (mut from, mut all) = (0, Vec::new());
        for _ in 0..4 {
            let chunk =
                run(tick_chunk_forward(&server, 1, from, 10_000_000, 300_000)).expect("chunk");
            from = chunk.to_ms + 1;
            all.extend(chunk.ticks);
        }
        assert_eq!(all.len(), 1_200);
        assert!(all.windows(2).all(|w| w[0].time_ms + 1_000 == w[1].time_ms));
    }

    #[test]
    fn a_failing_server_is_an_error_not_an_empty_chunk() {
        struct Down;
        impl History for Down {
            fn bars(
                &self,
                _: i64,
                _: Period,
                _: i64,
                _: i64,
            ) -> impl Future<Output = Result<Vec<Bar>>> + Send {
                std::future::ready(Err(ApiError::Closed))
            }
            fn ticks(
                &self,
                _: i64,
                _: i64,
                _: i64,
            ) -> impl Future<Output = Result<Vec<Tick>>> + Send {
                std::future::ready(Err(ApiError::Closed))
            }
        }
        assert!(run(tick_chunk_forward(&Down, 1, 0, 1_000, 500)).is_err());
    }
}
