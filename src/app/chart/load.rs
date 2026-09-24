//! Fetching history for the chart. These functions run on the tokio runtime.
//!
//! The server limits how long a range one request may cover (and truncates long answers), so a
//! range is always asked for in chunks that stay inside the documented limits. Every request goes
//! through the client's historical rate limit, so a deep load takes a moment but never trips the
//! server's frequency limit.
//!
//! The rules (how far back to walk, when to give up, how ticks become bars) work on any
//! [`History`], so they are tested with a fake server; the app gives them the real client.

use std::future::Future;

use wyck::openapi::market::{Bar, MAX_TICK_RANGE_MS, MarketClient, Period, QuoteType, Tick};
use wyck::openapi::session::Session;
use wyck::openapi::{OpenApiError, Result};

use super::data::{aggregate_ticks, bucket_start, group_bars, last_group};
use super::timeframe::Timeframe;

/// Bars in one request. Under the range limit the server sets for every period.
const CHUNK_BARS: i64 = 1_500;
/// How many bars a first load, and each older step, aims for.
pub const WANT_BARS: usize = 1_200;
/// Requests one call may make: a guard, not a target.
const MAX_CHUNKS: usize = 40;
/// Empty chunks in a row that mean the history has begun (or the market is closed for long).
const MAX_EMPTY_RUN: usize = 6;

pub enum Loaded {
    Bars(Vec<Bar>),
    Ticks(Vec<Tick>),
    /// Bars of a grouped timeframe, and the server bars of the newest group, which live bars
    /// join.
    Grouped {
        bars: Vec<Bar>,
        tail: Vec<Bar>,
    },
}

impl Loaded {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Bars(bars) | Self::Grouped { bars, .. } => bars.is_empty(),
            Self::Ticks(ticks) => ticks.is_empty(),
        }
    }
}

/// How many server bars a grouped load asks for: enough for [`WANT_BARS`] groups, within reason.
fn want_base(timeframe: Timeframe) -> usize {
    (WANT_BARS * timeframe.group_size() as usize).min(12_000)
}

/// Groups server bars fetched going back. The oldest group may lack its first bars, which the
/// next step back holds, so it is left for that step.
fn grouped_back(base: &[Bar], timeframe: Timeframe) -> Loaded {
    let mut bars = group_bars(base, timeframe);
    if bars.len() > 1 {
        bars.remove(0);
    }
    Loaded::Grouped {
        bars,
        tail: last_group(base, timeframe),
    }
}

/// Where history comes from: the server's bars and ticks for a range.
pub trait History: Sync {
    fn bars(
        &self,
        symbol_id: i64,
        period: Period,
        from_ms: i64,
        to_ms: i64,
    ) -> impl Future<Output = Result<Vec<Bar>>> + Send;

    fn ticks(
        &self,
        symbol_id: i64,
        from_ms: i64,
        to_ms: i64,
    ) -> impl Future<Output = Result<Vec<Tick>>> + Send;
}

impl History for MarketClient {
    fn bars(
        &self,
        symbol_id: i64,
        period: Period,
        from_ms: i64,
        to_ms: i64,
    ) -> impl Future<Output = Result<Vec<Bar>>> + Send {
        MarketClient::bars(self, symbol_id, period, from_ms, to_ms)
    }

    fn ticks(
        &self,
        symbol_id: i64,
        from_ms: i64,
        to_ms: i64,
    ) -> impl Future<Output = Result<Vec<Tick>>> + Send {
        MarketClient::ticks(self, symbol_id, QuoteType::Bid, from_ms, to_ms)
    }
}

fn market(session: &Session) -> Result<MarketClient> {
    let client = session.client().ok_or(OpenApiError::Closed)?;
    Ok(client.account(session.account_id()).market())
}

/// The newest history: enough to fill a screen and some to scroll back into.
pub async fn initial(
    session: &Session,
    symbol_id: i64,
    timeframe: Timeframe,
    now_ms: i64,
) -> Result<Loaded> {
    initial_from(&market(session)?, symbol_id, timeframe, now_ms).await
}

/// History older than `oldest_ms`, the time of the oldest point held.
pub async fn older(
    session: &Session,
    symbol_id: i64,
    timeframe: Timeframe,
    oldest_ms: i64,
) -> Result<Loaded> {
    older_from(&market(session)?, symbol_id, timeframe, oldest_ms).await
}

/// What was missed between `from_ms` and `to_ms`, after a lost connection.
pub async fn gap(
    session: &Session,
    symbol_id: i64,
    timeframe: Timeframe,
    from_ms: i64,
    to_ms: i64,
) -> Result<Loaded> {
    gap_from(&market(session)?, symbol_id, timeframe, from_ms, to_ms).await
}

/// [`initial`], from any source.
pub async fn initial_from(
    market: &impl History,
    symbol_id: i64,
    timeframe: Timeframe,
    now_ms: i64,
) -> Result<Loaded> {
    match timeframe {
        Timeframe::Bars(period) => Ok(Loaded::Bars(
            bars_before(market, symbol_id, period, now_ms + 1, WANT_BARS).await?,
        )),
        Timeframe::Multiple(period, _) => {
            let want = want_base(timeframe);
            let base = bars_before(market, symbol_id, period, now_ms + 1, want).await?;
            Ok(grouped_back(&base, timeframe))
        }
        Timeframe::Ticks | Timeframe::Seconds(_) => {
            ticks_before(market, symbol_id, timeframe, now_ms).await
        }
    }
}

/// [`older`], from any source.
pub async fn older_from(
    market: &impl History,
    symbol_id: i64,
    timeframe: Timeframe,
    oldest_ms: i64,
) -> Result<Loaded> {
    match timeframe {
        Timeframe::Bars(period) => Ok(Loaded::Bars(
            bars_before(market, symbol_id, period, oldest_ms, WANT_BARS).await?,
        )),
        Timeframe::Multiple(period, _) => {
            let want = want_base(timeframe);
            let base = bars_before(market, symbol_id, period, oldest_ms, want).await?;
            Ok(grouped_back(&base, timeframe))
        }
        Timeframe::Ticks => ticks_before(market, symbol_id, timeframe, oldest_ms).await,
        Timeframe::Seconds(_) => ticks_before(market, symbol_id, timeframe, oldest_ms - 1).await,
    }
}

/// [`gap`], from any source.
pub async fn gap_from(
    market: &impl History,
    symbol_id: i64,
    timeframe: Timeframe,
    from_ms: i64,
    to_ms: i64,
) -> Result<Loaded> {
    match timeframe {
        Timeframe::Bars(period) => Ok(Loaded::Bars(
            bars_forward(market, symbol_id, period, from_ms, to_ms).await?,
        )),
        // `from_ms` is where the newest group held opens, so every group fetched is whole.
        Timeframe::Multiple(period, _) => {
            let base = bars_forward(market, symbol_id, period, from_ms, to_ms).await?;
            Ok(Loaded::Grouped {
                bars: group_bars(&base, timeframe),
                tail: last_group(&base, timeframe),
            })
        }
        Timeframe::Ticks => Ok(Loaded::Ticks(
            market.ticks(symbol_id, from_ms, to_ms).await?,
        )),
        Timeframe::Seconds(seconds) => {
            let bucket = i64::from(seconds) * 1_000;
            let from = bucket_start(from_ms, bucket);
            let ticks = market.ticks(symbol_id, from, to_ms).await?;
            Ok(Loaded::Bars(aggregate_ticks(&ticks, bucket)))
        }
    }
}

/// Bars in `[from_ms, to_ms]`, oldest first, asked for in chunks the server accepts.
async fn bars_forward(
    market: &impl History,
    symbol_id: i64,
    period: Period,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<Bar>> {
    let span = CHUNK_BARS * period.millis();
    let mut all: Vec<Bar> = Vec::new();
    let mut from = from_ms;
    for _ in 0..MAX_CHUNKS {
        if from > to_ms {
            break;
        }
        let to = (from + span - 1).min(to_ms);
        all.extend(market.bars(symbol_id, period, from, to).await?);
        from = to + 1;
    }
    all.sort_by_key(|b| b.time_ms);
    all.dedup_by_key(|b| b.time_ms);
    Ok(all)
}

/// About `want` bars that open before `before_ms`, the newest of them last. Walks back a chunk at
/// a time and stops when it has enough, when the history runs out, or after a long empty stretch.
async fn bars_before(
    market: &impl History,
    symbol_id: i64,
    period: Period,
    before_ms: i64,
    want: usize,
) -> Result<Vec<Bar>> {
    let span = CHUNK_BARS * period.millis();
    let mut all: Vec<Bar> = Vec::new();
    let mut to = before_ms - 1;
    let mut empty_run = 0;
    for _ in 0..MAX_CHUNKS {
        if to < 0 || all.len() >= want || empty_run >= MAX_EMPTY_RUN {
            break;
        }
        let from = (to - span + 1).max(0);
        let mut chunk = market.bars(symbol_id, period, from, to).await?;
        if chunk.is_empty() {
            empty_run += 1;
        } else {
            empty_run = 0;
            chunk.append(&mut all);
            all = chunk;
        }
        to = from - 1;
    }
    Ok(all)
}

/// Bid ticks before `before_ms` (as bars of the timeframe, or as they are for tick by tick).
/// The window doubles while it finds nothing, so a weekend does not end the search.
async fn ticks_before(
    market: &impl History,
    symbol_id: i64,
    timeframe: Timeframe,
    before_ms: i64,
) -> Result<Loaded> {
    let mut window = timeframe.tick_span_ms();
    let bucket = timeframe.bar_ms();
    let mut to = before_ms;
    for _ in 0..12 {
        if to < 0 {
            break;
        }
        let mut from = (to - window + 1).max(0);
        if let Some(bucket) = bucket {
            from = bucket_start(from, bucket);
        }
        let ticks = market.ticks(symbol_id, from, to).await?;
        if !ticks.is_empty() {
            return Ok(match bucket {
                Some(bucket) => Loaded::Bars(aggregate_ticks(&ticks, bucket)),
                None => Loaded::Ticks(ticks),
            });
        }
        to = from - 1;
        window = (window * 2).min(MAX_TICK_RANGE_MS - 60_000);
    }
    Ok(match bucket {
        Some(_) => Loaded::Bars(Vec::new()),
        None => Loaded::Ticks(Vec::new()),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    const MINUTE: i64 = 60_000;
    /// 2026-01-05 00:00 UTC.
    const NOW: i64 = 1_767_571_200_000;

    /// A fake server: bars every minute from `start` to `end`, ticks every second in the same
    /// span, and a log of every request.
    struct Fake {
        start: i64,
        end: i64,
        requests: Mutex<Vec<(i64, i64)>>,
        fail: bool,
    }

    impl Fake {
        fn new(start: i64, end: i64) -> Self {
            Self {
                start,
                end,
                requests: Mutex::new(Vec::new()),
                fail: false,
            }
        }

        fn count(&self) -> usize {
            self.requests.lock().unwrap().len()
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
            self.requests.lock().unwrap().push((from_ms, to_ms));
            let step = period.millis();
            let (start, end, fail) = (self.start, self.end, self.fail);
            async move {
                if fail {
                    return Err(OpenApiError::Closed);
                }
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
        ) -> impl Future<Output = Result<Vec<Tick>>> + Send {
            self.requests.lock().unwrap().push((from_ms, to_ms));
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
                        price: 100 + (t / 1_000) % 7,
                    })
                    .collect())
            }
        }
    }

    fn run<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn bars(loaded: Loaded) -> Vec<Bar> {
        match loaded {
            Loaded::Bars(bars) | Loaded::Grouped { bars, .. } => bars,
            Loaded::Ticks(_) => panic!("ticks where bars were expected"),
        }
    }

    #[test]
    fn a_first_load_fills_a_screen_newest_last_in_chunks_the_server_accepts() {
        let fake = Fake::new(NOW - 30 * 86_400_000, NOW);
        let loaded = run(initial_from(&fake, 1, Timeframe::Bars(Period::M1), NOW)).unwrap();
        let bars = bars(loaded);
        assert!(bars.len() >= WANT_BARS, "{}", bars.len());
        assert!(bars.windows(2).all(|w| w[0].time_ms < w[1].time_ms));
        assert_eq!(bars.last().unwrap().time_ms, NOW);
        assert!(fake.count() <= MAX_CHUNKS);
    }

    #[test]
    fn walking_back_stops_where_the_history_begins() {
        // Only 100 bars exist: the walk gives up after a run of empty chunks.
        let fake = Fake::new(NOW - 99 * MINUTE, NOW);
        let bars = bars(run(initial_from(&fake, 1, Timeframe::Bars(Period::M1), NOW)).unwrap());
        assert_eq!(bars.len(), 100);
        assert!(
            fake.count() <= 1 + MAX_EMPTY_RUN,
            "{} requests",
            fake.count()
        );
    }

    #[test]
    fn older_history_ends_before_the_oldest_bar_held() {
        let fake = Fake::new(NOW - 30 * 86_400_000, NOW);
        let oldest = NOW - 2_000 * MINUTE;
        let bars = bars(run(older_from(&fake, 1, Timeframe::Bars(Period::M1), oldest)).unwrap());
        assert!(!bars.is_empty());
        assert!(bars.iter().all(|b| b.time_ms < oldest));
    }

    #[test]
    fn a_gap_is_fetched_forward_in_order_without_repeats() {
        let fake = Fake::new(NOW - 30 * 86_400_000, NOW);
        let from = NOW - 3_500 * MINUTE;
        let bars = bars(run(gap_from(&fake, 1, Timeframe::Bars(Period::M1), from, NOW)).unwrap());
        assert_eq!(bars.first().unwrap().time_ms, from);
        assert_eq!(bars.last().unwrap().time_ms, NOW);
        assert_eq!(bars.len(), 3_501);
        assert!(fake.count() >= 3, "a long gap takes several chunks");
    }

    #[test]
    fn second_bars_are_built_from_ticks_and_cover_whole_buckets() {
        let fake = Fake::new(NOW - 3_600_000, NOW);
        let from = NOW - 95_500;
        let loaded = run(gap_from(&fake, 1, Timeframe::Seconds(15), from, NOW)).unwrap();
        let bars = bars(loaded);
        assert!(bars.iter().all(|b| b.time_ms % 15_000 == 0));
        // The first bucket starts on its boundary, before the gap began.
        assert!(bars[0].time_ms <= from);
        assert!(bars.iter().all(|b| b.low <= b.high && b.volume > 0));
    }

    #[test]
    fn tick_history_widens_its_window_over_a_quiet_stretch() {
        // The last tick is two hours before "now": a first 10 minute window is empty.
        let fake = Fake::new(NOW - 3 * 3_600_000, NOW - 2 * 3_600_000);
        let loaded = run(initial_from(&fake, 1, Timeframe::Ticks, NOW)).unwrap();
        let Loaded::Ticks(ticks) = loaded else {
            panic!("bars where ticks were expected");
        };
        assert!(!ticks.is_empty());
        assert!(fake.count() > 1, "it looked further back");
        assert!(ticks.last().unwrap().time_ms <= NOW - 2 * 3_600_000);
    }

    #[test]
    fn a_grouped_timeframe_loads_whole_groups_of_server_bars() {
        let fake = Fake::new(NOW - 30 * 86_400_000, NOW);
        let m7 = Timeframe::from_code("M7").unwrap();
        let loaded = run(initial_from(&fake, 1, m7, NOW)).unwrap();
        let Loaded::Grouped {
            bars: grouped,
            tail,
        } = loaded
        else {
            panic!("grouped bars expected");
        };
        assert!(grouped.len() >= WANT_BARS - 1, "{}", grouped.len());
        assert!(grouped.windows(2).all(|w| w[0].time_ms < w[1].time_ms));
        // Every group but the newest holds its seven minutes, or the five that end a day.
        assert!(grouped[..grouped.len() - 1].iter().all(
            |b| b.volume == 7 || (b.volume == 5 && (b.time_ms + 5 * MINUTE) % 86_400_000 == 0)
        ));
        assert!(!tail.is_empty() && tail.len() <= 7);
        assert_eq!(tail.last().unwrap().time_ms, NOW);
        let older = bars(run(older_from(&fake, 1, m7, grouped[0].time_ms)).unwrap());
        assert!(!older.is_empty());
        assert!(older.iter().all(|b| b.time_ms < grouped[0].time_ms));
    }

    #[test]
    fn an_error_from_the_server_is_passed_on() {
        let mut fake = Fake::new(NOW - 86_400_000, NOW);
        fake.fail = true;
        assert!(run(initial_from(&fake, 1, Timeframe::Bars(Period::H1), NOW)).is_err());
    }
}
