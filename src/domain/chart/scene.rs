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

pub use self::cmd::{Align, Cmd, P, font, hsla, line_height, rgb_alpha, with_alpha};
use self::color::{Hsla, Rgba};
pub use self::geometry::{Band, Geometry, axis_h, axis_w};
pub use self::price::PriceMap;
use super::axis;
use super::data::Series;
use super::display::{Display, effective_kind};
use super::flow::Flow;
use super::options::CrosshairStyle;
use super::settings::{ChartKind, ChartSettings, ScaleMode};
use super::timeframe::Timeframe;
use super::view::{PriceScale, View, padded_range};
use crate::domain::drawings::model::{Dash, Drawing};
use crate::domain::indicators::{Placement, ValueFormat};

/// The opacity of a grid color the user chose.
const GRID_ALPHA: f32 = 0.3;

/// The size of the text of the watermark.
fn watermark_size() -> f32 {
    crate::domain::chart::text_scale::scaled(46.0)
}

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
    /// The drawings selected along with it.
    pub also: &'a [u64],
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
    pub pip_position: Option<i64>,
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
                        let style = config.plot_style(plot.key);
                        if !style.visible {
                            continue;
                        }
                        if let Some((l, h)) = studies::visible_range(
                            plot,
                            style.kind.unwrap_or(plot.kind),
                            first,
                            last,
                        ) {
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
    let main_ticks = map.ticks(axis_tick_target(main.h), unit);

    let mut plot: Vec<Cmd> = Vec::new();
    if let Some(text) = &frame.watermark {
        plot.push(Cmd::Text {
            text: text.clone(),
            x: ox + plot_w as f32 / 2.0,
            y: oy + (main.top + main.h / 2.0) as f32 - watermark_size() * 0.65,
            size: watermark_size(),
            color: with_alpha(p.text_strong, 0.07),
            align: Align::Center,
            bold: true,
            face: Default::default(),
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
    studies::overlays(&cx, first, last, &mut plot);
    draw_script_drawings(&cx, &mut plot);
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
    // Price marks remain readable even when a scripted or manual drawing fills their area.
    series::draw(&cx, kind, first, last, &mut plot);
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

    let panes = frame.settings.panes();
    let mut pane_maps: Vec<(Band, PriceMap, ValueFormat, Vec<f64>)> = Vec::new();
    for (i, pane) in panes.iter().enumerate() {
        let band = geometry.bands[i + 1];
        let config = &frame.settings.studies[pane.study];
        let Some(Some(output)) = frame.display.studies.get(pane.study) else {
            continue;
        };
        let spec = config.spec();
        let pmap = studies::pane_map(config, output, spec.range, band, first, last);
        let format = config.value_format();
        let min_step = match format {
            ValueFormat::Price => unit,
            ValueFormat::Plain(decimals) | ValueFormat::Percent(decimals) => {
                10f64.powi(-(decimals as i32))
            }
            ValueFormat::Count => 1.0,
        };
        let ticks = axis::price_ticks(pmap.lo, pmap.hi, axis_tick_target(band.h), min_step);
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
        pane_maps.push((band, pmap, format, ticks));
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
        y: y - line_height() / 2.0,
        size: font(),
        color: hsla(p.text),
        align: Align::Left,
        bold: false,
        face: Default::default(),
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
    spread_tags(&mut tags, &geometry, oy);
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
            size: font(),
            color: if label.major {
                hsla(p.text_strong)
            } else {
                hsla(p.text)
            },
            align: Align::Center,
            bold: label.major,
            face: Default::default(),
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
        x: ox + plot_w as f32 + axis_w() / 2.0,
        y: oy + plot_h as f32 + 8.0,
        size: font(),
        color: hsla(p.text),
        align: Align::Center,
        bold: false,
        face: Default::default(),
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

mod axes;
mod drawings;
mod palette;
use axes::{
    axes_background, axis_tick_target, day_breaks, draw_axis_tags, draw_price_lines, spread_tags,
};
pub use axes::{countdown, dash_pattern};
pub use drawings::ellipse_points;
use drawings::{draw_drawings, draw_script_drawings};
pub use palette::Palette;

#[cfg(test)]
#[path = "../../../tests/unit/domain/chart/scene.rs"]
mod tests;
