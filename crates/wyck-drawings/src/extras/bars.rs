//! The tools that read the bars of the chart: ghost bars, patterns, VWAP and volume.

use super::{DOWN, MAX_GHOST_BARS, MAX_ROWS, MIN_ROWS, POC, ROW_PX, UP, polyline, stat};

use crate::geometry::fib::{label, visible_levels};
use crate::geometry::{Anchor, P, Prim, Projection, seg};
use crate::model::style::Dash;
use crate::model::{Drawing, Point};

/// The candle of a bar shifted by `offset` in price, at `x`.
pub(super) fn candle(
    proj: &dyn Projection,
    x: f32,
    prices: (f64, f64, f64, f64),
    offset: f64,
    width: f32,
    alpha: f32,
    out: &mut Vec<Prim>,
) {
    let (open, high, low, close) = prices;
    let color = if close >= open { UP } else { DOWN };
    out.push(seg(
        (x, proj.y_of(high + offset)),
        (x, proj.y_of(low + offset)),
        color,
        alpha,
        1.0,
        Dash::Solid,
    ));
    out.push(Prim::Rect {
        a: (x - width / 2.0, proj.y_of(open + offset)),
        b: (x + width / 2.0, proj.y_of(close + offset)),
        fill: Some((color, alpha)),
        stroke: None,
    });
}

/// The bars between the first two points of a bars pattern or a ghost feed.
pub(super) fn source_bars(
    drawing: &Drawing,
    proj: &dyn Projection,
) -> Vec<crate::geometry::BarView> {
    let (a, b) = (drawing.points[0].t, drawing.points[1].t);
    let mut bars = proj.bars_between(a.min(b), a.max(b));
    bars.truncate(MAX_GHOST_BARS);
    bars
}

/// A bars pattern (the bars of the range, copied) or a ghost feed (bars made from the moves of
/// the range in another order), starting at the third point.
pub(super) fn bars_pattern(
    drawing: &Drawing,
    proj: &dyn Projection,
    out: &mut Vec<Prim>,
    ghost: bool,
) {
    let style = &drawing.style;
    let bars = source_bars(drawing, proj);
    let (Some(first), Some(last)) = (bars.first(), bars.last()) else {
        return;
    };
    let start = drawing.points[2];
    let Some(first_index) = proj.index_of(first.time) else {
        return;
    };
    let spacing = if bars.len() >= 2 {
        (bars[1].x - bars[0].x).abs()
    } else {
        6.0
    };
    let width = (spacing * 0.7).clamp(1.0, 20.0);

    // The range the bars come from.
    let high = bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let low = bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    out.push(Prim::Rect {
        a: (first.x - spacing / 2.0, proj.y_of(high)),
        b: (last.x + spacing / 2.0, proj.y_of(low)),
        fill: Some((style.color, 0.08)),
        stroke: None,
    });

    let place = |offset_bars: f64| -> Option<f32> {
        let t = proj.shift_bars(start.t, offset_bars)?;
        proj.to_screen(Point { t, p: start.p }).map(|p| p.0)
    };
    if ghost {
        let n = bars.len();
        let mut seed = drawing
            .id
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .wrapping_add(n as u64);
        let mut price = start.p;
        for i in 0..n {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let source = &bars[(seed >> 33) as usize % n];
            let (open, close) = (price, price + (source.close - source.open));
            let (high, low) = (
                price + (source.high - source.open),
                price + (source.low - source.open),
            );
            if let Some(x) = place(i as f64) {
                candle(proj, x, (open, high, low, close), 0.0, width, 0.55, out);
            }
            price = close;
        }
    } else {
        let offset = start.p - first.close;
        for bar in &bars {
            let Some(index) = proj.index_of(bar.time) else {
                continue;
            };
            if let Some(x) = place(index - first_index) {
                candle(
                    proj,
                    x,
                    (bar.open, bar.high, bar.low, bar.close),
                    offset,
                    width,
                    0.7,
                    out,
                );
            }
        }
    }
}

pub(super) fn anchored_vwap(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    out: &mut Vec<Prim>,
) {
    let style = &drawing.style;
    let rect = proj.plot();
    let bars = proj.bars_between(drawing.points[0].t, i64::MAX);
    if bars.is_empty() {
        return;
    }
    let levels = visible_levels(drawing);
    let (mut volume, mut weighted, mut weighted_squares) = (0.0, 0.0, 0.0);
    let mut line: Vec<P> = Vec::new();
    let mut bands: Vec<(Vec<P>, Vec<P>)> =
        levels.iter().map(|_| (Vec::new(), Vec::new())).collect();
    for bar in &bars {
        let typical = (bar.high + bar.low + bar.close) / 3.0;
        let weight = if bar.volume > 0.0 { bar.volume } else { 1.0 };
        volume += weight;
        weighted += typical * weight;
        weighted_squares += typical * typical * weight;
        let vwap = weighted / volume;
        #[expect(
            clippy::suspicious_operation_groupings,
            reason = "the variance is E[x^2] minus the square of the mean"
        )]
        let deviation = (weighted_squares / volume - vwap * vwap).max(0.0).sqrt();
        // Only what is near the plot is kept: the rest of a long history is out of sight.
        if bar.x < rect.l - 20.0 || bar.x > rect.r + 20.0 {
            continue;
        }
        line.push((bar.x, proj.y_of(vwap)));
        for (band, level) in bands.iter_mut().zip(&levels) {
            band.0
                .push((bar.x, proj.y_of(vwap + level.value * deviation)));
            band.1
                .push((bar.x, proj.y_of(vwap - level.value * deviation)));
        }
    }
    for ((upper, lower), level) in bands.into_iter().zip(&levels) {
        out.push(polyline(upper, level.color, 1.0, 0.8));
        out.push(polyline(lower, level.color, 1.0, 0.8));
    }
    if style.labels
        && let Some(end) = line.last()
    {
        out.push(stat(
            style,
            (end.0 + 6.0, end.1),
            "VWAP".to_owned(),
            style.color,
            Anchor::Left,
        ));
    }
    out.push(polyline(line, style.color, style.width, 1.0));
    out.push(seg(
        (pts[0].0, rect.t),
        (pts[0].0, rect.b),
        style.color,
        0.4,
        1.0,
        Dash::Dashed,
    ));
}

pub(super) fn volume_profile(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    out: &mut Vec<Prim>,
    fixed: bool,
) {
    let style = &drawing.style;
    let rect = proj.plot();
    let (from, to) = if fixed {
        let (a, b) = (drawing.points[0].t, drawing.points[1].t);
        (a.min(b), a.max(b))
    } else {
        (drawing.points[0].t, i64::MAX)
    };
    let bars = proj.bars_between(from, to);
    if bars.is_empty() {
        return;
    }
    let high = bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let low = bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    if high <= low {
        return;
    }
    let pixels = (proj.y_of(low) - proj.y_of(high)).abs();
    // The rows the style asks for, or as many as the height on the screen has room for.
    let rows = match style.profile.rows {
        0 => ((pixels / ROW_PX).round() as usize).clamp(MIN_ROWS, MAX_ROWS),
        asked => usize::from(asked),
    };
    let step = (high - low) / rows as f64;
    let row_of = |price: f64| (((price - low) / step) as usize).min(rows - 1);

    let mut volume = vec![0.0_f64; rows];
    for bar in &bars {
        let weight = if bar.volume > 0.0 { bar.volume } else { 1.0 };
        let (a, b) = (row_of(bar.low), row_of(bar.high));
        let (first, last) = (a.min(b), a.max(b));
        let share = weight / (last - first + 1) as f64;
        for row in &mut volume[first..=last] {
            *row += share;
        }
    }
    let total: f64 = volume.iter().sum();
    let (poc, most) =
        volume.iter().copied().enumerate().fold(
            (0, 0.0),
            |best, (i, v)| if v > best.1 { (i, v) } else { best },
        );
    if most <= 0.0 {
        return;
    }
    // The value area grows from the point of control, taking the busier neighbour each time,
    // until it holds its share of the volume.
    let (mut lower, mut upper, mut held) = (poc, poc, volume[poc]);
    while held < total * f64::from(style.profile.value_area) && (lower > 0 || upper + 1 < rows) {
        let below = if lower > 0 { volume[lower - 1] } else { -1.0 };
        let above = if upper + 1 < rows {
            volume[upper + 1]
        } else {
            -1.0
        };
        if above >= below {
            upper += 1;
            held += volume[upper];
        } else {
            lower -= 1;
            held += volume[lower];
        }
    }

    let left = if fixed {
        pts[0].0.min(pts[1].0)
    } else {
        pts[0].0
    };
    let widest = if fixed {
        ((pts[0].0 - pts[1].0).abs() * 0.9).max(30.0)
    } else {
        ((rect.r - left) * 0.4).clamp(30.0, 300.0)
    };
    let opacity = style.fill_opacity;
    for (i, amount) in volume.iter().enumerate() {
        let w = (amount / most) as f32 * widest;
        if w < 0.5 {
            continue;
        }
        let inside = (lower..=upper).contains(&i);
        let (top, bottom) = (
            proj.y_of(low + (i + 1) as f64 * step),
            proj.y_of(low + i as f64 * step),
        );
        out.push(Prim::Rect {
            a: (left, top),
            b: (left + w, bottom),
            fill: style.fill.then(|| {
                (
                    style.fill_color(),
                    if inside { opacity } else { opacity * 0.5 },
                )
            }),
            stroke: (!style.fill).then_some((style.color, 1.0)),
        });
    }
    let poc_y = proj.y_of(low + (poc as f64 + 0.5) * step);
    let end = if fixed {
        pts[0].0.max(pts[1].0)
    } else {
        rect.r
    };
    out.push(seg((left, poc_y), (end, poc_y), POC, 1.0, 1.0, Dash::Solid));
    if style.labels {
        out.push(label(
            (left + widest + 4.0, poc_y),
            format!("POC {}", proj.format_price(low + (poc as f64 + 0.5) * step)),
            POC,
            Anchor::Left,
            style,
        ));
    }
    let edges: &[f32] = if fixed {
        &[pts[0].0, pts[1].0]
    } else {
        &[pts[0].0]
    };
    for &x in edges {
        out.push(seg(
            (x, rect.t),
            (x, rect.b),
            style.color,
            0.4,
            1.0,
            Dash::Dashed,
        ));
    }
}
