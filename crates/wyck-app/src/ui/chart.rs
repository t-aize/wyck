//! The price chart: candles, axes, crosshair, and the loading of the bars behind them.
//!
//! [`ChartView`] is a view of its own, held by the dashboard. It draws a
//! [`ChartModel`] with GPUI's low level painting (one
//! `canvas`), turns the raw mouse and keyboard input into calls on that model, and keeps the
//! model supplied with bars. All decisions (zoom, pan, price range, what to fetch) are in
//! [`crate::chart`] and tested there; this file only draws and plumbs.
//!
//! # Where the bars come from
//!
//! Following a target (symbol, time frame, server family, and whether the session is ready):
//!
//! 1. The bars saved on disk are read and drawn at once, so a chart the user has seen before
//!    appears immediately.
//! 2. When the session is ready the tail is fetched (see [`crate::chart::history`]): everything
//!    since the last covered time, merged in and saved. A fresh symbol also loads a few older
//!    pages so the screen is full.
//! 3. Every second the engine's quote is folded into the newest bar. The polled quote misses the
//!    ticks in between, so every 15 seconds the tail is fetched again and the server's own bar
//!    replaces the one built from quotes.
//! 4. Scrolling near the oldest bar loads one older page, from disk first, then from the server
//!    for the parts not covered yet.
//!
//! A late answer for a target the user has already left is dropped: every target change bumps a
//! generation number, and each task checks it before touching the view.
//!
//! # Drawing
//!
//! The plot is painted with pixel snapped rectangles (see [`crate::chart::lod`]), so a candle is
//! a wick and a body, a thin line, or a per-pixel column depending on the zoom. Text (axis
//! labels, tags) is shaped and painted by hand in the same pass. The legend, the status message
//! and the "latest" button are ordinary elements laid over the canvas.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Bounds, ContentMask, Context, Div, Entity, Font, FontStyle, FontWeight, Hsla, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render,
    ScrollWheelEvent, SharedString, Task, TextAlign, TextRun, Window, canvas, div, fill, point, px,
    size,
};
use wyck_config::AppPaths;
use wyck_engine::EngineError;
use wyck_engine::domain::{Candle, UnixMillis, now_millis};

use super::theme::{self, sz};
use super::widgets::{Glyph, glyph, tip};
use crate::chart::coverage::Coverage;
use crate::chart::history::{
    EARLIEST, PAGE_BARS, fetch_span, older_span, page_bars, tail_span, unit_millis,
};
use crate::chart::interaction::{ChartKey, ChartModel};
use crate::chart::lod::{Detail, Rect, candle_shape, columns, detail_for, line_shape};
use crate::chart::scale::format_price;
use crate::chart::store::{CandleStore, SeriesKey};
use crate::chart::timeaxis::{full_text, labels};
use crate::dashboard::Timeframe;
use crate::shell::Shell;

/// The room the auto and log buttons take at the right end of the time axis, in design pixels.
const BUTTONS_WIDTH: f32 = 96.;

/// How often the tail is fetched again while the chart is open, to replace the bar built from
/// polled quotes with the server's.
const REFRESH: Duration = Duration::from_secs(15);

/// A fresh symbol keeps loading older pages until it has at least this many bars, so the first
/// screen is full at any zoom.
const INITIAL_BARS: usize = 400;

/// The most older pages fetched in one go for a fresh symbol (pages are short for weekly and
/// monthly bars, see [`page_bars`]), so a first screen is only ever a few requests.
const INITIAL_PAGES: usize = 3;

/// Pages in a row that may come back empty before the history is taken to start there. Eight
/// pages of one-minute bars are a little over five days, longer than any market closure.
const EMPTY_PAGES: usize = 8;

/// No older bars are loaded past this many, which bounds memory (about 10 MB).
const MAX_BARS: usize = 200_000;

/// What the chart follows. The view reloads when the series changes and refreshes when the
/// session becomes ready.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The traded symbol.
    pub symbol: String,
    /// The time frame.
    pub timeframe: Timeframe,
    /// The server family (`remote` or `local`): the cache keeps them apart.
    pub namespace: &'static str,
    /// Whether the session can answer requests.
    pub ready: bool,
}

/// Where the newest bars stand.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    /// Waiting for the first bars.
    Loading,
    /// Bars are on screen.
    Ready,
    /// The last request failed. The text is for the user.
    Failed(String),
}

/// Older history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Older {
    /// Nothing in flight; more may exist.
    Idle,
    /// A page is being loaded.
    Loading,
    /// The history starts here (or the memory cap was reached).
    Exhausted,
}

/// The chart view. See the [module docs](self).
pub struct ChartView {
    shell: Shell,
    model: ChartModel,
    store: Option<Arc<CandleStore>>,
    target: Option<Target>,
    coverage: Coverage,
    generation: u64,
    status: Status,
    older: Older,
    fetching_tail: bool,
    /// The drawing area's origin in window coordinates, to turn event positions into local ones.
    origin: Point<Pixels>,
    _refresh: Task<()>,
}

fn px_f64(p: Pixels) -> f64 {
    f64::from(f32::from(p))
}

fn f(value: f64) -> f32 {
    value as f32
}

impl ChartView {
    /// A chart with no target yet. The bar cache is opened in the background.
    pub fn new(shell: Shell, cx: &mut Context<Self>) -> Self {
        let opening = cx.background_executor().spawn(async {
            let path = AppPaths::discover()
                .ok()?
                .data_dir()
                .join("cache")
                .join("candles.redb");
            match CandleStore::open_or_reset(&path) {
                Ok(store) => Some(Arc::new(store)),
                Err(error) => {
                    tracing::warn!(%error, "the bar cache is unavailable, running without it");
                    None
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let store = opening.await;
            this.update(cx, |this, cx| {
                this.store = store;
                // A target set before the store was ready starts over so it can read the cache.
                if this.target.is_some() && this.model.series.is_empty() {
                    this.begin_load(cx);
                }
            })
            .ok();
        })
        .detach();

        let refresh = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(REFRESH).await;
                if this.update(cx, |this, cx| this.refresh_tail(cx)).is_err() {
                    break;
                }
            }
        });

        Self {
            shell,
            model: ChartModel::new(Timeframe::default().period(), 64.0, 24.0),
            store: None,
            target: None,
            coverage: Coverage::new(),
            generation: 0,
            status: Status::Loading,
            older: Older::Idle,
            fetching_tail: false,
            origin: point(px(0.), px(0.)),
            _refresh: refresh,
        }
    }

    // ---- following a target ----

    /// Points the chart at `target`. Nothing happens when it is the one it already follows.
    pub fn retarget(&mut self, target: Target, cx: &mut Context<Self>) {
        if self.target.as_ref() == Some(&target) {
            return;
        }
        let same_series = self.target.as_ref().is_some_and(|t| {
            t.symbol == target.symbol
                && t.timeframe == target.timeframe
                && t.namespace == target.namespace
        });
        let became_ready = target.ready && self.target.as_ref().is_none_or(|t| !t.ready);
        self.target = Some(target);
        if same_series {
            if became_ready {
                self.refresh_tail(cx);
            }
            cx.notify();
        } else {
            self.begin_load(cx);
        }
    }

    /// Starts over for the current target: clears the view, reads the cache, then fetches.
    fn begin_load(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        let period = target.timeframe.period();
        let digits = self
            .shell
            .model
            .read(cx)
            .state
            .instruments
            .get(&target.symbol)
            .map_or(5, |i| i.price_digits);
        self.model.clear(period, digits);
        self.coverage = Coverage::new();
        self.status = Status::Loading;
        self.older = Older::Idle;
        self.fetching_tail = false;
        cx.notify();

        let key = SeriesKey::new(target.namespace, &target.symbol, period);
        let store = self.store.clone();
        cx.spawn(async move |this, cx| {
            // 1. The cache, drawn at once.
            if let Some(store) = store {
                let read_key = key.clone();
                let cached = cx
                    .background_executor()
                    .spawn(async move {
                        let bars = store.load_latest(&read_key, i64::MAX, PAGE_BARS as usize);
                        let coverage = store.coverage(&read_key);
                        (bars, coverage)
                    })
                    .await;
                let current = this.update(cx, |this, cx| {
                    if this.generation != generation {
                        return false;
                    }
                    match cached {
                        (Ok(bars), Ok(coverage)) => {
                            this.coverage = coverage;
                            if this.model.merge(bars) {
                                this.status = Status::Ready;
                            }
                        }
                        (Err(error), _) | (_, Err(error)) => {
                            tracing::warn!(%error, "the bar cache could not be read");
                        }
                    }
                    cx.notify();
                    true
                });
                if !current.unwrap_or(false) {
                    return;
                }
            }
            // 2. The server: the digits of the symbol, then the tail.
            this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.load_digits(cx);
                    this.refresh_tail(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Reads the decimals of the symbol from the engine, when the state does not have them yet.
    fn load_digits(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        if !target.ready {
            return;
        }
        let generation = self.generation;
        let handle = self.shell.controller.handle();
        cx.spawn(async move |this, cx| {
            if let Ok(instrument) = handle.instrument(&target.symbol).await {
                this.update(cx, |this, cx| {
                    if this.generation == generation && this.model.digits != instrument.price_digits
                    {
                        this.model.digits = instrument.price_digits;
                        cx.notify();
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    /// Fetches everything since the last covered time and merges it in.
    fn refresh_tail(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        if !target.ready || self.fetching_tail {
            return;
        }
        self.fetching_tail = true;
        let generation = self.generation;
        let period = target.timeframe.period();
        let now = now_millis();
        let span = tail_span(&self.coverage, now, period);
        let key = SeriesKey::new(target.namespace, &target.symbol, period);
        let store = self.store.clone();
        let handle = self.shell.controller.handle();
        let symbol = target.symbol.clone();

        cx.spawn(async move |this, cx| {
            let fetched = fetch_span(span, now, period, |from, to| {
                let handle = handle.clone();
                let symbol = symbol.clone();
                async move { handle.candles(&symbol, period, from, to).await }
            })
            .await;
            let saved = match &fetched {
                Ok(fetched) => {
                    save_in_background(cx, store, key, fetched.bars.clone(), fetched.covered).await
                }
                Err(_) => None,
            };
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.fetching_tail = false;
                match fetched {
                    Ok(fetched) => {
                        // The coverage is kept in memory even when the disk write failed: the bars
                        // are in the series either way.
                        let _ = saved;
                        if let Some((from, to)) = fetched.covered {
                            this.coverage.add(from, to);
                        }
                        this.model.merge(fetched.bars);
                        this.status = if this.model.series.is_empty() {
                            Status::Loading
                        } else {
                            Status::Ready
                        };
                        this.fill_first_screen(cx);
                    }
                    Err(error) => this.load_failed(&error),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn load_failed(&mut self, error: &EngineError) {
        tracing::warn!(%error, "the price history could not be loaded");
        // With bars on screen a failed refresh is only logged: the chart is still usable and the
        // next refresh tries again.
        if self.model.series.is_empty() {
            self.status = Status::Failed(crate::messages::describe_error(error).title);
        }
    }

    /// A fresh symbol has one page; load older ones until the screen is full.
    fn fill_first_screen(&mut self, cx: &mut Context<Self>) {
        if self.model.series.len() < INITIAL_BARS && self.older == Older::Idle {
            let pages = if self.model.series.is_empty() {
                EMPTY_PAGES
            } else {
                INITIAL_PAGES
            };
            self.load_older(pages, INITIAL_BARS, cx);
        }
    }

    /// Loads up to `pages` older pages, stopping early once the series holds `enough` bars.
    fn load_older(&mut self, pages: usize, enough: usize, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        if !target.ready || self.older != Older::Idle {
            return;
        }
        // With no bars at all (a closed market, a symbol with a quiet stretch) start from now and
        // walk back until bars turn up.
        let oldest = self
            .model
            .series
            .first_time()
            .unwrap_or_else(|| now_millis() + unit_millis(target.timeframe.period()));
        if self.model.series.len() >= MAX_BARS || oldest <= EARLIEST {
            self.older = Older::Exhausted;
            return;
        }
        self.older = Older::Loading;
        let generation = self.generation;
        let period = target.timeframe.period();
        let key = SeriesKey::new(target.namespace, &target.symbol, period);
        let store = self.store.clone();
        let handle = self.shell.controller.handle();
        let symbol = target.symbol.clone();
        let mut coverage = self.coverage.clone();
        let have = self.model.series.len();

        cx.spawn(async move |this, cx| {
            let mut before = oldest;
            let mut empty_run = 0;
            let mut gathered: Vec<Candle> = Vec::new();
            let mut failure = None;
            let now = now_millis();
            'pages: for _ in 0..pages.max(1) {
                let (from, to) = older_span(before, period);
                let from = from.max(EARLIEST);
                if to <= from {
                    empty_run = EMPTY_PAGES;
                    break;
                }
                let mut page: Vec<Candle> = Vec::new();
                // What the disk already holds for this page.
                if let Some(store) = store.clone() {
                    let read_key = key.clone();
                    let read = cx
                        .background_executor()
                        .spawn(async move { store.load(&read_key, from, to) })
                        .await;
                    if let Ok(bars) = read {
                        page.extend(bars);
                    }
                }
                // What it does not.
                for (gap_from, gap_to) in coverage.missing(from, to) {
                    let fetched = fetch_span((gap_from, gap_to), now, period, |a, b| {
                        let handle = handle.clone();
                        let symbol = symbol.clone();
                        async move { handle.candles(&symbol, period, a, b).await }
                    })
                    .await;
                    match fetched {
                        Ok(fetched) => {
                            save_in_background(
                                cx,
                                store.clone(),
                                key.clone(),
                                fetched.bars.clone(),
                                fetched.covered,
                            )
                            .await;
                            if let Some((a, b)) = fetched.covered {
                                coverage.add(a, b);
                            }
                            page.extend(fetched.bars);
                        }
                        Err(error) => {
                            failure = Some(error);
                            break 'pages;
                        }
                    }
                }
                if page.is_empty() {
                    empty_run += 1;
                    if empty_run >= EMPTY_PAGES {
                        break;
                    }
                } else {
                    empty_run = 0;
                    gathered.extend(page);
                    if gathered.len() + have >= enough {
                        break;
                    }
                }
                before = from;
            }
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.coverage = coverage;
                let added = !gathered.is_empty();
                this.model.merge(gathered);
                this.older = if failure.is_some() {
                    // A failure is not the start of history: try again on the next scroll.
                    Older::Idle
                } else if empty_run >= EMPTY_PAGES {
                    Older::Exhausted
                } else {
                    Older::Idle
                };
                let failed = failure.is_some();
                if let Some(error) = failure {
                    tracing::warn!(%error, "older price history could not be loaded");
                }
                // Short pages (weekly, monthly) may leave the screen only part full: go on while
                // the view still reaches past the oldest bar and the last page brought bars.
                if added && !failed {
                    this.after_view_change(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Asks again after a failure.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.begin_load(cx);
    }

    // ---- live prices ----

    /// Folds the latest bid into the newest bar. `at` is when the broker stamped it. A quote with
    /// no stamp is ignored: guessing the time would turn an old closing price into new bars while
    /// the market is shut.
    pub fn on_price(&mut self, bid: f64, at: Option<UnixMillis>, cx: &mut Context<Self>) {
        let Some(at) = at else {
            return;
        };
        if self.model.apply_price(bid, at) {
            cx.notify();
        }
    }

    // ---- input ----

    fn local(&self, position: Point<Pixels>) -> (f64, f64) {
        (
            px_f64(position.x - self.origin.x),
            px_f64(position.y - self.origin.y),
        )
    }

    fn after_view_change(&mut self, cx: &mut Context<Self>) {
        if self.model.wants_older() {
            // Enough pages to cover a screen, so a weekly or monthly chart (short pages) does not
            // crawl back a few bars at a time.
            let across = self.model.viewport.bars_across();
            let page = page_bars(self.model.period) as f64;
            let pages = (across / page).ceil().clamp(1.0, 8.0) as usize;
            self.load_older(pages, usize::MAX, cx);
        }
        cx.notify();
    }

    fn on_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let (x, y) = self.local(event.position);
        let delta = event.delta.pixel_delta(px(24.));
        let ctrl = event.modifiers.control || event.modifiers.platform;
        if self.model.wheel(
            x,
            y,
            px_f64(delta.x),
            px_f64(delta.y),
            ctrl,
            event.modifiers.shift,
        ) {
            self.after_view_change(cx);
        }
    }

    fn on_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let (x, y) = self.local(event.position);
        if self.model.press(x, y, event.click_count) {
            self.after_view_change(cx);
        }
        cx.notify();
    }

    fn on_move(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let (x, y) = self.local(position);
        if self.model.is_dragging() {
            if self.model.drag_to(x, y) {
                self.after_view_change(cx);
            }
        } else if self.model.hover(x, y) {
            cx.notify();
        }
    }

    fn on_up(&mut self, cx: &mut Context<Self>) {
        if self.model.release() {
            cx.notify();
        }
    }

    /// A key the dashboard forwards. Returns whether the chart took it.
    pub fn on_keystroke(
        &mut self,
        keystroke: &gpui_kit::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let modifiers = &keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform {
            return false;
        }
        let key = match keystroke.key.as_str() {
            "left" => ChartKey::Left,
            "right" => ChartKey::Right,
            "+" | "=" => ChartKey::ZoomIn,
            "-" => ChartKey::ZoomOut,
            "end" => ChartKey::Latest,
            "home" | "0" => ChartKey::Reset,
            _ => return false,
        };
        if self.model.key(key) {
            self.after_view_change(cx);
        }
        true
    }

    // ---- drawing ----

    fn set_bounds(&mut self, bounds: Bounds<Pixels>) {
        self.origin = bounds.origin;
        self.model.set_size(
            px_f64(bounds.size.width),
            px_f64(bounds.size.height),
            px_f64(sz(64.)),
            px_f64(sz(24.)),
        );
    }

    /// Paints the plot, the axes, the last price and the crosshair.
    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let model = &self.model;
        let (ox, oy) = (px_f64(bounds.origin.x), px_f64(bounds.origin.y));
        let plot_w = model.plot_width();
        let plot_h = model.plot_height();
        let (width, height) = model.size();
        let n = model.series.len();
        let bars = model.series.bars();
        let vp = &model.viewport;
        let scale = &model.scale;
        let up = theme::green();
        let down = theme::red();
        let grid = theme::alpha(theme::fg(), 0.05);
        let line = theme::border();
        let dim = theme::dim();

        let rect = |window: &mut Window, r: Rect, color: Hsla| {
            window.paint_quad(fill(
                Bounds::new(
                    point(px(f(ox + r.x)), px(f(oy + r.y))),
                    size(px(f(r.w)), px(f(r.h))),
                ),
                color,
            ));
        };

        // Grid lines under everything.
        let (ticks, price_digits) = scale.grid(plot_h, model.digits);
        for price in &ticks {
            let y = scale.y_of(*price, plot_h).round();
            rect(
                window,
                Rect {
                    x: 0.0,
                    y,
                    w: plot_w,
                    h: 1.0,
                },
                grid,
            );
        }
        // The right end of the time axis holds the scale buttons: no labels under them.
        let mut time_labels = labels(bars, vp, model.period, px_f64(sz(84.)));
        time_labels.retain(|label| label.x <= plot_w - px_f64(sz(BUTTONS_WIDTH)));
        for label in &time_labels {
            let x = label.x.floor();
            rect(
                window,
                Rect {
                    x,
                    y: 0.0,
                    w: 1.0,
                    h: plot_h,
                },
                grid,
            );
        }

        // Bars, clipped to the plot.
        let plot_bounds = Bounds::new(bounds.origin, size(px(f(plot_w)), px(f(plot_h))));
        window.with_content_mask(
            Some(ContentMask {
                bounds: plot_bounds,
            }),
            |window| {
                let range = vp.visible(n);
                match detail_for(vp.bar_spacing) {
                    Detail::Candles => {
                        for i in range {
                            let bar = &bars[i];
                            let color = if bar.is_up() { up } else { down };
                            let shape = candle_shape(
                                vp.x_of(i as f64, n),
                                vp.bar_spacing,
                                scale.y_of(bar.open, plot_h),
                                scale.y_of(bar.close, plot_h),
                                scale.y_of(bar.high, plot_h),
                                scale.y_of(bar.low, plot_h),
                            );
                            rect(window, shape.wick, color);
                            rect(window, shape.body, color);
                        }
                    }
                    Detail::Lines => {
                        for i in range {
                            let bar = &bars[i];
                            let color = if bar.is_up() { up } else { down };
                            rect(
                                window,
                                line_shape(
                                    vp.x_of(i as f64, n),
                                    scale.y_of(bar.high, plot_h),
                                    scale.y_of(bar.low, plot_h),
                                ),
                                color,
                            );
                        }
                    }
                    Detail::Columns => {
                        for column in columns(bars, range, vp) {
                            let color = if column.up { up } else { down };
                            rect(
                                window,
                                line_shape(
                                    column.x,
                                    scale.y_of(column.high, plot_h),
                                    scale.y_of(column.low, plot_h),
                                ),
                                color,
                            );
                        }
                    }
                }
            },
        );

        // The last price: a dotted line across the plot, and a tag on the axis.
        if let Some(last) = model.series.last() {
            let y = scale.y_of(last.close, plot_h);
            if (0.0..=plot_h).contains(&y) {
                let color = if last.is_up() { up } else { down };
                let y = y.round();
                let mut x = 0.0;
                while x < plot_w {
                    rect(
                        window,
                        Rect {
                            x,
                            y,
                            w: 3.0,
                            h: 1.0,
                        },
                        theme::alpha(color, 0.6),
                    );
                    x += 6.0;
                }
            }
        }

        // The axes: their own ground and the hairline against the plot.
        let axis_w = width - plot_w;
        let axis_h = height - plot_h;
        rect(
            window,
            Rect {
                x: plot_w,
                y: 0.0,
                w: axis_w,
                h: height,
            },
            theme::bg(),
        );
        rect(
            window,
            Rect {
                x: 0.0,
                y: plot_h,
                w: width,
                h: axis_h,
            },
            theme::bg(),
        );
        rect(
            window,
            Rect {
                x: plot_w,
                y: 0.0,
                w: 1.0,
                h: height,
            },
            line,
        );
        rect(
            window,
            Rect {
                x: 0.0,
                y: plot_h,
                w: width,
                h: 1.0,
            },
            line,
        );

        let text_size = sz(11.);
        let pad = px_f64(sz(8.));
        for price in &ticks {
            let y = scale.y_of(*price, plot_h);
            paint_text(
                window,
                cx,
                &format_price(*price, price_digits),
                (ox + width - pad, oy + y),
                text_size,
                dim,
                FontWeight::NORMAL,
                Anchor::RightMiddle,
            );
        }
        for label in &time_labels {
            let bold = label.rank > crate::chart::timeaxis::Rank::Plain;
            paint_text(
                window,
                cx,
                &label.text,
                (ox + label.x, oy + plot_h + axis_h / 2.0),
                text_size,
                if bold { theme::fg() } else { dim },
                if bold {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                },
                Anchor::CentreMiddle,
            );
        }

        // The last price tag.
        if let Some(last) = model.series.last() {
            let y = scale.y_of(last.close, plot_h);
            if (0.0..=plot_h).contains(&y) {
                let color = if last.is_up() { up } else { down };
                tag(
                    window,
                    cx,
                    (ox, oy),
                    Tag::Price { y, width, plot_w },
                    &format_price(last.close, model.digits),
                    color,
                    theme::bg(),
                    text_size,
                );
            }
        }

        // The crosshair.
        if let Some(hover) = model.hover_info() {
            let cross = theme::alpha(theme::fg(), 0.35);
            let mut x = 0.0;
            while x < plot_w {
                rect(
                    window,
                    Rect {
                        x,
                        y: hover.y.round(),
                        w: 3.0,
                        h: 1.0,
                    },
                    cross,
                );
                x += 6.0;
            }
            let mut y = 0.0;
            while y < plot_h {
                rect(
                    window,
                    Rect {
                        x: hover.x.round(),
                        y,
                        w: 1.0,
                        h: 3.0,
                    },
                    cross,
                );
                y += 6.0;
            }
            let tag_fill = theme::over(theme::bg(), theme::fg(), 0.85);
            tag(
                window,
                cx,
                (ox, oy),
                Tag::Price {
                    y: hover.y,
                    width,
                    plot_w,
                },
                &format_price(hover.price, model.digits),
                tag_fill,
                theme::bg(),
                text_size,
            );
            if let Some(index) = hover.bar {
                let x = vp.x_of(index as f64, n);
                tag(
                    window,
                    cx,
                    (ox, oy),
                    Tag::Time {
                        x,
                        plot_h,
                        axis_h,
                        plot_w,
                    },
                    &full_text(bars[index].time, model.period),
                    tag_fill,
                    theme::bg(),
                    text_size,
                );
            }
        }
    }

    /// The legend: symbol, time frame and the figures of the bar under the crosshair (the newest
    /// bar without one).
    fn legend(&self) -> Div {
        let title = self.target.as_ref().map_or_else(String::new, |t| {
            format!("{}  {}", t.symbol, t.timeframe.label())
        });
        let mut row = div()
            .absolute()
            .top(sz(8.))
            .left(sz(12.))
            .flex()
            .flex_row()
            .items_center()
            .gap(sz(10.))
            .text_size(sz(11.5))
            .font_features(theme::tabular())
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::fg())
                    .child(title),
            );
        if let Some(bar) = self.model.legend_bar() {
            let color = if bar.is_up() {
                theme::green()
            } else {
                theme::red()
            };
            let digits = self.model.digits;
            let figure = |name: &'static str, value: f64| {
                div()
                    .flex()
                    .flex_row()
                    .gap(sz(4.))
                    .child(div().text_color(theme::dim()).child(name))
                    .child(div().text_color(color).child(format_price(value, digits)))
            };
            row = row
                .child(figure("O", bar.open))
                .child(figure("H", bar.high))
                .child(figure("L", bar.low))
                .child(figure("C", bar.close));
            if bar.open != 0.0 {
                let change = (bar.close - bar.open) / bar.open * 100.0;
                row = row.child(div().text_color(color).child(format!("{change:+.2}%")));
            }
        }
        row
    }

    /// A message over the plot while there is nothing to draw.
    fn message(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.model.series.is_empty() {
            return None;
        }
        let ready = self.target.as_ref().is_some_and(|t| t.ready);
        let (text, retry) = match &self.status {
            Status::Failed(text) => (text.clone(), true),
            _ if !ready => ("Waiting for the connection".to_owned(), false),
            Status::Loading if self.fetching_tail || self.older == Older::Loading => {
                ("Loading the price history".to_owned(), false)
            }
            Status::Loading => (
                "No price history for this symbol and time frame".to_owned(),
                false,
            ),
            Status::Ready => return None,
        };
        let mut column = div()
            .absolute()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(sz(10.))
            .text_size(sz(13.))
            .text_color(theme::dim())
            .child(text);
        if retry {
            column = column.child(
                div()
                    .id("chart-retry")
                    .px(sz(12.))
                    .py(sz(6.))
                    .rounded(sz(6.))
                    .bg(theme::muted())
                    .text_color(theme::fg())
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.reload(cx)))
                    .child("Try again"),
            );
        }
        Some(column)
    }

    /// The auto and log buttons, at the right end of the time axis.
    fn scale_buttons(&self, cx: &mut Context<Self>) -> Div {
        let (width, height) = self.model.size();
        let axis_w = width - self.model.plot_width();
        let axis_h = height - self.model.plot_height();
        let pill = |id: &'static str,
                    label: &'static str,
                    hint: &'static str,
                    on: bool,
                    press: fn(&mut ChartView, &mut Context<ChartView>)| {
            div()
                .id(id)
                .flex()
                .items_center()
                .px(sz(8.))
                .h_full()
                .rounded(sz(4.))
                .text_size(sz(11.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if on { theme::fg() } else { theme::dim() })
                .bg(if on {
                    theme::muted()
                } else {
                    theme::alpha(theme::muted(), 0.0)
                })
                .hover(|style| style.bg(theme::muted()).text_color(theme::fg()))
                .cursor_pointer()
                .tooltip(tip(hint))
                .on_click(cx.listener(move |this, _, _, cx| press(this, cx)))
                .child(label)
        };
        div()
            .absolute()
            .right(px(f(axis_w) + f(px_f64(sz(8.)))))
            .bottom(px(3.))
            .h(px((f(axis_h) - 6.0).max(12.0)))
            .flex()
            .flex_row()
            .gap(sz(4.))
            .child(pill(
                "chart-auto",
                "Auto",
                "Fit the price range to the bars on screen",
                self.model.is_auto(),
                |this, cx| {
                    this.model.toggle_auto();
                    cx.notify();
                },
            ))
            .child(pill(
                "chart-log",
                "Log",
                "Logarithmic price axis",
                self.model.is_log(),
                |this, cx| {
                    this.model.toggle_log();
                    cx.notify();
                },
            ))
    }

    /// The button that brings the newest bars back after scrolling away.
    fn latest_button(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.model.viewport.is_following() || self.model.series.is_empty() {
            return None;
        }
        let plot_w = self.model.plot_width();
        let plot_h = self.model.plot_height();
        let (width, height) = self.model.size();
        Some(
            div()
                .absolute()
                .right(px(f(width - plot_w) + f(px_f64(sz(12.)))))
                .bottom(px(f(height - plot_h) + f(px_f64(sz(12.)))))
                .child(
                    div()
                        .id("chart-latest")
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(sz(4.))
                        .pl(sz(10.))
                        .pr(sz(6.))
                        .py(sz(5.))
                        .rounded(sz(6.))
                        .bg(theme::over(theme::bg(), theme::fg(), 0.12))
                        .border_1()
                        .border_color(theme::border())
                        .text_size(sz(11.5))
                        .text_color(theme::fg())
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model.key(ChartKey::Latest);
                            cx.notify();
                        }))
                        .child("Latest")
                        .child(glyph(Glyph::ChevronRight, 12., theme::dim())),
                ),
        )
    }
}

/// Where the point handed to [`paint_text`] sits in the text.
#[derive(Clone, Copy)]
enum Anchor {
    RightMiddle,
    CentreMiddle,
}

/// Shapes and paints one line of text.
#[allow(clippy::too_many_arguments)]
fn paint_text(
    window: &mut Window,
    cx: &mut App,
    text: &str,
    at: (f64, f64),
    size_px: Pixels,
    color: Hsla,
    weight: FontWeight,
    anchor: Anchor,
) {
    if text.is_empty() {
        return;
    }
    let font = Font {
        family: theme::SANS.into(),
        features: theme::tabular(),
        fallbacks: None,
        weight,
        style: FontStyle::Normal,
    };
    let run = TextRun {
        len: text.len(),
        font,
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line =
        window
            .text_system()
            .shape_line(SharedString::from(text.to_owned()), size_px, &[run], None);
    let width = px_f64(line.width);
    let height = px_f64(size_px * 1.3);
    let x = match anchor {
        Anchor::RightMiddle => at.0 - width,
        Anchor::CentreMiddle => at.0 - width / 2.0,
    };
    let origin = point(px(f(x)), px(f(at.1 - height / 2.0)));
    let _ = line.paint(origin, px(f(height)), TextAlign::Left, None, window, cx);
}

/// Which axis a tag sits on, and where.
enum Tag {
    /// On the price axis at `y`.
    Price { y: f64, width: f64, plot_w: f64 },
    /// On the time axis at `x`.
    Time {
        x: f64,
        plot_h: f64,
        axis_h: f64,
        plot_w: f64,
    },
}

/// A filled label on an axis: the last price, or what the crosshair points at.
#[allow(clippy::too_many_arguments)]
fn tag(
    window: &mut Window,
    cx: &mut App,
    origin: (f64, f64),
    which: Tag,
    text: &str,
    background: Hsla,
    color: Hsla,
    size_px: Pixels,
) {
    let (ox, oy) = origin;
    let pad = px_f64(sz(6.));
    let text_h = px_f64(size_px) * 1.5;
    let measure = |window: &mut Window, weight| -> f64 {
        let run = TextRun {
            len: text.len(),
            font: Font {
                family: theme::SANS.into(),
                features: theme::tabular(),
                fallbacks: None,
                weight,
                style: FontStyle::Normal,
            },
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        px_f64(
            window
                .text_system()
                .shape_line(SharedString::from(text.to_owned()), size_px, &[run], None)
                .width,
        )
    };
    let text_w = measure(window, FontWeight::MEDIUM);
    let (x, y, w, h, at) = match which {
        Tag::Price { y, width, plot_w } => (
            plot_w + 1.0,
            (y - text_h / 2.0).round(),
            width - plot_w - 1.0,
            text_h.round(),
            (ox + width - pad, oy + y.round()),
        ),
        Tag::Time {
            x,
            plot_h,
            axis_h,
            plot_w,
        } => {
            let w = text_w + pad * 2.0;
            let left = (x - w / 2.0).clamp(0.0, (plot_w - w).max(0.0)).round();
            (
                left,
                plot_h + 1.0,
                w,
                axis_h - 1.0,
                (ox + left + w / 2.0, oy + plot_h + axis_h / 2.0),
            )
        }
    };
    window.paint_quad(fill(
        Bounds::new(
            point(px(f(ox + x)), px(f(oy + y))),
            size(px(f(w)), px(f(h))),
        ),
        background,
    ));
    let anchor = match which {
        Tag::Price { .. } => Anchor::RightMiddle,
        Tag::Time { .. } => Anchor::CentreMiddle,
    };
    paint_text(
        window,
        cx,
        text,
        at,
        size_px,
        color,
        FontWeight::MEDIUM,
        anchor,
    );
}

/// Saves what a request brought on the background executor. Returns `Some(())` when a store took
/// it, so the caller knows the coverage is on disk too. A failure is logged and the chart goes on.
async fn save_in_background(
    cx: &mut gpui_kit::AsyncApp,
    store: Option<Arc<CandleStore>>,
    key: SeriesKey,
    bars: Vec<Candle>,
    covered: Option<(UnixMillis, UnixMillis)>,
) -> Option<()> {
    let store = store?;
    let (from, to) = covered?;
    let result = cx
        .background_executor()
        .spawn(async move { store.save(&key, &bars, from, to) })
        .await;
    match result {
        Ok(()) => Some(()),
        Err(error) => {
            tracing::warn!(%error, "the bars could not be saved");
            None
        }
    }
}

impl Render for ChartView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let paint_entity: Entity<Self> = cx.entity();
        let bounds_entity = paint_entity.clone();
        let drag_entity = paint_entity.clone();
        let dragging = self.model.is_dragging();

        let surface = canvas(
            move |bounds, _window, cx| {
                bounds_entity.update(cx, |this, _| this.set_bounds(bounds));
                bounds
            },
            move |bounds, _, window, cx| {
                paint_entity.update(cx, |this, cx| this.paint(bounds, window, cx));
                if dragging {
                    // The pointer may leave the chart while a button is held: keep following it
                    // and end the drag wherever the button comes up.
                    let mover = drag_entity.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, _, _, cx| {
                        mover.update(cx, |this, cx| this.on_move(event.position, cx));
                    });
                    let upper = drag_entity.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, _, _, cx| {
                        if event.button == MouseButton::Left {
                            upper.update(cx, |this, cx| this.on_up(cx));
                        }
                    });
                }
            },
        )
        .absolute()
        .size_full();

        let mut root = div()
            .id("chart")
            .relative()
            .flex_1()
            .w_full()
            .overflow_hidden()
            .bg(theme::bg())
            .cursor_crosshair()
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                this.on_wheel(event, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| this.on_down(event, cx)),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                this.on_move(event.position, cx);
            }))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !*hovered && this.model.leave() {
                    cx.notify();
                }
            }))
            .child(surface)
            .child(self.legend())
            .child(self.scale_buttons(cx));
        if let Some(message) = self.message(cx) {
            root = root.child(message);
        }
        if let Some(button) = self.latest_button(cx) {
            root = root.child(button);
        }
        root
    }
}
