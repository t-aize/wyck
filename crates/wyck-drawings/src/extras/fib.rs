//! The Fibonacci tools that are circles, arcs, spirals, wedges and time extensions.

use super::{dist, mid, polyline, sample, sub, wrap};

use crate::model::style::Dash;
use crate::model::{Drawing, Point};
use crate::prim::fib::{fill_of, label, level_label, visible_levels};
use crate::prim::{Anchor, P, Prim, Projection, lead, seg};
use std::f32::consts::{FRAC_PI_2, PI};
use wyck_core::math::f32_of;

pub(super) fn fib_time_extension(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    out: &mut Vec<Prim>,
) {
    let style = &drawing.style;
    let rect = proj.plot();
    let (Some([pa, pb, pc]), Some([s0, s1, s2])) = (lead(&drawing.points), lead(pts)) else {
        return;
    };
    let bars = match (proj.index_of(pa.t), proj.index_of(pb.t)) {
        (Some(i), Some(j)) => j - i,
        _ => return,
    };
    let mut previous: Option<f32> = None;
    for level in visible_levels(drawing) {
        let x = if bars.abs() < 1e-9 {
            s2.0
        } else {
            let Some(t) = proj.shift_bars(pc.t, bars * level.value) else {
                continue;
            };
            let Some(at) = proj.to_screen(Point { t, p: pc.p }) else {
                continue;
            };
            at.0
        };
        if let (Some(before), Some((_, opacity))) = (previous, fill_of(style)) {
            out.push(Prim::Rect {
                a: (before, rect.t),
                b: (x, rect.b),
                fill: Some((level.color, opacity)),
                stroke: None,
            });
        }
        out.push(seg(
            (x, rect.t),
            (x, rect.b),
            level.color,
            0.9,
            level.line_width(style.width),
            level.line_dash(style.dash),
        ));
        if style.labels {
            out.push(label(
                (x + 4.0, rect.b - 12.0),
                level_label(style, level.value, None),
                level.color,
                Anchor::Left,
                style,
            ));
        }
        previous = Some(x);
    }
    out.push(seg(s0, s1, style.color, 0.6, 1.0, Dash::Dashed));
    out.push(seg(s1, s2, style.color, 0.6, 1.0, Dash::Dashed));
}

pub(super) fn fib_circles(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let Some([a, b]) = lead(pts) else {
        return;
    };
    let center = mid(a, b);
    let base = dist(a, b) / 2.0;
    if base < 1.0 {
        return;
    }
    for level in visible_levels(drawing) {
        let radius = base * f32_of(level.value);
        if radius < 1.0 {
            continue;
        }
        out.push(Prim::Ellipse {
            center,
            rx: radius,
            ry: radius,
            fill: None,
            stroke: Some((level.color, level.line_width(style.width))),
        });
        if style.labels {
            out.push(label(
                (center.0, center.1 - radius - 8.0),
                level_label(style, level.value, None),
                level.color,
                Anchor::Center,
                style,
            ));
        }
    }
    out.push(seg(a, b, style.color, 0.6, 1.0, Dash::Dashed));
}

pub(super) fn fib_arcs(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let Some([a, b]) = lead(pts) else {
        return;
    };
    let base = dist(a, b);
    if base < 1.0 {
        return;
    }
    // Half circles about the second point, on the side of the first.
    let toward = sub(a, b).1.atan2(sub(a, b).0);
    for level in visible_levels(drawing) {
        let radius = base * f32_of(level.value);
        if radius < 1.0 {
            continue;
        }
        let arc = sample(32, |t| {
            let angle = toward - FRAC_PI_2 + PI * t;
            (b.0 + radius * angle.cos(), b.1 + radius * angle.sin())
        });
        out.push(polyline(
            arc,
            level.color,
            level.line_width(style.width),
            0.9,
        ));
        if style.labels {
            out.push(label(
                (
                    b.0 + radius * toward.cos() + 4.0,
                    b.1 + radius * toward.sin(),
                ),
                level_label(style, level.value, None),
                level.color,
                Anchor::Left,
                style,
            ));
        }
    }
    out.push(seg(a, b, style.color, 0.6, 1.0, Dash::Dashed));
}

pub(super) fn fib_spiral(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let Some([center, through]) = lead(pts) else {
        return;
    };
    let base = dist(center, through);
    if base < 2.0 {
        return;
    }
    let start = sub(through, center).1.atan2(sub(through, center).0);
    let turn = if drawing.reverse { -1.0 } else { 1.0 };
    // A golden spiral grows by the golden ratio every quarter turn, and ends at the second point.
    let golden: f32 = f32::midpoint(1.0, 5f32.sqrt());
    let spiral = sample(240, |t| {
        let theta = -4.0 * PI * (1.0 - t);
        let radius = base * golden.powf(theta / FRAC_PI_2);
        let angle = start + turn * theta;
        (
            center.0 + radius * angle.cos(),
            center.1 + radius * angle.sin(),
        )
    });
    out.push(polyline(spiral, style.color, style.width, 1.0));
    out.push(seg(center, through, style.color, 0.6, 1.0, Dash::Dashed));
}

pub(super) fn fib_wedge(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let Some([center, b, c]) = lead(pts) else {
        return;
    };
    let base = dist(center, b);
    if base < 2.0 {
        return;
    }
    let (from, toward) = (
        sub(b, center).1.atan2(sub(b, center).0),
        sub(c, center).1.atan2(sub(c, center).0),
    );
    let sweep = wrap(toward - from);
    let levels = visible_levels(drawing);
    let widest = f32_of(levels.iter().map(|l| l.value).fold(0.0, f64::max));
    if widest <= 0.0 {
        return;
    }
    let arc_at = |radius: f32| -> Vec<P> {
        sample(32, |t| {
            let angle = from + sweep * t;
            (
                center.0 + radius * angle.cos(),
                center.1 + radius * angle.sin(),
            )
        })
    };
    if let Some((_, opacity)) = fill_of(style) {
        let mut wedge = arc_at(base * widest);
        wedge.push(center);
        out.push(Prim::Polygon {
            points: wedge,
            fill: (style.fill_color(), opacity),
        });
    }
    for level in &levels {
        let radius = base * f32_of(level.value);
        out.push(polyline(
            arc_at(radius),
            level.color,
            level.line_width(style.width),
            0.9,
        ));
        if style.labels {
            out.push(label(
                (
                    center.0 + radius * from.cos() + 4.0,
                    center.1 + radius * from.sin(),
                ),
                level_label(style, level.value, None),
                level.color,
                Anchor::Left,
                style,
            ));
        }
    }
    for angle in [from, from + sweep] {
        let end = (
            center.0 + base * widest * angle.cos(),
            center.1 + base * widest * angle.sin(),
        );
        out.push(seg(center, end, style.color, 1.0, style.width, style.dash));
    }
}
