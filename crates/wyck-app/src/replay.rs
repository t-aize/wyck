//! The replay session: one cursor in time that every chart follows, the history it needs, and the
//! transport (play, step, speed). Nothing here is saved: a replay ends with the app.
//!
//! Charts read the bars from the stores and write what the cursor shows into their own series;
//! this entity never touches a chart, and never the live prices, the orders or the alerts.

mod loading;
mod plan;
mod store;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui::{Context, EventEmitter};
use wyck_chart::{
    ReplayBounds, ReplayClock, ReplayNoGaps, ReplayStep, replay_auto_step, replay_random_start,
};
use wyck_market::Period;
use wyck_platform::runtime;

use crate::broker::session::Session;
use crate::broker::{Error as ApiError, Result as ApiResult};
use crate::lost::lost;
use crate::market_data::load;
use crate::market_data::now_ms;
use crate::market_data::replay_load::{self, Chunk};
use store::DataTimes;
pub use store::Store;

type Key = (i64, Period);

const FRAME: Duration = Duration::from_millis(33);
const RETRY_MS: i64 = 5_000;
const DAY_MS: i64 = 86_400_000;
/// The most a month-long period is asked for before the cursor: twenty years.
const MAX_BACK_MS: i64 = 7_300 * DAY_MS;
const MAX_LEAD_MS: i64 = 31 * DAY_MS;
/// Requests a replay keeps in flight: the history limit is shared with the charts.
const MAX_IN_FLIGHT: usize = 2;

/// What a chart needs the replay to hold for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Demand {
    pub symbol_id: i64,
    /// The server period the chart's bars are made of.
    pub period: Period,
    /// The length of one bar of the chart, to pick the automatic step.
    pub bar_ms: i64,
    /// How many bars before the cursor the chart wants to see.
    pub back_bars: usize,
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

    /// The market time one update covers.
    #[must_use]
    pub fn step_ms(&self) -> i64 {
        self.step
            .millis()
            .unwrap_or_else(|| replay_auto_step(self.demand.iter().map(|demand| demand.bar_ms)))
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
        let mut clock = ReplayClock::new(ms, self.step_ms());
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
        self.pending = None;
        self.buffering = false;
    }

    fn find_start(&mut self, kind: Start, cx: &mut Context<Self>) {
        let mut keys: Vec<Key> = self
            .demand
            .iter()
            .map(|demand| (demand.symbol_id, demand.period))
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

    // ---- The transport.

    pub fn play(&mut self, cx: &mut Context<Self>) {
        if self.phase != Phase::Active {
            return;
        }
        if let Some(clock) = &mut self.clock {
            clock.play(now_ms());
        }
        self.buffering = false;
        self.ticker += 1;
        let id = self.ticker;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(FRAME).await;
                match this.update(cx, |this, cx| this.tick(id, cx)) {
                    Ok(true) => {}
                    _ => break,
                }
            }
        })
        .detach();
        cx.notify();
    }

    pub fn pause(&mut self, cx: &mut Context<Self>) {
        if let Some(clock) = &mut self.clock {
            clock.pause();
        }
        self.ticker += 1;
        self.buffering = false;
        cx.notify();
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.is_playing() {
            self.pause(cx);
        } else {
            self.play(cx);
        }
    }

    pub fn step_forward(&mut self, cx: &mut Context<Self>) {
        self.step_by_hand(true, cx);
    }

    pub fn step_back(&mut self, cx: &mut Context<Self>) {
        self.step_by_hand(false, cx);
    }

    fn step_by_hand(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.phase != Phase::Active {
            return;
        }
        let Some(mut clock) = self.clock.take() else {
            return;
        };
        let bounds = self.bounds(&clock, now_ms());
        let moved = if self.skip_gaps {
            let data = self.data_times();
            if forward {
                clock.step_forward(bounds, &data)
            } else {
                clock.step_back(bounds, &data)
            }
        } else if forward {
            clock.step_forward(bounds, &ReplayNoGaps)
        } else {
            clock.step_back(bounds, &ReplayNoGaps)
        };
        self.clock = Some(clock);
        self.ticker += 1;
        self.buffering = false;
        if moved {
            cx.emit(ReplayEvent::Cursor);
        }
        self.pump(cx);
        cx.notify();
    }

    /// Goes to `ms`: a jump inside the data held, or a new start when it is outside.
    pub fn go_to(&mut self, ms: i64, cx: &mut Context<Self>) {
        if self.phase == Phase::Active
            && let Some(mut clock) = self.clock.take()
        {
            let bounds = self.bounds(&clock, now_ms());
            if (bounds.floor_ms..=bounds.cap_ms).contains(&ms) {
                clock.seek(ms, bounds);
                self.clock = Some(clock);
                self.ticker += 1;
                self.buffering = false;
                cx.emit(ReplayEvent::Cursor);
                self.pump(cx);
                cx.notify();
                return;
            }
            self.clock = Some(clock);
        }
        self.start_at(ms, cx);
    }

    /// Back to the bar the replay started from.
    pub fn restart(&mut self, cx: &mut Context<Self>) {
        self.go_to(self.start_ms, cx);
    }

    pub fn set_rate(&mut self, rate_hz: f64, cx: &mut Context<Self>) {
        if let Some(clock) = &mut self.clock {
            clock.set_rate(rate_hz, now_ms());
            self.rate_hz = clock.rate_hz();
        } else {
            self.rate_hz = rate_hz.clamp(0.05, 60.0);
        }
        cx.notify();
    }

    pub fn set_step(&mut self, step: ReplayStep, cx: &mut Context<Self>) {
        self.step = step;
        self.refresh_step();
        self.pump(cx);
        cx.notify();
    }

    pub fn set_skip_gaps(&mut self, skip: bool, cx: &mut Context<Self>) {
        self.skip_gaps = skip;
        cx.notify();
    }

    pub fn set_future_drawings(&mut self, show: bool, cx: &mut Context<Self>) {
        self.future_drawings = show;
        cx.emit(ReplayEvent::DataChanged);
        cx.notify();
    }

    /// What the charts need held. Charts call it when they enter the replay and whenever their
    /// symbol or timeframe changes; the same demand twice is nothing.
    pub fn set_demand(&mut self, demand: Vec<Demand>, cx: &mut Context<Self>) {
        if demand == self.demand {
            return;
        }
        self.demand = demand;
        let wanted = self.wanted();
        self.stores.retain(|key, _| wanted.contains_key(key));
        self.refresh_step();
        self.pump(cx);
        cx.emit(ReplayEvent::DataChanged);
        cx.notify();
    }

    /// The connection is back: try again what failed.
    pub fn on_ready(&mut self, cx: &mut Context<Self>) {
        self.blocked.clear();
        self.pump(cx);
    }

    // ---- Playing.

    fn tick(&mut self, id: u64, cx: &mut Context<Self>) -> bool {
        if id != self.ticker {
            return false;
        }
        let Some(mut clock) = self.clock.take() else {
            return false;
        };
        let now = now_ms();
        let bounds = self.bounds(&clock, now);
        let advance = if self.skip_gaps {
            let data = self.data_times();
            clock.advance(now, bounds, &data)
        } else {
            clock.advance(now, bounds, &ReplayNoGaps)
        };
        let caught_up = advance.hit_cap && bounds.cap_ms >= plan::live_edge_ms(now);
        if caught_up {
            clock.pause();
        }
        let playing = clock.is_playing();
        self.clock = Some(clock);
        self.buffering = advance.hit_cap && !caught_up;
        if advance.steps > 0 {
            cx.emit(ReplayEvent::Cursor);
        }
        if caught_up {
            cx.emit(ReplayEvent::ReachedNow);
        }
        self.pump(cx);
        cx.notify();
        playing
    }

    /// The range the cursor may take: from where every store has data to where all of them reach.
    fn bounds(&self, clock: &ReplayClock, now_ms: i64) -> ReplayBounds {
        let cursor = clock.cursor_ms();
        let floor = self
            .stores
            .values()
            .map(Store::oldest_ms)
            .max()
            .unwrap_or(cursor)
            .min(cursor);
        let cap = self
            .stores
            .values()
            .map(|store| store.covered_to_ms() + 1)
            .min()
            .unwrap_or(cursor)
            .min(plan::live_edge_ms(now_ms));
        ReplayBounds {
            floor_ms: floor,
            cap_ms: cap,
        }
    }

    fn data_times(&self) -> DataTimes<'_> {
        DataTimes(self.stores.values().map(Store::bars).collect())
    }

    fn refresh_step(&mut self) {
        let step_ms = self.step_ms();
        if let Some(clock) = &mut self.clock {
            clock.set_step(step_ms);
        }
    }
}
