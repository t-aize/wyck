//! Turns the data, the view and the settings into a flat list of drawing commands.
//!
//! Building the list is plain arithmetic (no window needed), so it can be unit tested, and the
//! amount of work is bounded by the size of the screen, never by the size of the data:
//!
//! - Only the points that reach the plot are visited.
//! - Once points are narrower than a pixel they are folded into one column each (their low, high,
//!   first and last), so a chart with a hundred thousand bars costs what a screenful costs.
//!
//! The plot is cut into bands ([`mod@geometry`]): the prices on top, then one pane per indicator that
//! needs its own scale. Each band has its own [`PriceMap`]; the time axis is shared.

pub mod cmd;
pub mod color;
mod footprint;
pub mod geometry;
pub mod price;
mod series;
mod studies;
mod tpo;

pub use self::cmd::{Align, Cmd, FONT, LINE, P, hsla, rgb_alpha, with_alpha};
use self::color::{Hsla, Rgba, rgb, transparent_black};
pub use self::geometry::{AXIS_H, AXIS_W, Band, Geometry};
pub use self::price::PriceMap;
use super::axis;
use super::data::Series;
use super::display::{Display, effective_kind};
use super::drawing::geometry::{self as shapes, Anchor, Prim};
use super::drawing::model::{Dash, Drawing};
use super::flow::Flow;
use super::options::{ChartColors, CrosshairStyle};
use super::projection::ChartProjection;
use super::settings::{ChartKind, ChartSettings, ScaleMode};
use super::study::{Placement, ValueFormat};
use super::timeframe::Timeframe;
use super::view::{PriceScale, View, padded_range};

/// The opacity of a grid color the user chose.
const GRID_ALPHA: f32 = 0.3;

/// The size of the text of the watermark.
const WATERMARK_SIZE: f32 = 46.0;

/// The colors of a TPO profile that a theme does not have: mid tones that read on light and dark.
pub const TPO_MARKS: u32 = 0x7f93b2;
pub const TPO_POC: u32 = 0xf0a020;
pub const TPO_VALUE_AREA: u32 = 0x5b8def;
pub const TPO_IB: u32 = 0xb27be6;
pub const TPO_SINGLE: u32 = 0xe5586a;

/// One unit of the symbol's last decimal, in raw price units.
pub fn quote_unit(digits: u32) -> f64 {
    10f64.powi(5 - i32::try_from(digits.min(5)).unwrap_or(5))
}

/// The colors the chart draws with.
#[derive(Debug, Clone, Copy)]
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
    pub crosshair: Rgba,
    pub accent: Rgba,
    /// The thick and thin lines of Kagi.
    pub kagi_yang: Rgba,
    pub kagi_yin: Rgba,
    /// The X and the O of point and figure.
    pub pnf_up: Rgba,
    pub pnf_down: Rgba,
    /// The marks, the point of control, the value area, the initial balance and the single
    /// prints of a TPO profile.
    pub tpo: Rgba,
    pub tpo_poc: Rgba,
    pub tpo_value_area: Rgba,
    pub tpo_ib: Rgba,
    pub tpo_single: Rgba,
}

impl Palette {
    pub fn new() -> Self {
        Self {
            up: rgb(0x26a69a),
            down: rgb(0xef5350),
            line: rgb(0x5b8def),
            grid: rgb(0x30343b),
            text: rgb(0x9aa0a6),
            text_strong: rgb(0xf0f0f0),
            bg: rgb(0x0a0a0a),
            tag: rgb(0x363a45),
            border: rgb(0x30343b),
            crosshair: rgb(0x9aa0a6),
            accent: rgb(0x7c86ff),
            kagi_yang: rgb(0x26a69a),
            kagi_yin: rgb(0xef5350),
            pnf_up: rgb(0x26a69a),
            pnf_down: rgb(0xef5350),
            tpo: rgb(TPO_MARKS),
            tpo_poc: rgb(TPO_POC),
            tpo_value_area: rgb(TPO_VALUE_AREA),
            tpo_ib: rgb(TPO_IB),
            tpo_single: rgb(TPO_SINGLE),
        }
    }

    /// The colors of the theme, with the ones a chart overrides.
    pub fn for_chart(colors: &ChartColors) -> Self {
        Self::for_chart_with_base(colors, Self::new())
    }

    /// Applies per-chart colors to a frontend-supplied palette.
    pub fn for_chart_with_base(colors: &ChartColors, mut palette: Self) -> Self {
        let pick = |color: u32| rgb(color);
        if let Some(c) = colors.up {
            palette.up = pick(c);
        }
        if let Some(c) = colors.down {
            palette.down = pick(c);
        }
        // What follows the rising and falling colors, unless it has a color of its own.
        (palette.kagi_yang, palette.kagi_yin) = (palette.up, palette.down);
        (palette.pnf_up, palette.pnf_down) = (palette.up, palette.down);
        if let Some(c) = colors.kagi_yang {
            palette.kagi_yang = pick(c);
        }
        if let Some(c) = colors.kagi_yin {
            palette.kagi_yin = pick(c);
        }
        if let Some(c) = colors.pnf_up {
            palette.pnf_up = pick(c);
        }
        if let Some(c) = colors.pnf_down {
            palette.pnf_down = pick(c);
        }
        for (own, slot) in [
            (colors.tpo, &mut palette.tpo),
            (colors.tpo_poc, &mut palette.tpo_poc),
            (colors.tpo_value_area, &mut palette.tpo_value_area),
            (colors.tpo_ib, &mut palette.tpo_ib),
            (colors.tpo_single, &mut palette.tpo_single),
        ] {
            if let Some(c) = own {
                *slot = pick(c);
            }
        }
        if let Some(c) = colors.line {
            palette.line = pick(c);
        }
        if let Some(c) = colors.background {
            palette.bg = pick(c);
        }
        if let Some(c) = colors.grid {
            // The grid sits behind the prices: a chosen color is drawn faint, like the theme's.
            palette.grid = Rgba {
                a: GRID_ALPHA,
                ..pick(c)
            };
        }
        if let Some(c) = colors.crosshair {
            palette.crosshair = pick(c);
        }
        if let Some(c) = colors.text {
            palette.text = pick(c);
            palette.text_strong = pick(c);
        }
        palette
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::new()
    }
}

/// A horizontal line the chart is told to show on the prices: an order, a position, an alert.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceMark {
    /// Raw price units.
    pub price: f64,
    pub color: u32,
    pub dash: Dash,
    pub width: f32,
    /// Where the line starts, from the left of the plot (a line may start at a time).
    pub from_x: Option<f32>,
    /// Whether the price shows as a tag on the axis.
    pub axis_tag: bool,
}

/// The drawings to show on a chart.
pub struct DrawingView<'a> {
    pub list: &'a [Drawing],
    /// The drawing being made, which follows the pointer.
    pub creating: Option<&'a Drawing>,
    pub selected: Option<u64>,
    /// Whether a drawing shows on this timeframe.
    pub visible: &'a dyn Fn(&Drawing) -> bool,
}

pub struct Frame<'a> {
    /// The prices as held, for the last price.
    pub raw: &'a Series,
    pub display: &'a Display,
    pub settings: &'a ChartSettings,
    pub view: &'a View,
    pub timeframe: Timeframe,
    pub digits: u32,
    pub origin: P,
    pub w: f64,
    pub h: f64,
    /// Device pixels per logical pixel, for snapping lines to the pixel grid.
    pub scale: f32,
    /// The pointer, from the top left of the chart.
    pub hover: Option<P>,
    /// The pointer of another chart (time and raw price), when the crosshairs are linked.
    pub remote: Option<(i64, f64)>,
    pub ask: Option<i64>,
    pub now_ms: i64,
    pub palette: Palette,
    pub drawings: Option<DrawingView<'a>>,
    pub marks: &'a [PriceMark],
    /// What traded at each price of each bar, for the footprint chart type.
    pub flow: Option<&'a Flow>,
    /// The text written faint behind the prices, when the chart has a watermark.
    pub watermark: Option<String>,
}

impl Frame<'_> {
    fn series(&self) -> &Series {
        self.display.shown(self.raw)
    }

    fn kind(&self) -> ChartKind {
        effective_kind(self.raw, self.settings.kind)
    }

    /// The time between two points, for placing times past the data.
    fn step_ms(&self) -> f64 {
        let nominal = if self.display.is_derived() && self.settings.kind != ChartKind::HeikinAshi {
            None
        } else {
            self.timeframe.bar_ms()
        };
        self.series().step_ms(nominal)
    }
}

/// The bands of a chart of `w` by `h` with these settings.
pub fn geometry(settings: &ChartSettings, w: f64, h: f64) -> Geometry {
    let mut weights = vec![settings.main_weight];
    weights.extend(settings.panes().iter().map(|pane| pane.weight));
    Geometry::new(w, h, &weights)
}

/// The price scale of the prices band, or `None` without data.
pub fn main_map(
    raw: &Series,
    display: &Display,
    settings: &ChartSettings,
    view: &View,
    geometry: &Geometry,
    digits: u32,
) -> Option<PriceMap> {
    let series = display.shown(raw);
    let plot_w = geometry.plot_w();
    let (first, last) = view.visible(series.len(), plot_w);
    let kind = effective_kind(raw, settings.kind);
    let (lo, hi) = match view.price {
        PriceScale::Manual { lo, hi } if hi > lo => (lo, hi),
        _ => {
            let (mut lo, mut hi) = series
                .price_range(first, last, kind.uses_ohlc())
                .map(|(l, h)| (l as f64, h as f64))?;
            // Indicators on the prices count too, so a band never runs off the top.
            for (config, output) in settings.studies.iter().zip(&display.studies) {
                if config.spec().placement != Placement::Overlay || !config.visible {
                    continue;
                }
                if let Some(output) = output {
                    for plot in &output.plots {
                        if let Some((l, h)) = studies::visible_range(plot, first, last) {
                            lo = lo.min(l);
                            hi = hi.max(h);
                        }
                    }
                }
            }
            padded_range(lo, hi, quote_unit(digits) * 10.0, settings.margin.share())
        }
    };
    let (lo, hi) = if settings.scale == ScaleMode::Log {
        (lo.max(1.0), hi.max(lo.max(1.0) + 1.0))
    } else {
        (lo, hi)
    };
    let band = geometry.main();
    let reserved = 12.0 + f64::from(footprint::reserved(settings));
    let base = series
        .value_at(first.min(series.len().saturating_sub(1)))
        .map_or(0.0, |v| v as f64);
    Some(PriceMap {
        lo,
        hi,
        top: band.top + 12.0,
        bottom: (band.bottom() - reserved).max(band.top + 30.0),
        mode: settings.scale,
        invert: settings.invert,
        base,
    })
}

/// The drawing context of the prices band.
struct Ctx<'a> {
    f: &'a Frame<'a>,
    series: &'a Series,
    map: PriceMap,
    len: usize,
    plot_w: f64,
    band: Band,
    ox: f32,
    oy: f32,
}

impl Ctx<'_> {
    fn snap(&self, v: f32) -> f32 {
        (v * self.f.scale).round() / self.f.scale
    }

    /// Absolute x of point `index` (fractional, possibly past the data).
    fn xf(&self, index: f64) -> f32 {
        self.ox + self.f.view.x_of(index, self.len, self.plot_w) as f32
    }

    fn x(&self, index: usize) -> f32 {
        self.xf(index as f64)
    }

    /// Absolute y of a value on `map`, kept to a range a GPU can draw.
    fn y_on(&self, map: &PriceMap, value: f64) -> f32 {
        self.oy + (map.y(value).clamp(-4_000.0, 20_000.0)) as f32
    }

    fn y(&self, price: f64) -> f32 {
        self.y_on(&self.map, price)
    }

    fn rect(&self, x: f32, y: f32, w: f32, h: f32, fill: Hsla) -> Cmd {
        Cmd::Rect {
            x,
            y,
            w: w.max(0.0),
            h: h.max(0.0),
            fill,
            border: None,
            radius: 0.0,
        }
    }

    /// A one device pixel wide vertical line from `y0` to `y1`.
    fn vline(&self, x: f32, y0: f32, y1: f32, c: Hsla) -> Cmd {
        let w = 1.0 / self.f.scale;
        let (a, b) = (y0.min(y1), y0.max(y1));
        self.rect(
            self.snap(x),
            self.snap(a),
            w,
            self.snap(b) - self.snap(a),
            c,
        )
    }

    fn hline(&self, y: f32, x0: f32, x1: f32, c: Hsla) -> Cmd {
        let h = 1.0 / self.f.scale;
        self.rect(
            self.snap(x0),
            self.snap(y),
            self.snap(x1) - self.snap(x0),
            h,
            c,
        )
    }

    fn up_color(&self, up: bool) -> Rgba {
        if up {
            self.f.palette.up
        } else {
            self.f.palette.down
        }
    }
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

/// Where the crosshair is (from the top left of the chart) and which band it is in: under the
/// pointer when it is over this chart, else where another chart's pointer is (on the prices,
/// when its time is on screen).
fn pointer_of(frame: &Frame<'_>, geometry: &Geometry, map: &PriceMap) -> Option<(P, usize)> {
    let plot_w = geometry.plot_w();
    if let Some((x, y)) = frame.hover
        && (0.0..=plot_w).contains(&f64::from(x))
        && let Some(band) = geometry.band_at(f64::from(y))
    {
        return Some(((x, y), band));
    }
    let (time_ms, price) = frame.remote?;
    let series = frame.series();
    let index = series.index_of_time(time_ms, frame.step_ms())?;
    let x = frame.view.x_of(index, series.len(), plot_w);
    if !(0.0..=plot_w).contains(&x) {
        return None;
    }
    // Another symbol's price means nothing here: only its time is shown, as the vertical line.
    let y = if price.is_finite() {
        map.y(price)
    } else {
        -1.0
    };
    let main = geometry.main();
    (main.contains(y) || !price.is_finite()).then_some(((x as f32, y as f32), 0))
}

/// Builds every command for one frame.
pub fn build(frame: &Frame<'_>) -> Vec<Cmd> {
    let series = frame.series();
    let len = series.len();
    let geometry = geometry(frame.settings, frame.w, frame.h);
    let plot_w = geometry.plot_w();
    let plot_h = geometry.plot_h();
    let mut cmds: Vec<Cmd> = Vec::new();
    let p = frame.palette;
    let (ox, oy) = frame.origin;

    let Some(map) = main_map(
        frame.raw,
        frame.display,
        frame.settings,
        frame.view,
        &geometry,
        frame.digits,
    ) else {
        cmds.push(axes_background(frame, &geometry));
        return cmds;
    };
    let pointer = if frame.settings.crosshair == CrosshairStyle::Off {
        None
    } else {
        pointer_of(frame, &geometry, &map)
    };
    let main = geometry.main();
    let cx = Ctx {
        f: frame,
        series,
        map,
        len,
        plot_w,
        band: main,
        ox,
        oy,
    };
    let (first, last) = frame.view.visible(len, plot_w);
    let kind = frame.kind();
    let time_labels = if frame.settings.kind.keeps_time() || !frame.display.is_derived() {
        axis::time_labels(
            first,
            last,
            |i| series.time_at(i),
            |i| frame.view.x_of(i as f64, len, plot_w),
            plot_w,
            96.0,
            frame.settings.zone,
        )
    } else {
        // Bricks and columns are not periods: label them by date, further apart.
        axis::time_labels(
            first,
            last,
            |i| series.time_at(i),
            |i| frame.view.x_of(i as f64, len, plot_w),
            plot_w,
            120.0,
            frame.settings.zone,
        )
    };
    let unit = quote_unit(frame.digits);
    let main_ticks = map.ticks(8.max((main.h / 60.0) as usize).min(14), unit);

    // ---- the prices band ----
    let mut plot: Vec<Cmd> = Vec::new();
    if let Some(text) = &frame.watermark {
        plot.push(Cmd::Text {
            text: text.clone(),
            x: ox + plot_w as f32 / 2.0,
            y: oy + (main.top + main.h / 2.0) as f32 - WATERMARK_SIZE * 0.65,
            size: WATERMARK_SIZE,
            color: with_alpha(p.text_strong, 0.07),
            align: Align::Center,
            bold: true,
        });
    }
    let (grid_h, grid_v) = (
        frame.settings.grid && frame.settings.grid_horizontal,
        frame.settings.grid && frame.settings.grid_vertical,
    );
    if grid_h {
        for value in &main_ticks {
            plot.push(cx.hline(cx.y(*value), ox, ox + plot_w as f32, hsla(p.grid)));
        }
    }
    if grid_v {
        for label in &time_labels {
            plot.push(cx.vline(
                cx.x(label.index),
                oy + main.top as f32,
                oy + main.bottom() as f32,
                hsla(p.grid),
            ));
        }
    }
    if frame.settings.price_lines.day_breaks {
        for index in day_breaks(frame, first, last) {
            plot.push(cx.vline(
                cx.x(index),
                oy,
                oy + plot_h as f32,
                with_alpha(p.text, 0.28),
            ));
        }
    }
    studies::overlays_under(&cx, first, last, &mut plot);
    series::draw(&cx, kind, first, last, &mut plot);
    studies::overlays(&cx, first, last, &mut plot);
    for mark in frame.marks {
        let y = cx.y(mark.price);
        let x0 = ox + mark.from_x.unwrap_or(0.0).max(0.0);
        plot.push(Cmd::Stroke {
            points: vec![
                (x0, cx.snap(y) + 0.5),
                (ox + plot_w as f32, cx.snap(y) + 0.5),
            ],
            width: mark.width,
            color: rgb_alpha(mark.color, 0.9),
            dash: dash_pattern(mark.dash, mark.width),
        });
    }
    if let Some(view) = &frame.drawings {
        draw_drawings(&cx, view, &mut plot);
    }
    draw_price_lines(&cx, &mut plot);
    if let Some(((_, hy), 0)) = pointer {
        plot.push(crosshair_line(&cx, hy));
    }
    cmds.push(Cmd::Clip {
        x: ox,
        y: oy + main.top as f32,
        w: plot_w as f32,
        h: main.h as f32,
        inner: plot,
    });

    // ---- the indicator panes ----
    let panes = frame.settings.panes();
    let mut pane_maps: Vec<(Band, PriceMap, ValueFormat, Vec<f64>)> = Vec::new();
    for (i, pane) in panes.iter().enumerate() {
        let band = geometry.bands[i + 1];
        let config = &frame.settings.studies[pane.study];
        let Some(Some(output)) = frame.display.studies.get(pane.study) else {
            continue;
        };
        let spec = config.spec();
        let pmap = studies::pane_map(output, spec.range, band, first, last);
        let ticks = axis::price_ticks(
            pmap.lo,
            pmap.hi,
            (band.h / 40.0).clamp(2.0, 8.0) as usize,
            f64::MIN_POSITIVE,
        );
        let mut inner = Vec::new();
        if grid_h {
            for value in &ticks {
                inner.push(cx.hline(cx.y_on(&pmap, *value), ox, ox + plot_w as f32, hsla(p.grid)));
            }
        }
        if grid_v {
            for label in &time_labels {
                inner.push(cx.vline(
                    cx.x(label.index),
                    oy + band.top as f32,
                    oy + band.bottom() as f32,
                    hsla(p.grid),
                ));
            }
        }
        studies::pane(&cx, config, output, &pmap, first, last, &mut inner);
        if let Some(((_, hy), b)) = pointer
            && b == i + 1
        {
            inner.push(crosshair_line(&cx, hy));
        }
        cmds.push(Cmd::Clip {
            x: ox,
            y: oy + band.top as f32,
            w: plot_w as f32,
            h: band.h as f32,
            inner,
        });
        pane_maps.push((band, pmap, spec.format, ticks));
    }

    // The crosshair's vertical line crosses every band.
    if let Some(((hx, _), _)) = pointer {
        let x = if len > 0 {
            let index = frame
                .view
                .index_at(f64::from(hx), len, plot_w)
                .round()
                .clamp(0.0, (len - 1) as f64);
            cx.xf(index)
        } else {
            ox + hx
        };
        cmds.push(Cmd::Clip {
            x: ox,
            y: oy,
            w: plot_w as f32,
            h: plot_h as f32,
            inner: vec![Cmd::Stroke {
                points: vec![
                    (cx.snap(x) + 0.5, oy),
                    (cx.snap(x) + 0.5, oy + plot_h as f32),
                ],
                width: 1.0,
                color: hsla(p.crosshair),
                dash: frame.settings.crosshair.dash(),
            }],
        });
    }

    // ---- the axes, over the plot ----
    cmds.push(axes_background(frame, &geometry));
    for band in geometry.bands.iter().skip(1) {
        cmds.push(cx.hline(
            oy + band.top as f32,
            ox,
            ox + frame.w as f32,
            hsla(p.border),
        ));
    }
    let axis_x = ox + plot_w as f32 + 8.0;
    let text = |text: String, y: f32| Cmd::Text {
        text,
        x: axis_x,
        y: y - LINE / 2.0,
        size: FONT,
        color: hsla(p.text),
        align: Align::Left,
        bold: false,
    };
    let in_band = |band: &Band, y: f32| {
        y >= oy + band.top as f32 + 6.0 && y <= oy + band.bottom() as f32 - 6.0
    };
    // The tags of the price axes are made first, spread so none hides another, and the scales'
    // labels then keep clear of them. The pointer's tags go over everything and move nothing.
    let mut tags = Vec::new();
    studies::value_tags(&cx, &pane_maps_for_tags(&panes, &pane_maps), &mut tags);
    let mut pointer_tags = Vec::new();
    draw_axis_tags(
        &cx,
        pointer,
        &geometry,
        &pane_maps,
        &mut tags,
        &mut pointer_tags,
    );
    spread_tags(&mut tags, oy, oy + plot_h as f32);
    let taken: Vec<(f32, f32)> = tags
        .iter()
        .chain(&pointer_tags)
        .filter_map(|cmd| match cmd {
            Cmd::Tag {
                y,
                height,
                fixed_width: Some(_),
                ..
            } => Some((*y, y + height)),
            _ => None,
        })
        .collect();
    let free = |y: f32| {
        taken
            .iter()
            .all(|(top, bottom)| y + 6.0 < *top || y - 6.0 > *bottom)
    };
    for value in &main_ticks {
        let y = cx.y(*value);
        if in_band(&main, y) && free(y) {
            cmds.push(text(map.label(*value, ValueFormat::Price, frame.digits), y));
        }
    }
    for (band, pmap, format, ticks) in &pane_maps {
        for value in ticks {
            let y = cx.y_on(pmap, *value);
            if in_band(band, y) && free(y) {
                cmds.push(text(price::format_value(*value, *format, frame.digits), y));
            }
        }
    }
    for label in &time_labels {
        cmds.push(Cmd::Text {
            text: label.text.clone(),
            x: cx.x(label.index),
            y: oy + plot_h as f32 + 8.0,
            size: FONT,
            color: if label.major {
                hsla(p.text_strong)
            } else {
                hsla(p.text)
            },
            align: Align::Center,
            bold: label.major,
        });
    }
    // The zone the times are in, in the corner between the axes, and the scale mode.
    let corner_time = frame.raw.last_time().unwrap_or(frame.now_ms);
    let mut corner = frame.settings.zone.label(corner_time);
    if !frame.settings.scale.short().is_empty() {
        corner = format!("{corner}  {}", frame.settings.scale.short());
    }
    cmds.push(Cmd::Text {
        text: corner,
        x: ox + plot_w as f32 + AXIS_W / 2.0,
        y: oy + plot_h as f32 + 8.0,
        size: FONT,
        color: hsla(p.text),
        align: Align::Center,
        bold: false,
    });
    cmds.extend(tags);
    cmds.extend(pointer_tags);
    cmds
}

fn pane_maps_for_tags(
    panes: &[super::settings::PaneRef],
    maps: &[(Band, PriceMap, ValueFormat, Vec<f64>)],
) -> Vec<(usize, PriceMap, Band, ValueFormat)> {
    panes
        .iter()
        .zip(maps)
        .map(|(pane, (band, map, format, _))| (pane.study, *map, *band, *format))
        .collect()
}

fn crosshair_line(cx: &Ctx<'_>, hy: f32) -> Cmd {
    let y = cx.snap(cx.oy + hy) + 0.5;
    Cmd::Stroke {
        points: vec![(cx.ox, y), (cx.ox + cx.plot_w as f32, y)],
        width: 1.0,
        color: hsla(cx.f.palette.crosshair),
        dash: cx.f.settings.crosshair.dash(),
    }
}

/// The solid strips of the two axes and the lines that set them off.
fn axes_background(frame: &Frame<'_>, geometry: &Geometry) -> Cmd {
    let (ox, oy) = frame.origin;
    let plot_w = geometry.plot_w() as f32;
    let plot_h = geometry.plot_h() as f32;
    let (w, h) = (frame.w as f32, frame.h as f32);
    let p = frame.palette;
    let one = 1.0 / frame.scale;
    let rect = |x: f32, y: f32, rw: f32, rh: f32, c: Hsla| Cmd::Rect {
        x,
        y,
        w: rw.max(0.0),
        h: rh.max(0.0),
        fill: c,
        border: None,
        radius: 0.0,
    };
    Cmd::Clip {
        x: ox,
        y: oy,
        w,
        h,
        inner: vec![
            rect(ox + plot_w, oy, w - plot_w, h, hsla(p.bg)),
            rect(ox, oy + plot_h, plot_w, h - plot_h, hsla(p.bg)),
            rect(ox + plot_w, oy, one, h, hsla(p.border)),
            rect(ox, oy + plot_h, w, one, hsla(p.border)),
        ],
    }
}

/// The dash pattern of a line style, for a line `width` wide.
pub fn dash_pattern(dash: Dash, width: f32) -> Option<[f32; 2]> {
    match dash {
        Dash::Solid => None,
        Dash::Dashed => Some([6.0 + width * 1.5, 4.0 + width]),
        Dash::Dotted => Some([width.max(1.0), 3.0 + width]),
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

/// The close of the last bar of the day before the newest bar's, in raw units, when the prices
/// are bars.
fn previous_close(series: &Series, zone: super::zone::Zone) -> Option<i64> {
    let Series::Bars(bars) = series else {
        return None;
    };
    let day = zone.day(bars.last()?.time_ms);
    bars.iter()
        .rev()
        .find(|bar| zone.day(bar.time_ms) != day)
        .map(|bar| bar.close)
}

/// The indexes in `first..last` of the points that start a new day, when the chart's points are
/// periods shorter than a day.
fn day_breaks(frame: &Frame<'_>, first: usize, last: usize) -> Vec<usize> {
    let intraday = frame
        .timeframe
        .bar_ms()
        .is_some_and(|ms| ms < 86_400_000 && frame.settings.kind.keeps_time());
    if !intraday {
        return Vec::new();
    }
    let series = frame.series();
    let zone = frame.settings.zone;
    (first.max(1)..last)
        .filter(|&i| match (series.time_at(i - 1), series.time_at(i)) {
            (Some(before), Some(now)) => zone.day(before) != zone.day(now),
            _ => false,
        })
        .collect()
}

fn draw_price_lines(cx: &Ctx<'_>, out: &mut Vec<Cmd>) {
    let p = cx.f.palette;
    let lines = cx.f.settings.price_lines;
    let (x0, x1) = (cx.ox, cx.ox + cx.plot_w as f32);
    if lines.previous_close
        && let Some(close) = previous_close(cx.f.raw, cx.f.settings.zone)
    {
        let y = cx.snap(cx.y(close as f64)) + 0.5;
        out.push(Cmd::Stroke {
            points: vec![(x0, y), (x1, y)],
            width: 1.0,
            color: with_alpha(p.text, 0.55),
            dash: Some([6.0, 4.0]),
        });
    }
    if let Some(ask) = cx.f.ask.filter(|_| lines.ask_line) {
        out.push(Cmd::Stroke {
            points: vec![
                (x0, cx.snap(cx.y(ask as f64)) + 0.5),
                (x1, cx.snap(cx.y(ask as f64)) + 0.5),
            ],
            width: 1.0,
            color: with_alpha(p.text, 0.35),
            dash: Some([2.0, 3.0]),
        });
    }
    if let Some(price) = cx.f.raw.last_price().filter(|_| lines.last_line) {
        let c = with_alpha(cx.up_color(last_is_up(cx.f.raw)), 0.8);
        let y = cx.snap(cx.y(price as f64)) + 0.5;
        out.push(Cmd::Stroke {
            points: vec![(x0, y), (x1, y)],
            width: 1.0,
            color: c,
            dash: Some([1.0, 2.0]),
        });
    }
}

/// `05:12`, `1:05:12` or `2d 03:04`: the time left before the bar closes.
pub fn countdown(ms: i64) -> String {
    let seconds = ms.max(0) / 1_000;
    let (days, hours, minutes, secs) = (
        seconds / 86_400,
        seconds % 86_400 / 3_600,
        seconds % 3_600 / 60,
        seconds % 60,
    );
    if days > 0 {
        format!("{days}d {hours:02}:{minutes:02}")
    } else if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

/// The tags on the axes: the newest price with the time left in its bar, the marks, and the
/// crosshair's value and time.
/// Draws the tags on the price axis (the order and position lines, the last price and the time
/// left in its bar) into `out`, and the pointer's price and time into `pointer_out`.
fn draw_axis_tags(
    cx: &Ctx<'_>,
    pointer: Option<(P, usize)>,
    geometry: &Geometry,
    panes: &[(Band, PriceMap, ValueFormat, Vec<f64>)],
    out: &mut Vec<Cmd>,
    pointer_out: &mut Vec<Cmd>,
) {
    let p = cx.f.palette;
    let axis_x = cx.ox + cx.plot_w as f32 + 1.0;
    let main = geometry.main();
    let clamp_y = |band: &Band, y: f32| {
        y.clamp(
            cx.oy + band.top as f32 + 9.0,
            cx.oy + band.bottom() as f32 - 9.0,
        )
    };
    let tag = |text: String, y: f32, bg: Hsla, fg: Hsla| Cmd::Tag {
        text,
        x: axis_x,
        y: y - 9.0,
        height: 18.0,
        pad: 7.0,
        bg,
        fg,
        align: Align::Left,
        fixed_width: Some(AXIS_W - 2.0),
        within: None,
        size: FONT,
        bold: false,
    };
    for mark in cx.f.marks.iter().filter(|m| m.axis_tag) {
        let y = cx.y(mark.price);
        if main.contains(f64::from(y - cx.oy)) {
            out.push(tag(
                cx.map.label(mark.price, ValueFormat::Price, cx.f.digits),
                y,
                rgb_alpha(mark.color, 1.0),
                hsla(p.bg),
            ));
        }
    }
    if let Some(price) = cx.f.raw.last_price() {
        let y = clamp_y(&main, cx.y(price as f64));
        let bg = hsla(cx.up_color(last_is_up(cx.f.raw)));
        let lines = cx.f.settings.price_lines;
        if lines.last_tag {
            out.push(tag(
                cx.map.label(price as f64, ValueFormat::Price, cx.f.digits),
                y,
                bg,
                hsla(p.bg),
            ));
        }
        // The time left in the bar, under the price, for bars that are periods of time.
        if let (Some(bar_ms), Some(last)) = (cx.f.timeframe.bar_ms(), cx.f.raw.last_time())
            && lines.countdown
            && cx.f.settings.kind.keeps_time()
            && matches!(cx.f.raw, Series::Bars(_))
        {
            let left = last + bar_ms - cx.f.now_ms;
            if left > 0 && left <= bar_ms {
                out.push(Cmd::Tag {
                    text: countdown(left),
                    x: axis_x,
                    // Under the tag of the price, or where it would be without one.
                    y: if lines.last_tag { y + 9.0 } else { y - 8.0 },
                    height: 16.0,
                    pad: 7.0,
                    bg: shade(cx.up_color(last_is_up(cx.f.raw)), 0.72),
                    fg: hsla(p.bg),
                    align: Align::Left,
                    fixed_width: Some(AXIS_W - 2.0),
                    within: None,
                    size: FONT,
                    bold: false,
                });
            }
        }
    }
    let Some(((hx, hy), band_index)) = pointer else {
        return;
    };
    let band = geometry.bands[band_index];
    let y = clamp_y(&band, cx.oy + hy);
    let text = if !band.contains(f64::from(hy)) {
        None
    } else if band_index == 0 {
        Some(
            cx.map
                .label(cx.map.price(f64::from(hy)), ValueFormat::Price, cx.f.digits),
        )
    } else {
        panes.get(band_index - 1).map(|(_, map, format, _)| {
            price::format_value(map.price(f64::from(hy)), *format, cx.f.digits)
        })
    };
    if let Some(text) = text {
        pointer_out.push(tag(text, y, hsla(p.tag), hsla(p.text_strong)));
    }
    if cx.len > 0 {
        let index =
            cx.f.view
                .index_at(f64::from(hx), cx.len, cx.plot_w)
                .round()
                .clamp(0.0, (cx.len - 1) as f64) as usize;
        if let Some(time) = cx.series.time_at(index) {
            let text = axis::full_time(
                time,
                cx.f.settings.zone,
                matches!(cx.f.timeframe, Timeframe::Seconds(_) | Timeframe::Ticks),
                matches!(cx.f.timeframe, Timeframe::Ticks),
            );
            pointer_out.push(Cmd::Tag {
                text,
                x: cx.x(index),
                y: cx.oy + geometry.plot_h() as f32 + 4.0,
                height: 20.0,
                pad: 8.0,
                bg: hsla(p.tag),
                fg: hsla(p.text_strong),
                align: Align::Center,
                fixed_width: None,
                within: Some((cx.ox, cx.ox + cx.plot_w as f32)),
                size: FONT,
                bold: false,
            });
        }
    }
}

/// Moves the tags of the price axes apart so none covers another. The later a tag comes, the
/// more it matters: it keeps its place, and the earlier ones make room around it, each moved as
/// little as it can be within `top..bottom`.
fn spread_tags(tags: &mut [Cmd], top: f32, bottom: f32) {
    let mut placed: Vec<(f32, f32)> = Vec::new();
    for cmd in tags.iter_mut().rev() {
        let Cmd::Tag {
            y,
            height,
            fixed_width: Some(_),
            ..
        } = cmd
        else {
            continue;
        };
        let h = *height;
        let fits = |at: f32| {
            at >= top && at + h <= bottom && placed.iter().all(|(t, b)| at + h <= *t || at >= *b)
        };
        let wanted = *y;
        let mut chosen = wanted;
        if !fits(wanted) {
            // The nearest free place, looking up and down a pixel at a time.
            for step in 1..400 {
                let d = step as f32;
                if fits(wanted - d) {
                    chosen = wanted - d;
                    break;
                }
                if fits(wanted + d) {
                    chosen = wanted + d;
                    break;
                }
            }
        }
        *y = chosen;
        placed.push((chosen, chosen + h));
    }
}

/// A color darkened towards black by `amount` of its brightness kept, opaque.
fn shade(color: Rgba, keep: f32) -> Hsla {
    hsla(Rgba {
        r: color.r * keep,
        g: color.g * keep,
        b: color.b * keep,
        a: 1.0,
    })
}

/// Draws the drawings of the symbol, and the grips of the selected one.
fn draw_drawings(cx: &Ctx<'_>, view: &DrawingView<'_>, out: &mut Vec<Cmd>) {
    let projection = ChartProjection {
        series: cx.series,
        view: cx.f.view,
        map: cx.map,
        plot_w: cx.plot_w,
        plot_h: cx.band.h,
        step_ms: cx.f.step_ms(),
        digits: cx.f.digits,
    };
    for drawing in view
        .list
        .iter()
        .filter(|d| (view.visible)(d))
        .chain(view.creating)
    {
        let selected = view.selected == Some(drawing.id);
        for prim in shapes::prims_with(drawing, &projection, selected) {
            push_prim(cx, prim, out);
        }
        if selected && !drawing.locked {
            for at in shapes::handles(drawing, &projection) {
                push_prim(cx, Prim::Handle { at }, out);
            }
        }
    }
}

/// A ring of points around an ellipse.
pub fn ellipse_points(center: P, rx: f32, ry: f32, steps: usize) -> Vec<P> {
    (0..=steps)
        .map(|i| {
            let angle = i as f32 / steps as f32 * std::f32::consts::TAU;
            (center.0 + rx * angle.cos(), center.1 + ry * angle.sin())
        })
        .collect()
}

/// Turns one shape of a drawing (in plot coordinates) into drawing commands.
fn push_prim(cx: &Ctx<'_>, prim: Prim, out: &mut Vec<Cmd>) {
    let (ox, oy) = (cx.ox, cx.oy);
    let at = |p: P| (ox + p.0, oy + p.1);
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
                out.push(Cmd::Stroke {
                    points: vec![a, b],
                    width,
                    color: paint,
                    dash: dash_pattern(dash, width),
                });
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
            out.push(Cmd::Rect {
                x: left,
                y: top,
                w: right - left,
                h: bottom - top,
                fill: inside.map_or_else(transparent_black, |(c, al)| rgb_alpha(c, al)),
                border: edge.map(|(c, width)| (width.max(thin), rgb_alpha(c, 1.0))),
                radius: 0.0,
            });
        }
        Prim::Ellipse {
            center,
            rx,
            ry,
            fill: inside,
            stroke: edge,
        } => {
            let ring = ellipse_points(at(center), rx, ry, 64);
            if let Some((c, al)) = inside {
                out.push(Cmd::Fill {
                    points: ring.clone(),
                    color: rgb_alpha(c, al),
                });
            }
            if let Some((c, width)) = edge {
                out.push(Cmd::Stroke {
                    points: ring,
                    width,
                    color: rgb_alpha(c, 1.0),
                    dash: None,
                });
            }
        }
        Prim::Polygon {
            points,
            fill: (c, al),
        } => {
            out.push(Cmd::Fill {
                points: points.into_iter().map(at).collect(),
                color: rgb_alpha(c, al),
            });
        }
        Prim::Polyline {
            points,
            color,
            width,
            alpha,
        } => {
            out.push(Cmd::Stroke {
                points: points.into_iter().map(at).collect(),
                width,
                color: rgb_alpha(color, alpha),
                dash: None,
            });
        }
        Prim::Board {
            tl,
            rows,
            fg,
            bg: (bg, bg_alpha),
            size,
            bold,
        } => {
            let (w, h) = shapes::board_size(&rows, size);
            let (row, pad_x, pad_y) = shapes::board_metrics();
            let (x, y) = at(tl);
            out.push(Cmd::Rect {
                x,
                y,
                w,
                h,
                fill: rgb_alpha(bg, bg_alpha),
                border: None,
                radius: 4.0,
            });
            for (i, text) in rows.into_iter().enumerate() {
                out.push(Cmd::Text {
                    text,
                    x: x + pad_x,
                    y: y + pad_y + i as f32 * size * row + (size * row - size * 1.3) / 2.0,
                    size,
                    color: rgb_alpha(fg, 1.0),
                    align: Align::Left,
                    bold,
                });
            }
        }
        Prim::Label {
            at: position,
            text,
            color,
            background,
            anchor,
            size,
            bold,
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
                    y: y - size * 0.8,
                    height: size * 1.6,
                    pad: 6.0,
                    bg: rgb_alpha(bg, al),
                    fg: rgb_alpha(color, 1.0),
                    align,
                    fixed_width: None,
                    within: None,
                    size,
                    bold,
                },
                None => Cmd::Text {
                    text,
                    x,
                    y: y - size * 1.3 / 2.0,
                    size,
                    color: rgb_alpha(color, 1.0),
                    align,
                    bold,
                },
            });
        }
        Prim::Handle { at: position } => {
            let (x, y) = at(position);
            let r = 4.5;
            out.push(Cmd::Rect {
                x: x - r,
                y: y - r,
                w: r * 2.0,
                h: r * 2.0,
                fill: hsla(cx.f.palette.bg),
                border: Some((1.5, hsla(cx.f.palette.accent))),
                radius: r,
            });
        }
    }
}

#[cfg(test)]
mod tests;
