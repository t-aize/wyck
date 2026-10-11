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

pub mod axes;
pub mod cmd;
pub mod color;
pub mod drawings;
mod footprint;
pub mod geometry;
pub mod palette;
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
use crate::domain::drawings::model::Drawing;
use crate::domain::drawings::model::style::Dash;
use crate::domain::indicators::spec::{Placement, ValueFormat};

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
    pub palette: palette::Palette,
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
        cmds.push(axes::axes_background(frame, &geometry));
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
    let main_ticks = map.ticks(axes::axis_tick_target(main.h), unit);

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
        for index in axes::day_breaks(frame, first, last) {
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
    drawings::draw_script_drawings(&cx, &mut plot);
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
            dash: axes::dash_pattern(mark.dash, mark.width),
        });
    }
    if let Some(view) = &frame.drawings {
        drawings::draw_drawings(&cx, view, &mut plot);
    }
    axes::draw_price_lines(&cx, &mut plot);
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
        let ticks = axis::price_ticks(pmap.lo, pmap.hi, axes::axis_tick_target(band.h), min_step);
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

    cmds.push(axes::axes_background(frame, &geometry));
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
    axes::draw_axis_tags(
        &cx,
        pointer,
        &geometry,
        &pane_maps,
        &mut tags,
        &mut pointer_tags,
    );
    axes::spread_tags(&mut tags, &geometry, oy);
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

#[cfg(test)]
mod tests {
    use super::axes::*;

    use super::color::rgb;

    use super::palette::*;
    use crate::domain::market::{Bar, Period, Quote, Tick};

    use super::axes::previous_close;
    use super::cmd::{count, rect_widths, texts};
    use super::*;
    use crate::domain::chart::flow::Flow;
    use crate::domain::chart::options::{ChartColors, CrosshairStyle, ScaleMargin};
    use crate::domain::chart::zone::Zone;
    use crate::domain::indicators::compute::{ScriptDrawing, StudyOutput};
    use crate::domain::indicators::config::StudyConfig;
    use crate::domain::indicators::kind::StudyKind;

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

    struct Fixture {
        raw: Series,
        display: Display,
        settings: ChartSettings,
        view: View,
        flow: Flow,
    }

    impl Fixture {
        fn new(raw: Series, settings: ChartSettings, view: View) -> Self {
            let display = Display::build(&raw, &settings, 1);
            Self {
                raw,
                display,
                settings,
                view,
                flow: Flow::default(),
            }
        }

        fn frame(&self) -> Frame<'_> {
            Frame {
                raw: &self.raw,
                display: &self.display,
                settings: &self.settings,
                view: &self.view,
                timeframe: Timeframe::DEFAULT,
                digits: 5,
                pip_position: Some(4),
                origin: (0.0, 0.0),
                w: 1_000.0,
                h: 600.0,
                scale: 1.0,
                hover: Some((300.0, 200.0)),
                remote: None,
                ask: Some(100_020),
                now_ms: 1_767_571_200_000,
                palette: Palette::new(),
                drawings: None,
                marks: &[],
                flow: Some(&self.flow),
                watermark: None,
            }
        }
    }

    fn kind(kind: ChartKind) -> ChartSettings {
        ChartSettings {
            kind,
            ..ChartSettings::default()
        }
    }

    #[test]
    fn a_screenful_of_candles_draws() {
        let f = Fixture::new(
            Series::Bars(bars(500)),
            kind(ChartKind::Candles),
            View::new(8.0),
        );
        assert!(count(&build(&f.frame())) > 100);
    }

    #[test]
    fn script_drawings_follow_indicator_visibility_and_reach_the_scene() {
        let mut settings = kind(ChartKind::Candles);
        settings.studies.push(StudyConfig::for_script("generated"));
        let mut fixture = Fixture::new(Series::Bars(bars(20)), settings, View::new(8.0));
        let mut drawing = Drawing::new(
            0,
            crate::domain::drawings::model::tool::Tool::Text,
            vec![crate::domain::drawings::model::Point {
                t: 1_767_571_200_000,
                p: 100_020.0,
            }],
        );
        drawing.text = "SCRIPT_MARKER_UNIQUE".into();
        fixture.display.studies[0] = Some(StudyOutput {
            drawings: vec![ScriptDrawing {
                key: "marker".into(),
                drawing,
            }],
            ..StudyOutput::default()
        });
        let cmds = build(&fixture.frame());
        assert!(
            texts(&cmds)
                .iter()
                .any(|text| text.contains("SCRIPT_MARKER_UNIQUE"))
        );
        let main = cmds
            .iter()
            .find_map(|cmd| match cmd {
                Cmd::Clip { inner, .. }
                    if inner.iter().any(
                        |cmd| matches!(cmd, Cmd::Text { text, .. } if text == "SCRIPT_MARKER_UNIQUE"),
                    ) =>
                {
                    Some(inner)
                }
                _ => None,
            })
            .unwrap();
        let label = main
            .iter()
            .position(|cmd| matches!(cmd, Cmd::Text { text, .. } if text == "SCRIPT_MARKER_UNIQUE"))
            .unwrap();
        let candle = main.iter().rposition(|cmd| matches!(cmd, Cmd::Rect { fill, .. } if *fill == hsla(fixture.frame().palette.up) || *fill == hsla(fixture.frame().palette.down))).unwrap();
        assert!(
            label < candle,
            "the price series must cover script drawings"
        );
        fixture.settings.studies[0].visible = false;
        assert!(
            !texts(&build(&fixture.frame()))
                .iter()
                .any(|text| text.contains("SCRIPT_MARKER_UNIQUE"))
        );
    }

    #[test]
    fn the_work_does_not_grow_with_the_data() {
        // A hundred thousand bars squeezed into one screen make about as many commands as a
        // thousand: they are folded into pixel columns.
        for chart_kind in ChartKind::ALL {
            let mut view = View::new(0.2);
            view.offset = View::home_offset();
            let small = Fixture::new(Series::Bars(bars(1_000)), kind(chart_kind), view);
            let huge = Fixture::new(Series::Bars(bars(100_000)), kind(chart_kind), view);
            let a = count(&build(&small.frame()));
            let b = count(&build(&huge.frame()));
            assert!(b < 8_000, "{chart_kind:?}: {b} commands");
            assert!(b < a * 8 + 800, "{chart_kind:?}: {a} against {b}");
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
        let mut view = View::new(0.2);
        view.offset = View::home_offset();
        let f = Fixture::new(Series::Ticks(ticks), kind(ChartKind::Line), view);
        let cmds = build(&f.frame());
        assert!(count(&cmds) < 1_000, "{}", count(&cmds));
    }

    #[test]
    fn an_empty_series_only_draws_the_axes() {
        let f = Fixture::new(
            Series::Bars(Vec::new()),
            kind(ChartKind::Candles),
            View::new(8.0),
        );
        assert_eq!(build(&f.frame()).len(), 1);
    }

    #[test]
    fn the_price_map_round_trips_and_keeps_the_data_on_screen() {
        let f = Fixture::new(
            Series::Bars(bars(200)),
            kind(ChartKind::Candles),
            View::new(8.0),
        );
        let g = geometry(&f.settings, 1_000.0, 600.0);
        let map = main_map(&f.raw, &f.display, &f.settings, &f.view, &g, 5).unwrap();
        for price in [100_000.0, 100_250.0] {
            assert!((map.price(map.y(price)) - price).abs() < 1e-6);
        }
        assert!(map.y(100_500.0) >= 0.0 && map.y(99_980.0) <= g.plot_h());
    }

    #[test]
    fn a_manual_scale_is_honoured() {
        let mut view = View::new(8.0);
        view.price = PriceScale::Manual {
            lo: 0.0,
            hi: 1_000_000.0,
        };
        let f = Fixture::new(Series::Bars(bars(50)), kind(ChartKind::Candles), view);
        let g = geometry(&f.settings, 1_000.0, 600.0);
        let map = main_map(&f.raw, &f.display, &f.settings, &f.view, &g, 5).unwrap();
        assert_eq!((map.lo, map.hi), (0.0, 1_000_000.0));
    }

    #[test]
    fn another_charts_pointer_draws_a_crosshair_when_its_time_is_on_screen() {
        let f = Fixture::new(
            Series::Bars(bars(200)),
            kind(ChartKind::Candles),
            View::new(8.0),
        );
        let mut frame = f.frame();
        frame.hover = None;
        let bare = count(&build(&frame));
        let time = f.raw.time_at(190).unwrap();
        frame.remote = Some((time, 100_030.0));
        let with_pointer = count(&build(&frame));
        assert!(with_pointer > bare, "{with_pointer} against {bare}");
        frame.remote = Some((f.raw.time_at(0).unwrap() - 86_400_000, 100_030.0));
        assert_eq!(count(&build(&frame)), bare);
    }

    #[test]
    fn the_local_pointer_wins_over_a_remote_one() {
        let f = Fixture::new(
            Series::Bars(bars(200)),
            kind(ChartKind::Candles),
            View::new(8.0),
        );
        let mut frame = f.frame();
        let local_only = count(&build(&frame));
        frame.remote = Some((f.raw.time_at(150).unwrap(), 100_030.0));
        assert_eq!(count(&build(&frame)), local_only);
    }

    #[test]
    fn quotes_with_fewer_decimals_have_a_bigger_unit() {
        assert_eq!(quote_unit(5), 1.0);
        assert_eq!(quote_unit(3), 100.0);
        assert_eq!(quote_unit(2), 1_000.0);
    }

    #[test]
    fn every_indicator_draws_on_the_prices_or_in_its_pane() {
        let mut settings = ChartSettings::default();
        for kind in StudyKind::ALL {
            settings.studies.push(StudyConfig::new(kind));
        }
        let f = Fixture::new(Series::Bars(bars(600)), settings, View::new(8.0));
        let g = geometry(&f.settings, 1_000.0, 600.0);
        assert_eq!(g.bands.len(), 1 + f.settings.panes().len());
        let bare = Fixture::new(
            Series::Bars(bars(600)),
            ChartSettings::default(),
            View::new(8.0),
        );
        let with = count(&build(&f.frame()));
        let without = count(&build(&bare.frame()));
        assert!(with > without + 500, "{with} against {without}");
    }

    #[test]
    fn a_percent_scale_labels_the_axis_in_percent() {
        let settings = ChartSettings {
            scale: ScaleMode::Percent,
            ..ChartSettings::default()
        };
        let f = Fixture::new(Series::Bars(bars(300)), settings, View::new(8.0));
        let labels = texts(&build(&f.frame()));
        assert!(
            labels.iter().filter(|t| t.ends_with('%')).count() >= 3,
            "{labels:?}"
        );
    }

    #[test]
    fn the_time_left_in_a_bar_reads_like_a_clock() {
        assert_eq!(countdown(65_000), "01:05");
        assert_eq!(countdown(3_725_000), "1:02:05");
        assert_eq!(countdown(2 * 86_400_000 + 3_600_000), "2d 01:00");
        assert_eq!(countdown(-5), "00:00");
    }

    #[test]
    fn marks_draw_a_line_and_a_tag() {
        let f = Fixture::new(
            Series::Bars(bars(100)),
            kind(ChartKind::Candles),
            View::new(8.0),
        );
        let marks = [PriceMark {
            price: 100_200.0,
            color: 0x00ff00,
            dash: Dash::Dashed,
            width: 1.0,
            from_x: None,
            axis_tag: true,
        }];
        let mut frame = f.frame();
        let before = texts(&build(&frame)).len();
        frame.marks = &marks;
        let after = texts(&build(&frame));
        // The tag is added; a label of the scale under it gives way.
        assert!(after.len() >= before, "{before} then {after:?}");
        assert!(after.contains(&"1.00200".to_owned()), "{after:?}");
    }

    #[test]
    fn tags_on_the_axis_are_moved_apart_the_later_keeping_their_place() {
        let tag = |y: f32| Cmd::Tag {
            text: String::new(),
            x: 0.0,
            y,
            height: 18.0,
            pad: 0.0,
            bg: hsla(rgb(0)),
            fg: hsla(rgb(0)),
            align: Align::Left,
            fixed_width: Some(10.0),
            within: None,
            size: font(),
            bold: false,
            face: Default::default(),
        };
        let mut tags = vec![tag(100.0), tag(105.0), tag(104.0)];
        spread_tags(&mut tags, &Geometry::single(1_000.0, 528.0), 0.0);
        let ys: Vec<f32> = tags
            .iter()
            .map(|t| match t {
                Cmd::Tag { y, .. } => *y,
                _ => 0.0,
            })
            .collect();
        assert_eq!(ys[2], 104.0, "the last one stays");
        let mut sorted = ys.clone();
        sorted.sort_by(f32::total_cmp);
        assert!(sorted.windows(2).all(|w| w[1] - w[0] >= 18.0), "{ys:?}");
    }

    #[test]
    fn a_compact_atr_pane_keeps_its_last_value_on_the_axis() {
        let mut settings = ChartSettings::default();
        let mut atr = StudyConfig::new(StudyKind::Atr);
        atr.weight = 0.1;
        settings.studies.push(atr);
        let f = Fixture::new(Series::Bars(bars(300)), settings, View::new(8.0));
        let g = geometry(&f.settings, 1_000.0, 600.0);
        assert_eq!(g.bands[1].h, geometry::MIN_BAND);
        let value = f.display.studies[0].as_ref().unwrap().plots[0]
            .values
            .iter()
            .rev()
            .find(|v| v.is_finite())
            .copied()
            .unwrap();
        let label = price::format_value(value, ValueFormat::Price, 5);
        let cmds = build(&f.frame());
        assert!(
            cmds.iter().any(|cmd| match cmd {
                Cmd::Tag {
                    text,
                    y,
                    height,
                    fixed_width: Some(_),
                    ..
                } => {
                    text == &label
                        && g.bands[1].top as f32 <= *y
                        && *y + *height <= g.bands[1].bottom() as f32
                }
                _ => false,
            }),
            "missing ATR tag: {label}"
        );
    }

    #[test]
    fn price_axis_uses_the_available_vertical_space() {
        let target = axis_tick_target(300.0);
        let ticks = axis::price_ticks(30_580.0, 30_660.0, target, 1.0);
        assert!(ticks.len() >= 8, "{ticks:?}");
    }

    /// The flow of some bars: the price walks from the low to the high (buyers) and back (sellers).
    fn flow_of(bars: &[Bar]) -> Flow {
        let times: Vec<i64> = bars.iter().map(|b| b.time_ms).collect();
        let mut quotes = Vec::new();
        for bar in bars {
            let mut time = bar.time_ms;
            let up: Vec<i64> = (bar.low..=bar.high).step_by(5).collect();
            for price in up.iter().chain(up.iter().rev()) {
                time += 10;
                quotes.push(Quote {
                    time_ms: time,
                    bid: Some(*price),
                    ask: Some(*price + 2),
                });
            }
        }
        let mut flow = Flow::default();
        flow.ingest(&times, Some(60_000), &quotes, true);
        flow.cover_from(times[0]);
        flow
    }

    fn footprint_fixture(n: usize, bar_px: f64) -> Fixture {
        let data = bars(n);
        let mut f = Fixture::new(
            Series::Bars(data.clone()),
            kind(ChartKind::Footprint),
            View::new(bar_px),
        );
        f.flow = flow_of(&data);
        f
    }

    #[test]
    fn a_footprint_shows_numbers_and_the_summary_of_each_bar() {
        let f = footprint_fixture(60, 110.0);
        let labels = texts(&build(&f.frame()));
        assert!(labels.contains(&"Delta".to_owned()), "{labels:?}");
        assert!(labels.contains(&"Vol".to_owned()));
        let numbers = labels
            .iter()
            .filter(|t| t.parse::<u32>().is_ok_and(|n| n > 0))
            .count();
        assert!(numbers > 20, "{numbers} numbers in {labels:?}");
    }

    #[test]
    fn the_cells_can_show_the_delta_or_the_volume() {
        let mut f = footprint_fixture(60, 110.0);
        f.settings.footprint.mode = crate::domain::chart::footprint::CellMode::Delta;
        let labels = texts(&build(&f.frame()));
        assert!(
            labels
                .iter()
                .any(|t| t.starts_with('+') || t.starts_with('-')),
            "{labels:?}"
        );
        f.settings.footprint.summary = false;
        let labels = texts(&build(&f.frame()));
        assert!(!labels.contains(&"Delta".to_owned()), "the summary is off");
    }

    #[test]
    fn the_numbers_can_be_turned_off_and_the_cells_still_draw() {
        let mut f = footprint_fixture(60, 110.0);
        let with = count(&build(&f.frame()));
        f.settings.footprint.numbers = false;
        let cmds = build(&f.frame());
        let numbers = texts(&cmds)
            .iter()
            .filter(|t| t.parse::<u32>().is_ok_and(|n| n > 0 && n < 1_000))
            .count();
        assert!(numbers < 10, "{numbers}");
        assert!(count(&cmds) > 50 && count(&cmds) < with);
    }

    #[test]
    fn narrow_bars_are_plain_candles() {
        let f = footprint_fixture(300, 8.0);
        let cmds = build(&f.frame());
        assert!(!texts(&cmds).contains(&"Delta".to_owned()));
        assert!(count(&cmds) > 100);
    }

    #[test]
    fn a_timeframe_a_footprint_cannot_be_built_for_gives_candles() {
        let f = footprint_fixture(60, 110.0);
        let mut frame = f.frame();
        frame.timeframe = Timeframe::Bars(Period::W1);
        assert!(!texts(&build(&frame)).contains(&"Delta".to_owned()));
    }

    #[test]
    fn bars_without_flow_are_still_drawn() {
        let mut f = footprint_fixture(60, 110.0);
        f.flow = Flow::default();
        let cmds = build(&f.frame());
        // A wick and a body for each of the bars on screen.
        assert!(count(&cmds) > 16, "{}", count(&cmds));
    }

    #[test]
    fn a_footprint_frame_stays_bounded_at_any_zoom() {
        for bar_px in [24.0, 60.0, 110.0, 260.0] {
            let f = footprint_fixture(500, bar_px);
            let n = count(&build(&f.frame()));
            assert!(n < 12_000, "{bar_px}px: {n} commands");
        }
    }

    #[test]
    fn the_prices_leave_room_for_the_summary_under_them() {
        let f = footprint_fixture(60, 110.0);
        let g = geometry(&f.settings, 1_000.0, 600.0);
        let with = main_map(&f.raw, &f.display, &f.settings, &f.view, &g, 5).unwrap();
        let mut off = f.settings.clone();
        off.footprint.summary = false;
        let without = main_map(&f.raw, &f.display, &off, &f.view, &g, 5).unwrap();
        assert!(with.bottom < without.bottom - 20.0);
    }

    fn drawn(settings: ChartSettings, edit: impl FnOnce(&mut Frame<'_>)) -> usize {
        let f = Fixture::new(Series::Bars(bars(300)), settings, View::new(8.0));
        let mut frame = f.frame();
        edit(&mut frame);
        count(&build(&frame))
    }

    #[test]
    fn the_grid_options_take_lines_out_one_kind_at_a_time() {
        let with = |grid, horizontal, vertical| {
            drawn(
                ChartSettings {
                    grid,
                    grid_horizontal: horizontal,
                    grid_vertical: vertical,
                    ..ChartSettings::default()
                },
                |_| {},
            )
        };
        let both = with(true, true, true);
        let horizontal_only = with(true, true, false);
        let vertical_only = with(true, false, true);
        let none = with(false, true, true);
        assert!(both > horizontal_only && both > vertical_only, "{both}");
        assert!(horizontal_only > none && vertical_only > none);
        assert_eq!(with(true, false, false), none);
    }

    #[test]
    fn a_crosshair_that_is_off_draws_neither_lines_nor_tags() {
        let with = |crosshair| {
            drawn(
                ChartSettings {
                    crosshair,
                    ..ChartSettings::default()
                },
                |_| {},
            )
        };
        let dashed = with(CrosshairStyle::Dashed);
        assert_eq!(with(CrosshairStyle::Solid), dashed);
        let off = with(CrosshairStyle::Off);
        // Two lines and two tags. A tag may also cover one scale label.
        assert!((3..=4).contains(&(dashed - off)));
        assert_eq!(CrosshairStyle::Solid.dash(), None);
        assert!(CrosshairStyle::Dotted.dash().is_some());
    }

    #[test]
    fn the_price_lines_and_tags_can_each_be_hidden() {
        let f = Fixture::new(
            Series::Bars(bars(300)),
            ChartSettings::default(),
            View::new(8.0),
        );
        let mut frame = f.frame();
        // The bar that is current at the moment of the frame, so the countdown shows.
        frame.now_ms = f.raw.last_time().unwrap() + 1_000;
        let all = count(&build(&frame));
        let hide = |change: fn(&mut crate::domain::chart::options::PriceLines)| {
            let mut settings = ChartSettings::default();
            change(&mut settings.price_lines);
            let f = Fixture::new(Series::Bars(bars(300)), settings, View::new(8.0));
            let mut frame = f.frame();
            frame.now_ms = f.raw.last_time().unwrap() + 1_000;
            count(&build(&frame))
        };
        assert_eq!(all - hide(|p| p.last_line = false), 1);
        assert_eq!(all - hide(|p| p.last_tag = false), 1);
        assert_eq!(all - hide(|p| p.countdown = false), 1);
        assert_eq!(all - hide(|p| p.ask_line = false), 1);
    }

    #[test]
    fn the_watermark_is_written_behind_the_prices() {
        let f = Fixture::new(
            Series::Bars(bars(50)),
            ChartSettings::default(),
            View::new(8.0),
        );
        let mut frame = f.frame();
        assert!(!texts(&build(&frame)).iter().any(|t| t == "EURUSD, M5"));
        frame.watermark = Some("EURUSD, M5".to_owned());
        assert!(texts(&build(&frame)).iter().any(|t| t == "EURUSD, M5"));
    }

    #[test]
    fn day_separators_only_show_when_asked_and_on_intraday_bars() {
        let hourly: Vec<Bar> = (0..120)
            .map(|i| Bar {
                time_ms: 1_767_571_200_000 + i * 3_600_000,
                open: 100_000,
                high: 100_030,
                low: 99_980,
                close: 100_010,
                volume: 1,
            })
            .collect();
        let build_with = |day_breaks: bool| {
            let mut settings = ChartSettings::default();
            settings.price_lines.day_breaks = day_breaks;
            let f = Fixture::new(Series::Bars(hourly.clone()), settings, View::new(8.0));
            let mut frame = f.frame();
            frame.timeframe = Timeframe::Bars(Period::H1);
            count(&build(&frame))
        };
        // 120 hourly bars are five days: at least four changes of the day are on screen.
        assert!(build_with(true) >= build_with(false) + 4);
        let f = Fixture::new(
            Series::Bars(hourly),
            ChartSettings::default(),
            View::new(8.0),
        );
        let mut frame = f.frame();
        frame.timeframe = Timeframe::Bars(Period::D1);
        assert!(day_breaks(&frame, 0, 120).is_empty());
    }

    #[test]
    fn the_previous_close_is_the_last_close_of_the_day_before() {
        let day = 86_400_000;
        let bar = |time_ms, close| Bar {
            time_ms,
            open: close,
            high: close,
            low: close,
            close,
            volume: 1,
        };
        let series = Series::Bars(vec![
            bar(day, 10),
            bar(day + 3_600_000, 11),
            bar(2 * day, 20),
            bar(2 * day + 3_600_000, 21),
        ]);
        assert_eq!(previous_close(&series, Zone::Utc), Some(11));
        assert_eq!(
            previous_close(&Series::Bars(vec![bar(day, 10)]), Zone::Utc),
            None
        );
        assert_eq!(previous_close(&Series::Ticks(Vec::new()), Zone::Utc), None);
    }

    #[test]
    fn a_chart_overrides_only_the_colors_it_sets() {
        let theme = Palette::new();
        let colors = ChartColors {
            up: Some(0x00ff00),
            background: Some(0x101010),
            ..ChartColors::default()
        };
        let own = Palette::for_chart(&colors);
        assert_eq!(own.up, rgb(0x00ff00));
        assert_eq!(own.bg, rgb(0x101010));
        assert_eq!(own.down, theme.down);
        let grid = Palette::for_chart(&ChartColors {
            grid: Some(0xffffff),
            ..ChartColors::default()
        });
        assert!(grid.grid.a < 0.5, "a chosen grid color is drawn faint");
        assert_eq!(own.line, theme.line);
    }

    #[test]
    fn the_margins_widen_the_automatic_price_scale() {
        let range = |margin| {
            let settings = ChartSettings {
                margin,
                ..ChartSettings::default()
            };
            let f = Fixture::new(Series::Bars(bars(300)), settings, View::new(8.0));
            let geometry = geometry(&f.settings, 1_000.0, 600.0);
            let map = main_map(&f.raw, &f.display, &f.settings, &f.view, &geometry, 5).unwrap();
            map.hi - map.lo
        };
        assert!(range(ScaleMargin::Tight) < range(ScaleMargin::Normal));
        assert!(range(ScaleMargin::Normal) < range(ScaleMargin::Loose));
    }

    #[test]
    fn trading_lines_that_are_hidden_are_not_marks() {
        let settings = super::super::options::TradingLines {
            alerts: false,
            ..super::super::options::TradingLines::default()
        };
        assert!(!settings.shows(super::super::lines::LineId::Alert(3)));
        assert!(settings.shows(super::super::lines::LineId::Order(3)));
    }

    #[test]
    fn a_volume_candle_is_as_wide_as_its_volume() {
        let mut data = bars(40);
        // Two neighbours, one quiet and one very busy: same room, different widths.
        data[20].volume = 1;
        data[21].volume = 100_000;
        let mut settings = kind(ChartKind::VolumeCandles);
        settings.volume_candles.min_width = 0.2;
        settings.volume_candles.max_width = 1.0;
        settings.volume_candles.reference = crate::domain::chart::volume::WidthReference::Max;
        settings.volume_candles.scale = crate::domain::chart::volume::WidthScale::Linear;
        let f = Fixture::new(Series::Bars(data), settings, View::new(20.0));
        let widths = rect_widths(&build(&f.frame()));
        let (mut thin, mut wide) = (f32::MAX, 0.0_f32);
        for w in widths.iter().copied().filter(|w| *w >= 3.0 && *w <= 20.5) {
            thin = thin.min(w);
            wide = wide.max(w);
        }
        assert!(wide >= 19.0, "the busiest fills its room: {wide}");
        assert!(thin <= 6.0, "the quietest is thin: {thin}");
    }

    #[test]
    fn volume_candles_can_write_the_volume_over_wide_candles() {
        let mut settings = kind(ChartKind::VolumeCandles);
        settings.volume_candles.labels = true;
        let f = Fixture::new(Series::Bars(bars(30)), settings, View::new(40.0));
        let text = texts(&build(&f.frame()));
        assert!(
            text.iter()
                .any(|t| t.parse::<i64>().is_ok_and(|v| (1..=7).contains(&v))),
            "{text:?}"
        );
    }

    #[test]
    fn volume_bars_draw_and_make_a_different_number_of_bars_than_the_source() {
        let mut settings = kind(ChartKind::VolumeBars);
        settings.volume_bars.size = crate::domain::chart::volume::VolumeSize::Fixed { volume: 20 };
        let f = Fixture::new(Series::Bars(bars(200)), settings, View::new(8.0));
        assert!(f.display.is_derived());
        assert_eq!(f.display.volume_size, 20);
        assert_ne!(f.display.shown(&f.raw).len(), 200);
        assert!(count(&build(&f.frame())) > 50);
    }

    #[test]
    fn renko_wicks_add_a_line_to_each_brick() {
        let source = bars(400);
        let plain = kind(ChartKind::Renko);
        let mut wicked = kind(ChartKind::Renko);
        wicked.transform.renko_wicks = true;
        wicked.transform.renko_box =
            crate::domain::chart::transform::BoxSize::Fixed { price: 0.0002 };
        let mut plain = plain;
        plain.transform.renko_box = wicked.transform.renko_box;
        let a = Fixture::new(Series::Bars(source.clone()), plain, View::new(10.0));
        let b = Fixture::new(Series::Bars(source), wicked, View::new(10.0));
        assert!(count(&build(&b.frame())) > count(&build(&a.frame())));
    }

    #[test]
    fn bricks_can_drop_their_border_and_change_their_width() {
        let mut narrow = kind(ChartKind::Renko);
        narrow.transform.brick_width = 0.4;
        narrow.transform.brick_border = false;
        narrow.transform.renko_box =
            crate::domain::chart::transform::BoxSize::Fixed { price: 0.0002 };
        let mut wide = narrow.clone();
        wide.transform.brick_width = 1.0;
        let a = Fixture::new(Series::Bars(bars(300)), narrow, View::new(20.0));
        let b = Fixture::new(Series::Bars(bars(300)), wide, View::new(20.0));
        let widest = |f: &Fixture| {
            rect_widths(&build(&f.frame()))
                .into_iter()
                .filter(|w| *w <= 21.0)
                .fold(0.0_f32, f32::max)
        };
        assert!(
            widest(&a) < widest(&b),
            "{} against {}",
            widest(&a),
            widest(&b)
        );
    }

    #[test]
    fn kagi_and_point_figure_take_their_own_colors_and_follow_the_rising_ones_otherwise() {
        let plain = Palette::for_chart(&ChartColors::default());
        assert_eq!(plain.kagi_yang, plain.up);
        assert_eq!(plain.kagi_yin, plain.down);
        assert_eq!(plain.pnf_up, plain.up);
        let colors = ChartColors {
            up: Some(0x112233),
            kagi_yin: Some(0x445566),
            pnf_up: Some(0x778899),
            ..ChartColors::default()
        };
        let p = Palette::for_chart(&colors);
        assert_eq!(p.kagi_yang, rgb(0x112233), "follows the rising color");
        assert_eq!(p.kagi_yin, rgb(0x445566));
        assert_eq!(p.pnf_up, rgb(0x778899));
        assert_eq!(p.pnf_down, p.down);
    }

    #[test]
    fn every_price_based_type_draws_from_the_whole_path_by_default() {
        for chart_kind in [
            ChartKind::Renko,
            ChartKind::LineBreak,
            ChartKind::Kagi,
            ChartKind::PointFigure,
            ChartKind::Range,
        ] {
            let f = Fixture::new(Series::Bars(bars(300)), kind(chart_kind), View::new(8.0));
            assert!(f.display.is_derived(), "{chart_kind:?}");
            assert!(count(&build(&f.frame())) > 20, "{chart_kind:?}");
        }
    }

    /// Three days of five minute bars, so a session has a dozen letters or so per day.
    fn tpo_bars() -> Vec<Bar> {
        (0..864)
            .map(|i| {
                let wave = ((i as f64 * 0.05).sin() * 300.0) as i64;
                let base = 100_000 + wave;
                Bar {
                    time_ms: 1_767_571_200_000 + i as i64 * 300_000,
                    open: base,
                    high: base + 60,
                    low: base - 60,
                    close: base + 10,
                    volume: 5,
                }
            })
            .collect()
    }

    fn tpo_fixture(edit: impl FnOnce(&mut ChartSettings), bar_px: f64) -> Fixture {
        let mut settings = kind(ChartKind::Tpo);
        settings.zone = Zone::Utc;
        settings.tpo.row_units = 20;
        edit(&mut settings);
        // A little air after the last profile, not the six points a narrow view keeps.
        let mut view = View::new(bar_px);
        view.offset = 1.0;
        Fixture::new(Series::Bars(tpo_bars()), settings, view)
    }

    #[test]
    fn a_tpo_chart_makes_one_element_for_each_session() {
        let f = tpo_fixture(|_| {}, 200.0);
        assert_eq!(f.display.tpo.len(), 3);
        assert_eq!(f.display.shown(&f.raw).len(), 3);
        assert!(f.display.is_derived());
    }

    #[test]
    fn profiles_are_written_in_letters_with_their_levels() {
        let f = tpo_fixture(
            |s| s.tpo.display = crate::domain::chart::tpo::TpoDisplay::Letters,
            200.0,
        );
        let text = texts(&build(&f.frame()));
        assert!(text.iter().any(|t| t == "A"), "{text:?}");
        assert!(text.iter().any(|t| t == "B"), "{text:?}");
        assert!(text.iter().any(|t| t.starts_with("POC ")), "{text:?}");
        assert!(text.iter().any(|t| t.starts_with("VAH ")), "{text:?}");
        assert!(text.iter().any(|t| t.starts_with("VAL ")), "{text:?}");
    }

    #[test]
    fn blocks_have_no_letters_and_the_levels_can_be_turned_off() {
        let f = tpo_fixture(
            |s| {
                s.tpo.display = crate::domain::chart::tpo::TpoDisplay::Blocks;
                s.tpo.poc = false;
                s.tpo.value_area = false;
            },
            200.0,
        );
        let text = texts(&build(&f.frame()));
        assert!(!text.iter().any(|t| t == "A" || t == "B"), "{text:?}");
        assert!(
            !text
                .iter()
                .any(|t| t.starts_with("POC ") || t.starts_with("VAH "))
        );
        let plain = tpo_fixture(|s| s.tpo.labels = false, 200.0);
        let text = texts(&build(&plain.frame()));
        assert!(!text.iter().any(|t| t.starts_with("POC ")), "{text:?}");
    }

    #[test]
    fn the_profile_extras_add_what_they_draw() {
        let bare = tpo_fixture(
            |s| {
                s.tpo.value_area = false;
                s.tpo.poc_line = false;
                s.tpo.initial_balance = false;
                s.tpo.single_prints = false;
                s.tpo.poor_extremes = false;
                s.tpo.open_close = false;
                s.tpo.labels = false;
            },
            200.0,
        );
        let full = tpo_fixture(|s| s.tpo.midpoint = true, 200.0);
        assert!(count(&build(&full.frame())) > count(&build(&bare.frame())));
    }

    #[test]
    fn zoomed_out_the_sessions_are_bars_and_the_work_stays_bounded() {
        let f = tpo_fixture(|_| {}, 4.0);
        let text = texts(&build(&f.frame()));
        assert!(!text.iter().any(|t| t == "A"), "{text:?}");
        let mut view = View::new(0.2);
        view.offset = View::home_offset();
        let many: Vec<Bar> = (0..60_000)
            .map(|i| {
                let base = 100_000 + ((i as f64 * 0.01).sin() * 900.0) as i64;
                Bar {
                    time_ms: 1_767_571_200_000 + i as i64 * 3_600_000,
                    open: base,
                    high: base + 50,
                    low: base - 50,
                    close: base + 5,
                    volume: 3,
                }
            })
            .collect();
        let mut settings = kind(ChartKind::Tpo);
        settings.zone = Zone::Utc;
        let huge = Fixture::new(Series::Bars(many), settings, view);
        assert!(count(&build(&huge.frame())) < 8_000);
    }

    #[test]
    fn every_color_mode_draws() {
        use crate::domain::chart::tpo::TpoColor;
        for mode in TpoColor::ALL {
            let f = tpo_fixture(|s| s.tpo.color = mode, 200.0);
            assert!(count(&build(&f.frame())) > 100, "{mode:?}");
        }
    }

    #[test]
    fn the_automatic_display_keeps_letters_for_columns_wide_enough_to_read() {
        // With few periods in a session the columns are wide, and the letters show by themselves.
        let f = tpo_fixture(|s| s.tpo.period_minutes = 240, 200.0);
        let text = texts(&build(&f.frame()));
        assert!(text.iter().any(|t| t == "A"), "{text:?}");
        let crowded = tpo_fixture(|s| s.tpo.period_minutes = 5, 200.0);
        let text = texts(&build(&crowded.frame()));
        assert!(!text.iter().any(|t| t == "A"), "{text:?}");
    }

    /// How many strokes of `points` or more points a frame has (the ring of an O has twenty one).
    fn long_strokes(cmds: &[Cmd], points: usize) -> usize {
        cmds.iter()
            .map(|c| match c {
                Cmd::Stroke { points: p, .. } if p.len() >= points => 1,
                Cmd::Clip { inner, .. } => long_strokes(inner, points),
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn point_and_figure_draws_its_x_and_o_on_a_real_price_scale() {
        // The prices are around 100000 raw units, far from the zero the boxes used to be measured
        // from: the boxes then had no height and every column fell back to a bar.
        let mut settings = kind(ChartKind::PointFigure);
        settings.transform.pnf_box =
            crate::domain::chart::transform::BoxSize::Fixed { price: 0.0015 };
        let mut view = View::new(22.0);
        view.offset = 1.0;
        let f = Fixture::new(Series::Bars(bars(500)), settings, view);
        let mut frame = f.frame();
        frame.hover = None;
        assert!(
            long_strokes(&build(&frame), 10) > 0,
            "no O drawn: the columns fell back to bars"
        );
    }
}
