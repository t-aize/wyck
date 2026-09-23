//! Drawing the indicators: their lines, histograms, dots and shaded areas, on the prices or in a
//! pane of their own, the volume profile of the bars on screen, and their values on the axis.

use super::super::data::Series;
use super::super::study::profile::{self, Slice};
use super::super::study::{
    DOWN_COLOR, FillOut, PlotKind, PlotOut, StudyConfig, StudyKind, StudyOutput, UP_COLOR,
    ValueFormat,
};
use super::cmd::{Align, Cmd, P, rgb_alpha};
use super::geometry::{AXIS_W, Band};
use super::price::{self, PriceMap};
use super::{Ctx, Placement};

/// The part of the data a plot draws on screen: data indices `from..to`.
fn data_range(cx: &Ctx<'_>, plot: &PlotOut, first: usize, _last: usize) -> (usize, usize) {
    let n = plot.values.len() as i64;
    // The right edge may be past the data: plots shifted forward draw there.
    let right = cx.f.view.index_at(cx.plot_w, cx.len, cx.plot_w).ceil() as i64 + 2;
    let from = (first as i64 - 1 - plot.offset).clamp(0, n);
    let to = (right - plot.offset).clamp(0, n);
    (from as usize, to as usize)
}

/// The lowest and highest value of a plot among the points on screen.
pub(super) fn visible_range(plot: &PlotOut, first: usize, last: usize) -> Option<(f64, f64)> {
    let n = plot.values.len() as i64;
    let from = (first as i64 - plot.offset).clamp(0, n) as usize;
    let to = (last as i64 - plot.offset).clamp(0, n) as usize;
    let mut range: Option<(f64, f64)> = None;
    for v in plot.values.get(from..to)?.iter().filter(|v| v.is_finite()) {
        range = Some(range.map_or((*v, *v), |(lo, hi)| (lo.min(*v), hi.max(*v))));
    }
    if plot.kind == PlotKind::Histogram {
        range = range.map(|(lo, hi)| (lo.min(0.0), hi.max(0.0)));
    }
    range
}

/// The scale of an indicator pane: its fixed range, or what its plots and levels span on screen.
pub(super) fn pane_map(
    output: &StudyOutput,
    fixed: Option<(f64, f64)>,
    band: Band,
    first: usize,
    last: usize,
) -> PriceMap {
    let (lo, hi) = match fixed {
        Some(range) => range,
        None => {
            let mut range: Option<(f64, f64)> = None;
            for plot in &output.plots {
                if let Some((l, h)) = visible_range(plot, first, last) {
                    range = Some(range.map_or((l, h), |(lo, hi)| (lo.min(l), hi.max(h))));
                }
            }
            let (mut lo, mut hi) = range.unwrap_or((0.0, 1.0));
            for level in &output.levels {
                if *level >= lo - (hi - lo) && *level <= hi + (hi - lo) {
                    lo = lo.min(*level);
                    hi = hi.max(*level);
                }
            }
            if hi - lo < 1e-12 {
                let pad = hi.abs().max(1.0) * 0.01;
                (lo - pad, hi + pad)
            } else {
                let pad = (hi - lo) * 0.08;
                (lo - pad, hi + pad)
            }
        }
    };
    PriceMap::linear(lo, hi, band.top + 6.0, band.bottom() - 6.0)
}

/// The points of a plot on screen, cut where it has no value. Several points per pixel column
/// keep only the last of each column.
fn segments(
    cx: &Ctx<'_>,
    plot: &PlotOut,
    map: &PriceMap,
    first: usize,
    last: usize,
) -> Vec<Vec<P>> {
    let (from, to) = data_range(cx, plot, first, last);
    let mut out: Vec<Vec<P>> = Vec::new();
    let mut current: Vec<P> = Vec::new();
    let mut last_column: Option<i32> = None;
    for i in from..to {
        let v = plot.values[i];
        if !v.is_finite() {
            if current.len() > 1 {
                out.push(std::mem::take(&mut current));
            }
            current.clear();
            last_column = None;
            continue;
        }
        let x = cx.xf((i as i64 + plot.offset) as f64);
        let point = (x, cx.y_on(map, v));
        let column = x.floor() as i32;
        if cx.f.view.bar_px < 1.0 && last_column == Some(column) {
            if let Some(end) = current.last_mut() {
                *end = point;
            }
            continue;
        }
        last_column = Some(column);
        current.push(point);
    }
    if current.len() > 1 {
        out.push(current);
    }
    out
}

fn draw_plot(
    cx: &Ctx<'_>,
    config: &StudyConfig,
    plot: &PlotOut,
    map: &PriceMap,
    first: usize,
    last: usize,
    out: &mut Vec<Cmd>,
) {
    let style = config.plot_style(plot.key);
    if !style.visible {
        return;
    }
    match plot.kind {
        PlotKind::Line => {
            for points in segments(cx, plot, map, first, last) {
                out.push(Cmd::Stroke {
                    points,
                    width: style.width,
                    color: rgb_alpha(style.color, 1.0),
                    dash: None,
                });
            }
        }
        PlotKind::Dots => {
            let (from, to) = data_range(cx, plot, first, last);
            let r = (style.width.max(1.0) + (cx.f.view.bar_px as f32 / 6.0).min(2.0)) / 2.0;
            for i in from..to {
                let v = plot.values[i];
                if v.is_finite() {
                    let (x, y) = (cx.xf((i as i64 + plot.offset) as f64), cx.y_on(map, v));
                    out.push(Cmd::Rect {
                        x: x - r,
                        y: y - r,
                        w: r * 2.0,
                        h: r * 2.0,
                        fill: rgb_alpha(style.color, 1.0),
                        border: None,
                        radius: r,
                    });
                }
            }
        }
        PlotKind::Histogram => {
            let (from, to) = data_range(cx, plot, first, last);
            let zero = cx.y_on(map, 0.0);
            let bar_px = cx.f.view.bar_px as f32;
            let width = if bar_px >= 2.0 {
                (bar_px * 0.6).max(1.0)
            } else {
                1.0
            };
            let mut last_column: Option<i32> = None;
            for i in from..to {
                let v = plot.values[i];
                if !v.is_finite() {
                    continue;
                }
                let x = cx.xf((i as i64 + plot.offset) as f64);
                if bar_px < 1.0 && last_column == Some(x.floor() as i32) {
                    continue;
                }
                last_column = Some(x.floor() as i32);
                let y = cx.y_on(map, v);
                let color = match &plot.up {
                    Some(up) => {
                        let rising = up.get(i).copied().unwrap_or(true);
                        if rising { UP_COLOR } else { DOWN_COLOR }
                    }
                    None => style.color,
                };
                out.push(Cmd::Rect {
                    x: cx.snap(x - width / 2.0),
                    y: y.min(zero),
                    w: width,
                    h: (y - zero).abs().max(1.0 / cx.f.scale),
                    fill: rgb_alpha(color, 0.6),
                    border: None,
                    radius: 0.0,
                });
            }
        }
    }
}

fn draw_fill(
    cx: &Ctx<'_>,
    output: &StudyOutput,
    fill: &FillOut,
    map: &PriceMap,
    first: usize,
    last: usize,
    out: &mut Vec<Cmd>,
) {
    let (Some(a), Some(b)) = (output.plots.get(fill.a), output.plots.get(fill.b)) else {
        return;
    };
    if a.offset != b.offset {
        return;
    }
    let (from, to) = data_range(cx, a, first, last);
    let to = to.min(b.values.len());
    // Runs where both have a value and the same one is on top.
    let mut run: Vec<usize> = Vec::new();
    let flush = |run: &mut Vec<usize>, out: &mut Vec<Cmd>| {
        if run.len() > 1 {
            let above = a.values[run[0]] >= b.values[run[0]];
            let color = match fill.other {
                Some(other) if !above => other,
                _ => fill.color,
            };
            let mut points: Vec<P> = run
                .iter()
                .map(|&i| {
                    (
                        cx.xf((i as i64 + a.offset) as f64),
                        cx.y_on(map, a.values[i]),
                    )
                })
                .collect();
            points.extend(run.iter().rev().map(|&i| {
                (
                    cx.xf((i as i64 + b.offset) as f64),
                    cx.y_on(map, b.values[i]),
                )
            }));
            out.push(Cmd::Fill {
                points,
                color: rgb_alpha(color, fill.alpha),
            });
        }
        run.clear();
    };
    for i in from..to {
        let (va, vb) = (a.values[i], b.values[i]);
        if !(va.is_finite() && vb.is_finite()) {
            flush(&mut run, out);
            continue;
        }
        if fill.other.is_some()
            && let Some(&prev) = run.last()
            && (a.values[prev] >= b.values[prev]) != (va >= vb)
        {
            // Close this run on the crossing bar so the two colors meet.
            run.push(i);
            flush(&mut run, out);
        }
        run.push(i);
    }
    flush(&mut run, out);
}

/// The overlays drawn under the prices: shaded areas and the volume profile.
pub(super) fn overlays_under(cx: &Ctx<'_>, first: usize, last: usize, out: &mut Vec<Cmd>) {
    for (config, output) in cx.f.settings.studies.iter().zip(&cx.f.display.studies) {
        if !config.visible || config.spec().placement != Placement::Overlay {
            continue;
        }
        if config.kind == StudyKind::VolumeProfile {
            volume_profile(cx, config, first, last, out);
            continue;
        }
        if let Some(output) = output {
            for fill in &output.fills {
                draw_fill(cx, output, fill, &cx.map, first, last, out);
            }
        }
    }
}

/// The lines of the overlays, over the prices.
pub(super) fn overlays(cx: &Ctx<'_>, first: usize, last: usize, out: &mut Vec<Cmd>) {
    for (config, output) in cx.f.settings.studies.iter().zip(&cx.f.display.studies) {
        if !config.visible || config.spec().placement != Placement::Overlay {
            continue;
        }
        if let Some(output) = output {
            for plot in &output.plots {
                draw_plot(cx, config, plot, &cx.map, first, last, out);
            }
        }
    }
}

/// Everything of an indicator in its pane.
pub(super) fn pane(
    cx: &Ctx<'_>,
    config: &StudyConfig,
    output: &StudyOutput,
    map: &PriceMap,
    first: usize,
    last: usize,
    out: &mut Vec<Cmd>,
) {
    let (left, right) = (cx.ox, cx.ox + cx.plot_w as f32);
    let accent = config
        .spec()
        .plots
        .first()
        .map_or(0x787b86, |plot| config.plot_style(plot.key).color);
    if let Some((lo, hi)) = output.band {
        let (y0, y1) = (cx.y_on(map, hi), cx.y_on(map, lo));
        out.push(Cmd::Rect {
            x: left,
            y: y0.min(y1),
            w: right - left,
            h: (y1 - y0).abs(),
            fill: rgb_alpha(accent, 0.06),
            border: None,
            radius: 0.0,
        });
    }
    for level in &output.levels {
        let y = cx.snap(cx.y_on(map, *level)) + 0.5;
        out.push(Cmd::Stroke {
            points: vec![(left, y), (right, y)],
            width: 1.0,
            color: rgb_alpha(0x787b86, 0.55),
            dash: Some([4.0, 4.0]),
        });
    }
    for fill in &output.fills {
        draw_fill(cx, output, fill, map, first, last, out);
    }
    for plot in &output.plots {
        draw_plot(cx, config, plot, map, first, last, out);
    }
}

/// The volume profile of the bars on screen, as horizontal rows along one side of the plot.
fn volume_profile(
    cx: &Ctx<'_>,
    config: &StudyConfig,
    first: usize,
    last: usize,
    out: &mut Vec<Cmd>,
) {
    let slices: Vec<Slice> = match cx.series {
        Series::Bars(bars) => bars[first.min(bars.len())..last.min(bars.len())]
            .iter()
            .map(|b| Slice {
                open: b.open as f64,
                high: b.high as f64,
                low: b.low as f64,
                close: b.close as f64,
                volume: b.volume as f64,
            })
            .collect(),
        Series::Ticks(ticks) => {
            let slice = &ticks[first.min(ticks.len())..last.min(ticks.len())];
            slice
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let before = if i > 0 { slice[i - 1].price } else { t.price } as f64;
                    Slice {
                        open: before,
                        high: before.max(t.price as f64),
                        low: before.min(t.price as f64),
                        close: t.price as f64,
                        volume: 1.0,
                    }
                })
                .collect()
        }
    };
    let rows = config.input("rows") as usize;
    let Some(profile) = profile::build(&slices, rows, config.input("value_area") / 100.0) else {
        return;
    };
    let max_w = cx.plot_w as f32 * (config.input("width") as f32 / 100.0);
    let right_side = config.input("side") == 0.0;
    let highlight = config.input("highlight") != 0.0;
    let (up_style, down_style, poc_style) = (
        config.plot_style("up"),
        config.plot_style("down"),
        config.plot_style("poc"),
    );
    let edge = if right_side {
        cx.ox + cx.plot_w as f32
    } else {
        cx.ox
    };
    for (index, row) in profile.rows.iter().enumerate() {
        let total = row.total();
        if total <= 0.0 {
            continue;
        }
        let in_area = (profile.value_area.0..=profile.value_area.1).contains(&index);
        let alpha = if in_area && highlight {
            0.45
        } else if highlight {
            0.2
        } else {
            0.35
        };
        let (y_top, y_bottom) = (cx.y(row.hi), cx.y(row.lo));
        let height = (y_bottom - y_top).abs() - 1.0;
        if height <= 0.0 {
            continue;
        }
        let top = y_top.min(y_bottom) + 0.5;
        let width = (total / profile.max_total) as f32 * max_w;
        let up_w = (row.up / total) as f32 * width;
        let parts = [
            (up_w, up_style.color, up_style.visible),
            (width - up_w, down_style.color, down_style.visible),
        ];
        let mut offset = 0.0;
        for (w, color, visible) in parts {
            if visible && w > 0.0 {
                let x = if right_side {
                    edge - offset - w
                } else {
                    edge + offset
                };
                out.push(Cmd::Rect {
                    x,
                    y: top,
                    w,
                    h: height,
                    fill: rgb_alpha(color, alpha),
                    border: None,
                    radius: 0.0,
                });
            }
            offset += w;
        }
    }
    if poc_style.visible {
        let row = profile.rows[profile.poc];
        let y = cx.snap(cx.y((row.lo + row.hi) / 2.0)) + 0.5;
        out.push(Cmd::Stroke {
            points: vec![(cx.ox, y), (cx.ox + cx.plot_w as f32, y)],
            width: poc_style.width,
            color: rgb_alpha(poc_style.color, 0.9),
            dash: None,
        });
    }
}

/// The last value of each visible plot, as a colored tag on the axis of its band.
pub(super) fn value_tags(
    cx: &Ctx<'_>,
    panes: &[(usize, PriceMap, Band, ValueFormat)],
    out: &mut Vec<Cmd>,
) {
    let axis_x = cx.ox + cx.plot_w as f32 + 1.0;
    let mut tag = |text: String, y: f32, band: &Band, color: u32| {
        let top = cx.oy + band.top as f32 + 9.0;
        let bottom = cx.oy + band.bottom() as f32 - 9.0;
        if y < top || y > bottom {
            return;
        }
        out.push(Cmd::Tag {
            text,
            x: axis_x,
            y: y - 9.0,
            height: 18.0,
            pad: 7.0,
            bg: rgb_alpha(color, 1.0),
            fg: rgb_alpha(0xffffff, 1.0),
            align: Align::Left,
            fixed_width: Some(AXIS_W - 2.0),
            within: None,
        });
    };
    let last_value = |plot: &PlotOut| plot.values.iter().rev().find(|v| v.is_finite()).copied();
    // Overlays, on the prices.
    for (config, output) in cx.f.settings.studies.iter().zip(&cx.f.display.studies) {
        if !config.visible || config.spec().placement != Placement::Overlay {
            continue;
        }
        let Some(output) = output else { continue };
        for plot in &output.plots {
            let style = config.plot_style(plot.key);
            if style.visible
                && plot.kind == PlotKind::Line
                && plot.offset == 0
                && let Some(v) = last_value(plot)
            {
                tag(
                    cx.map.label(v, ValueFormat::Price, cx.f.digits),
                    cx.y(v),
                    &cx.band,
                    style.color,
                );
            }
        }
    }
    for (study, map, band, format) in panes {
        let config = &cx.f.settings.studies[*study];
        let Some(Some(output)) = cx.f.display.studies.get(*study) else {
            continue;
        };
        for plot in &output.plots {
            let style = config.plot_style(plot.key);
            if style.visible
                && plot.offset == 0
                && let Some(v) = last_value(plot)
            {
                let color = match (&plot.up, plot.kind) {
                    (Some(up), PlotKind::Histogram) => {
                        if up.last().copied().unwrap_or(true) {
                            UP_COLOR
                        } else {
                            DOWN_COLOR
                        }
                    }
                    _ => style.color,
                };
                tag(
                    price::format_value(v, *format, cx.f.digits),
                    cx.y_on(map, v),
                    band,
                    color,
                );
            }
        }
    }
}
