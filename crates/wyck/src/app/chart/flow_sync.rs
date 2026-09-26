//! Getting the order flow of a footprint chart: the newest bars first, older ones as the user
//! scrolls back, the gap after a lost connection, and the live quotes.
//!
//! Nothing here runs unless the chart is a footprint on a timeframe that supports it. The
//! flow is built from the bid and the ask ticks of the range (two requests, joined into quotes),
//! a few dozen bars at a time, so what is on screen fills in quickly and a long history is only
//! asked for when it is looked at.

use std::time::{Duration, Instant};

use gpui::Context;
use wyck_openapi::Result as ApiResult;
use wyck_openapi::market::{Quote, QuoteType, merge_sides};
use wyck_openapi::session::Session;

use super::data::Series;
use super::flow::{self, Flow};
use super::footprint;
use super::live::LiveUpdate;
use super::load;
use super::settings::ChartKind;
use super::{Chart, Load, flatten, now_ms};
use crate::app::runtime;

/// About how much time one request covers, in bars of the timeframe (at most [`MAX_CHUNK_BARS`]).
const CHUNK_MS: i64 = 6 * 3_600_000;
const MAX_CHUNK_BARS: i64 = 60;
/// A pause before asking again after a request failed.
const RETRY: Duration = Duration::from_secs(5);
/// A gap longer than this is not patched: the flow starts again.
const MAX_GAP_MS: i64 = 12 * 3_600_000;
/// The most live quotes kept while the newest history is on its way.
const MAX_HELD: usize = 20_000;

/// The order flow of a chart: nothing to do, a request in flight, or a pause after one that
/// failed.
pub(super) enum FlowLoad {
    Idle,
    Loading,
    Retry(Instant),
}

/// A live quote that arrived before the history it follows.
#[derive(Debug, Clone, Copy)]
pub(super) struct HeldQuote {
    time_ms: i64,
    bid: Option<i64>,
    ask: Option<i64>,
}

/// What a request was for, so its answer can be put in place.
struct Chunk {
    epoch: u64,
    flow_epoch: u64,
    /// The open times of the bars the quotes fall in.
    times: Vec<i64>,
    bar_ms: Option<i64>,
    /// The open time of the oldest bar the request makes complete.
    covers: i64,
    /// Whether it reaches the present (the flow carries on from its end).
    newest: bool,
}

/// The two sides of the ticks of `[from_ms, to_ms]`, joined into quotes.
async fn fetch_quotes(
    session: &Session,
    symbol_id: i64,
    from_ms: i64,
    to_ms: i64,
) -> ApiResult<Vec<Quote>> {
    let market = load::market(session)?;
    let (bids, asks) = tokio::try_join!(
        market.ticks(symbol_id, QuoteType::Bid, from_ms, to_ms),
        market.ticks(symbol_id, QuoteType::Ask, from_ms, to_ms),
    )?;
    Ok(merge_sides(&bids, &asks))
}

impl Chart {
    /// Whether the chart is a footprint that has what it needs to load the flow.
    pub(super) fn wants_flow(&self) -> bool {
        self.settings.kind == ChartKind::Footprint
            && footprint::supports(self.timeframe)
            && matches!(self.load, Load::Ready)
            && self.symbol.is_some()
    }

    /// Whether the flow of a footprint is still on its way in (nothing is shown for it yet, or
    /// a request is in flight).
    pub(super) fn flow_busy(&self) -> bool {
        self.wants_flow() && (matches!(self.flow_load, FlowLoad::Loading) || !self.flow.is_loaded())
    }

    /// Throws the flow away (the symbol, the timeframe or the type changed).
    pub(super) fn reset_flow(&mut self) {
        self.flow = Flow::default();
        self.flow_load = FlowLoad::Idle;
        self.flow_held.clear();
        self.flow_newest_pending = false;
        self.flow_gap = false;
        self.flow_epoch += 1;
    }

    /// The connection came back: the quotes missed since the last one are fetched.
    pub(super) fn flow_gap_open(&mut self) {
        if self.flow.is_loaded() {
            self.flow_gap = true;
        }
    }

    /// Asks for the flow the screen needs and does not have yet, one step at a time; the answer
    /// comes back here to ask for the next step. Cheap to call: it returns at once when there is
    /// nothing to do.
    pub(super) fn ensure_flow(&mut self, cx: &mut Context<Self>) {
        if !self.wants_flow() {
            return;
        }
        match self.flow_load {
            FlowLoad::Idle => {}
            FlowLoad::Retry(at) if Instant::now() >= at => {}
            _ => return,
        }
        let Some(plan) = self.plan_flow() else {
            return;
        };
        let Some(symbol) = &self.symbol else { return };
        self.flow_load = FlowLoad::Loading;
        self.flow_newest_pending = plan.chunk.newest;
        cx.notify();
        let (session, id) = (self.session.clone(), symbol.id);
        let (from, to) = (plan.from, plan.to);
        let chunk = plan.chunk;
        cx.spawn(async move |this, cx| {
            let result =
                runtime::spawn(async move { fetch_quotes(&session, id, from, to).await }).await;
            let _ = this.update(cx, |this, cx| {
                this.flow_loaded(chunk, flatten(result), cx);
            });
        })
        .detach();
    }

    /// The next request: the newest bars when nothing is held, the gap after a lost connection,
    /// or the bars before the oldest held when the screen reaches back further.
    fn plan_flow(&mut self) -> Option<Plan> {
        let now = now_ms();
        // A gap this long is not patched: the flow starts again from the newest bars.
        if self.flow.is_loaded() && self.flow_gap && now - self.flow.last_ms() > MAX_GAP_MS {
            self.reset_flow();
        }
        let Series::Bars(bars) = &self.series else {
            return None;
        };
        let bar_ms = self.timeframe.bar_ms();
        let per_chunk = bar_ms.map_or(MAX_CHUNK_BARS, |ms| {
            (CHUNK_MS / ms.max(1)).clamp(1, MAX_CHUNK_BARS)
        }) as usize;
        // The oldest bar the screen (and half a screen more) needs.
        let plot_w = self.geometry().plot_w();
        let (first, _) = self.view.visible(bars.len(), plot_w);
        let span = self.view.span(plot_w).ceil() as usize;
        let want = first.saturating_sub(span / 2 + 2);
        let gap_after = (self.flow.is_loaded() && self.flow_gap).then(|| self.flow.last_ms());
        let held = flow::Held {
            covered: self.flow.covered_from(),
            len: self.flow.len(),
            gap_after,
        };
        let step = flow::next_step(bars, |b| b.time_ms, held, want, per_chunk, now)?;
        self.flow_gap = false;
        Some(Plan {
            from: step.from,
            to: step.to,
            chunk: Chunk {
                epoch: self.epoch,
                flow_epoch: self.flow_epoch,
                times: bars[step.start..step.end]
                    .iter()
                    .map(|b| b.time_ms)
                    .collect(),
                bar_ms,
                covers: step.covers,
                newest: step.newest,
            },
        })
    }

    fn flow_loaded(&mut self, chunk: Chunk, result: ApiResult<Vec<Quote>>, cx: &mut Context<Self>) {
        if chunk.epoch != self.epoch || chunk.flow_epoch != self.flow_epoch {
            return;
        }
        if chunk.newest {
            self.flow_newest_pending = false;
        }
        match result {
            Ok(quotes) => {
                self.flow
                    .ingest(&chunk.times, chunk.bar_ms, &quotes, chunk.newest);
                self.flow.cover_from(chunk.covers);
                self.flow_load = FlowLoad::Idle;
                if chunk.newest {
                    // What arrived while the history was on its way follows it.
                    for held in std::mem::take(&mut self.flow_held) {
                        if held.time_ms > self.flow.last_ms() {
                            self.count_live(held);
                        }
                    }
                }
                cx.notify();
                self.ensure_flow(cx);
            }
            Err(error) => {
                tracing::warn!(%error, "could not load the order flow");
                if chunk.newest && self.flow.is_loaded() {
                    // The gap is still open: try it again.
                    self.flow_gap = true;
                }
                self.flow_load = FlowLoad::Retry(Instant::now() + RETRY);
                cx.notify();
            }
        }
    }

    /// A live price event: counted into the flow, or held until the history it follows is in.
    pub(super) fn feed_flow(&mut self, update: &LiveUpdate) {
        if !self.wants_flow() || (update.bid.is_none() && update.ask.is_none()) {
            return;
        }
        let quote = HeldQuote {
            time_ms: update.timestamp.unwrap_or_else(now_ms),
            bid: update.bid,
            ask: update.ask,
        };
        if !self.flow.is_loaded() || self.flow_newest_pending {
            if self.flow_held.len() < MAX_HELD {
                self.flow_held.push(quote);
            }
            return;
        }
        self.count_live(quote);
    }

    /// Counts a live quote into the bar it belongs to.
    fn count_live(&mut self, quote: HeldQuote) {
        let Series::Bars(bars) = &self.series else {
            return;
        };
        let time_ms = quote.time_ms.max(self.flow.last_ms());
        // The newest bars are enough to place it.
        let tail: Vec<i64> = bars.iter().rev().take(8).rev().map(|b| b.time_ms).collect();
        if let Some(open) = flow::bar_open(&tail, self.timeframe.bar_ms(), time_ms) {
            self.flow.push_live(open, time_ms, quote.bid, quote.ask);
        }
    }
}

/// One request to make.
struct Plan {
    from: i64,
    to: i64,
    chunk: Chunk,
}
