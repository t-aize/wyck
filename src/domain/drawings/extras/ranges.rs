//! Price and date ranges, and forecasts.

use super::{move_text, span_text, stat, stat_rows};

use crate::domain::drawings::geometry::{
    Anchor, P, Prim, Projection, arrow_head, fill_of, label_size, seg,
};
use crate::domain::drawings::model::{Dash, Drawing};

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
    let (a, b) = (pts[0], pts[1]);
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
    let x = (left + right) / 2.0;
    arrow((x, a.1), (x, b.1), style.color, 1.0, out);
    if style.labels {
        let text = move_text(proj, drawing.points[0], drawing.points[1], &style.measure);
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
    let (a, b) = (pts[0], pts[1]);
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
            ((a.0 + b.0) / 2.0, a.1 - 14.0),
            span_text(proj, drawing.points[0], drawing.points[1], &style.measure),
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
    let (a, b) = (pts[0], pts[1]);
    out.push(Prim::Rect {
        a,
        b,
        fill: fill_of(style),
        stroke: Some((style.color, style.width)),
    });
    let (mx, my) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    arrow((a.0, my), (b.0, my), style.color, 1.0, out);
    arrow((mx, a.1), (mx, b.1), style.color, 1.0, out);
    if style.labels {
        let (p, q) = (drawing.points[0], drawing.points[1]);
        out.push(stat(
            style,
            (mx, a.1.min(b.1) - 14.0),
            move_text(proj, p, q, &style.measure),
            style.color,
            Anchor::Center,
        ));
        out.push(stat(
            style,
            (mx, a.1.max(b.1) + 14.0),
            span_text(proj, p, q, &style.measure),
            style.color,
            Anchor::Center,
        ));
    }
}

pub(super) fn forecast(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
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
        let (p, q) = (drawing.points[0], drawing.points[1]);
        let side = if b.0 >= a.0 { 12.0 } else { -12.0 };
        let anchor = if b.0 >= a.0 {
            Anchor::Left
        } else {
            Anchor::Right
        };
        let rows = vec![
            {
                let moved = move_text(proj, p, q, &style.measure);
                if moved.is_empty() {
                    proj.format_price(q.p)
                } else {
                    format!("{} ({moved})", proj.format_price(q.p))
                }
            },
            span_text(proj, p, q, &style.measure),
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
