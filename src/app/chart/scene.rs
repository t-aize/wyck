//! Turns the data and the view into a flat list of drawing commands.
//!
//! Building the list is plain arithmetic (no window needed), so it can be unit tested, and the
//! amount of work is bounded by the size of the screen, never by the size of the data:
//!
//! - Only the points that reach the plot are visited.
//! - Once points are narrower than a pixel they are folded into one column each (their low, high,
//!   first and last), so a chart with a hundred thousand bars costs what a screenful costs.
//! - Long lines are cut in pieces, because a single GPU path may only hold so many vertices.

use gpui::{
    Bounds, Hsla, PaintQuad, Path, PathBuilder, Pixels, Point, Rgba, fill, point, px, size,
};
use wyck::openapi::market::{Bar, format_price};

use super::axis;
use super::data::Series;
use super::drawing::geometry::{self, Anchor, Prim};
use super::drawing::model::{Dash, Drawing};
use super::projection::ChartProjection;
use super::timeframe::Timeframe;
use super::view::{PriceScale, View, padded_range};
use super::zone::Zone;
use crate::app::theme;

/// Width of the price axis on the right.
pub const AXIS_W: f32 = 76.0;
/// Height of the time axis at the bottom.
pub const AXIS_H: f32 = 28.0;
/// Share of the plot height the volume bars may use.
const VOLUME_SHARE: f64 = 0.15;
/// Bars narrower than this are drawn as one line each, not as a body with wicks.
const CANDLE_MIN_PX: f64 = 4.0;
/// Most points in one stroked path.
const PATH_CHUNK: usize = 1_500;
const FONT: f32 = 11.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    Candles,
    /// Candles whose body is empty when the price closed above its open, colored by the change
    /// from the previous close.
    Hollow,
    /// Candles of averaged prices, which smooth out the noise of the trend.
    HeikinAshi,
    Bars,
    Line,
    /// A line that holds each price until the next one, like the price itself does.
    Step,
    Area,
}

impl ChartKind {
    pub const ALL: [Self; 7] = [
        Self::Candles,
        Self::Hollow,
        Self::HeikinAshi,
        Self::Bars,
        Self::Line,
        Self::Step,
        Self::Area,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Candles => "Candles",
            Self::Hollow => "Hollow candles",
            Self::HeikinAshi => "Heikin Ashi",
            Self::Bars => "Bars",
            Self::Line => "Line",
            Self::Step => "Step line",
            Self::Area => "Area",
        }
    }

    /// The name written in saved files. It never changes.
    pub fn code(self) -> &'static str {
        match self {
            Self::Candles => "candles",
            Self::Hollow => "hollow",
            Self::HeikinAshi => "heikin_ashi",
            Self::Bars => "bars",
            Self::Line => "line",
            Self::Step => "step",
            Self::Area => "area",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.code() == code)
    }

    /// Whether the kind is drawn from open, high, low and close (and so needs bars).
    pub fn uses_ohlc(self) -> bool {
        matches!(
            self,
            Self::Candles | Self::Hollow | Self::HeikinAshi | Self::Bars
        )
    }
}

/// The size of the chart's drawing area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub w: f64,
    pub h: f64,
}

impl Layout {
    pub fn plot_w(&self) -> f64 {
        (self.w - f64::from(AXIS_W)).max(1.0)
    }

    pub fn plot_h(&self) -> f64 {
        (self.h - f64::from(AXIS_H)).max(1.0)
    }
}

/// The price to y mapping of the plot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriceMap {
    pub lo: f64,
    pub hi: f64,
    pub top: f64,
    pub bottom: f64,
}

impl PriceMap {
    pub fn y(&self, price: f64) -> f64 {
        self.top + (self.hi - price) / (self.hi - self.lo) * (self.bottom - self.top)
    }

    pub fn price(&self, y: f64) -> f64 {
        self.hi - (y - self.top) / (self.bottom - self.top) * (self.hi - self.lo)
    }
}

/// What the series is drawn as: ticks have no body, so they are always a line.
pub fn effective_kind(series: &Series, kind: ChartKind) -> ChartKind {
    match (series, kind) {
        (Series::Ticks(_), kind) if kind.uses_ohlc() => ChartKind::Line,
        _ => kind,
    }
}

fn has_volume(series: &Series) -> bool {
    matches!(series, Series::Bars(_))
}

/// The price range on screen and how it maps to the plot, or `None` without data.
pub fn price_map(
    series: &Series,
    view: &View,
    layout: Layout,
    kind: ChartKind,
    digits: u32,
) -> Option<PriceMap> {
    let plot_w = layout.plot_w();
    let plot_h = layout.plot_h();
    let (first, last) = view.visible(series.len(), plot_w);
    let kind = effective_kind(series, kind);
    let (lo, hi) = match view.price {
        PriceScale::Manual { lo, hi } if hi > lo => (lo, hi),
        _ => {
            let wicks = kind.uses_ohlc();
            let (lo, hi) = series.price_range(first, last, wicks)?;
            padded_range(lo as f64, hi as f64, quote_unit(digits) * 10.0)
        }
    };
    let reserved = if has_volume(series) {
        plot_h * VOLUME_SHARE + 6.0
    } else {
        14.0
    };
    Some(PriceMap {
        lo,
        hi,
        top: 14.0,
        bottom: (plot_h - reserved).max(40.0),
    })
}

/// One unit of the symbol's last decimal, in raw price units.
pub fn quote_unit(digits: u32) -> f64 {
    10f64.powi(5 - i32::try_from(digits.min(5)).unwrap_or(5))
}

/// The colors the chart draws with.
#[derive(Clone, Copy)]
pub struct Palette {
    pub up: Rgba,
    pub down: Rgba,
    pub line: Rgba,
    pub grid: Rgba,
    pub text: Rgba,
    pub text_strong: Rgba,
    pub bg: Rgba,
    pub tag: Rgba,
    pub border: Rgba,
}

impl Palette {
    pub fn new() -> Self {
        Self {
            up: theme::emerald(),
            down: theme::destructive(),
            line: theme::accent(),
            grid: gpui::rgba(0xffffff0a),
            text: theme::muted_fg(),
            text_strong: theme::fg(),
            bg: theme::bg(),
            tag: gpui::rgb(0x2a2a2a),
            border: theme::border_hairline(),
        }
    }
}

fn color(c: Rgba) -> Hsla {
    c.into()
}

fn with_alpha(c: Rgba, alpha: f32) -> Hsla {
    let mut h: Hsla = c.into();
    h.a = alpha;
    h
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// A drawing command, in window coordinates.
pub enum Cmd {
    Quad(PaintQuad),
    Path(Path<Pixels>, Hsla),
    Text {
        text: String,
        x: f32,
        y: f32,
        color: Hsla,
        align: Align,
    },
    /// Text on a filled tag, sized to the text.
    Tag {
        text: String,
        x: f32,
        y: f32,
        height: f32,
        pad: f32,
        bg: Hsla,
        fg: Hsla,
        align: Align,
        /// A width to use whatever the text measures, for the tags on the price axis.
        fixed_width: Option<f32>,
    },
    /// Commands drawn only inside these bounds.
    Clip(Bounds<Pixels>, Vec<Cmd>),
}

pub struct Frame<'a> {
    pub series: &'a Series,
    pub view: &'a View,
    pub kind: ChartKind,
    pub timeframe: Timeframe,
    pub zone: Zone,
    pub digits: u32,
    pub origin: Point<Pixels>,
    pub layout: Layout,
    /// Device pixels per logical pixel, for snapping lines to the pixel grid.
    pub scale: f32,
    /// The pointer, from the top left of the chart.
    pub hover: Option<(f32, f32)>,
    /// The pointer of another chart (time and raw price), when the crosshairs are linked.
    pub remote: Option<(i64, f64)>,
    pub ask: Option<i64>,
    pub palette: Palette,
    /// The drawings of the symbol, when there are any to show.
    pub drawings: Option<DrawingView<'a>>,
}

/// The drawings to show on a chart.
pub struct DrawingView<'a> {
    pub list: &'a [Drawing],
    /// The drawing being made, which follows the pointer.
    pub creating: Option<&'a Drawing>,
    pub selected: Option<u64>,
}

struct Ctx<'a> {
    f: &'a Frame<'a>,
    map: PriceMap,
    len: usize,
    plot_w: f64,
    plot_h: f64,
    ox: f32,
    oy: f32,
    /// Where the crosshair is, from the top left of the plot.
    pointer: Option<(f32, f32)>,
}

impl Ctx<'_> {
    fn snap(&self, v: f32) -> f32 {
        (v * self.f.scale).round() / self.f.scale
    }

    /// Absolute x of point `index`.
    fn x(&self, index: usize) -> f32 {
        self.ox + self.f.view.x_of(index as f64, self.len, self.plot_w) as f32
    }

    /// Absolute y of a price, kept to a range a GPU can draw.
    fn y(&self, price: f64) -> f32 {
        self.oy + (self.map.y(price).clamp(-4_000.0, 20_000.0)) as f32
    }

    fn quad(&self, x: f32, y: f32, w: f32, h: f32, c: Hsla) -> Cmd {
        Cmd::Quad(fill(
            Bounds::new(point(px(x), px(y)), size(px(w.max(0.0)), px(h.max(0.0)))),
            c,
        ))
    }

    /// A one device pixel wide vertical line from `y0` to `y1`.
    fn vline(&self, x: f32, y0: f32, y1: f32, c: Hsla) -> Cmd {
        let w = 1.0 / self.f.scale;
        let (a, b) = (y0.min(y1), y0.max(y1));
        self.quad(
            self.snap(x),
            self.snap(a),
            w,
            self.snap(b) - self.snap(a),
            c,
        )
    }

    fn hline(&self, y: f32, x0: f32, x1: f32, c: Hsla) -> Cmd {
        let h = 1.0 / self.f.scale;
        self.quad(
            self.snap(x0),
            self.snap(y),
            self.snap(x1) - self.snap(x0),
            h,
            c,
        )
    }
}

/// Where the crosshair goes: under the pointer when it is over this chart, else where another
/// chart's pointer is in time and price (when that time is on screen).
fn pointer_of(frame: &Frame<'_>, map: &PriceMap, plot_w: f64, plot_h: f64) -> Option<(f32, f32)> {
    let inside = |x: f64, y: f64| (0.0..=plot_w).contains(&x) && (0.0..=plot_h).contains(&y);
    if let Some((x, y)) = frame.hover
        && inside(f64::from(x), f64::from(y))
    {
        return Some((x, y));
    }
    let (time_ms, price) = frame.remote?;
    let step = frame.series.step_ms(frame.timeframe.bar_ms());
    let index = frame.series.index_of_time(time_ms, step)?;
    let x = frame.view.x_of(index, frame.series.len(), plot_w);
    let y = map.y(price);
    inside(x, y).then_some((x as f32, y as f32))
}

/// Builds every command for one frame.
pub fn build(frame: &Frame<'_>) -> Vec<Cmd> {
    let len = frame.series.len();
    let plot_w = frame.layout.plot_w();
    let plot_h = frame.layout.plot_h();
    let mut cmds: Vec<Cmd> = Vec::new();
    let p = frame.palette;
    let (ox, oy) = (f32::from(frame.origin.x), f32::from(frame.origin.y));
    let plot_bounds = Bounds::new(frame.origin, size(px(plot_w as f32), px(plot_h as f32)));

    let Some(map) = price_map(
        frame.series,
        frame.view,
        frame.layout,
        frame.kind,
        frame.digits,
    ) else {
        cmds.push(axes_background(frame));
        return cmds;
    };
    let pointer = pointer_of(frame, &map, plot_w, plot_h);
    let cx = Ctx {
        f: frame,
        map,
        len,
        plot_w,
        plot_h,
        ox,
        oy,
        pointer,
    };
    let (first, last) = frame.view.visible(len, plot_w);
    let kind = effective_kind(frame.series, frame.kind);

    let price_ticks = axis::price_ticks(map.lo, map.hi, 8, quote_unit(frame.digits));
    let time_labels = axis::time_labels(
        first,
        last,
        |i| frame.series.time_at(i),
        |i| frame.view.x_of(i as f64, len, plot_w),
        plot_w,
        96.0,
        frame.zone,
    );

    // The plot, clipped to itself.
    let mut plot: Vec<Cmd> = Vec::new();
    for value in &price_ticks {
        plot.push(cx.hline(cx.y(*value), ox, ox + plot_w as f32, color(p.grid)));
    }
    for label in &time_labels {
        plot.push(cx.vline(cx.x(label.index), oy, oy + plot_h as f32, color(p.grid)));
    }
    if let Series::Bars(bars) = frame.series {
        draw_volume(&cx, bars, first, last, &mut plot);
    }
    match (frame.series, kind) {
        (Series::Bars(bars), ChartKind::Candles) => {
            draw_candles(&cx, bars, 0, first, last, CandleStyle::Solid, &mut plot)
        }
        (Series::Bars(bars), ChartKind::Hollow) => {
            draw_candles(&cx, bars, 0, first, last, CandleStyle::Hollow, &mut plot)
        }
        (Series::Bars(bars), ChartKind::Bars) => {
            draw_candles(&cx, bars, 0, first, last, CandleStyle::Ohlc, &mut plot)
        }
        (Series::Bars(bars), ChartKind::HeikinAshi) => {
            let (base, averaged) = heikin_ashi(bars, first, last);
            draw_candles(
                &cx,
                &averaged,
                base,
                first,
                last,
                CandleStyle::Solid,
                &mut plot,
            );
        }
        _ => draw_line(
            &cx,
            first,
            last,
            matches!(kind, ChartKind::Area),
            matches!(kind, ChartKind::Step),
            &mut plot,
        ),
    }
    if let Some(view) = &frame.drawings {
        draw_drawings(&cx, view, &mut plot);
    }
    draw_price_lines(&cx, &mut plot);
    if let Some((hx, hy)) = cx.pointer {
        draw_crosshair(&cx, hx, hy, &mut plot);
    }
    cmds.push(Cmd::Clip(plot_bounds, plot));

    // The axes go over the plot.
    cmds.push(axes_background(frame));
    let axis_x = ox + plot_w as f32 + 8.0;
    for value in &price_ticks {
        let y = cx.y(*value);
        if y < oy + 6.0 || y > oy + plot_h as f32 - 6.0 {
            continue;
        }
        cmds.push(Cmd::Text {
            text: format_price(*value as i64, frame.digits),
            x: axis_x,
            y: y - FONT * 0.65,
            color: color(p.text),
            align: Align::Left,
        });
    }
    for label in &time_labels {
        cmds.push(Cmd::Text {
            text: label.text.clone(),
            x: cx.x(label.index),
            y: oy + plot_h as f32 + 8.0,
            color: if label.major {
                color(p.text_strong)
            } else {
                color(p.text)
            },
            align: Align::Center,
        });
    }
    // The zone the times are in, in the corner between the axes.
    let corner_time = frame.series.last_time().unwrap_or(0);
    cmds.push(Cmd::Text {
        text: frame.zone.label(corner_time),
        x: ox + plot_w as f32 + AXIS_W / 2.0,
        y: oy + plot_h as f32 + 8.0,
        color: color(p.text),
        align: Align::Center,
    });
    draw_axis_tags(&cx, &mut cmds);
    cmds
}

/// The solid strips of the two axes and the lines that set them off.
fn axes_background(frame: &Frame<'_>) -> Cmd {
    let (ox, oy) = (f32::from(frame.origin.x), f32::from(frame.origin.y));
    let plot_w = frame.layout.plot_w() as f32;
    let plot_h = frame.layout.plot_h() as f32;
    let (w, h) = (frame.layout.w as f32, frame.layout.h as f32);
    let p = frame.palette;
    let one = 1.0 / frame.scale;
    let quad = |x: f32, y: f32, qw: f32, qh: f32, c: Hsla| {
        Cmd::Quad(fill(
            Bounds::new(point(px(x), px(y)), size(px(qw), px(qh))),
            c,
        ))
    };
    Cmd::Clip(
        Bounds::new(frame.origin, size(px(w), px(h))),
        vec![
            quad(ox + plot_w, oy, w - plot_w, h, color(p.bg)),
            quad(ox, oy + plot_h, plot_w, h - plot_h, color(p.bg)),
            quad(ox + plot_w, oy, one, h, color(p.border)),
            quad(ox, oy + plot_h, w, one, color(p.border)),
        ],
    )
}

fn up_color(cx: &Ctx<'_>, up: bool) -> Rgba {
    if up {
        cx.f.palette.up
    } else {
        cx.f.palette.down
    }
}

fn draw_volume(
    cx: &Ctx<'_>,
    bars: &[wyck::openapi::market::Bar],
    first: usize,
    last: usize,
    out: &mut Vec<Cmd>,
) {
    let slice = &bars[first.min(bars.len())..last.min(bars.len())];
    let max = slice.iter().map(|b| b.volume).max().unwrap_or(0);
    if max <= 0 {
        return;
    }
    let height = (cx.plot_h * VOLUME_SHARE) as f32;
    let base = cx.oy + cx.plot_h as f32;
    let bar_px = cx.f.view.bar_px as f32;
    columns(cx, first, last, |x, range| {
        let volume = bars[range.clone()]
            .iter()
            .map(|b| b.volume)
            .max()
            .unwrap_or(0);
        let up = bars[range.end - 1].close >= bars[range.start].open;
        let h = (volume as f32 / max as f32 * height).max(1.0 / cx.f.scale);
        let width = if bar_px >= 2.0 {
            (bar_px * 0.7).max(1.0)
        } else {
            1.0
        };
        let c = with_alpha(up_color(cx, up), 0.32);
        out.push(cx.quad(cx.snap(x - width / 2.0), base - h, width, h, c));
    });
}

/// Calls `each` for every group of points that share a pixel column, with the column's center x.
/// Wider than a pixel, every point is its own group.
fn columns(
    cx: &Ctx<'_>,
    first: usize,
    last: usize,
    mut each: impl FnMut(f32, std::ops::Range<usize>),
) {
    let mut start = first;
    while start < last {
        let column = cx.x(start).floor();
        let mut end = start + 1;
        while end < last && cx.x(end).floor() == column {
            end += 1;
        }
        let x = if end - start == 1 {
            cx.x(start)
        } else {
            column + 0.5
        };
        each(x, start..end);
        start = end;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CandleStyle {
    Solid,
    Hollow,
    Ohlc,
}

/// Bars of averaged prices for the points `first..last`, starting a little before `first` so
/// that the average has settled by the first one drawn (each bar is half made of the one
/// before, so the start is forgotten within a few dozen bars). Returns the index the first
/// returned bar stands for, and the bars.
fn heikin_ashi(bars: &[Bar], first: usize, last: usize) -> (usize, Vec<Bar>) {
    const WARM_UP: usize = 48;
    let last = last.min(bars.len());
    let base = first.saturating_sub(WARM_UP).min(last);
    let mut out: Vec<Bar> = Vec::with_capacity(last - base);
    for bar in &bars[base..last] {
        let close = ((bar.open + bar.high + bar.low + bar.close) as f64 / 4.0).round() as i64;
        let open = out
            .last()
            .map_or((bar.open + bar.close) / 2, |p| (p.open + p.close) / 2);
        out.push(Bar {
            open,
            close,
            high: bar.high.max(open).max(close),
            low: bar.low.min(open).min(close),
            ..*bar
        });
    }
    (base, out)
}

/// Draws `bars`, where `bars[0]` stands for point `base` of the series.
fn draw_candles(
    cx: &Ctx<'_>,
    bars: &[Bar],
    base: usize,
    first: usize,
    last: usize,
    style: CandleStyle,
    out: &mut Vec<Cmd>,
) {
    let bar_px = cx.f.view.bar_px;
    let last = last.min(base + bars.len());
    let first = first.max(base);
    let wide = bar_px >= CANDLE_MIN_PX;
    let wick = 1.0 / cx.f.scale;
    if !wide {
        // One line per pixel column, from the lowest low to the highest high.
        columns(cx, first, last, |x, range| {
            let group = &bars[range.start - base..range.end - base];
            let high = group.iter().map(|b| b.high).max().unwrap_or(0) as f64;
            let low = group.iter().map(|b| b.low).min().unwrap_or(0) as f64;
            let up = group[group.len() - 1].close >= group[0].open;
            let c = color(up_color(cx, up));
            out.push(cx.vline(x, cx.y(high), cx.y(low).max(cx.y(high) + wick), c));
        });
        return;
    }
    let body_w = (bar_px * 0.72).max(3.0) as f32;
    for i in first..last {
        let bar = &bars[i - base];
        let x = cx.x(i);
        // Hollow candles are colored by the change from the previous close, the others by the
        // change over the bar itself.
        let up = match style {
            CandleStyle::Hollow => {
                let previous = (i > base).then(|| bars[i - base - 1].close);
                bar.close >= previous.unwrap_or(bar.open)
            }
            _ => bar.close >= bar.open,
        };
        let c = color(up_color(cx, up));
        let (y_high, y_low) = (cx.y(bar.high as f64), cx.y(bar.low as f64));
        let (y_open, y_close) = (cx.y(bar.open as f64), cx.y(bar.close as f64));
        out.push(cx.vline(x, y_high, y_low.max(y_high + wick), c));
        if style == CandleStyle::Ohlc {
            let tick = (bar_px as f32 * 0.38).max(2.0);
            out.push(cx.hline(y_open, x - tick, x, c));
            out.push(cx.hline(y_close, x, x + tick, c));
            continue;
        }
        let (top, bottom) = (y_open.min(y_close), y_open.max(y_close));
        let x0 = cx.snap(x - body_w / 2.0);
        let x1 = cx.snap(x + body_w / 2.0).max(x0 + 1.0 / cx.f.scale);
        let top = cx.snap(top);
        let height = (cx.snap(bottom) - top).max(1.0 / cx.f.scale);
        if style == CandleStyle::Hollow && bar.close > bar.open {
            let mut quad = fill(
                Bounds::new(point(px(x0), px(top)), size(px(x1 - x0), px(height))),
                gpui::transparent_black(),
            );
            quad.border_widths = (1.0 / cx.f.scale).max(1.0).into();
            quad.border_color = c;
            out.push(Cmd::Quad(quad));
        } else {
            out.push(cx.quad(x0, top, x1 - x0, height, c));
        }
    }
}

/// The value a point is plotted at in a line: a bar's close, or a tick's price.
fn value_at(series: &Series, index: usize) -> Option<i64> {
    match series {
        Series::Bars(bars) => bars.get(index).map(|b| b.close),
        Series::Ticks(ticks) => ticks.get(index).map(|t| t.price),
    }
}

fn draw_line(cx: &Ctx<'_>, first: usize, last: usize, area: bool, step: bool, out: &mut Vec<Cmd>) {
    let mut points: Vec<(f32, f32)> = Vec::new();
    if cx.f.view.bar_px >= 1.0 {
        for i in first..last {
            if let Some(v) = value_at(cx.f.series, i) {
                points.push((cx.x(i), cx.y(v as f64)));
            }
        }
    } else {
        // Several points per pixel column: keep the first, lowest, highest and last, in order.
        columns(cx, first, last, |x, range| {
            let mut keep: Vec<(usize, i64)> = Vec::with_capacity(4);
            let mut lowest: Option<(usize, i64)> = None;
            let mut highest: Option<(usize, i64)> = None;
            for i in range.clone() {
                let Some(v) = value_at(cx.f.series, i) else {
                    continue;
                };
                if lowest.is_none_or(|(_, l)| v < l) {
                    lowest = Some((i, v));
                }
                if highest.is_none_or(|(_, h)| v > h) {
                    highest = Some((i, v));
                }
            }
            keep.extend(value_at(cx.f.series, range.start).map(|v| (range.start, v)));
            keep.extend(lowest);
            keep.extend(highest);
            keep.extend(value_at(cx.f.series, range.end - 1).map(|v| (range.end - 1, v)));
            keep.sort_by_key(|(i, _)| *i);
            keep.dedup_by_key(|(i, _)| *i);
            for (_, v) in keep {
                points.push((x, cx.y(v as f64)));
            }
        });
    }
    if points.len() < 2 {
        return;
    }
    if step {
        // Hold each price until the next point, then jump.
        let mut stepped = Vec::with_capacity(points.len() * 2);
        for (i, &(x, y)) in points.iter().enumerate() {
            if i > 0 {
                stepped.push((x, points[i - 1].1));
            }
            stepped.push((x, y));
        }
        points = stepped;
    }
    let p = cx.f.palette;
    if area {
        let base = cx.oy + cx.plot_h as f32;
        let mut polygon: Vec<Point<Pixels>> =
            points.iter().map(|(x, y)| point(px(*x), px(*y))).collect();
        polygon.push(point(px(points[points.len() - 1].0), px(base)));
        polygon.push(point(px(points[0].0), px(base)));
        let mut builder = PathBuilder::fill();
        builder.add_polygon(&polygon, true);
        if let Ok(path) = builder.build() {
            out.push(Cmd::Path(path, with_alpha(p.line, 0.13)));
        }
    }
    stroke(&points, 1.6, color(p.line), out);
    if let Some(&(x, y)) = points.last() {
        let r = 3.5;
        out.push(Cmd::Quad(PaintQuad {
            bounds: Bounds::new(point(px(x - r), px(y - r)), size(px(r * 2.0), px(r * 2.0))),
            corner_radii: px(r).into(),
            background: color(p.line).into(),
            border_widths: (0.0).into(),
            border_color: color(p.bg),
            border_style: gpui::BorderStyle::default(),
        }));
    }
}

/// Strokes a polyline in pieces the GPU path can hold, each piece sharing its end point with the
/// next so the line has no break.
fn stroke(points: &[(f32, f32)], width: f32, c: Hsla, out: &mut Vec<Cmd>) {
    stroke_dashed(points, width, c, None, out);
}

/// Like [`stroke`], with a dash pattern (the lengths of a dash and of the gap after it).
fn stroke_dashed(
    points: &[(f32, f32)],
    width: f32,
    c: Hsla,
    dash: Option<[f32; 2]>,
    out: &mut Vec<Cmd>,
) {
    let mut start = 0;
    while start + 1 < points.len() {
        let end = (start + PATH_CHUNK).min(points.len());
        let mut builder = PathBuilder::stroke(px(width));
        if let Some([on, off]) = dash {
            builder = builder.dash_array(&[px(on), px(off)]);
        }
        builder.move_to(point(px(points[start].0), px(points[start].1)));
        for (x, y) in &points[start + 1..end] {
            builder.line_to(point(px(*x), px(*y)));
        }
        if let Ok(path) = builder.build() {
            out.push(Cmd::Path(path, c));
        }
        start = end - 1;
    }
}

/// Whether the newest price moved up (or is unchanged) from the one before it.
fn last_is_up(series: &Series) -> bool {
    match series {
        Series::Bars(bars) => bars.last().is_none_or(|b| b.close >= b.open),
        Series::Ticks(ticks) => match ticks.as_slice() {
            [.., a, b] => b.price >= a.price,
            _ => true,
        },
    }
}

fn draw_price_lines(cx: &Ctx<'_>, out: &mut Vec<Cmd>) {
    let p = cx.f.palette;
    let (x0, x1) = (cx.ox, cx.ox + cx.plot_w as f32);
    if let Some(ask) = cx.f.ask {
        out.push(cx.hline(cx.y(ask as f64), x0, x1, with_alpha(p.text, 0.3)));
    }
    if let Some(price) = cx.f.series.last_price() {
        let c = with_alpha(up_color(cx, last_is_up(cx.f.series)), 0.75);
        out.push(cx.hline(cx.y(price as f64), x0, x1, c));
    }
}

fn draw_crosshair(cx: &Ctx<'_>, hx: f32, hy: f32, out: &mut Vec<Cmd>) {
    let p = cx.f.palette;
    let c = with_alpha(p.text, 0.35);
    let x = if cx.len > 0 {
        let index =
            cx.f.view
                .index_at(f64::from(hx), cx.len, cx.plot_w)
                .round()
                .clamp(0.0, (cx.len - 1) as f64) as usize;
        cx.x(index)
    } else {
        cx.ox + hx
    };
    out.push(cx.vline(x, cx.oy, cx.oy + cx.plot_h as f32, c));
    out.push(cx.hline(cx.oy + hy, cx.ox, cx.ox + cx.plot_w as f32, c));
}

/// The tags on the axes: the newest price, and the crosshair's price and time.
fn draw_axis_tags(cx: &Ctx<'_>, out: &mut Vec<Cmd>) {
    let p = cx.f.palette;
    let axis_x = cx.ox + cx.plot_w as f32 + 1.0;
    let (min_y, max_y) = (cx.oy + 9.0, cx.oy + cx.plot_h as f32 - 9.0);
    if let Some(price) = cx.f.series.last_price() {
        let y = cx.y(price as f64).clamp(min_y, max_y);
        out.push(Cmd::Tag {
            text: format_price(price, cx.f.digits),
            x: axis_x,
            y: y - 9.0,
            height: 18.0,
            pad: 7.0,
            bg: color(up_color(cx, last_is_up(cx.f.series))),
            fg: color(p.bg),
            align: Align::Left,
            fixed_width: Some(AXIS_W - 2.0),
        });
    }
    let Some((hx, hy)) = cx.pointer else { return };
    let price = cx.map.price(f64::from(hy));
    out.push(Cmd::Tag {
        text: format_price(price.round() as i64, cx.f.digits),
        x: axis_x,
        y: (cx.oy + hy).clamp(min_y, max_y) - 9.0,
        height: 18.0,
        pad: 7.0,
        bg: color(p.tag),
        fg: color(p.text_strong),
        align: Align::Left,
        fixed_width: Some(AXIS_W - 2.0),
    });
    if cx.len > 0 {
        let index =
            cx.f.view
                .index_at(f64::from(hx), cx.len, cx.plot_w)
                .round()
                .clamp(0.0, (cx.len - 1) as f64) as usize;
        if let Some(time) = cx.f.series.time_at(index) {
            let text = axis::full_time(
                time,
                cx.f.zone,
                matches!(cx.f.timeframe, Timeframe::Seconds(_) | Timeframe::Ticks),
                matches!(cx.f.timeframe, Timeframe::Ticks),
            );
            out.push(Cmd::Tag {
                text,
                x: cx.x(index),
                y: cx.oy + cx.plot_h as f32 + 4.0,
                height: 20.0,
                pad: 8.0,
                bg: color(p.tag),
                fg: color(p.text_strong),
                align: Align::Center,
                fixed_width: None,
            });
        }
    }
}

fn rgb_alpha(color: u32, alpha: f32) -> Hsla {
    let mut hsla: Hsla = gpui::rgb(color).into();
    hsla.a = alpha;
    hsla
}

/// Draws the drawings of the symbol, and the grips of the selected one.
fn draw_drawings(cx: &Ctx<'_>, view: &DrawingView<'_>, out: &mut Vec<Cmd>) {
    let projection = ChartProjection {
        series: cx.f.series,
        view: cx.f.view,
        map: cx.map,
        plot_w: cx.plot_w,
        plot_h: cx.plot_h,
        step_ms: cx.f.series.step_ms(cx.f.timeframe.bar_ms()),
        digits: cx.f.digits,
    };
    for drawing in view.list.iter().chain(view.creating) {
        for prim in geometry::prims(drawing, &projection) {
            push_prim(cx, prim, out);
        }
        if view.selected == Some(drawing.id) && !drawing.locked {
            for at in geometry::handles(drawing, &projection) {
                push_prim(cx, Prim::Handle { at }, out);
            }
        }
    }
}

/// Turns one shape of a drawing (in plot coordinates) into drawing commands.
fn push_prim(cx: &Ctx<'_>, prim: Prim, out: &mut Vec<Cmd>) {
    let (ox, oy) = (cx.ox, cx.oy);
    let at = |p: (f32, f32)| (ox + p.0, oy + p.1);
    let thin = 1.0 / cx.f.scale;
    match prim {
        Prim::Segment {
            a,
            b,
            color,
            alpha,
            width,
            dash,
        } => {
            let (a, b) = (at(a), at(b));
            let paint = rgb_alpha(color, alpha);
            let solid = dash == Dash::Solid;
            if solid && width <= 1.0 && (a.1 - b.1).abs() < 0.01 {
                out.push(cx.hline(a.1, a.0.min(b.0), a.0.max(b.0), paint));
            } else if solid && width <= 1.0 && (a.0 - b.0).abs() < 0.01 {
                out.push(cx.vline(a.0, a.1, b.1, paint));
            } else {
                let pattern = match dash {
                    Dash::Solid => None,
                    Dash::Dashed => Some([6.0 + width * 1.5, 4.0 + width]),
                    Dash::Dotted => Some([width.max(1.0), 3.0 + width]),
                };
                stroke_dashed(&[a, b], width, paint, pattern, out);
            }
        }
        Prim::Rect {
            a,
            b,
            fill: inside,
            stroke: edge,
        } => {
            let (a, b) = (at(a), at(b));
            let (left, right) = (a.0.min(b.0), a.0.max(b.0));
            let (top, bottom) = (a.1.min(b.1), a.1.max(b.1));
            let background =
                inside.map_or_else(gpui::transparent_black, |(c, al)| rgb_alpha(c, al));
            let mut quad = fill(
                Bounds::new(
                    point(px(left), px(top)),
                    size(px(right - left), px(bottom - top)),
                ),
                background,
            );
            if let Some((c, width)) = edge {
                quad.border_widths = px(width.max(thin)).into();
                quad.border_color = rgb_alpha(c, 1.0);
            }
            out.push(Cmd::Quad(quad));
        }
        Prim::Ellipse {
            center,
            rx,
            ry,
            fill: inside,
            stroke: edge,
        } => {
            let center = at(center);
            let ring: Vec<(f32, f32)> = (0..=48)
                .map(|i| {
                    let angle = i as f32 / 48.0 * std::f32::consts::TAU;
                    (center.0 + rx * angle.cos(), center.1 + ry * angle.sin())
                })
                .collect();
            if let Some((c, al)) = inside {
                fill_polygon(&ring, rgb_alpha(c, al), out);
            }
            if let Some((c, width)) = edge {
                stroke(&ring, width, rgb_alpha(c, 1.0), out);
            }
        }
        Prim::Polygon {
            points,
            fill: (c, al),
        } => {
            let points: Vec<(f32, f32)> = points.into_iter().map(at).collect();
            fill_polygon(&points, rgb_alpha(c, al), out);
        }
        Prim::Polyline {
            points,
            color,
            width,
        } => {
            let points: Vec<(f32, f32)> = points.into_iter().map(at).collect();
            stroke(&points, width, rgb_alpha(color, 1.0), out);
        }
        Prim::Label {
            at: position,
            text,
            color,
            background,
            anchor,
        } => {
            let (x, y) = at(position);
            let align = match anchor {
                Anchor::Left => Align::Left,
                Anchor::Right => Align::Right,
                Anchor::Center => Align::Center,
            };
            out.push(match background {
                Some((bg, al)) => Cmd::Tag {
                    text,
                    x,
                    y: y - 9.0,
                    height: 18.0,
                    pad: 6.0,
                    bg: rgb_alpha(bg, al),
                    fg: rgb_alpha(color, 1.0),
                    align,
                    fixed_width: None,
                },
                None => Cmd::Text {
                    text,
                    x,
                    y: y - FONT * 0.65,
                    color: rgb_alpha(color, 1.0),
                    align,
                },
            });
        }
        Prim::Handle { at: position } => {
            let (x, y) = at(position);
            let r = 4.5;
            out.push(Cmd::Quad(PaintQuad {
                bounds: Bounds::new(point(px(x - r), px(y - r)), size(px(r * 2.0), px(r * 2.0))),
                corner_radii: px(r).into(),
                background: color(cx.f.palette.text_strong).into(),
                border_widths: px(1.5).into(),
                border_color: color(cx.f.palette.bg),
                border_style: gpui::BorderStyle::default(),
            }));
        }
    }
}

fn fill_polygon(points: &[(f32, f32)], paint: Hsla, out: &mut Vec<Cmd>) {
    if points.len() < 3 {
        return;
    }
    let polygon: Vec<Point<Pixels>> = points.iter().map(|(x, y)| point(px(*x), px(*y))).collect();
    let mut builder = PathBuilder::fill();
    builder.add_polygon(&polygon, true);
    if let Ok(path) = builder.build() {
        out.push(Cmd::Path(path, paint));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyck::openapi::market::{Bar, Tick};

    fn bars(n: usize) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let base = 100_000 + (i as i64 % 50) * 10;
                Bar {
                    time_ms: 1_767_571_200_000 + i as i64 * 60_000,
                    open: base,
                    high: base + 30,
                    low: base - 20,
                    close: base + 10,
                    volume: 1 + i as i64 % 7,
                }
            })
            .collect()
    }

    fn frame<'a>(series: &'a Series, view: &'a View, kind: ChartKind) -> Frame<'a> {
        Frame {
            series,
            view,
            kind,
            timeframe: Timeframe::DEFAULT,
            zone: Zone::Utc,
            digits: 5,
            origin: point(px(0.0), px(0.0)),
            layout: Layout {
                w: 1_000.0,
                h: 600.0,
            },
            scale: 1.0,
            hover: Some((300.0, 200.0)),
            remote: None,
            ask: Some(100_020),
            palette: Palette::new(),
            drawings: None,
        }
    }

    fn count(cmds: &[Cmd]) -> usize {
        cmds.iter()
            .map(|c| match c {
                Cmd::Clip(_, inner) => count(inner),
                _ => 1,
            })
            .sum()
    }

    #[test]
    fn a_screenful_of_candles_draws() {
        let series = Series::Bars(bars(500));
        let view = View::new(8.0);
        let cmds = build(&frame(&series, &view, ChartKind::Candles));
        assert!(count(&cmds) > 100);
    }

    #[test]
    fn the_work_does_not_grow_with_the_data() {
        // A hundred thousand bars squeezed into one screen make about as many commands as a
        // thousand: they are folded into pixel columns.
        let small = Series::Bars(bars(1_000));
        let huge = Series::Bars(bars(100_000));
        for kind in ChartKind::ALL {
            let mut view = View::new(0.2);
            view.offset = View::home_offset();
            let a = count(&build(&frame(&small, &view, kind)));
            let b = count(&build(&frame(&huge, &view, kind)));
            assert!(b < 6_000, "{kind:?}: {b} commands");
            assert!(b < a * 8 + 500, "{kind:?}: {a} against {b}");
        }
    }

    #[test]
    fn a_line_over_many_ticks_stays_bounded() {
        let ticks: Vec<Tick> = (0..200_000)
            .map(|i| Tick {
                time_ms: 1_767_571_200_000 + i as i64 * 100,
                price: 100_000 + ((i as i64 * 7919) % 400),
            })
            .collect();
        let series = Series::Ticks(ticks);
        let mut view = View::new(0.2);
        view.offset = View::home_offset();
        let cmds = build(&frame(&series, &view, ChartKind::Line));
        assert!(count(&cmds) < 1_000, "{}", count(&cmds));
    }

    #[test]
    fn ticks_are_never_drawn_as_candles() {
        let series = Series::Ticks(vec![Tick {
            time_ms: 0,
            price: 1,
        }]);
        assert_eq!(effective_kind(&series, ChartKind::Candles), ChartKind::Line);
        assert_eq!(effective_kind(&series, ChartKind::Area), ChartKind::Area);
    }

    #[test]
    fn an_empty_series_only_draws_the_axes() {
        let series = Series::Bars(Vec::new());
        let view = View::new(8.0);
        let cmds = build(&frame(&series, &view, ChartKind::Candles));
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn the_price_map_round_trips_and_keeps_the_data_on_screen() {
        let series = Series::Bars(bars(200));
        let view = View::new(8.0);
        let layout = Layout {
            w: 1_000.0,
            h: 600.0,
        };
        let map = price_map(&series, &view, layout, ChartKind::Candles, 5).unwrap();
        for price in [100_000.0, 100_250.0] {
            assert!((map.price(map.y(price)) - price).abs() < 1e-6);
        }
        assert!(map.y(100_500.0) >= 0.0 && map.y(99_980.0) <= layout.plot_h());
    }

    #[test]
    fn a_manual_scale_is_honoured() {
        let series = Series::Bars(bars(50));
        let mut view = View::new(8.0);
        view.price = PriceScale::Manual {
            lo: 0.0,
            hi: 1_000_000.0,
        };
        let layout = Layout {
            w: 1_000.0,
            h: 600.0,
        };
        let map = price_map(&series, &view, layout, ChartKind::Candles, 5).unwrap();
        assert_eq!((map.lo, map.hi), (0.0, 1_000_000.0));
    }

    #[test]
    fn another_charts_pointer_draws_a_crosshair_when_its_time_is_on_screen() {
        let series = Series::Bars(bars(200));
        let view = View::new(8.0);
        let mut f = frame(&series, &view, ChartKind::Candles);
        f.hover = None;
        let bare = count(&build(&f));
        // The time of a bar near the right edge, and a price inside the range.
        let time = series.time_at(190).unwrap();
        f.remote = Some((time, 100_030.0));
        let with_pointer = count(&build(&f));
        assert!(with_pointer > bare, "{with_pointer} against {bare}");
        // A time far in the past is off screen: nothing to draw.
        f.remote = Some((series.time_at(0).unwrap() - 86_400_000, 100_030.0));
        assert_eq!(count(&build(&f)), bare);
    }

    #[test]
    fn the_local_pointer_wins_over_a_remote_one() {
        let series = Series::Bars(bars(200));
        let view = View::new(8.0);
        let mut f = frame(&series, &view, ChartKind::Candles);
        let local_only = count(&build(&f));
        f.remote = Some((series.time_at(150).unwrap(), 100_030.0));
        assert_eq!(count(&build(&f)), local_only);
    }

    #[test]
    fn quotes_with_fewer_decimals_have_a_bigger_unit() {
        assert_eq!(quote_unit(5), 1.0);
        assert_eq!(quote_unit(3), 100.0);
        assert_eq!(quote_unit(2), 1_000.0);
    }
}
