//! The replay session: one cursor in time that every chart follows, the history it needs, and the
//! transport (play, step, speed). Nothing here is saved: a replay ends with the app.
//!
//! Charts read the bars from the stores and write what the cursor shows into their own series;
//! this entity never touches a chart, and never the live prices, the orders or the alerts.

mod loading;
mod plan;
mod store;
mod tick_loading;
mod ticks;
mod transport;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::system::runtime;
use gpui::{Context, EventEmitter};
use wyck_domain::chart::{
    ClockStep, ReplayClock, ReplayStep, replay_auto_step, replay_random_start,
};
use wyck_domain::market::Period;

use crate::broker::session::Session;
use crate::broker::{Error as ApiError, Result as ApiResult};
use crate::lost::lost;
use crate::market_data::load;
use crate::market_data::now_ms;
use crate::market_data::replay_load::{self, Chunk};
use crate::market_data::replay_ticks::TickChunk;
pub use store::Store;
pub use ticks::{TickStore, TickSupport};

type Key = (i64, Period);

const FRAME: Duration = Duration::from_millis(33);
const RETRY_MS: i64 = 5_000;
const DAY_MS: i64 = 86_400_000;
/// The most a month-long period is asked for before the cursor: twenty years.
const MAX_BACK_MS: i64 = 7_300 * DAY_MS;
const MAX_LEAD_MS: i64 = 31 * DAY_MS;
/// Requests a replay keeps in flight: the history limit is shared with the charts.
const MAX_IN_FLIGHT: usize = 2;

/// What a chart is made of, so what the replay must hold for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemandKind {
    /// Server bars of a period (alone or grouped).
    Bars(Period),
    /// Bars of this many seconds, made of ticks.
    Seconds(u32),
    /// Tick by tick.
    Ticks,
}

/// What a chart needs the replay to hold for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Demand {
    pub symbol_id: i64,
    pub kind: DemandKind,
    /// The length of one bar of the chart, to pick the automatic step (none for a tick chart).
    pub bar_ms: Option<i64>,
    /// How many bars before the cursor the chart wants to see.
    pub back_bars: usize,
}

impl Demand {
    /// The server period of the bars the chart is made of, if it has any.
    #[must_use]
    pub fn period(&self) -> Option<Period> {
        match self.kind {
            DemandKind::Bars(period) => Some(period),
            DemandKind::Seconds(_) | DemandKind::Ticks => None,
        }
    }
}

/// What the replay builds the candle in progress from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TickMode {
    /// Real ticks where the broker has them, minute bars where it has not.
    #[default]
    Auto,
    /// Minute bars always, along a synthetic path inside the minute.
    Minutes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Off,
    /// Waiting for the user to click the bar to start from.
    Picking,
    /// Looking for the start, or loading what is needed around it.
    Preparing,
    Active,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayEvent {
    /// The user is choosing the start on a chart.
    Picking,
    /// The pick was cancelled.
    PickCancelled,
    /// A replay began: charts leave the live data and show the past.
    Entered,
    /// The data around the start is held: playing can begin.
    Ready,
    /// The cursor moved.
    Cursor,
    /// More bars are held, or the demand changed.
    DataChanged,
    /// Playing caught up with the present and stopped.
    ReachedNow,
    /// The replay ended.
    Exited,
    /// The replay could not start or went on; the text is for the user.
    Failed(String),
}

enum Start {
    First,
    Random,
}

enum Done {
    Before(Chunk),
    Forward(Chunk),
}

enum TickDone {
    Before(TickChunk),
    Forward(TickChunk),
}

pub struct Replay {
    session: Session,
    phase: Phase,
    clock: Option<ReplayClock>,
    start_ms: i64,
    step: ReplayStep,
    rate_hz: f64,
    skip_gaps: bool,
    future_drawings: bool,
    buffering: bool,
    demand: Vec<Demand>,
    stores: HashMap<Key, Store>,
    inflight: HashSet<Key>,
    blocked: HashMap<Key, i64>,
    pending: Option<Start>,
    tick_mode: TickMode,
    tick_stores: HashMap<i64, TickStore>,
    tick_support: HashMap<i64, TickSupport>,
    tick_inflight: HashSet<i64>,
    tick_blocked: HashMap<i64, i64>,
    /// Bumped when the replay starts over, so a late answer is recognized and dropped.
    epoch: u64,
    /// Bumped when playing stops or restarts, so the timer of an old play ends.
    ticker: u64,
}

impl EventEmitter<ReplayEvent> for Replay {}

impl Replay {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            phase: Phase::Off,
            clock: None,
            start_ms: 0,
            step: ReplayStep::Auto,
            rate_hz: 3.0,
            skip_gaps: true,
            future_drawings: false,
            buffering: false,
            demand: Vec::new(),
            stores: HashMap::new(),
            inflight: HashSet::new(),
            blocked: HashMap::new(),
            pending: None,
            tick_mode: TickMode::Auto,
            tick_stores: HashMap::new(),
            tick_support: HashMap::new(),
            tick_inflight: HashSet::new(),
            tick_blocked: HashMap::new(),
            epoch: 0,
            ticker: 0,
        }
    }

    // ---- What charts and the toolbar read.

    #[must_use]
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Whether a replay is under way, loading or running.
    #[must_use]
    pub fn is_on(&self) -> bool {
        matches!(self.phase, Phase::Preparing | Phase::Active)
    }

    #[must_use]
    pub fn is_picking(&self) -> bool {
        self.phase == Phase::Picking
    }

    /// The replay time in Unix milliseconds: what happened before it is known.
    #[must_use]
    pub fn cursor_ms(&self) -> Option<i64> {
        self.clock.as_ref().map(ReplayClock::cursor_ms)
    }

    #[must_use]
    pub fn start_ms(&self) -> i64 {
        self.start_ms
    }

    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.clock.as_ref().is_some_and(ReplayClock::is_playing)
    }

    /// Whether playing waits for history to arrive.
    #[must_use]
    pub fn is_buffering(&self) -> bool {
        self.buffering
    }

    #[must_use]
    pub fn rate_hz(&self) -> f64 {
        self.rate_hz
    }

    #[must_use]
    pub fn step(&self) -> ReplayStep {
        self.step
    }

    /// The market time one update covers, as far as it is a time: the length of a step in time, the
    /// shortest chart for Auto, a second for a step in ticks.
    #[must_use]
    pub fn step_ms(&self) -> i64 {
        self.step.millis().unwrap_or_else(|| match self.step {
            ReplayStep::Ticks(_) => 1_000,
            _ => replay_auto_step(
                self.demand.iter().filter_map(|demand| demand.bar_ms),
                self.auto_floor_ms(),
            ),
        })
    }

    /// How the clock takes a step. A step in ticks needs real ticks; without them it is the time
    /// step Auto would take.
    #[must_use]
    pub fn clock_step(&self) -> ClockStep {
        let ticks = self.ticks_available();
        match self.step {
            ReplayStep::Ticks(count) if ticks => ClockStep::Ticks(count),
            ReplayStep::Auto if ticks && self.has_tick_chart() => ClockStep::Ticks(1),
            _ => ClockStep::time(self.step_ms()),
        }
    }

    /// The shortest step Auto may take: a minute while minute bars are the finest data, a second
    /// when there are ticks.
    fn auto_floor_ms(&self) -> i64 {
        if self.ticks_available() {
            1_000
        } else {
            60_000
        }
    }

    fn has_tick_chart(&self) -> bool {
        self.demand
            .iter()
            .any(|demand| demand.kind == DemandKind::Ticks)
    }

    // ---- The ticks.

    #[must_use]
    pub fn tick_mode(&self) -> TickMode {
        self.tick_mode
    }

    /// Whether the candle in progress of this symbol is made of real ticks: the user lets the
    /// replay use them, and the broker has them where the replay is.
    #[must_use]
    pub fn symbol_uses_ticks(&self, symbol_id: i64) -> bool {
        self.tick_mode == TickMode::Auto
            && self.tick_support.get(&symbol_id) == Some(&TickSupport::Available)
    }

    /// Whether the broker has ticks for this symbol where the replay is (see [`TickSupport`]).
    #[must_use]
    pub fn tick_support(&self, symbol_id: i64) -> TickSupport {
        self.tick_support
            .get(&symbol_id)
            .copied()
            .unwrap_or_default()
    }

    /// Whether real ticks are held for some chart, so a step in ticks means something.
    #[must_use]
    pub fn ticks_available(&self) -> bool {
        self.tick_stores
            .iter()
            .any(|(symbol, store)| self.symbol_uses_ticks(*symbol) && !store.ticks().is_empty())
    }

    /// The ticks of a symbol, when it uses them (see [`Self::symbol_uses_ticks`]).
    #[must_use]
    pub fn tick_store(&self, symbol_id: i64) -> Option<&TickStore> {
        self.tick_stores
            .get(&symbol_id)
            .filter(|_| self.symbol_uses_ticks(symbol_id))
    }

    /// The ticks of a symbol, when they reach from the minute the cursor is in up to the cursor,
    /// which is what the candle in progress needs to be made of them.
    #[must_use]
    pub fn ticks_for(&self, symbol_id: i64) -> Option<&TickStore> {
        let cursor = self.cursor_ms()?;
        self.tick_store(symbol_id)
            .filter(|store| store.covers(cursor))
    }

    #[must_use]
    pub fn skips_gaps(&self) -> bool {
        self.skip_gaps
    }

    /// Whether drawings that start after the cursor are shown.
    #[must_use]
    pub fn shows_future_drawings(&self) -> bool {
        self.future_drawings
    }

    #[must_use]
    pub fn store(&self, symbol_id: i64, period: Period) -> Option<&Store> {
        self.stores.get(&(symbol_id, period))
    }

    /// The minute bars of a symbol, which the candle in progress is built from. `None` when the
    /// symbol has none (yet), and a chart forms its candle along its own path instead.
    #[must_use]
    pub fn minutes(&self, symbol_id: i64) -> Option<&Store> {
        self.store(symbol_id, Period::M1)
            .filter(|store| !store.bars().is_empty())
    }

    // ---- Starting and ending.

    /// The user is about to click the bar to start from.
    pub fn begin_pick(&mut self, cx: &mut Context<Self>) {
        if self.phase != Phase::Off {
            return;
        }
        self.phase = Phase::Picking;
        cx.emit(ReplayEvent::Picking);
        cx.notify();
    }

    pub fn cancel_pick(&mut self, cx: &mut Context<Self>) {
        if self.phase == Phase::Picking {
            self.phase = Phase::Off;
            cx.emit(ReplayEvent::PickCancelled);
            cx.notify();
        }
    }

    /// Starts at `ms`: the bar that opens then is the first one hidden.
    pub fn start_at(&mut self, ms: i64, cx: &mut Context<Self>) {
        self.begin();
        let mut clock = ReplayClock::new(ms, self.clock_step());
        clock.set_rate(self.rate_hz, now_ms());
        self.clock = Some(clock);
        self.start_ms = ms;
        self.phase = Phase::Preparing;
        cx.emit(ReplayEvent::Entered);
        self.pump(cx);
        cx.notify();
    }

    /// Starts at the oldest bar every chart has.
    pub fn start_first(&mut self, cx: &mut Context<Self>) {
        self.find_start(Start::First, cx);
    }

    /// Starts at a random moment of the available history.
    pub fn start_random(&mut self, cx: &mut Context<Self>) {
        self.find_start(Start::Random, cx);
    }

    /// Ends the replay: charts go back to the live data.
    pub fn exit(&mut self, cx: &mut Context<Self>) {
        let was = self.phase;
        self.begin();
        self.phase = Phase::Off;
        self.clock = None;
        if was != Phase::Off {
            cx.emit(ReplayEvent::Exited);
        }
        cx.notify();
    }

    fn fail(&mut self, message: &str, cx: &mut Context<Self>) {
        tracing::warn!(message, "replay stopped");
        let was = self.phase;
        self.begin();
        self.phase = Phase::Off;
        self.clock = None;
        if was != Phase::Off {
            cx.emit(ReplayEvent::Exited);
        }
        cx.emit(ReplayEvent::Failed(message.to_owned()));
        cx.notify();
    }

    /// Forgets everything about the run that ends: data, requests, timers.
    fn begin(&mut self) {
        self.epoch += 1;
        self.ticker += 1;
        self.stores.clear();
        self.inflight.clear();
        self.blocked.clear();
        self.tick_stores.clear();
        self.tick_support.clear();
        self.tick_inflight.clear();
        self.tick_blocked.clear();
        self.pending = None;
        self.buffering = false;
    }

    fn find_start(&mut self, kind: Start, cx: &mut Context<Self>) {
        let mut keys: Vec<Key> = self
            .demand
            .iter()
            .map(|demand| (demand.symbol_id, demand.period().unwrap_or(Period::M1)))
            .collect();
        keys.sort();
        keys.dedup();
        self.begin();
        self.clock = None;
        if keys.is_empty() {
            self.phase = Phase::Preparing;
            self.fail("There is no chart to replay.", cx);
            return;
        }
        self.phase = Phase::Preparing;
        self.pending = Some(kind);
        let (session, epoch) = (self.session.clone(), self.epoch);
        cx.spawn(async move |this, cx| {
            let found = runtime::spawn(async move {
                let market = load::market(&session)?;
                let now = now_ms();
                let mut found = Vec::new();
                for (symbol, period) in keys {
                    found.push(replay_load::earliest(&market, symbol, period, now).await?);
                }
                Ok(found)
            })
            .await;
            lost(
                "replay: find the start",
                this.update(cx, |this, cx| {
                    this.start_found(epoch, found.unwrap_or(Err(ApiError::Closed)), cx);
                }),
            );
        })
        .detach();
        cx.notify();
    }

    fn start_found(
        &mut self,
        epoch: u64,
        found: ApiResult<Vec<Option<i64>>>,
        cx: &mut Context<Self>,
    ) {
        if epoch != self.epoch {
            return;
        }
        let Some(kind) = self.pending.take() else {
            return;
        };
        let first = match found {
            Ok(found) => found.into_iter().flatten().max(),
            Err(error) => {
                tracing::warn!(%error, "could not look for the oldest bar");
                return self.fail("The history could not be read to start the replay.", cx);
            }
        };
        let Some(first) = first else {
            return self.fail("There is no history to replay.", cx);
        };
        let start = match kind {
            Start::First => first,
            Start::Random => {
                let now = now_ms();
                let last = plan::live_edge_ms(now) - DAY_MS;
                let seed = u64::try_from(now).unwrap_or_default();
                replay_random_start(seed, first, last, self.step_ms()).unwrap_or(first)
            }
        };
        self.start_at(start, cx);
    }
}
