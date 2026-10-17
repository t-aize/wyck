//! Drawing the prices: candles and bars, lines, areas and the baseline, the volume columns, and
//! the chart types with their own marks (Kagi lines, point and figure boxes).

use super::color::transparent_black;
use wyck_core::math::{f32_of, i64_to_f64};
use wyck_market::Bar;

use super::cmd::{Align, Cmd, P, font, hsla, line_height, with_alpha};
use super::ctx::{Ctx, columns};
use super::drawings::ellipse_points;
use crate::data::Series;
use crate::settings::ChartKind;
use wyck_drawings::look::Face;

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
    /// Bricks with the wicks of the extremes the price saw while they formed.
    BodyWicks,
}

impl CandleStyle {
    /// Whether the body is a brick, drawn from the brick settings.
    fn is_brick(self) -> bool {
        matches!(self, Self::Body | Self::BodyWicks)
    }
}

/// Draws the series as `kind`.
pub(super) fn draw(cx: &Ctx<'_>, kind: ChartKind, first: usize, last: usize, out: &mut Vec<Cmd>) {
    match (cx.series, kind) {
        (
            Series::Bars(bars),
            ChartKind::Candles | ChartKind::HeikinAshi | ChartKind::Range | ChartKind::VolumeBars,
        ) => {
            candles(cx, bars, first, last, CandleStyle::Solid, out);
        }
        (Series::Bars(bars), ChartKind::Hollow) => {
            candles(cx, bars, first, last, CandleStyle::Hollow, out);
        }
        (Series::Bars(bars), ChartKind::Bars) => {
            candles(cx, bars, first, last, CandleStyle::Ohlc, out);
        }
        (Series::Bars(bars), ChartKind::Renko) => {
            let style = if cx.f.settings.transform.renko_wicks {
                CandleStyle::BodyWicks
            } else {
                CandleStyle::Body
            };
            candles(cx, bars, first, last, style, out);
        }
        (Series::Bars(bars), ChartKind::LineBreak) => {
            candles(cx, bars, first, last, CandleStyle::Body, out);
        }
        (Series::Bars(bars), ChartKind::VolumeCandles) => {
            volume_candles(cx, bars, first, last, out);
        }
        (Series::Bars(bars), ChartKind::Tpo) => super::tpo::draw(cx, bars, first, last, out),
        (Series::Bars(bars), ChartKind::Footprint) => {
            super::footprint::draw(cx, bars, first, last, out);
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

/// Plain candles, for what a chart type falls back to when it cannot show its details.
pub(super) fn solid_candles(
    cx: &Ctx<'_>,
    bars: &[Bar],
    first: usize,
    last: usize,
    out: &mut Vec<Cmd>,
) {
    candles(cx, bars, first, last, CandleStyle::Solid, out);
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
            let group = bars.get(range).unwrap_or_default();
            let (Some(oldest), Some(newest)) = (group.first(), group.last()) else {
                return;
            };
            let high = i64_to_f64(group.iter().map(|b| b.high).max().unwrap_or(0));
            let low = i64_to_f64(group.iter().map(|b| b.low).min().unwrap_or(0));
            let up = newest.close >= oldest.open;
            let c = hsla(cx.up_color(up));
            out.push(cx.vline(x, cx.y(high), cx.y(low).max(cx.y(high) + wick), c));
        });
        return;
    }
    let look = &cx.f.settings.transform;
    let body_w = if style.is_brick() {
        f32_of((bar_px * f64::from(look.brick_width)).max(3.0))
    } else {
        f32_of((bar_px * 0.72).max(3.0))
    };
    for (i, bar) in (first..last).zip(bars.iter().skip(first)) {
        let x = cx.x(i);
        // Hollow candles are colored by the change from the previous close, the others by the
        // change over the bar itself.
        let up = match style {
            CandleStyle::Hollow => {
                let previous = i
                    .checked_sub(1)
                    .and_then(|p| bars.get(p))
                    .map(|prev| prev.close);
                bar.close >= previous.unwrap_or(bar.open)
            }
            _ => bar.close >= bar.open,
        };
        let c = hsla(cx.up_color(up));
        let (y_high, y_low) = (cx.y(i64_to_f64(bar.high)), cx.y(i64_to_f64(bar.low)));
        let (y_open, y_close) = (cx.y(i64_to_f64(bar.open)), cx.y(i64_to_f64(bar.close)));
        if style != CandleStyle::Body {
            out.push(cx.vline(x, y_high.min(y_low), y_low.max(y_high) + wick, c));
        }
        if style == CandleStyle::Ohlc {
            let tick = (f32_of(bar_px) * 0.38).max(2.0);
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
                fill: transparent_black(),
                border: Some(((1.0 / cx.f.scale).max(1.0), c)),
                radius: 0.0,
            });
        } else if style.is_brick() {
            out.push(Cmd::Rect {
                x: x0,
                y: top,
                w: x1 - x0,
                h: height,
                fill: with_alpha(cx.up_color(up), look.brick_opacity),
                border: look.brick_border.then_some((1.0, c)),
                radius: 0.0,
            });
        } else {
            out.push(Cmd::rect(x0, top, x1 - x0, height, c));
        }
    }
}

/// The volume as short text: `830`, `12.4k`, `3.1M`.
fn compact(volume: i64) -> String {
    let v = i64_to_f64(volume.max(0));
    if v >= 1_000_000.0 {
        format!("{:.1}M", v / 1_000_000.0)
    } else if v >= 10_000.0 {
        format!("{:.1}k", v / 1_000.0)
    } else if v >= 1_000.0 {
        format!("{:.2}k", v / 1_000.0)
    } else {
        format!("{v:.0}")
    }
}

/// Candles as wide as the volume of their period: same place on the time axis, but a busy period
/// is a thick candle. Too narrow to tell the widths apart, they are drawn as plain candles.
fn volume_candles(cx: &Ctx<'_>, bars: &[Bar], first: usize, last: usize, out: &mut Vec<Cmd>) {
    let bar_px = cx.f.view.bar_px;
    let last = last.min(bars.len());
    if bar_px < CANDLE_MIN_PX || first >= last {
        candles(cx, bars, first, last, CandleStyle::Solid, out);
        return;
    }
    let settings = &cx.f.settings.volume_candles;
    let volumes: Vec<i64> = bars
        .get(first..last)
        .unwrap_or_default()
        .iter()
        .map(|b| b.volume)
        .collect();
    let widths = settings.widths(&volumes);
    let thin = 1.0 / cx.f.scale;
    let wick_w = settings.wick_width.max(thin);
    let gap_edge = hsla(cx.f.palette.bg);
    let (top_edge, label_ink) = (cx.oy + f32_of(cx.band.top), hsla(cx.f.palette.text));
    for (n, (i, bar)) in (first..last).zip(bars.iter().skip(first)).enumerate() {
        let x = cx.x(i);
        let up = match settings.color_by {
            crate::order_flow::volume::ColorBy::Body => bar.close >= bar.open,
            crate::order_flow::volume::ColorBy::PreviousClose => {
                bar.close
                    >= i.checked_sub(1)
                        .and_then(|p| bars.get(p))
                        .map_or(bar.open, |prev| prev.close)
            }
        };
        let c = hsla(cx.up_color(up));
        let body_w = (f32_of(bar_px) * widths.get(n).copied().unwrap_or(0.0)).max(2.0);
        let (y_high, y_low) = (cx.y(i64_to_f64(bar.high)), cx.y(i64_to_f64(bar.low)));
        let (y_open, y_close) = (cx.y(i64_to_f64(bar.open)), cx.y(i64_to_f64(bar.close)));
        let (wick_top, wick_bottom) = (y_high.min(y_low), y_low.max(y_high) + thin);
        out.push(Cmd::rect(
            cx.snap(x - wick_w / 2.0),
            cx.snap(wick_top),
            wick_w,
            cx.snap(wick_bottom) - cx.snap(wick_top),
            c,
        ));
        let (top, bottom) = (y_open.min(y_close), y_open.max(y_close));
        let x0 = cx.snap(x - body_w / 2.0);
        let x1 = cx.snap(x + body_w / 2.0).max(x0 + thin);
        let top = cx.snap(top);
        let height = (cx.snap(bottom) - top).max(thin);
        if settings.fill.is_hollow(up) {
            out.push(Cmd::Rect {
                x: x0,
                y: top,
                w: x1 - x0,
                h: height,
                fill: transparent_black(),
                border: Some((thin.max(1.0), c)),
                radius: 0.0,
            });
        } else if settings.outline {
            out.push(Cmd::Rect {
                x: x0,
                y: top,
                w: x1 - x0,
                h: height,
                fill: c,
                border: Some((thin.max(1.0), gap_edge)),
                radius: 0.0,
            });
        } else {
            out.push(Cmd::rect(x0, top, x1 - x0, height, c));
        }
        if settings.labels && body_w >= 26.0 && wick_top - line_height() - 2.0 >= top_edge {
            out.push(Cmd::Text {
                text: compact(bar.volume),
                x,
                y: wick_top - line_height() - 2.0,
                size: font() - 1.0,
                color: label_ink,
                align: Align::Center,
                bold: false,
                face: Face::default(),
            });
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
                points.push((cx.x(i), cx.y(i64_to_f64(v))));
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
            points.push((x, cx.y(i64_to_f64(v))));
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
        let mut before: Option<f32> = None;
        for &(x, y) in &points {
            if let Some(held) = before {
                stepped.push((x, held));
            }
            stepped.push((x, y));
            before = Some(y);
        }
        points = stepped;
    }
    let p = cx.f.palette;
    if area {
        let base = cx.oy + f32_of(cx.band.bottom());
        let mut polygon = points.clone();
        if let (Some(&(last_x, _)), Some(&(first_x, _))) = (points.last(), points.first()) {
            polygon.push((last_x, base));
            polygon.push((first_x, base));
        }
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
    let base_price = f64::midpoint(cx.map.lo, cx.map.hi);
    let base_y = cx.y(base_price);
    let (left, right) = (cx.ox, cx.ox + f32_of(cx.plot_w));
    let (top, bottom) = (
        cx.oy + f32_of(cx.band.top),
        cx.oy + f32_of(cx.band.bottom()),
    );
    let mut polygon = points.clone();
    if let (Some(&(last_x, _)), Some(&(first_x, _))) = (points.last(), points.first()) {
        polygon.push((last_x, base_y));
        polygon.push((first_x, base_y));
    }
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
    let look = &cx.f.settings.transform;
    let (thick, thin) = (look.kagi_thick, look.kagi_thin);
    let style = |yang: bool| {
        if yang {
            (thick, hsla(cx.f.palette.kagi_yang))
        } else {
            (thin, hsla(cx.f.palette.kagi_yin))
        }
    };
    let last = last.min(bars.len());
    if cx.f.view.bar_px < 2.0 {
        // Too narrow for shoulders and waists: one line per pixel column over its range.
        columns(cx, first, last, |x, range| {
            let group = bars.get(range.clone()).unwrap_or_default();
            let high = i64_to_f64(group.iter().map(|b| b.high).max().unwrap_or(0));
            let low = i64_to_f64(group.iter().map(|b| b.low).min().unwrap_or(0));
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
        let (y_open, y_close) = (cx.y(i64_to_f64(bar.open)), cx.y(i64_to_f64(bar.close)));
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
        if let Some(at) = line.switch_at {
            let y_switch = cx.y(i64_to_f64(at));
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
        } else {
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

/// Point and figure: a column of X or O boxes per element.
fn point_figure(cx: &Ctx<'_>, bars: &[Bar], first: usize, last: usize, out: &mut Vec<Cmd>) {
    let columns_meta = &cx.f.display.pnf;
    let look = &cx.f.settings.transform;
    let ink = |up: bool| {
        if up {
            cx.f.palette.pnf_up
        } else {
            cx.f.palette.pnf_down
        }
    };
    let last = last.min(bars.len()).min(columns_meta.len());
    let width = (f32_of(cx.f.view.bar_px) * 0.8).max(1.0);
    let boxes: i64 = columns_meta
        .iter()
        .take(last)
        .skip(first)
        .map(|column| column.top - column.bottom + 1)
        .sum();
    if width < BOX_MIN_PX || boxes > 4_000 {
        // Too small for the boxes to read: each pixel column is a bar over its range.
        columns(cx, first, last, |x, range| {
            let group = bars.get(range.clone()).unwrap_or_default();
            let high = i64_to_f64(group.iter().map(|b| b.high).max().unwrap_or(0));
            let low = i64_to_f64(group.iter().map(|b| b.low).min().unwrap_or(0));
            let up = range
                .end
                .checked_sub(1)
                .and_then(|n| columns_meta.get(n))
                .is_some_and(|column| column.up);
            let (y0, y1) = (cx.y(high), cx.y(low));
            out.push(Cmd::Rect {
                x: x - width / 2.0,
                y: y0.min(y1),
                w: width,
                h: (y1 - y0).abs().max(1.0),
                fill: with_alpha(ink(up), 0.5),
                border: None,
                radius: 0.0,
            });
        });
        return;
    }
    for (i, (column, bar)) in
        (first..last).zip(columns_meta.iter().skip(first).zip(bars.iter().skip(first)))
    {
        let size = i64_to_f64(column.size);
        let x = cx.x(i);
        let color = hsla(ink(column.up));
        // Measured where the boxes are: the price 0 is far off the screen, where the heights
        // are cut to a range that can be drawn, and would give a height of nothing.
        let base = i64_to_f64(column.bottom) * size;
        let box_h = (cx.y(base) - cx.y(base + size)).abs();
        if box_h < BOX_MIN_PX {
            let (y0, y1) = (cx.y(i64_to_f64(bar.high)), cx.y(i64_to_f64(bar.low)));
            out.push(Cmd::Rect {
                x: x - width / 2.0,
                y: y0.min(y1),
                w: width,
                h: (y1 - y0).abs().max(1.0),
                fill: with_alpha(ink(column.up), 0.5),
                border: None,
                radius: 0.0,
            });
            continue;
        }
        let (hw, hh) = (width / 2.0 * look.pnf_glyph, box_h / 2.0 * look.pnf_glyph);
        let line_w = if look.pnf_line > 0.0 {
            look.pnf_line
        } else {
            (width / 10.0).clamp(1.0, 2.2)
        };
        for level in column.bottom..=column.top {
            let y = cx.y(i64_to_f64(level) * size);
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
