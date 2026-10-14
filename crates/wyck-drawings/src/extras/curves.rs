//! Time cycles, circles, arcs and curves.

use super::{MAX_CYCLES, add, dist, length, polyline, sample, scale, sub};

use crate::geometry::fib::fill_of;
use crate::geometry::{P, Prim, Projection, seg};
use crate::model::style::Dash;
use crate::model::{Drawing, Point};
use std::f32::consts::{PI, TAU};

/// The x positions of the cycle lines a pair of points makes: one every distance between them,
/// counted in bars, in both directions across the plot, left to right.
pub(super) fn cycle_xs(drawing: &Drawing, pts: &[P], proj: &dyn Projection) -> Vec<f32> {
    let rect = proj.plot();
    let (pa, pb) = (drawing.points[0], drawing.points[1]);
    let bars = match (proj.index_of(pa.t), proj.index_of(pb.t)) {
        (Some(i), Some(j)) => j - i,
        _ => return Vec::new(),
    };
    let step = pts[1].0 - pts[0].0;
    if bars.abs() < 1e-9 || step.abs() < 3.0 {
        return Vec::new();
    }
    let (k_left, k_right) = ((rect.l - pts[0].0) / step, (rect.r - pts[0].0) / step);
    let first = (k_left.min(k_right).floor() as i64).max(-MAX_CYCLES);
    let last = (k_left.max(k_right).ceil() as i64).min(MAX_CYCLES);
    let mut xs: Vec<f32> = (first..=last)
        .filter_map(|k| {
            if k == 0 {
                return Some(pts[0].0);
            }
            let t = proj.shift_bars(pa.t, bars * k as f64)?;
            proj.to_screen(Point { t, p: pa.p }).map(|p| p.0)
        })
        .collect();
    xs.sort_by(f32::total_cmp);
    xs
}

pub(super) fn cyclic_lines(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    out: &mut Vec<Prim>,
) {
    let style = &drawing.style;
    let rect = proj.plot();
    for x in cycle_xs(drawing, pts, proj) {
        out.push(seg(
            (x, rect.t),
            (x, rect.b),
            style.color,
            0.9,
            style.width,
            style.dash,
        ));
    }
}

pub(super) fn time_cycles(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    out: &mut Vec<Prim>,
) {
    let style = &drawing.style;
    let base = pts[0].1;
    for pair in cycle_xs(drawing, pts, proj).windows(2) {
        let (left, width) = (pair[0], pair[1] - pair[0]);
        if width < 1.0 {
            continue;
        }
        let height = width / 2.0;
        let arch = sample(24, |t| (left + width * t, base - height * (PI * t).sin()));
        if let Some(fill) = fill_of(style) {
            out.push(Prim::Polygon {
                points: arch.clone(),
                fill,
            });
        }
        out.push(polyline(arch, style.color, style.width, 1.0));
    }
}

pub(super) fn sine_line(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let rect = proj.plot();
    let (a, b) = (pts[0], pts[1]);
    let half = b.0 - a.0;
    if half.abs() < 2.0 {
        out.push(seg(a, b, style.color, 0.6, 1.0, Dash::Dashed));
        return;
    }
    let (center, amplitude) = ((a.1 + b.1) / 2.0, (a.1 - b.1) / 2.0);
    let (left, right) = (
        if style.extend_left {
            rect.l
        } else {
            a.0.min(b.0)
        },
        if style.extend_right {
            rect.r
        } else {
            a.0.max(b.0)
        },
    );
    if right <= left {
        return;
    }
    let steps = (((right - left) / 3.0).ceil() as usize).clamp(2, 1_500);
    let wave = sample(steps, |t| {
        let x = left + (right - left) * t;
        (x, center + amplitude * (PI * (x - a.0) / half).cos())
    });
    out.push(polyline(wave, style.color, style.width, 1.0));
}

pub(super) fn rotated_rectangle(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b, c) = (pts[0], pts[1], pts[2]);
    let side = sub(b, a);
    let run = length(side);
    if run < 1e-3 {
        out.push(seg(a, b, style.color, 1.0, style.width, style.dash));
        return;
    }
    let normal = (-side.1 / run, side.0 / run);
    let height = sub(c, a).0 * normal.0 + sub(c, a).1 * normal.1;
    let push = scale(normal, height);
    let corners = vec![a, b, add(b, push), add(a, push)];
    if let Some(fill) = fill_of(style) {
        out.push(Prim::Polygon {
            points: corners.clone(),
            fill,
        });
    }
    for i in 0..4 {
        out.push(seg(
            corners[i],
            corners[(i + 1) % 4],
            style.color,
            1.0,
            style.width,
            style.dash,
        ));
    }
}

pub(super) fn circle(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let radius = dist(pts[0], pts[1]);
    out.push(Prim::Ellipse {
        center: pts[0],
        rx: radius,
        ry: radius,
        fill: fill_of(style),
        stroke: Some((style.color, style.width)),
    });
}

/// The circle through three points: its center and radius, or `None` when they are on a line
/// (or so nearly that the circle is far larger than any screen).
pub(super) fn circumcircle(a: P, b: P, c: P) -> Option<(P, f32)> {
    let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
    if d.abs() < 1e-2 {
        return None;
    }
    let (sa, sb, sc) = (
        a.0 * a.0 + a.1 * a.1,
        b.0 * b.0 + b.1 * b.1,
        c.0 * c.0 + c.1 * c.1,
    );
    let center = (
        (sa * (b.1 - c.1) + sb * (c.1 - a.1) + sc * (a.1 - b.1)) / d,
        (sa * (c.0 - b.0) + sb * (a.0 - c.0) + sc * (b.0 - a.0)) / d,
    );
    let radius = dist(center, a);
    (radius.is_finite() && radius < 50_000.0).then_some((center, radius))
}

pub(super) fn arc(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b, c) = (pts[0], pts[1], pts[2]);
    let Some((center, radius)) = circumcircle(a, b, c) else {
        out.push(polyline(vec![a, c, b], style.color, style.width, 1.0));
        return;
    };
    let angle = |p: P| (p.1 - center.1).atan2(p.0 - center.0);
    let (from, to, through) = (angle(a), angle(b), angle(c));
    // The arc runs from the first point to the second, on the side of the third.
    let around = |x: f32| x.rem_euclid(TAU);
    let counter = around(to - from);
    let sweep = if around(through - from) <= counter {
        counter
    } else {
        counter - TAU
    };
    let curve = sample(64, |t| {
        let at = from + sweep * t;
        (center.0 + radius * at.cos(), center.1 + radius * at.sin())
    });
    if let Some(fill) = fill_of(style) {
        out.push(Prim::Polygon {
            points: curve.clone(),
            fill,
        });
    }
    out.push(polyline(curve, style.color, style.width, 1.0));
}

pub(super) fn curve(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b, c) = (pts[0], pts[1], pts[2]);
    // A quadratic curve whose control is placed so that the curve passes through the middle point.
    let control = sub(scale(b, 2.0), scale(add(a, c), 0.5));
    let line = sample(48, |t| {
        let u = 1.0 - t;
        (
            u * u * a.0 + 2.0 * u * t * control.0 + t * t * c.0,
            u * u * a.1 + 2.0 * u * t * control.1 + t * t * c.1,
        )
    });
    out.push(polyline(line, style.color, style.width, 1.0));
}

pub(super) fn double_curve(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
    let half = (b.0 - a.0) / 2.0;
    let (c1, c2) = ((a.0 + half, a.1), (a.0 + half, b.1));
    let line = sample(48, |t| {
        let u = 1.0 - t;
        let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        (
            w0 * a.0 + w1 * c1.0 + w2 * c2.0 + w3 * b.0,
            w0 * a.1 + w1 * c1.1 + w2 * c2.1 + w3 * b.1,
        )
    });
    out.push(polyline(line, style.color, style.width, 1.0));
}

#[cfg(test)]
mod tests {

    use super::circumcircle;

    #[test]
    fn a_circle_goes_through_three_points() {
        let (center, radius) = circumcircle((0.0, 0.0), (10.0, 0.0), (5.0, 5.0)).unwrap();
        assert!((center.0 - 5.0).abs() < 1e-3 && center.1.abs() < 1e-3);
        assert!((radius - 5.0).abs() < 1e-3);
        assert!(circumcircle((0.0, 0.0), (5.0, 5.0), (10.0, 10.0)).is_none());
    }
}
