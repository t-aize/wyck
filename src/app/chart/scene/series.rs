//! Drawing the prices: candles and bars, lines, areas and the baseline, the volume columns, and
//! the chart types with their own marks (Kagi lines, point and figure boxes).

use wyck::openapi::market::Bar;

use super::super::data::Series;
use super::super::settings::ChartKind;
use super::cmd::{Cmd, P, hsla, with_alpha};
use super::{Ctx, VOLUME_SHARE, columns, ellipse_points};

/// Bars narrower than this are drawn as one line each, not as a body with wicks.
const CANDLE_MIN_PX: f64 = 4.0;
/// Point and figure boxes smaller than this are drawn as a plain column.
const BOX_MIN_PX: f32 = 5.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CandleStyle {
    Solid,
    Hollow,
    Ohlc,
    /// Bodies without wicks (bricks and lines).
    Body,
}

/// Draws the series as `kind`.
pub(super) fn draw(cx: &Ctx<'_>, kind: ChartKind, first: usize, last: usize, out: &mut Vec<Cmd>) {
    match (cx.series, kind) {
        (Series::Bars(bars), ChartKind::Candles | ChartKind::HeikinAshi) => {
            candles(cx, bars, first, last, CandleStyle::Solid, out);
        }
        (Series::Bars(bars), ChartKind::Hollow) => {
            candles(cx, bars, first, last, CandleStyle::Hollow, out);
        }
        (Series::Bars(bars), ChartKind::Bars) => {
            candles(cx, bars, first, last, CandleStyle::Ohlc, out);
        }
        (Series::Bars(bars), ChartKind::Renko | ChartKind::LineBreak) => {
            candles(cx, bars, first, last, CandleStyle::Body, out);
        }
        (Series::Bars(bars), ChartKind::Range) => {
            candles(cx, bars, first, last, CandleStyle::Solid, out);
        }
        (Series::Bars(bars), ChartKind::Kagi) => kagi(cx, bars, first, last, out),
        (Series::Bars(bars), ChartKind::PointFigure) => point_figure(cx, bars, first, last, out),
        (_, ChartKind::Baseline) => baseline(cx, first, last, out),
        _ => line(
            cx,
            first,
            last,
            matches!(kind, ChartKind::Area),
            matches!(kind, ChartKind::Step),
            out,
        ),
    }
}

pub(super) fn volume(cx: &Ctx<'_>, bars: &[Bar], first: usize, last: usize, out: &mut Vec<Cmd>) {
    let slice = &bars[first.min(bars.len())..last.min(bars.len())];
    let max = slice.iter().map(|b| b.volume).max().unwrap_or(0);
    if max <= 0 {
        return;
    }
    let height = (cx.band.h * VOLUME_SHARE) as f32;
    let base = cx.oy + cx.band.bottom() as f32;
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
        let c = with_alpha(cx.up_color(up), 0.28);
        out.push(cx.rect(cx.snap(x - width / 2.0), base - h, width, h, c));
    });
}

fn candles(
    cx: &Ctx<'_>,
    bars: &[Bar],
    first: usize,
    last: usize,
    style: CandleStyle,
    out: &mut Vec<Cmd>,
) {
    let bar_px = cx.f.view.bar_px;
    let last = last.min(bars.len());
    let wide = bar_px >= CANDLE_MIN_PX;
    let wick = 1.0 / cx.f.scale;
    if !wide {
        // One line per pixel column, from the lowest low to the highest high.
        columns(cx, first, last, |x, range| {
            let group = &bars[range];
            let high = group.iter().map(|b| b.high).max().unwrap_or(0) as f64;
            let low = group.iter().map(|b| b.low).min().unwrap_or(0) as f64;
            let up = group[group.len() - 1].close >= group[0].open;
            let c = hsla(cx.up_color(up));
            out.push(cx.vline(x, cx.y(high), cx.y(low).max(cx.y(high) + wick), c));
        });
        return;
    }
    let body_w = if style == CandleStyle::Body {
        (bar_px * 0.9).max(3.0) as f32
    } else {
        (bar_px * 0.72).max(3.0) as f32
    };
    for i in first..last {
        let bar = &bars[i];
        let x = cx.x(i);
        // Hollow candles are colored by the change from the previous close, the others by the
        // change over the bar itself.
        let up = match style {
            CandleStyle::Hollow => {
                let previous = (i > 0).then(|| bars[i - 1].close);
                bar.close >= previous.unwrap_or(bar.open)
            }
            _ => bar.close >= bar.open,
        };
        let c = hsla(cx.up_color(up));
        let (y_high, y_low) = (cx.y(bar.high as f64), cx.y(bar.low as f64));
        let (y_open, y_close) = (cx.y(bar.open as f64), cx.y(bar.close as f64));
        if style != CandleStyle::Body {
            out.push(cx.vline(x, y_high.min(y_low), y_low.max(y_high) + wick, c));
        }
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
            out.push(Cmd::Rect {
                x: x0,
                y: top,
                w: x1 - x0,
                h: height,
                fill: gpui::transparent_black(),
                border: Some(((1.0 / cx.f.scale).max(1.0), c)),
                radius: 0.0,
            });
        } else if style == CandleStyle::Body {
            // Bricks read better with an edge in the background color between them.
            out.push(Cmd::Rect {
                x: x0,
                y: top,
                w: x1 - x0,
                h: height,
                fill: with_alpha(cx.up_color(up), 0.85),
                border: Some((1.0, c)),
                radius: 0.0,
            });
        } else {
            out.push(cx.rect(x0, top, x1 - x0, height, c));
        }
    }
}

/// The value a point is plotted at in a line: a bar's close, or a tick's price.
fn value_at(series: &Series, index: usize) -> Option<i64> {
    series.value_at(index)
}

/// The points of the line through the visible part of the series, a few per pixel column at
/// most (the first, lowest, highest and last of each column, in order).
fn line_points(cx: &Ctx<'_>, first: usize, last: usize) -> Vec<P> {
    let mut points: Vec<P> = Vec::new();
    if cx.f.view.bar_px >= 1.0 {
        for i in first..last {
            if let Some(v) = value_at(cx.series, i) {
                points.push((cx.x(i), cx.y(v as f64)));
            }
        }
        return points;
    }
    columns(cx, first, last, |x, range| {
        let mut keep: Vec<(usize, i64)> = Vec::with_capacity(4);
        let mut lowest: Option<(usize, i64)> = None;
        let mut highest: Option<(usize, i64)> = None;
        for i in range.clone() {
            let Some(v) = value_at(cx.series, i) else {
                continue;
            };
            if lowest.is_none_or(|(_, l)| v < l) {
                lowest = Some((i, v));
            }
            if highest.is_none_or(|(_, h)| v > h) {
                highest = Some((i, v));
            }
        }
        keep.extend(value_at(cx.series, range.start).map(|v| (range.start, v)));
        keep.extend(lowest);
        keep.extend(highest);
        keep.extend(value_at(cx.series, range.end - 1).map(|v| (range.end - 1, v)));
        keep.sort_by_key(|(i, _)| *i);
        keep.dedup_by_key(|(i, _)| *i);
        for (_, v) in keep {
            points.push((x, cx.y(v as f64)));
        }
    });
    points
}

fn line(cx: &Ctx<'_>, first: usize, last: usize, area: bool, step: bool, out: &mut Vec<Cmd>) {
    let mut points = line_points(cx, first, last);
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
        let base = cx.oy + cx.band.bottom() as f32;
        let mut polygon = points.clone();
        polygon.push((points[points.len() - 1].0, base));
        polygon.push((points[0].0, base));
        out.push(Cmd::Fill {
            points: polygon,
            color: with_alpha(p.line, 0.14),
        });
    }
    out.push(Cmd::Stroke {
        points: points.clone(),
        width: 1.8,
        color: hsla(p.line),
        dash: None,
    });
    if let Some(&(x, y)) = points.last() {
        let r = 3.5;
        out.push(Cmd::Rect {
            x: x - r,
            y: y - r,
            w: r * 2.0,
            h: r * 2.0,
            fill: hsla(p.line),
            border: Some((1.5, hsla(p.bg))),
            radius: r,
        });
    }
}

/// A line colored above and below a base (the middle of the visible range), each side filled
/// down (or up) to the base.
fn baseline(cx: &Ctx<'_>, first: usize, last: usize, out: &mut Vec<Cmd>) {
    let points = line_points(cx, first, last);
    if points.len() < 2 {
        return;
    }
    let p = cx.f.palette;
    let base_price = (cx.map.lo + cx.map.hi) / 2.0;
    let base_y = cx.y(base_price);
    let (left, right) = (cx.ox, cx.ox + cx.plot_w as f32);
    let (top, bottom) = (cx.oy + cx.band.top as f32, cx.oy + cx.band.bottom() as f32);
    let mut polygon = points.clone();
    polygon.push((points[points.len() - 1].0, base_y));
    polygon.push((points[0].0, base_y));
    for (up, y0, y1) in [(true, top, base_y), (false, base_y, bottom)] {
        let color = cx.up_color(up);
        out.push(Cmd::Clip {
            x: left,
            y: y0.min(y1),
            w: right - left,
            h: (y1 - y0).abs(),
            inner: vec![
                Cmd::Fill {
                    points: polygon.clone(),
                    color: with_alpha(color, 0.12),
                },
                Cmd::Stroke {
                    points: points.clone(),
                    width: 1.8,
                    color: hsla(color),
                    dash: None,
                },
            ],
        });
    }
    out.push(Cmd::Stroke {
        points: vec![(left, base_y), (right, base_y)],
        width: 1.0,
        color: with_alpha(p.text, 0.4),
        dash: Some([4.0, 4.0]),
    });
}

/// Kagi: a vertical line per element, joined by the horizontal shoulders and waists; thick where
/// it is yang, thin where it is yin.
fn kagi(cx: &Ctx<'_>, bars: &[Bar], first: usize, last: usize, out: &mut Vec<Cmd>) {
    let meta = &cx.f.display.kagi;
    let (thick, thin) = (2.6_f32, 1.2_f32);
    let style = |yang: bool| {
        if yang {
            (thick, hsla(cx.f.palette.up))
        } else {
            (thin, hsla(cx.f.palette.down))
        }
    };
    let last = last.min(bars.len());
    if cx.f.view.bar_px < 2.0 {
        // Too narrow for shoulders and waists: one line per pixel column over its range.
        columns(cx, first, last, |x, range| {
            let group = &bars[range.clone()];
            let high = group.iter().map(|b| b.high).max().unwrap_or(0) as f64;
            let low = group.iter().map(|b| b.low).min().unwrap_or(0) as f64;
            let yang = meta.get(range.end - 1).is_some_and(|line| line.yang);
            let (_, c) = style(yang);
            out.push(cx.vline(x, cx.y(high), cx.y(low), c));
        });
        return;
    }
    let start = first.saturating_sub(1);
    for (i, bar) in bars.iter().enumerate().take(last).skip(start) {
        let x = cx.x(i);
        let Some(line) = meta.get(i) else { continue };
        let (y_open, y_close) = (cx.y(bar.open as f64), cx.y(bar.close as f64));
        // The shoulder or waist from the line before.
        if i > 0 {
            let (w, c) = style(line.yang);
            out.push(Cmd::Stroke {
                points: vec![(cx.x(i - 1), y_open), (x, y_open)],
                width: w,
                color: c,
                dash: None,
            });
        }
        match line.switch_at {
            Some(at) => {
                let y_switch = cx.y(at as f64);
                let (w1, c1) = style(line.yang);
                let (w2, c2) = style(!line.yang);
                out.push(Cmd::Stroke {
                    points: vec![(x, y_open), (x, y_switch)],
                    width: w1,
                    color: c1,
                    dash: None,
                });
                out.push(Cmd::Stroke {
                    points: vec![(x, y_switch), (x, y_close)],
                    width: w2,
                    color: c2,
                    dash: None,
                });
            }
            None => {
                let (w, c) = style(line.yang);
                out.push(Cmd::Stroke {
                    points: vec![(x, y_open), (x, y_close)],
                    width: w,
                    color: c,
                    dash: None,
                });
            }
        }
    }
}

/// Point and figure: a column of X or O boxes per element.
fn point_figure(cx: &Ctx<'_>, bars: &[Bar], first: usize, last: usize, out: &mut Vec<Cmd>) {
    let columns_meta = &cx.f.display.pnf;
    let last = last.min(bars.len()).min(columns_meta.len());
    let width = (cx.f.view.bar_px as f32 * 0.8).max(1.0);
    let boxes: i64 = (first..last)
        .map(|i| columns_meta[i].top - columns_meta[i].bottom + 1)
        .sum();
    if width < BOX_MIN_PX || boxes > 4_000 {
        // Too small for the boxes to read: each pixel column is a bar over its range.
        columns(cx, first, last, |x, range| {
            let group = &bars[range.clone()];
            let high = group.iter().map(|b| b.high).max().unwrap_or(0) as f64;
            let low = group.iter().map(|b| b.low).min().unwrap_or(0) as f64;
            let up = columns_meta[range.end - 1].up;
            let (y0, y1) = (cx.y(high), cx.y(low));
            out.push(Cmd::Rect {
                x: x - width / 2.0,
                y: y0.min(y1),
                w: width,
                h: (y1 - y0).abs().max(1.0),
                fill: with_alpha(cx.up_color(up), 0.5),
                border: None,
                radius: 0.0,
            });
        });
        return;
    }
    for i in first..last {
        let column = &columns_meta[i];
        let size = column.size as f64;
        let x = cx.x(i);
        let color = hsla(cx.up_color(column.up));
        let box_h = (cx.y(0.0) - cx.y(size)).abs();
        if box_h < BOX_MIN_PX {
            let bar = &bars[i];
            let (y0, y1) = (cx.y(bar.high as f64), cx.y(bar.low as f64));
            out.push(Cmd::Rect {
                x: x - width / 2.0,
                y: y0.min(y1),
                w: width,
                h: (y1 - y0).abs().max(1.0),
                fill: with_alpha(cx.up_color(column.up), 0.5),
                border: None,
                radius: 0.0,
            });
            continue;
        }
        let (hw, hh) = (width / 2.0 * 0.8, box_h / 2.0 * 0.8);
        let line_w = (width / 10.0).clamp(1.0, 2.2);
        for level in column.bottom..=column.top {
            let y = cx.y(level as f64 * size);
            if column.up {
                out.push(Cmd::Stroke {
                    points: vec![(x - hw, y - hh), (x + hw, y + hh)],
                    width: line_w,
                    color,
                    dash: None,
                });
                out.push(Cmd::Stroke {
                    points: vec![(x - hw, y + hh), (x + hw, y - hh)],
                    width: line_w,
                    color,
                    dash: None,
                });
            } else {
                out.push(Cmd::Stroke {
                    points: ellipse_points((x, y), hw, hh, 20),
                    width: line_w,
                    color,
                    dash: None,
                });
            }
        }
    }
}
