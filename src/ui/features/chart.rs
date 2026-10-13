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
//! | [`volume`], `volume_ui` | volume candles (as wide as their volume) and volume bars (a bar every set volume) |
//! | [`tpo`], `tpo_ui` | the TPO chart type: sessions laid out as market profiles, with their levels |
//! | [`export`](crate::domain::chart::export), `export_ui` | exporting the data of a chart to a file or the clipboard, and its panel |
//!
//! The data, view, axis, study, transform, volume, tpo and export modules are plain data in and
//! out.
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
//! - Drawings ([`crate::app::drawings`]) are anchored to times and prices, so they follow the chart when it
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

pub mod api;
mod chart_settings_ui;
mod construction_ui;
mod corner;
mod custom_runs;
pub mod drawing_props;
pub mod events;
mod export_ui;
mod flow_sync;
mod follow;
mod footprint_ui;
mod glue;
mod header_menus;
mod history;
mod indicator_picker;
mod input;
pub mod lines;
pub mod object_tree;
mod overlay;
mod paint;
pub mod palette;
mod path_pieces;
pub mod raster;
mod settings_rows;
mod study_settings;
mod tpo_ui;
mod volume_ui;

pub use self::chart_settings_ui::open as open_chart_settings;
use self::data::Series;
use self::display::Display;
use self::events::{ChartEvent, Hover};
use self::flow::Flow;
use self::flow_sync::{FlowLoad, HeldQuote};
pub use self::glue::{DrawingCommand, open_drawing_settings, open_object_tree};
pub use self::lines::{ChartLine, LineId};
pub use self::overlay::kind_icon;
use self::scene::Geometry;
pub use self::settings::{ChartKind, ChartSettings};
use self::study::config::StudyConfig;
pub use self::timeframe::{GROUPS, Timeframe, Unit};
use self::view::View;
use crate::app::broker::session::Session;
use crate::app::broker::{Error as ApiError, Result as ApiResult};
use crate::app::drawings::Drawings;
use crate::app::market_data::live::LiveHub;
use crate::domain::chart::axis;
use crate::domain::chart::data;
use crate::domain::chart::display;
use crate::domain::chart::flow;
use crate::domain::chart::footprint;
use crate::domain::chart::options;
use crate::domain::chart::projection;
use crate::domain::chart::scene;
use crate::domain::chart::settings;
use crate::domain::chart::timeframe;
use crate::domain::chart::tpo;
use crate::domain::chart::transform;
use crate::domain::chart::view;
use crate::domain::chart::volume;
use crate::domain::chart::zone;
use crate::domain::indicators as study;
use gpui::{Bounds, Context, Entity, EventEmitter, Pixels, SharedString};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

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
        ChartScreenshotAll,
        ChartAddAlert,
        ChartCopyIndicators,
        ChartPaste,
        SelectAllDrawings,
        FindTool,
        NudgeUp,
        NudgeDown,
        NudgeBackFast,
        NudgeForwardFast,
        NudgeUpFast,
        NudgeDownFast,
        ToolTrendLine,
        ToolHorizontal,
        ToolVertical,
        ToolFib,
        ToolRectangle,
        ToolText,
        ToolMeasure,
        ToolBrush,
    ]
);

/// A chart smaller than this shows fewer numbers and a smaller toolbar.
const COMPACT_WIDTH: f32 = 640.0;
const COMPACT_HEIGHT: f32 = 300.0;

/// The symbol on the chart.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub id: i64,
    pub name: SharedString,
    pub digits: u32,
    pub pip_position: Option<i64>,
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
    Zone,
}

impl Chart {
    /// Closes the menu that is open, if any. Returns whether there was one.
    pub fn close_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let was_open = self.menu.take().is_some();
        if was_open {
            cx.notify();
        }
        was_open
    }
}

impl EventEmitter<ChartEvent> for Chart {}

pub struct Chart {
    session: Session,
    hub: Rc<LiveHub>,
    /// Tells this chart from the others in the hub.
    id: u64,
    symbol: Option<Symbol>,
    /// When the symbol trades, once its details are known.
    hours: Option<std::sync::Arc<crate::domain::market::TradingHours>>,
    /// Whether this is the chart the user works on; only that one shows its toolbar when there
    /// are several.
    selected: bool,
    /// How far from the bottom of the chart the hint of a drawing tool must stay, to clear what floats
    /// over the chart there (the favorites pill). 0 when nothing does.
    hint_floor: f32,
    timeframe: Timeframe,
    settings: ChartSettings,
    max_studies: usize,
    /// The prices, as held.
    series: Series,
    atr_history: HashMap<(i64, Timeframe), (Vec<crate::domain::market::Bar>, i64)>,
    atr_loading: HashSet<(i64, Timeframe)>,
    /// The stop an ATR following position drawing last got, by drawing, for the moments the ATR
    /// is not at hand (see `resolved_position`).
    atr_stops: std::cell::RefCell<HashMap<u64, f64>>,
    /// What is drawn from them.
    display: Display,
    /// The indicators written as scripts, run off the interface thread.
    custom: custom_runs::Custom,
    /// The ids of the symbols of the account by name (see `key_of`), for the indicators that read
    /// the prices of a second symbol.
    symbol_ids: std::sync::Arc<std::collections::HashMap<String, i64>>,
    _library: gpui::Subscription,
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
    group_tail: Vec<crate::domain::market::Bar>,
    hover: Option<(f32, f32)>,
    /// The pointer of another chart, when the crosshairs are linked.
    remote: Option<Hover>,
    /// A linked chart's picked time, kept while older history is being loaded.
    pending_focus: Option<i64>,
    drag: Option<input::Drag>,
    /// The drawings shared by the charts, and the subscription that redraws this chart when
    /// they change.
    drawings: Option<Entity<Drawings>>,
    /// How many charts the layout holds, for what only makes sense with several.
    layout_charts: usize,
    _drawings_observe: Option<gpui::Subscription>,
    /// Whether a press taken by a drawing is still down.
    drawing_drag: bool,
    /// Whether the pointer is over a drawing, for the mouse cursor.
    over_drawing: Option<crate::domain::drawings::book::Grab>,
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
        max_studies: usize,
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
                    this.request_drawing_atr(cx);
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
            hint_floor: 0.0,
            timeframe,
            view: View::new(bar_px_for(&settings, timeframe)),
            settings,
            max_studies: max_studies.clamp(1, settings::MAX_STUDIES),
            series: empty_series(timeframe),
            atr_history: HashMap::new(),
            atr_loading: HashSet::new(),
            atr_stops: std::cell::RefCell::default(),
            display: Display::default(),
            custom: custom_runs::Custom::default(),
            symbol_ids: std::sync::Arc::default(),
            _library: crate::app::scripts::observe(cx, |this: &mut Self, cx| {
                this.library_changed(cx);
            }),
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
            pending_focus: None,
            drag: None,
            drawings: None,
            layout_charts: 1,
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

    /// The colors this chart draws with: the theme's, and what the chart overrides.
    fn palette(&self) -> palette::DesktopPalette {
        let palette = palette::palette_for_chart(&self.settings.colors);
        palette::DesktopPalette {
            up: palette::gpui_rgba(palette.up),
            down: palette::gpui_rgba(palette.down),
            text: palette::gpui_rgba(palette.text),
            text_strong: palette::gpui_rgba(palette.text_strong),
            bg: palette::gpui_rgba(palette.bg),
        }
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
        // The scripts run apart, and what they last gave is put back for the chart to draw.
        self.custom.data_changed(self.series.time_at(0));
        self.custom.apply(&mut self.display.studies);
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
        scene::scale::geometry(&self.settings, w, h)
    }

    /// The price scale of the prices band as it is now.
    fn main_map(&self) -> Option<scene::PriceMap> {
        scene::scale::main_map(
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

fn flatten<T>(
    result: Result<ApiResult<T>, crate::app::system::runtime::JoinError>,
) -> ApiResult<T> {
    match result {
        Ok(inner) => inner,
        Err(_) => Err(ApiError::Closed),
    }
}
