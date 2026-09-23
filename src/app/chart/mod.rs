//! The price chart: candles, bars, a line or an area, on any timeframe from tick by tick to a
//! month, live to the tick.
//!
//! # How it stays fast and small
//!
//! - The chart is drawn by one custom element that turns the visible part of the data into
//!   drawing commands ([`scene`]). The cost of a frame follows the size of the screen, not the
//!   amount of data: points narrower than a pixel are folded into pixel columns.
//! - The data is bounded ([`data::MAX_BARS`], [`data::MAX_TICKS`]), and older history is only
//!   fetched when the user scrolls near the oldest point held.
//! - Live prices append to the last point in constant time. Many updates in a burst make one
//!   frame, because a redraw is only requested, never forced.
//! - Only one live bar subscription exists at a time, and it is dropped with the chart.
//!
//! # Where the data comes from
//!
//! Timeframes from a minute up are the server's own bars, kept current by its live bar events
//! and by the tick stream. Tick by tick and the second timeframes are built here from the bid
//! ticks (the Open API has no bars under a minute).

mod axis;
mod data;
mod live;
mod load;
mod scene;
mod timeframe;
mod view;

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui::prelude::*;
use gpui::{
    App, Bounds, ContentMask, Context, CursorStyle, Entity, EventEmitter, Hitbox, HitboxBehavior,
    KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PinchEvent, Pixels,
    ScrollWheelEvent, SharedString, TextAlign, TextRun, Window, canvas, div, fill, point, px, size,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use wyck::openapi::market::{SpotEvent, Tick, format_price};
use wyck::openapi::session::Session;
use wyck::openapi::{OpenApiError, Result as ApiResult};

use self::data::{MAX_BARS, MAX_TICKS, Series};
pub use self::live::LiveHub;
use self::load::Loaded;
use self::scene::{AXIS_H, AXIS_W, Align, ChartKind, Cmd, Frame, Layout, Palette};
pub use self::timeframe::{GROUPS, QUICK, Timeframe};
use self::view::{PriceScale, View, zoom_range};
use super::connection::ui;
use super::{anim, runtime, theme};

gpui::actions!(
    wyck_chart,
    [
        ChartPanBack,
        ChartPanForward,
        ChartZoomIn,
        ChartZoomOut,
        ChartLatest,
    ]
);

/// Registers the chart's key bindings. Call once, at startup.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("left", ChartPanBack, Some("Dashboard")),
        KeyBinding::new("right", ChartPanForward, Some("Dashboard")),
        KeyBinding::new("secondary-=", ChartZoomIn, Some("Dashboard")),
        KeyBinding::new("secondary-+", ChartZoomIn, Some("Dashboard")),
        KeyBinding::new("secondary--", ChartZoomOut, Some("Dashboard")),
        KeyBinding::new("end", ChartLatest, Some("Dashboard")),
    ]);
}

/// A chart smaller than this shows fewer numbers and no toolbar.
const COMPACT_WIDTH: f32 = 640.0;
const COMPACT_HEIGHT: f32 = 300.0;

/// The symbol on the chart.
struct Symbol {
    id: i64,
    name: SharedString,
    digits: u32,
}

enum Load {
    /// No symbol yet.
    Idle,
    Loading,
    Ready,
    Failed(SharedString),
}

/// Older history: a request in flight, or a pause after one that failed.
enum Older {
    Idle,
    Loading,
    /// The server has nothing older.
    Exhausted,
    Retry(Instant),
}

#[derive(Clone, Copy, PartialEq)]
enum Region {
    Plot,
    PriceAxis,
    TimeAxis,
    Corner,
}

#[derive(Clone, Copy)]
enum DragKind {
    Pan,
    Price,
    Time,
}

#[derive(Clone, Copy)]
struct Drag {
    kind: DragKind,
    last: (f32, f32),
}

/// What a chart tells the layout around it, so that charts can follow one another.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChartEvent {
    /// The user clicked in this chart.
    Activated,
    /// The pointer is over a point of this chart (or left it).
    Hover(Option<Hover>),
    /// The user moved or zoomed the view.
    ViewChanged(Span),
}

/// A point of the chart the pointer is on: its time and the price under the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hover {
    pub time_ms: i64,
    pub price: f64,
}

/// The times at the left and right edges of the plot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub left_ms: i64,
    pub right_ms: i64,
}

impl EventEmitter<ChartEvent> for Chart {}

pub struct Chart {
    session: Session,
    hub: Rc<LiveHub>,
    /// Tells this chart from the others in the hub.
    id: u64,
    symbol: Option<Symbol>,
    timeframe: Timeframe,
    kind: ChartKind,
    series: Series,
    view: View,
    load: Load,
    older: Older,
    /// Bumped whenever the data is thrown away, so an answer to an old request is recognised.
    epoch: u64,
    ask: Option<i64>,
    /// The time of the newest point, kept from going backwards when the local clock is used.
    last_time_ms: i64,
    hover: Option<(f32, f32)>,
    /// The pointer of another chart, when the crosshairs are linked.
    remote: Option<Hover>,
    drag: Option<Drag>,
    /// The drawing area of the last frame, for turning mouse positions into chart positions.
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Chart {
    pub fn new(session: Session, hub: Rc<LiveHub>, id: u64, timeframe: Timeframe) -> Self {
        Self {
            hub,
            id,
            session,
            symbol: None,
            timeframe,
            kind: ChartKind::Candles,
            series: match timeframe {
                Timeframe::Ticks => Series::Ticks(Vec::new()),
                _ => Series::Bars(Vec::new()),
            },
            view: View::new(timeframe.default_bar_px()),
            load: Load::Idle,
            older: Older::Idle,
            epoch: 0,
            ask: None,
            last_time_ms: 0,
            hover: None,
            remote: None,
            drag: None,
            bounds: Rc::new(Cell::new(None)),
        }
    }

    pub fn timeframe(&self) -> Timeframe {
        self.timeframe
    }

    // ---- what to show ----

    pub fn set_symbol(&mut self, id: i64, name: SharedString, digits: u32, cx: &mut Context<Self>) {
        if self.symbol.as_ref().is_some_and(|s| s.id == id) {
            return;
        }
        self.symbol = Some(Symbol { id, name, digits });
        self.reload(cx);
    }

    /// The broker said how the symbol is quoted.
    pub fn set_digits(&mut self, id: i64, digits: u32, cx: &mut Context<Self>) {
        if let Some(symbol) = self.symbol.as_mut().filter(|s| s.id == id) {
            symbol.digits = digits;
            cx.notify();
        }
    }

    pub fn set_timeframe(&mut self, timeframe: Timeframe, cx: &mut Context<Self>) {
        if self.timeframe == timeframe {
            return;
        }
        self.timeframe = timeframe;
        self.reload(cx);
    }

    fn set_kind(&mut self, kind: ChartKind, cx: &mut Context<Self>) {
        self.kind = kind;
        cx.notify();
    }

    fn digits(&self) -> u32 {
        self.symbol.as_ref().map_or(5, |s| s.digits)
    }

    /// Throws the data away and loads the current symbol and timeframe from scratch.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.epoch += 1;
        let epoch = self.epoch;
        self.series = match self.timeframe {
            Timeframe::Ticks => Series::Ticks(Vec::new()),
            _ => Series::Bars(Vec::new()),
        };
        self.view = View::new(self.timeframe.default_bar_px());
        self.older = Older::Idle;
        self.hover = None;
        self.drag = None;
        self.ask = None;
        self.last_time_ms = 0;
        let Some(symbol) = &self.symbol else {
            self.load = Load::Idle;
            self.hub.set(self.id, None);
            cx.notify();
            return;
        };
        self.load = Load::Loading;
        self.hub.set(
            self.id,
            self.timeframe.period().map(|period| (symbol.id, period)),
        );
        cx.notify();

        let (session, id, timeframe) = (self.session.clone(), symbol.id, self.timeframe);
        cx.spawn(async move |this, cx| {
            let result =
                runtime::spawn(
                    async move { load::initial(&session, id, timeframe, now_ms()).await },
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                this.initial_loaded(epoch, flatten(result), cx)
            });
        })
        .detach();
    }

    fn initial_loaded(&mut self, epoch: u64, result: ApiResult<Loaded>, cx: &mut Context<Self>) {
        if epoch != self.epoch {
            return;
        }
        match result {
            Ok(loaded) => {
                match (loaded, &mut self.series) {
                    (Loaded::Bars(bars), Series::Bars(held)) => *held = bars,
                    (Loaded::Ticks(ticks), Series::Ticks(held)) => *held = ticks,
                    _ => {}
                }
                self.last_time_ms = self.series.last_time().unwrap_or(0);
                self.load = Load::Ready;
                let plot_w = self.layout().plot_w();
                self.view.clamp(self.series.len(), plot_w);
                cx.notify();
                // Prices that arrived while the history was on its way are not in it: fetch the
                // few seconds in between. The server's own bars fix themselves on the next tick.
                if self.timeframe.is_tick_built() {
                    self.refill(cx);
                }
                self.load_older_if_needed(cx);
            }
            Err(error) => {
                tracing::warn!(%error, "could not load the chart history");
                self.load = Load::Failed(error.to_string().into());
                cx.notify();
            }
        }
    }

    // ---- the connection ----

    /// The session is connected (again): fetch what was missed, or start over if the first
    /// load never made it.
    pub fn on_ready(&mut self, cx: &mut Context<Self>) {
        match self.load {
            Load::Failed(_) => self.reload(cx),
            Load::Ready => self.refill(cx),
            Load::Idle | Load::Loading => {}
        }
    }

    /// Fetches the prices between the newest point held and now, and joins them in.
    fn refill(&mut self, cx: &mut Context<Self>) {
        let (Some(symbol), Some(from)) = (&self.symbol, self.series.last_time()) else {
            if self.symbol.is_some() && self.series.is_empty() {
                self.reload(cx);
            }
            return;
        };
        let to = now_ms();
        // A very long absence is cheaper to reload than to patch.
        if let Some(bar_ms) = self.timeframe.bar_ms()
            && self.timeframe.is_tick_built()
            && to - from > 6 * self.timeframe.tick_span_ms().max(bar_ms)
        {
            self.reload(cx);
            return;
        }
        if self.timeframe == Timeframe::Ticks && to - from > 6 * self.timeframe.tick_span_ms() {
            self.reload(cx);
            return;
        }
        let (session, id, timeframe, epoch) =
            (self.session.clone(), symbol.id, self.timeframe, self.epoch);
        cx.spawn(async move |this, cx| {
            let result =
                runtime::spawn(async move { load::gap(&session, id, timeframe, from, to).await })
                    .await;
            let _ = this.update(cx, |this, cx| {
                if epoch != this.epoch {
                    return;
                }
                match flatten(result) {
                    Ok(loaded) => this.gap_loaded(loaded, from, to, cx),
                    Err(error) => tracing::warn!(%error, "could not fill the gap in the chart"),
                }
            });
        })
        .detach();
    }

    fn gap_loaded(&mut self, loaded: Loaded, from: i64, to: i64, cx: &mut Context<Self>) {
        let before = self.series.len();
        match (loaded, &mut self.series) {
            (Loaded::Bars(bars), Series::Bars(held)) => data::merge_bars(held, bars),
            (Loaded::Ticks(ticks), Series::Ticks(held)) => {
                data::splice_ticks(held, ticks, from, to);
            }
            _ => return,
        }
        let added = self.series.len().saturating_sub(before);
        self.view.on_appended(added);
        self.last_time_ms = self.series.last_time().unwrap_or(self.last_time_ms);
        cx.notify();
    }

    // ---- older history ----

    /// Asks for older history when the view is near the oldest point held.
    fn load_older_if_needed(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.load, Load::Ready) {
            return;
        }
        match self.older {
            Older::Idle => {}
            Older::Retry(at) if Instant::now() >= at => {}
            _ => return,
        }
        let len = self.series.len();
        if len >= self.series.capacity_limit() {
            self.older = Older::Exhausted;
            return;
        }
        let plot_w = self.layout().plot_w();
        let (first, _) = self.view.visible(len, plot_w);
        if (first as f64) > self.view.span(plot_w) * 0.5 {
            return;
        }
        let (Some(symbol), Some(oldest)) = (&self.symbol, self.series.first_time()) else {
            return;
        };
        self.older = Older::Loading;
        cx.notify();
        let (session, id, timeframe, epoch) =
            (self.session.clone(), symbol.id, self.timeframe, self.epoch);
        cx.spawn(async move |this, cx| {
            let result =
                runtime::spawn(async move { load::older(&session, id, timeframe, oldest).await })
                    .await;
            let _ = this.update(cx, |this, cx| this.older_loaded(epoch, flatten(result), cx));
        })
        .detach();
    }

    fn older_loaded(&mut self, epoch: u64, result: ApiResult<Loaded>, cx: &mut Context<Self>) {
        if epoch != self.epoch {
            return;
        }
        match result {
            Ok(loaded) if loaded.is_empty() => self.older = Older::Exhausted,
            Ok(loaded) => {
                let added = match (loaded, &mut self.series) {
                    (Loaded::Bars(bars), Series::Bars(held)) => data::prepend_bars(held, bars),
                    (Loaded::Ticks(ticks), Series::Ticks(held)) => data::prepend_ticks(held, ticks),
                    _ => 0,
                };
                // Nothing new means the server has nothing older (or only what is held).
                self.older = if added == 0 {
                    Older::Exhausted
                } else {
                    Older::Idle
                };
            }
            Err(error) => {
                tracing::warn!(%error, "could not load older history");
                self.older = Older::Retry(Instant::now() + Duration::from_secs(5));
            }
        }
        cx.notify();
        if matches!(self.older, Older::Idle) {
            // Still near the edge after the step? Keep going until the screen is full.
            self.load_older_if_needed(cx);
        }
    }

    // ---- live prices ----

    /// A price event from the session.
    pub fn on_spot(&mut self, spot: &SpotEvent, cx: &mut Context<Self>) {
        let Some(id) = self.symbol.as_ref().map(|s| s.id) else {
            return;
        };
        if spot.symbol_id != id {
            return;
        }
        if let Some(ask) = spot.ask {
            self.ask = Some(ask);
        }
        if !matches!(self.load, Load::Ready) {
            cx.notify();
            return;
        }
        let mut appended = 0;
        if let Some(bid) = spot.bid {
            let time_ms = spot.timestamp.unwrap_or_else(now_ms).max(self.last_time_ms);
            self.last_time_ms = time_ms;
            let tick = Tick {
                time_ms,
                price: bid,
            };
            match (&mut self.series, self.timeframe) {
                (Series::Ticks(ticks), _) => {
                    ticks.push(tick);
                    appended += 1;
                    data::trim_front(ticks, MAX_TICKS);
                }
                (Series::Bars(bars), Timeframe::Seconds(seconds)) => {
                    if data::fold_tick(bars, i64::from(seconds) * 1_000, tick) {
                        appended += 1;
                    }
                    data::trim_front(bars, MAX_BARS);
                }
                (Series::Bars(bars), Timeframe::Bars(period)) => {
                    data::touch_last_bar(bars, period.millis(), tick);
                }
                _ => {}
            }
        }
        if let (Series::Bars(bars), Timeframe::Bars(period)) = (&mut self.series, self.timeframe) {
            for (live_period, bar) in spot.live_bars() {
                let bar = data::with_true_close(bar, bars.last(), spot.bid);
                if live_period == period && data::apply_live_bar(bars, bar) {
                    appended += 1;
                    self.last_time_ms = self.last_time_ms.max(bar.time_ms);
                }
            }
            data::trim_front(bars, MAX_BARS);
        }
        if appended > 0 {
            self.view.on_appended(appended);
        }
        cx.notify();
    }

    // ---- moving around ----

    fn layout(&self) -> Layout {
        match self.bounds.get() {
            Some(bounds) => Layout {
                w: f64::from(f32::from(bounds.size.width)),
                h: f64::from(f32::from(bounds.size.height)),
            },
            None => Layout {
                w: 1_000.0,
                h: 600.0,
            },
        }
    }

    fn region(&self, x: f32, y: f32) -> Region {
        let layout = self.layout();
        match (
            f64::from(x) < layout.plot_w(),
            f64::from(y) < layout.plot_h(),
        ) {
            (true, true) => Region::Plot,
            (false, true) => Region::PriceAxis,
            (true, false) => Region::TimeAxis,
            (false, false) => Region::Corner,
        }
    }

    fn moved(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        self.emit_span(cx);
        self.load_older_if_needed(cx);
    }

    fn pan_by(&mut self, dx: f32, cx: &mut Context<Self>) {
        let plot_w = self.layout().plot_w();
        self.view.pan(f64::from(dx), self.series.len(), plot_w);
        self.moved(cx);
    }

    fn zoom_by(&mut self, factor: f64, anchor_x: f32, cx: &mut Context<Self>) {
        let plot_w = self.layout().plot_w();
        self.view
            .zoom(factor, f64::from(anchor_x), self.series.len(), plot_w);
        self.moved(cx);
    }

    fn zoom_price_by(&mut self, factor: f64, anchor_y: Option<f32>, cx: &mut Context<Self>) {
        let Some(map) = scene::price_map(
            &self.series,
            &self.view,
            self.layout(),
            self.kind,
            self.digits(),
        ) else {
            return;
        };
        let anchor = match anchor_y {
            Some(y) => map.price(f64::from(y)),
            None => (map.lo + map.hi) / 2.0,
        };
        let (lo, hi) = zoom_range(map.lo, map.hi, anchor, factor);
        self.view.price = PriceScale::Manual { lo, hi };
        cx.notify();
    }

    fn pan_price_by(&mut self, dy: f32, cx: &mut Context<Self>) {
        let Some(map) = scene::price_map(
            &self.series,
            &self.view,
            self.layout(),
            self.kind,
            self.digits(),
        ) else {
            return;
        };
        let shift = f64::from(dy) / (map.bottom - map.top) * (map.hi - map.lo);
        self.view.price = PriceScale::Manual {
            lo: map.lo + shift,
            hi: map.hi + shift,
        };
        cx.notify();
    }

    pub fn pan_keys(&mut self, forward: bool, cx: &mut Context<Self>) {
        let step = (self.view.bar_px * 6.0) as f32;
        self.pan_by(if forward { -step } else { step }, cx);
    }

    pub fn zoom_keys(&mut self, magnify: bool, cx: &mut Context<Self>) {
        let anchor = self.layout().plot_w() as f32;
        self.zoom_by(if magnify { 1.25 } else { 0.8 }, anchor, cx);
    }

    pub fn jump_to_latest(&mut self, cx: &mut Context<Self>) {
        self.view.jump_to_latest();
        self.view.price = PriceScale::Auto;
        cx.notify();
        self.emit_span(cx);
    }

    fn on_mouse_down(&mut self, x: f32, y: f32, clicks: usize, cx: &mut Context<Self>) {
        cx.emit(ChartEvent::Activated);
        let region = self.region(x, y);
        if clicks >= 2 {
            match region {
                Region::Plot | Region::TimeAxis => self.jump_to_latest(cx),
                Region::PriceAxis => {
                    self.view.price = PriceScale::Auto;
                    cx.notify();
                }
                Region::Corner => {}
            }
            return;
        }
        let kind = match region {
            Region::Plot => DragKind::Pan,
            Region::PriceAxis => DragKind::Price,
            Region::TimeAxis => DragKind::Time,
            Region::Corner => return,
        };
        self.drag = Some(Drag { kind, last: (x, y) });
        cx.notify();
    }

    fn on_mouse_move(
        &mut self,
        x: f32,
        y: f32,
        left_down: bool,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(drag) = self.drag {
            if !left_down {
                self.drag = None;
                cx.notify();
                return;
            }
            let (dx, dy) = (x - drag.last.0, y - drag.last.1);
            self.drag = Some(Drag {
                last: (x, y),
                ..drag
            });
            match drag.kind {
                DragKind::Pan => {
                    self.set_hover(x, y, cx);
                    if shift {
                        self.pan_price_by(dy, cx);
                    }
                    self.pan_by(dx, cx);
                }
                DragKind::Price => self.zoom_price_by((-f64::from(dy) * 0.006).exp(), None, cx),
                DragKind::Time => {
                    let anchor = self.layout().plot_w() as f32;
                    self.zoom_by((f64::from(dx) * 0.006).exp(), anchor, cx);
                }
            }
            return;
        }
        self.set_hover(x, y, cx);
        cx.notify();
    }

    fn set_hover(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        self.hover = Some((x, y));
        let info = self.hover_info(x, y);
        cx.emit(ChartEvent::Hover(info));
    }

    fn on_mouse_up(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            cx.notify();
        }
    }

    fn on_pointer_left(&mut self, cx: &mut Context<Self>) {
        if self.hover.take().is_some() {
            cx.emit(ChartEvent::Hover(None));
            cx.notify();
        }
    }

    fn on_wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32, shift: bool, cx: &mut Context<Self>) {
        match self.region(x, y) {
            Region::PriceAxis => {
                self.zoom_price_by((f64::from(dy) * 0.002).exp(), Some(y), cx);
            }
            Region::Corner => {}
            Region::Plot | Region::TimeAxis => {
                if shift || dx.abs() > dy.abs() {
                    self.pan_by(if shift { dy } else { dx }, cx);
                } else {
                    let anchor = if matches!(self.region(x, y), Region::TimeAxis) {
                        self.layout().plot_w() as f32
                    } else {
                        x
                    };
                    self.zoom_by((f64::from(dy) * 0.002).exp(), anchor, cx);
                }
            }
        }
    }

    // ---- following other charts ----

    fn emit_span(&self, cx: &mut Context<Self>) {
        if let Some(span) = self.span() {
            cx.emit(ChartEvent::ViewChanged(span));
        }
    }

    /// The times at the edges of the plot, once there is data to tell them from.
    fn span(&self) -> Option<Span> {
        let plot_w = self.layout().plot_w();
        let len = self.series.len();
        let step = self.series.step_ms(self.timeframe.bar_ms());
        let left = self.view.index_at(0.0, len, plot_w);
        let right = self.view.index_at(plot_w, len, plot_w);
        Some(Span {
            left_ms: self.series.time_of_index(left, step)?,
            right_ms: self.series.time_of_index(right, step)?,
        })
    }

    /// The time and price under a pointer position, when it is over a point of the plot.
    fn hover_info(&self, x: f32, y: f32) -> Option<Hover> {
        if self.region(x, y) != Region::Plot {
            return None;
        }
        let len = self.series.len();
        let plot_w = self.layout().plot_w();
        let index = self
            .view
            .index_at(f64::from(x), len, plot_w)
            .round()
            .clamp(0.0, len.checked_sub(1)? as f64) as usize;
        let map = scene::price_map(
            &self.series,
            &self.view,
            self.layout(),
            self.kind,
            self.digits(),
        )?;
        Some(Hover {
            time_ms: self.series.time_at(index)?,
            price: map.price(f64::from(y)),
        })
    }

    /// Another chart was scrolled: show the same time at the right edge. Does not tell anyone.
    pub fn follow_right_edge(&mut self, right_ms: i64, cx: &mut Context<Self>) {
        let len = self.series.len();
        let step = self.series.step_ms(self.timeframe.bar_ms());
        let Some(index) = self.series.index_of_time(right_ms, step) else {
            return;
        };
        self.view.offset = index + 0.5 - len as f64;
        self.view.clamp(len, self.layout().plot_w());
        cx.notify();
        self.load_older_if_needed(cx);
    }

    /// Another chart was scrolled or zoomed: show the same span of time. Does not tell anyone.
    pub fn follow_span(&mut self, span: Span, cx: &mut Context<Self>) {
        let len = self.series.len();
        let step = self.series.step_ms(self.timeframe.bar_ms());
        let (Some(left), Some(right)) = (
            self.series.index_of_time(span.left_ms, step),
            self.series.index_of_time(span.right_ms, step),
        ) else {
            return;
        };
        let points = right - left;
        if !(points.is_finite() && points >= 1.0) {
            return;
        }
        let plot_w = self.layout().plot_w();
        self.view.bar_px = plot_w / points;
        self.view.offset = right + 0.5 - len as f64;
        self.view.clamp(len, plot_w);
        cx.notify();
        self.load_older_if_needed(cx);
    }

    /// The pointer of another chart, drawn here as a crosshair.
    pub fn show_remote_pointer(&mut self, pointer: Option<Hover>, cx: &mut Context<Self>) {
        if self.remote != pointer {
            self.remote = pointer;
            cx.notify();
        }
    }

    // ---- what the legend says ----

    /// The point under the pointer, or the newest.
    fn shown_index(&self) -> Option<usize> {
        let len = self.series.len();
        let last = len.checked_sub(1)?;
        let Some((x, _)) = self
            .hover
            .filter(|(x, y)| self.region(*x, *y) == Region::Plot)
        else {
            if let Some(remote) = self.remote {
                let step = self.series.step_ms(self.timeframe.bar_ms());
                let index = self.series.index_of_time(remote.time_ms, step)?.round();
                return Some(index.clamp(0.0, last as f64) as usize);
            }
            return Some(last);
        };
        let index = self
            .view
            .index_at(f64::from(x), len, self.layout().plot_w())
            .round();
        Some(index.clamp(0.0, last as f64) as usize)
    }

    fn scene(&self, bounds: Bounds<Pixels>, scale: f32) -> Vec<Cmd> {
        scene::build(&Frame {
            series: &self.series,
            view: &self.view,
            kind: self.kind,
            timeframe: self.timeframe,
            digits: self.digits(),
            origin: bounds.origin,
            layout: Layout {
                w: f64::from(f32::from(bounds.size.width)),
                h: f64::from(f32::from(bounds.size.height)),
            },
            scale,
            hover: self.hover,
            remote: self.remote.map(|r| (r.time_ms, r.price)),
            ask: self.ask,
            palette: Palette::new(),
        })
    }

    fn cursor(&self) -> CursorStyle {
        if let Some(drag) = self.drag {
            return match drag.kind {
                DragKind::Pan => CursorStyle::ClosedHand,
                DragKind::Price => CursorStyle::ResizeUpDown,
                DragKind::Time => CursorStyle::ResizeLeftRight,
            };
        }
        match self
            .hover
            .map_or(Region::Corner, |(x, y)| self.region(x, y))
        {
            Region::Plot => CursorStyle::Crosshair,
            Region::PriceAxis => CursorStyle::ResizeUpDown,
            Region::TimeAxis => CursorStyle::ResizeLeftRight,
            Region::Corner => CursorStyle::Arrow,
        }
    }
}

impl Drop for Chart {
    fn drop(&mut self) {
        self.hub.set(self.id, None);
    }
}

fn flatten<T>(result: Result<ApiResult<T>, tokio::task::JoinError>) -> ApiResult<T> {
    match result {
        Ok(inner) => inner,
        Err(_) => Err(OpenApiError::Closed),
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
}

// ---- drawing ----

/// Draws the commands of a frame.
fn execute(cmds: Vec<Cmd>, chart: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    for cmd in cmds {
        match cmd {
            Cmd::Quad(quad) => window.paint_quad(quad),
            Cmd::Path(path, color) => window.paint_path(path, color),
            Cmd::Clip(bounds, inner) => {
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    execute(inner, chart, window, cx);
                });
            }
            Cmd::Text {
                text,
                x,
                y,
                color,
                align,
            } => {
                let line = shape(window, &text, scene_font(), color);
                let x = aligned(x, f32::from(line.width), align);
                let _ = line.paint(
                    point(px(x), px(y)),
                    px(scene_font() * 1.3),
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
            Cmd::Tag {
                text,
                x,
                y,
                height,
                pad,
                bg,
                fg,
                align,
            } => {
                let line = shape(window, &text, scene_font(), fg);
                let width = if align == Align::Left {
                    AXIS_W - 2.0
                } else {
                    f32::from(line.width) + pad * 2.0
                };
                let (left, right) = (
                    f32::from(chart.origin.x),
                    f32::from(chart.origin.x) + f32::from(chart.size.width),
                );
                let x = aligned(x, width, align).clamp(left, (right - width).max(left));
                let mut quad = fill(
                    Bounds::new(point(px(x), px(y)), size(px(width), px(height))),
                    bg,
                );
                quad.corner_radii = px(4.0).into();
                window.paint_quad(quad);
                let text_x = x + pad;
                let _ = line.paint(
                    point(px(text_x), px(y + (height - scene_font() * 1.3) / 2.0)),
                    px(scene_font() * 1.3),
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
        }
    }
}

fn scene_font() -> f32 {
    11.0
}

fn aligned(x: f32, width: f32, align: Align) -> f32 {
    match align {
        Align::Left => x,
        Align::Center => x - width / 2.0,
    }
}

fn shape(window: &Window, text: &str, size_px: f32, color: gpui::Hsla) -> gpui::ShapedLine {
    let style = window.text_style();
    let run = TextRun {
        len: text.len(),
        font: style.font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window.text_system().shape_line(
        SharedString::from(text.to_owned()),
        px(size_px),
        &[run],
        None,
    )
}

/// The canvas: measures itself, draws a frame from the chart's data, and listens to the pointer.
fn surface(
    chart: &Entity<Chart>,
    bounds_cell: Rc<Cell<Option<Bounds<Pixels>>>>,
) -> impl IntoElement {
    let entity = chart.clone();
    canvas(
        move |bounds, window, _cx| {
            bounds_cell.set(Some(bounds));
            window.insert_hitbox(bounds, HitboxBehavior::Normal)
        },
        move |bounds, hitbox: Hitbox, window, cx| {
            let scale = window.scale_factor();
            let cmds = entity.read(cx).scene(bounds, scale);
            execute(cmds, bounds, window, cx);
            let cursor = entity.read(cx).cursor();
            window.set_cursor_style(cursor, &hitbox);
            listen(&entity, bounds, hitbox, window);
        },
    )
    .absolute()
    .size_full()
}

/// Registers the pointer handlers for this frame. They are window wide, so a drag that leaves the
/// chart keeps working, and each one checks the hitbox so an overlay above the chart wins.
fn listen(entity: &Entity<Chart>, bounds: Bounds<Pixels>, hitbox: Hitbox, window: &mut Window) {
    let relative = move |position: gpui::Point<Pixels>| {
        (
            f32::from(position.x - bounds.origin.x),
            f32::from(position.y - bounds.origin.y),
        )
    };

    let (e, h) = (entity.clone(), hitbox.clone());
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase != gpui::DispatchPhase::Bubble
            || event.button != MouseButton::Left
            || !h.is_hovered(window)
        {
            return;
        }
        let (x, y) = relative(event.position);
        let clicks = event.click_count;
        e.update(cx, |chart, cx| chart.on_mouse_down(x, y, clicks, cx));
    });

    let (e, h) = (entity.clone(), hitbox.clone());
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        if phase != gpui::DispatchPhase::Bubble {
            return;
        }
        let (x, y) = relative(event.position);
        let hovered = h.is_hovered(window);
        let left_down = event.pressed_button == Some(MouseButton::Left);
        let shift = event.modifiers.shift;
        e.update(cx, |chart, cx| {
            if chart.drag.is_some() || hovered {
                chart.on_mouse_move(x, y, left_down, shift, cx);
            } else {
                chart.on_pointer_left(cx);
            }
        });
    });

    let e = entity.clone();
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
        if phase == gpui::DispatchPhase::Bubble && event.button == MouseButton::Left {
            e.update(cx, |chart, cx| chart.on_mouse_up(cx));
        }
    });

    let (e, h) = (entity.clone(), hitbox.clone());
    window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
        if phase != gpui::DispatchPhase::Bubble || !h.is_hovered(window) {
            return;
        }
        let (x, y) = relative(event.position);
        let delta = event.delta.pixel_delta(px(20.0));
        let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
        let shift = event.modifiers.shift;
        e.update(cx, |chart, cx| chart.on_wheel(x, y, dx, dy, shift, cx));
    });

    let (e, h) = (entity.clone(), hitbox);
    window.on_mouse_event(move |event: &PinchEvent, phase, window, cx| {
        if phase != gpui::DispatchPhase::Bubble || !h.is_hovered(window) {
            return;
        }
        let (x, _) = relative(event.position);
        let factor = f64::from(1.0 + event.delta).max(0.1);
        e.update(cx, |chart, cx| chart.zoom_by(factor, x, cx));
    });
}

impl Render for Chart {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let latest = !self.view.is_following() && !self.series.is_empty();
        // Small charts (many on the screen) keep only what still fits.
        let compact = self.bounds.get().is_some_and(|b| {
            f32::from(b.size.width) < COMPACT_WIDTH || f32::from(b.size.height) < COMPACT_HEIGHT
        });

        div()
            .relative()
            .flex_1()
            .w_full()
            .min_h_0()
            .overflow_hidden()
            .bg(theme::bg())
            .child(surface(&entity, self.bounds.clone()))
            .child(self.legend(compact))
            .children((!compact).then(|| self.toolbar(latest, cx)))
            .children(self.status(cx))
    }
}

impl Chart {
    /// The symbol, the timeframe and the numbers of the point under the pointer.
    fn legend(&self, compact: bool) -> impl IntoElement {
        let digits = self.digits();
        let name = self
            .symbol
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let head = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .text_size(px(13.))
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme::fg())
                    .child(name),
            )
            .child(
                div()
                    .text_color(theme::muted_fg())
                    .child(self.timeframe.name()),
            );

        let mut numbers = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_x_3()
            .text_size(px(12.));
        if let Some(index) = self.shown_index() {
            let value = |label: &'static str, price: i64, tone: gpui::Rgba| {
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(div().text_color(theme::muted_fg()).child(label))
                    .child(div().text_color(tone).child(format_price(price, digits)))
            };
            match &self.series {
                Series::Bars(bars) => {
                    if let Some(bar) = bars.get(index) {
                        let tone = if bar.close >= bar.open {
                            theme::emerald()
                        } else {
                            theme::destructive()
                        };
                        let change = if bar.open != 0 {
                            (bar.close - bar.open) as f64 / bar.open as f64 * 100.0
                        } else {
                            0.0
                        };
                        if compact {
                            numbers = numbers
                                .child(value("C", bar.close, tone))
                                .child(div().text_color(tone).child(format!("{change:+.2}%")));
                        } else {
                            numbers = numbers
                                .child(value("O", bar.open, tone))
                                .child(value("H", bar.high, tone))
                                .child(value("L", bar.low, tone))
                                .child(value("C", bar.close, tone))
                                .child(div().text_color(tone).child(format!("{change:+.2}%")))
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap_1()
                                        .child(div().text_color(theme::muted_fg()).child("Ticks"))
                                        .child(
                                            div()
                                                .text_color(theme::fg())
                                                .child(bar.volume.to_string()),
                                        ),
                                );
                        }
                    }
                }
                Series::Ticks(ticks) => {
                    if let Some(tick) = ticks.get(index) {
                        numbers = numbers.child(value("Bid", tick.price, theme::fg()));
                    }
                }
            }
        }
        div()
            .absolute()
            .top(px(8.))
            .left(px(12.))
            .flex()
            .flex_col()
            .gap_1()
            .child(head)
            .child(numbers)
    }

    /// Chart type, and the buttons that bring the chart back to the newest prices.
    fn toolbar(&self, latest: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let kinds = ChartKind::ALL.into_iter().map(|kind| {
            let icon = match kind {
                ChartKind::Candles => IconName::ChartCandlestick,
                ChartKind::Bars => IconName::ChartNoAxesColumn,
                ChartKind::Line => IconName::ChartLine,
                ChartKind::Area => IconName::ChartArea,
            };
            Button::new(SharedString::from(format!("chart-kind-{}", kind.label())))
                .ghost()
                .compact()
                .icon(icon)
                .tooltip(kind.label())
                .toggled(self.kind == kind)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _event, _window, cx| this.set_kind(kind, cx)))
        });
        let auto = matches!(self.view.price, PriceScale::Auto);
        div()
            .absolute()
            .top(px(6.))
            .right(px(AXIS_W + 10.0))
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .p_1()
            .rounded_lg()
            .bg(gpui::rgba(0x0a0a0acc))
            .children(kinds)
            .when(!auto, |el| {
                el.child(
                    Button::new("chart-auto-scale")
                        .ghost()
                        .compact()
                        .icon(IconName::Scaling)
                        .tooltip("Fit the price scale (double click the price axis)")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.view.price = PriceScale::Auto;
                            cx.notify();
                        })),
                )
            })
            .when(latest, |el| {
                el.child(
                    Button::new("chart-latest")
                        .ghost()
                        .compact()
                        .icon(IconName::ChevronsRight)
                        .tooltip("Back to the latest price (End)")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.jump_to_latest(cx);
                        })),
                )
            })
    }

    /// Loading, failed and empty states, and the small note while older history comes in.
    fn status(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let centered = || {
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .px_6()
        };
        let mut out: Vec<gpui::AnyElement> = Vec::new();
        match &self.load {
            Load::Idle => out.push(
                centered()
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(theme::muted_fg())
                            .child("Pick a symbol to see its chart."),
                    )
                    .into_any_element(),
            ),
            Load::Loading => out.push(
                centered()
                    .child(anim::spin(
                        ui::icon_colored(IconName::LoaderCircle, 22., theme::muted_fg()),
                        "chart-loading",
                    ))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(theme::muted_fg())
                            .child("Loading the chart..."),
                    )
                    .into_any_element(),
            ),
            Load::Failed(message) => out.push(
                centered()
                    .child(ui::icon_colored(
                        IconName::CircleAlert,
                        26.,
                        theme::destructive(),
                    ))
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(theme::fg())
                            .child("Could not load the chart"),
                    )
                    .child(
                        div()
                            .max_w(px(440.))
                            .text_center()
                            .text_size(px(12.))
                            .text_color(theme::muted_fg())
                            .child(message.clone()),
                    )
                    .child(div().pt_1().w(px(200.)).child(ui::primary_button(
                        "chart-retry",
                        "Try again",
                        cx.listener(|this, _event, _window, cx| this.reload(cx)),
                    )))
                    .into_any_element(),
            ),
            Load::Ready if self.series.is_empty() => out.push(
                centered()
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(theme::muted_fg())
                            .child("No prices yet for this timeframe. New ones will show up here."),
                    )
                    .into_any_element(),
            ),
            Load::Ready => {}
        }
        if matches!(self.older, Older::Loading) {
            out.push(
                div()
                    .absolute()
                    .left(px(12.))
                    .bottom(px(AXIS_H + 8.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_size(px(11.))
                    .text_color(theme::muted_fg())
                    .child(anim::spin(
                        ui::icon_colored(IconName::LoaderCircle, 12., theme::muted_fg()),
                        "chart-older",
                    ))
                    .child("Loading older history")
                    .into_any_element(),
            );
        }
        out
    }
}
