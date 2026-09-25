//! The price chart: candles, bars, lines, areas, Renko, Kagi, point and figure and more, on any
//! timeframe from tick by tick to a month, live to the tick, with indicators, drawing tools and
//! lines for orders, positions and alerts on top.
//!
//! # How it is organised
//!
//! [`Chart`] is the gpui entity; its code is split by concern:
//!
//! | Module | Role |
//! |---|---|
//! | this file | the entity, what it shows, and what it tells the layout around it |
//! | [`history`] | loading the history, older pages, refilling after a reconnect, live prices |
//! | [`input`] | the pointer and the keys: scrolling, zooming, dragging scales, panes and lines |
//! | [`follow`] | following other charts: shared crosshair, time and range |
//! | [`glue`] | the drawings: handing presses and moves to the drawing book |
//! | [`paint`] | the canvas: turning a frame's commands into gpui quads, paths and text |
//! | [`overlay`] | what sits over the canvas: legend, toolbar, menus, line labels, status |
//! | [`scene`] | the frame itself, as plain drawing commands, tested without a window |
//! | [`display`] | what is drawn: the series of the chart type, and the indicators' values |
//! | `flow`, `flow_sync` | the order flow of a footprint: counting quotes into bars, and loading it |
//! | `footprint`, `footprint_ui` | the footprint chart type: its settings and analysis, and its dialog |
//!
//! The data, view, axis, study and transform modules are plain data in and out.
//!
//! # How it stays fast and small
//!
//! - The chart is drawn by one custom element that turns the visible part of the data into
//!   drawing commands ([`scene`]). The cost of a frame follows the size of the screen, not the
//!   amount of data: points narrower than a pixel are folded into pixel columns.
//! - What is derived from the prices (a Renko construction, the indicators) is computed once per
//!   change of the prices or settings ([`display`]), never per frame.
//! - The data is bounded ([`data::MAX_BARS`], [`data::MAX_TICKS`]), and older history is only
//!   fetched when the user scrolls near the oldest point held.
//! - Charts do not subscribe to live bars themselves: they tell a [`LiveHub`] what they want, and
//!   it keeps one subscription per symbol and period however many charts show it.
//! - Drawings ([`drawing`]) are anchored to times and prices, so they follow the chart when it
//!   moves and show on every timeframe of their symbol.
//!
//! # Where the data comes from
//!
//! Timeframes from a minute up are the server's own bars, kept current by its live bar events
//! and by the tick stream. Tick by tick and the second timeframes are built here from the bid
//! ticks (the Open API has no bars under a minute).
//!
//! The footprint chart type adds the bid and ask ticks of the bars on screen (see [`flow`]): the
//! Open API has no trade tape, so the side and the volume of each unit are inferred from the
//! quotes, and the chart says so.

mod axis;
mod chart_settings_ui;
mod data;
mod display;
pub mod drawing;
pub mod drawing_props;
mod flow;
mod flow_sync;
mod follow;
mod footprint;
mod footprint_ui;
mod glue;
mod history;
mod input;
pub mod lines;
pub mod live;
mod load;
pub mod object_tree;
pub mod options;
mod overlay;
mod paint;
mod projection;
pub mod raster;
pub mod scene;
pub mod settings;
pub mod study;
mod study_settings;
mod timeframe;
pub mod transform;
mod view;
pub mod zone;

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui::{App, Bounds, Context, Entity, EventEmitter, KeyBinding, Pixels, SharedString};
use wyck::openapi::session::Session;
use wyck::openapi::{OpenApiError, Result as ApiResult};

use self::data::Series;
use self::display::Display;
use self::drawing::Drawings;
use self::flow::Flow;
use self::flow_sync::{FlowLoad, HeldQuote};
pub use self::glue::{DrawingCommand, open_drawing_settings, open_object_tree};
pub use self::lines::{ChartLine, LineId};
pub use self::live::{LiveHub, LiveUpdate};
use self::scene::Geometry;
pub use self::settings::{ChartKind, ChartSettings};
use self::study::StudyConfig;
pub use self::timeframe::{GROUPS, QUICK, Timeframe, Unit};
use self::view::View;
pub use self::zone::Zone;

gpui::actions!(
    wyck_chart,
    [
        ChartPanBack,
        ChartPanForward,
        ChartZoomIn,
        ChartZoomOut,
        ChartLatest,
        ChartResetScale,
        DeleteDrawing,
        FinishDrawing,
        Favorite1,
        Favorite2,
        Favorite3,
        Favorite4,
        Favorite5,
        Favorite6,
        Favorite7,
        Favorite8,
        Favorite9,
        UndoDrawing,
        RedoDrawing,
        DuplicateDrawing,
        ChartScreenshot,
        ChartAddAlert,
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
        KeyBinding::new("alt-r", ChartResetScale, Some("Dashboard")),
        KeyBinding::new("delete", DeleteDrawing, Some("Dashboard")),
        KeyBinding::new("backspace", DeleteDrawing, Some("Dashboard")),
        // Only while a drawing is being made, so Enter is left alone the rest of the time.
        KeyBinding::new("enter", FinishDrawing, Some("DrawingInProgress")),
        // Alt and a number picks the favorite tool at that place in the bar.
        KeyBinding::new("alt-1", Favorite1, Some("Dashboard")),
        KeyBinding::new("alt-2", Favorite2, Some("Dashboard")),
        KeyBinding::new("alt-3", Favorite3, Some("Dashboard")),
        KeyBinding::new("alt-4", Favorite4, Some("Dashboard")),
        KeyBinding::new("alt-5", Favorite5, Some("Dashboard")),
        KeyBinding::new("alt-6", Favorite6, Some("Dashboard")),
        KeyBinding::new("alt-7", Favorite7, Some("Dashboard")),
        KeyBinding::new("alt-8", Favorite8, Some("Dashboard")),
        KeyBinding::new("alt-9", Favorite9, Some("Dashboard")),
        KeyBinding::new("secondary-z", UndoDrawing, Some("Dashboard")),
        KeyBinding::new("secondary-shift-z", RedoDrawing, Some("Dashboard")),
        KeyBinding::new("secondary-y", RedoDrawing, Some("Dashboard")),
        KeyBinding::new("secondary-d", DuplicateDrawing, Some("Dashboard")),
        KeyBinding::new("secondary-shift-s", ChartScreenshot, Some("Dashboard")),
        KeyBinding::new("alt-a", ChartAddAlert, Some("Dashboard")),
    ]);
}

/// A chart smaller than this shows fewer numbers and a smaller toolbar.
const COMPACT_WIDTH: f32 = 640.0;
const COMPACT_HEIGHT: f32 = 300.0;

/// The symbol on the chart.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub id: i64,
    pub name: SharedString,
    pub digits: u32,
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

/// The menus that open from the chart's own toolbar and legend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    Kind,
    Scale,
    Zone,
}

/// A time and price under the pointer.
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

/// What a chart tells the layout around it.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartEvent {
    /// The user clicked in this chart.
    Activated,
    /// The pointer is over a point of this chart (or left it).
    Hover(Option<Hover>),
    /// The user moved or zoomed the view.
    ViewChanged(Span),
    /// Something that is saved changed: the timeframe, the type, the indicators, the scale.
    SettingsChanged,
    /// The user asked to change the symbol of this chart.
    PickSymbol,
    /// The user asked for something at a price: an order, an alert.
    Action(ChartAction),
    /// A line was dragged to a new price (real units).
    LineMoved(LineId, f64),
    /// The close button of a line was clicked.
    LineClosed(LineId),
    /// A picture of the chart was asked for.
    Screenshot,
}

/// Something asked from the chart's context menu or a drawing, at a price in real units.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartAction {
    /// Open the order ticket with these values.
    Ticket {
        buy: bool,
        /// A limit or stop price; `None` for a market order.
        entry: Option<f64>,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    },
    AddAlert(f64),
}

impl EventEmitter<ChartEvent> for Chart {}

pub struct Chart {
    session: Session,
    hub: Rc<LiveHub>,
    /// Tells this chart from the others in the hub.
    id: u64,
    symbol: Option<Symbol>,
    /// When the symbol trades, once its details are known.
    hours: Option<std::sync::Arc<wyck::openapi::market::TradingHours>>,
    /// Whether this is the chart the user works on; only that one shows its toolbar when there
    /// are several.
    selected: bool,
    timeframe: Timeframe,
    settings: ChartSettings,
    /// The prices, as held.
    series: Series,
    /// What is drawn from them.
    display: Display,
    /// What traded at each price of each bar, for the footprint chart type.
    flow: Flow,
    flow_load: FlowLoad,
    /// Live quotes that came before the flow they follow.
    flow_held: Vec<HeldQuote>,
    /// Whether a request for the newest flow is in flight.
    flow_newest_pending: bool,
    /// Whether the quotes since the last one counted must be fetched (after a lost connection).
    flow_gap: bool,
    /// Bumped whenever the flow is thrown away, so an answer to an old request is recognised.
    flow_epoch: u64,
    view: View,
    load: Load,
    older: Older,
    /// Bumped whenever the data is thrown away, so an answer to an old request is recognised.
    epoch: u64,
    bid: Option<i64>,
    ask: Option<i64>,
    /// The time of the newest point, kept from going backwards when the local clock is used.
    last_time_ms: i64,
    /// The server bars of the newest bar of a grouped timeframe, which live bars join.
    group_tail: Vec<wyck::openapi::market::Bar>,
    hover: Option<(f32, f32)>,
    /// The pointer of another chart, when the crosshairs are linked.
    remote: Option<Hover>,
    drag: Option<input::Drag>,
    /// The drawings shared by the charts, and the subscription that redraws this chart when
    /// they change.
    drawings: Option<Entity<Drawings>>,
    _drawings_observe: Option<gpui::Subscription>,
    /// Whether a press taken by a drawing is still down.
    drawing_drag: bool,
    /// Whether the pointer is over a drawing, for the mouse cursor.
    over_drawing: Option<drawing::book::Grab>,
    /// A drawing was double-clicked: its settings open at the next render, which has the window.
    settings_for: Option<u64>,
    /// Orders, positions and alerts shown on the prices.
    lines: Vec<ChartLine>,
    menu: Option<Menu>,
    /// Where the pointer was at the last right click, for the context menu.
    context_at: Option<(f32, f32)>,
    /// The drawing area of the last frame, for turning mouse positions into chart positions.
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Keeps the bar countdown ticking while the chart lives.
    _clock: gpui::Task<()>,
}

impl Chart {
    pub fn new(
        session: Session,
        hub: Rc<LiveHub>,
        id: u64,
        timeframe: Timeframe,
        settings: ChartSettings,
        cx: &mut Context<Self>,
    ) -> Self {
        // Redraw once a second, so the time left in the current bar counts down.
        let clock = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let alive = this.update(cx, |this, cx| {
                    if this.timeframe.bar_ms().is_some() && matches!(this.load, Load::Ready) {
                        cx.notify();
                    }
                    // Also the moment to ask again for flow that failed to load.
                    this.ensure_flow(cx);
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        let settings = settings.normalized();
        Self {
            hub,
            id,
            session,
            symbol: None,
            hours: None,
            selected: true,
            timeframe,
            view: View::new(bar_px_for(&settings, timeframe)),
            settings,
            series: empty_series(timeframe),
            display: Display::default(),
            flow: Flow::default(),
            flow_load: FlowLoad::Idle,
            flow_held: Vec::new(),
            flow_newest_pending: false,
            flow_gap: false,
            flow_epoch: 0,
            load: Load::Idle,
            older: Older::Idle,
            epoch: 0,
            bid: None,
            ask: None,
            last_time_ms: 0,
            group_tail: Vec::new(),
            hover: None,
            remote: None,
            drag: None,
            drawings: None,
            _drawings_observe: None,
            drawing_drag: false,
            over_drawing: None,
            settings_for: None,
            lines: Vec::new(),
            menu: None,
            context_at: None,
            bounds: Rc::new(Cell::new(None)),
            _clock: clock,
        }
    }

    pub fn timeframe(&self) -> Timeframe {
        self.timeframe
    }

    pub fn settings(&self) -> &ChartSettings {
        &self.settings
    }

    pub fn symbol(&self) -> Option<&Symbol> {
        self.symbol.as_ref()
    }

    /// The last bid and ask seen for the chart's symbol.
    pub fn quote(&self) -> (Option<i64>, Option<i64>) {
        (self.bid, self.ask)
    }

    // ---- what to show ----

    /// The trading hours of the chart's symbol (ignored for another symbol).
    pub fn set_hours(
        &mut self,
        id: i64,
        hours: Option<std::sync::Arc<wyck::openapi::market::TradingHours>>,
        cx: &mut Context<Self>,
    ) {
        let hours = hours.filter(|_| self.symbol.as_ref().is_some_and(|s| s.id == id));
        let same = match (&self.hours, &hours) {
            (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.hours = hours;
            cx.notify();
        }
    }

    /// Whether this is the chart the user works on.
    pub fn set_selected(&mut self, selected: bool, cx: &mut Context<Self>) {
        if self.selected != selected {
            self.selected = selected;
            if !selected {
                self.menu = None;
            }
            cx.notify();
        }
    }

    /// Where the market of the chart's symbol stands now, when its hours are known.
    pub fn market_status(&self) -> Option<wyck::openapi::market::MarketStatus> {
        Some(self.hours.as_ref()?.status_at(now_ms()))
    }

    pub fn set_symbol(&mut self, id: i64, name: SharedString, digits: u32, cx: &mut Context<Self>) {
        if self.symbol.as_ref().is_some_and(|s| s.id == id) {
            return;
        }
        self.symbol = Some(Symbol { id, name, digits });
        self.lines.clear();
        self.reload(cx);
    }

    /// The broker said how the symbol is quoted.
    pub fn set_digits(&mut self, id: i64, digits: u32, cx: &mut Context<Self>) {
        if let Some(symbol) = self.symbol.as_mut().filter(|s| s.id == id)
            && symbol.digits != digits
        {
            symbol.digits = digits;
            self.data_changed(cx);
        }
    }

    pub fn set_timeframe(&mut self, timeframe: Timeframe, cx: &mut Context<Self>) {
        if self.timeframe == timeframe {
            return;
        }
        self.timeframe = timeframe;
        self.reload(cx);
        cx.emit(ChartEvent::SettingsChanged);
    }

    /// Changes the settings with `change`; recomputes what depends on them, redraws, and tells the
    /// layout (which saves them) when they really changed.
    pub fn edit_settings(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut ChartSettings),
    ) {
        let before = self.settings.clone();
        change(&mut self.settings);
        self.settings = std::mem::take(&mut self.settings).normalized();
        if self.settings == before {
            return;
        }
        let layout_changed = before.kind != self.settings.kind
            || (self.settings.kind.is_derived() && before.transform != self.settings.transform);
        let data_changed = layout_changed
            || before.studies != self.settings.studies
            || before.zone != self.settings.zone;
        if data_changed {
            self.rebuild_display();
        }
        if before.kind != self.settings.kind {
            let footprint = self.settings.kind == ChartKind::Footprint;
            if footprint {
                self.view.bar_px = self.view.bar_px.max(footprint::DEFAULT_BAR_PX);
            } else if before.kind == ChartKind::Footprint {
                // Bars as wide as a footprint's would be absurd as candles.
                self.view.bar_px = self.timeframe.default_bar_px();
                self.reset_flow();
            }
        }
        if layout_changed {
            // A different construction has a different number of points: start from the end.
            self.view = View::new(self.view.bar_px);
        }
        if before.scale != self.settings.scale || before.invert != self.settings.invert {
            self.view.price = view::PriceScale::Auto;
        }
        cx.emit(ChartEvent::SettingsChanged);
        cx.notify();
        self.ensure_flow(cx);
    }

    pub fn set_kind(&mut self, kind: ChartKind, cx: &mut Context<Self>) {
        self.edit_settings(cx, |s| s.kind = kind);
    }

    /// Adds an indicator, with its defaults.
    pub fn add_study(&mut self, config: StudyConfig, cx: &mut Context<Self>) {
        self.edit_settings(cx, |s| s.studies.push(config));
    }

    pub fn remove_study(&mut self, index: usize, cx: &mut Context<Self>) {
        self.edit_settings(cx, |s| {
            if index < s.studies.len() {
                s.studies.remove(index);
            }
        });
    }

    /// The orders, positions and alerts to show.
    pub fn set_lines(&mut self, lines: Vec<ChartLine>, cx: &mut Context<Self>) {
        if self.lines != lines {
            self.lines = lines;
            cx.notify();
        }
    }

    pub fn digits(&self) -> u32 {
        self.symbol.as_ref().map_or(5, |s| s.digits)
    }

    /// The colors this chart draws with: the theme's, and what the chart overrides.
    fn palette(&self) -> scene::Palette {
        scene::Palette::for_chart(&self.settings.colors)
    }

    /// The text written behind the prices, when the chart has a watermark.
    fn watermark(&self) -> Option<String> {
        if !self.settings.watermark {
            return None;
        }
        let symbol = self.symbol.as_ref()?;
        Some(format!("{}, {}", symbol.name, self.timeframe.label()))
    }

    /// One quote unit in raw price units.
    fn unit(&self) -> i64 {
        scene::quote_unit(self.digits()).max(1.0) as i64
    }

    /// Recomputes what is drawn from the prices. Keeps what the user looks at in place when the
    /// number of points of a construction grows, unless they are following the newest.
    fn rebuild_display(&mut self) {
        let before = self.display.shown(&self.series).len();
        self.display = Display::build(&self.series, &self.settings, self.unit());
        let after = self.display.shown(&self.series).len();
        if after > before && self.display.is_derived() {
            self.view.on_appended(after - before);
        }
    }

    /// The prices changed: recompute what depends on them and redraw.
    fn data_changed(&mut self, cx: &mut Context<Self>) {
        self.rebuild_display();
        cx.notify();
    }

    /// The series on screen.
    fn shown(&self) -> &Series {
        self.display.shown(&self.series)
    }

    /// The time between two points on screen, for placing times past the data.
    fn step_ms(&self) -> f64 {
        let nominal = if self.display.is_derived() && self.settings.kind != ChartKind::HeikinAshi {
            None
        } else {
            self.timeframe.bar_ms()
        };
        self.shown().step_ms(nominal)
    }

    /// The size of the chart as it was last drawn.
    fn size(&self) -> (f64, f64) {
        match self.bounds.get() {
            Some(bounds) => (
                f64::from(f32::from(bounds.size.width)),
                f64::from(f32::from(bounds.size.height)),
            ),
            None => (1_000.0, 600.0),
        }
    }

    /// The bands of the chart as it was last drawn.
    fn geometry(&self) -> Geometry {
        let (w, h) = self.size();
        scene::geometry(&self.settings, w, h)
    }

    /// The price scale of the prices band as it is now.
    fn main_map(&self) -> Option<scene::PriceMap> {
        scene::main_map(
            &self.series,
            &self.display,
            &self.settings,
            &self.view,
            &self.geometry(),
            self.digits(),
        )
    }

    fn is_compact(&self) -> bool {
        self.bounds.get().is_some_and(|b| {
            f32::from(b.size.width) < COMPACT_WIDTH || f32::from(b.size.height) < COMPACT_HEIGHT
        })
    }
}

impl Drop for Chart {
    fn drop(&mut self) {
        self.hub.set(self.id, None);
    }
}

/// Pixels per bar when a chart is first shown: a footprint needs room for its numbers.
fn bar_px_for(settings: &ChartSettings, timeframe: Timeframe) -> f64 {
    if settings.kind == ChartKind::Footprint && footprint::supports(timeframe) {
        footprint::DEFAULT_BAR_PX
    } else {
        timeframe.default_bar_px()
    }
}

fn empty_series(timeframe: Timeframe) -> Series {
    match timeframe {
        Timeframe::Ticks => Series::Ticks(Vec::new()),
        _ => Series::Bars(Vec::new()),
    }
}

fn flatten<T>(result: Result<ApiResult<T>, tokio::task::JoinError>) -> ApiResult<T> {
    match result {
        Ok(inner) => inner,
        Err(_) => Err(OpenApiError::Closed),
    }
}

/// The current time in Unix milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
}
