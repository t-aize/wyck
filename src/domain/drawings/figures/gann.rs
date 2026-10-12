//! The Gann tools: fan, box and square.

use super::{fan, sub};
use crate::domain::drawings::geometry::fib::{
    fill_of, label, level_label, level_text, visible_levels,
};
use crate::domain::drawings::geometry::{Anchor, P, Prim, Projection, Rect, seg};
use crate::domain::drawings::model::Drawing;
use crate::domain::drawings::model::style::Dash;

/// A Gann angle as it is written: `1/1`, `2/1`, `1/3`.
pub fn gann_text(ratio: f64) -> String {
    if ratio >= 1.0 {
        format!("{}/1", level_text(ratio))
    } else if ratio > 0.0 {
        format!("1/{}", level_text(1.0 / ratio))
    } else {
        level_text(ratio)
    }
}

pub(super) fn gann_fan(drawing: &Drawing, pts: &[P], rect: Rect, out: &mut Vec<Prim>) {
    let (a, b) = (pts[0], pts[1]);
    let d = sub(b, a);
    let mut levels = visible_levels(drawing);
    levels.reverse();
    let through: Vec<(P, u32, String)> = levels
        .into_iter()
        .map(|l| {
            let p = (a.0 + d.0, a.1 + d.1 * l.value as f32);
            (p, l.color, gann_text(l.value))
        })
        .collect();
    fan(drawing, a, &through, rect, out);
}

/// The positions of the levels along one side of a box, from `from` to `to`.
fn along(from: f32, to: f32, k: f64, reverse: bool) -> f32 {
    let k = if reverse { 1.0 - k } else { k } as f32;
    from + (to - from) * k
}

pub(super) fn gann_box(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
    let levels = visible_levels(drawing);
    let (top, bottom) = (a.1.min(b.1), a.1.max(b.1));
    let (left, right) = (a.0.min(b.0), a.0.max(b.0));
    // The zones between the price levels.
    if let Some((_, opacity)) = fill_of(style) {
        for pair in levels.windows(2) {
            out.push(Prim::Rect {
                a: (left, along(a.1, b.1, pair[0].value, drawing.reverse)),
                b: (right, along(a.1, b.1, pair[1].value, drawing.reverse)),
                fill: Some((pair[1].color, opacity)),
                stroke: None,
            });
        }
    }
    for level in &levels {
        let y = along(a.1, b.1, level.value, drawing.reverse);
        let x = along(a.0, b.0, level.value, drawing.reverse);
        out.push(seg(
            (left, y),
            (right, y),
            level.color,
            0.9,
            level.line_width(style.width),
            level.line_dash(style.dash),
        ));
        out.push(seg(
            (x, top),
            (x, bottom),
            level.color,
            0.9,
            level.line_width(style.width),
            level.line_dash(style.dash),
        ));
        if style.labels {
            out.push(label(
                (left - 4.0, y),
                level_label(style, level.value, None),
                level.color,
                Anchor::Right,
                style,
            ));
            out.push(label(
                (x, bottom + 10.0),
                level_label(style, level.value, None),
                level.color,
                Anchor::Center,
                style,
            ));
        }
    }
    if style.middle {
        out.push(seg(a, b, style.color, 0.8, 1.0, Dash::Dashed));
        out.push(seg(
            (a.0, b.1),
            (b.0, a.1),
            style.color,
            0.8,
            1.0,
            Dash::Dashed,
        ));
    }
    if style.labels {
        // How far the box reaches, in price and in bars.
        let (p, q) = (drawing.points[0], drawing.points[1]);
        let span = proj.format_price((q.p - p.p).abs());
        let bars = match (proj.index_of(p.t), proj.index_of(q.t)) {
            (Some(i), Some(j)) => format!(", {} bars", (j - i).abs().round()),
            _ => String::new(),
        };
        out.push(label(
            ((left + right) / 2.0, top - 10.0),
            format!("{span}{bars}"),
            style.color,
            Anchor::Center,
            style,
        ));
    }
}

pub(in crate::domain::drawings) fn gann_square(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
    let d = sub(b, a);
    let levels = visible_levels(drawing);
    // The grid.
    for level in &levels {
        let k = level.value as f32;
        let (x, y) = (a.0 + d.0 * k, a.1 + d.1 * k);
        out.push(seg(
            (a.0, y),
            (b.0, y),
            level.color,
            0.7,
            level.line_width(1.0),
            level.line_dash(style.dash),
        ));
        out.push(seg(
            (x, a.1),
            (x, b.1),
            level.color,
            0.7,
            level.line_width(1.0),
            level.line_dash(style.dash),
        ));
    }
    // The arcs about the first point, each a quarter towards the second.
    let (sx, sy) = (d.0.signum(), d.1.signum());
    for level in levels.iter().filter(|l| l.value > 0.0) {
        let k = level.value as f32;
        let (rx, ry) = (d.0.abs() * k, d.1.abs() * k);
        let points: Vec<P> = (0..=24)
            .map(|i| {
                let angle = i as f32 / 24.0 * std::f32::consts::FRAC_PI_2;
                (a.0 + sx * rx * angle.cos(), a.1 + sy * ry * angle.sin())
            })
            .collect();
        if let Some((_, opacity)) = fill_of(style) {
            let mut wedge = points.clone();
            wedge.push(a);
            out.push(Prim::Polygon {
                points: wedge,
                fill: (level.color, opacity * 0.5),
            });
        }
        out.push(Prim::Polyline {
            points,
            color: level.color,
            width: style.width,
            alpha: 1.0,
        });
    }
    // The fan from the first point to each level on the far sides.
    for level in &levels {
        let k = level.value as f32;
        out.push(seg(
            a,
            (b.0, a.1 + d.1 * k),
            level.color,
            0.8,
            level.line_width(1.0),
            level.line_dash(Dash::Solid),
        ));
        out.push(seg(
            a,
            (a.0 + d.0 * k, b.1),
            level.color,
            0.8,
            level.line_width(1.0),
            level.line_dash(Dash::Solid),
        ));
    }
    if style.middle {
        out.push(seg(a, b, style.color, 1.0, style.width, style.dash));
        out.push(seg(
            (a.0, b.1),
            (b.0, a.1),
            style.color,
            1.0,
            style.width,
            style.dash,
        ));
    }
    out.push(Prim::Rect {
        a,
        b,
        fill: None,
        stroke: Some((style.color, style.width)),
    });
}
