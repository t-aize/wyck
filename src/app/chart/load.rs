//! Fetching history for the chart. These functions run on the tokio runtime.
//!
//! The server limits how long a range one request may cover (and truncates long answers), so a
//! range is always asked for in chunks that stay inside the documented limits. Every request goes
//! through the client's historical rate limit, so a deep load takes a moment but never trips the
//! server's frequency limit.

use wyck::openapi::market::{Bar, MAX_TICK_RANGE_MS, MarketClient, Period, QuoteType, Tick};
use wyck::openapi::session::Session;
use wyck::openapi::{OpenApiError, Result};

use super::data::{aggregate_ticks, bucket_start};
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
}

impl Loaded {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Bars(bars) => bars.is_empty(),
            Self::Ticks(ticks) => ticks.is_empty(),
        }
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
    let market = market(session)?;
    match timeframe {
        Timeframe::Bars(period) => Ok(Loaded::Bars(
            bars_before(&market, symbol_id, period, now_ms + 1, WANT_BARS).await?,
        )),
        _ => ticks_before(&market, symbol_id, timeframe, now_ms).await,
    }
}

/// History older than `oldest_ms`, the time of the oldest point held.
pub async fn older(
    session: &Session,
    symbol_id: i64,
    timeframe: Timeframe,
    oldest_ms: i64,
) -> Result<Loaded> {
    let market = market(session)?;
    match timeframe {
        Timeframe::Bars(period) => Ok(Loaded::Bars(
            bars_before(&market, symbol_id, period, oldest_ms, WANT_BARS).await?,
        )),
        Timeframe::Ticks => ticks_before(&market, symbol_id, timeframe, oldest_ms).await,
        Timeframe::Seconds(_) => ticks_before(&market, symbol_id, timeframe, oldest_ms - 1).await,
    }
}

/// What was missed between `from_ms` and `to_ms`, after a lost connection.
pub async fn gap(
    session: &Session,
    symbol_id: i64,
    timeframe: Timeframe,
    from_ms: i64,
    to_ms: i64,
) -> Result<Loaded> {
    let market = market(session)?;
    match timeframe {
        Timeframe::Bars(period) => Ok(Loaded::Bars(
            bars_forward(&market, symbol_id, period, from_ms, to_ms).await?,
        )),
        Timeframe::Ticks => {
            let ticks = market
                .ticks(symbol_id, QuoteType::Bid, from_ms, to_ms)
                .await?;
            Ok(Loaded::Ticks(ticks))
        }
        Timeframe::Seconds(seconds) => {
            let bucket = i64::from(seconds) * 1_000;
            let from = bucket_start(from_ms, bucket);
            let ticks = market.ticks(symbol_id, QuoteType::Bid, from, to_ms).await?;
            Ok(Loaded::Bars(aggregate_ticks(&ticks, bucket)))
        }
    }
}

/// Bars in `[from_ms, to_ms]`, oldest first, asked for in chunks the server accepts.
async fn bars_forward(
    market: &MarketClient,
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
    market: &MarketClient,
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
    market: &MarketClient,
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
        let ticks = market.ticks(symbol_id, QuoteType::Bid, from, to).await?;
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
