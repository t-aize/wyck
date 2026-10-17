//! Price and date ranges, and forecasts.

use super::{move_text, span_text, stat, stat_rows};

use crate::model::Drawing;
use crate::model::style::Dash;
use crate::prim::fib::fill_of;
use crate::prim::{Anchor, P, Prim, Projection, arrow_head, label_size, lead, seg};

/// A line from `a` to `b` with an arrow head at `b`.
pub(super) fn arrow(a: P, b: P, color: u32, width: f32, out: &mut Vec<Prim>) {
    out.push(seg(a, b, color, 1.0, width, Dash::Solid));
    out.extend(arrow_head(a, b, color, width));
}

pub(super) fn price_range(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    out: &mut Vec<Prim>,
) {
    let style = &drawing.style;
    let (Some([a, b]), Some([from_point, to_point])) = (lead(pts), lead(&drawing.points)) else {
        return;
    };
    let (left, right) = (a.0.min(b.0), a.0.max(b.0).max(a.0.min(b.0) + 1.0));
    if let Some(fill) = fill_of(style) {
        out.push(Prim::Rect {
            a: (left, a.1),
            b: (right, b.1),
            fill: Some(fill),
            stroke: None,
        });
    }
    for y in [a.1, b.1] {
        out.push(seg(
            (left, y),
            (right, y),
            style.color,
            1.0,
            style.width,
            style.dash,
        ));
    }
    let x = f32::midpoint(left, right);
    arrow((x, a.1), (x, b.1), style.color, 1.0, out);
    if style.labels {
        let text = move_text(proj, from_point, to_point, &style.measure);
        out.push(stat(
            style,
            (x, a.1.min(b.1) - 14.0),
            text,
            style.color,
            Anchor::Center,
        ));
    }
}

pub(super) fn date_range(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let rect = proj.plot();
    let (Some([a, b]), Some([from_point, to_point])) = (lead(pts), lead(&drawing.points)) else {
        return;
    };
    let (left, right) = (a.0.min(b.0), a.0.max(b.0).max(a.0.min(b.0) + 1.0));
    if let Some(fill) = fill_of(style) {
        out.push(Prim::Rect {
            a: (left, rect.t),
            b: (right, rect.b),
            fill: Some(fill),
            stroke: None,
        });
    }
    for x in [left, right] {
        out.push(seg(
            (x, rect.t),
            (x, rect.b),
            style.color,
            1.0,
            style.width,
            style.dash,
        ));
    }
    arrow((a.0, a.1), (b.0, a.1), style.color, 1.0, out);
    if style.labels {
        out.push(stat(
            style,
            (f32::midpoint(a.0, b.0), a.1 - 14.0),
            span_text(proj, from_point, to_point, &style.measure),
            style.color,
            Anchor::Center,
        ));
    }
}

pub(super) fn date_price_range(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    out: &mut Vec<Prim>,
) {
    let style = &drawing.style;
    let (Some([a, b]), Some([from_point, to_point])) = (lead(pts), lead(&drawing.points)) else {
        return;
    };
    out.push(Prim::Rect {
        a,
        b,
        fill: fill_of(style),
        stroke: Some((style.color, style.width)),
    });
    let (mx, my) = (f32::midpoint(a.0, b.0), f32::midpoint(a.1, b.1));
    arrow((a.0, my), (b.0, my), style.color, 1.0, out);
    arrow((mx, a.1), (mx, b.1), style.color, 1.0, out);
    if style.labels {
        out.push(stat(
            style,
            (mx, a.1.min(b.1) - 14.0),
            move_text(proj, from_point, to_point, &style.measure),
            style.color,
            Anchor::Center,
        ));
        out.push(stat(
            style,
            (mx, a.1.max(b.1) + 14.0),
            span_text(proj, from_point, to_point, &style.measure),
            style.color,
            Anchor::Center,
        ));
    }
}

pub(super) fn forecast(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (Some([a, b]), Some([from_point, to_point])) = (lead(pts), lead(&drawing.points)) else {
        return;
    };
    out.push(seg(a, b, style.color, 1.0, style.width, style.dash));
    out.extend(arrow_head(a, b, style.color, style.width));
    out.push(Prim::Ellipse {
        center: a,
        rx: 3.5,
        ry: 3.5,
        fill: Some((style.color, 1.0)),
        stroke: None,
    });
    if style.labels {
        let side = if b.0 >= a.0 { 12.0 } else { -12.0 };
        let anchor = if b.0 >= a.0 {
            Anchor::Left
        } else {
            Anchor::Right
        };
        let rows = vec![
            {
                let moved = move_text(proj, from_point, to_point, &style.measure);
                if moved.is_empty() {
                    proj.format_price(to_point.p)
                } else {
                    format!("{} ({moved})", proj.format_price(to_point.p))
                }
            },
            span_text(proj, from_point, to_point, &style.measure),
        ];
        stat_rows(
            style,
            (b.0 + side, b.1 - label_size() * 0.9),
            rows,
            style.color,
            anchor,
            out,
        );
    }
}
