//! Information lines and angles, regression, channels and pitchfans.

use super::{
    add, dist, left_to_right, mid, move_text, polyline, sample, scale, slope_degrees, span_text,
    stat, stat_rows, sub,
};

use crate::domain::drawings::figures;
use crate::domain::drawings::geometry::{
    Anchor, P, Prim, Projection, extended, fill_of, label, label_size, level_label, level_text,
    seg, stretch, visible_levels,
};
use crate::domain::drawings::model::{Dash, Drawing};

pub(super) fn info_line(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
    if let Some((from, to)) = extended(a, b, style.extend_left, style.extend_right, proj.plot()) {
        out.push(seg(from, to, style.color, 1.0, style.width, style.dash));
    }
    if style.labels {
        let (p, q) = (drawing.points[0], drawing.points[1]);
        let rows = vec![
            move_text(proj, p, q, &style.measure),
            span_text(proj, p, q, &style.measure),
            format!("Angle {:.1}\u{b0}", slope_degrees(a, b)),
        ];
        let at = (b.0 + 10.0, b.1 - label_size() * 1.8);
        stat_rows(style, at, rows, style.color, Anchor::Left, out);
    }
}

pub(super) fn trend_angle(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    out: &mut Vec<Prim>,
) {
    let style = &drawing.style;
    let (a, b) = (pts[0], pts[1]);
    if let Some((from, to)) = extended(a, b, style.extend_left, style.extend_right, proj.plot()) {
        out.push(seg(from, to, style.color, 1.0, style.width, style.dash));
    }
    let run = dist(a, b);
    if run < 2.0 {
        return;
    }
    let side = if b.0 >= a.0 { 1.0 } else { -1.0 };
    out.push(seg(
        a,
        (a.0 + side * run, a.1),
        style.color,
        0.6,
        1.0,
        Dash::Dashed,
    ));
    if !style.labels || !style.measure.angle {
        return;
    }
    let phi = slope_degrees(a, b).to_radians();
    let radius = (run * 0.35).clamp(12.0, 60.0);
    let arc = sample(24, |t| {
        let angle = phi * t;
        (
            a.0 + side * radius * angle.cos(),
            a.1 - radius * angle.sin(),
        )
    });
    out.push(polyline(arc, style.color, 1.0, 0.9));
    let half = phi / 2.0;
    let at = (
        a.0 + side * (radius + 16.0) * half.cos(),
        a.1 - (radius + 16.0) * half.sin(),
    );
    out.push(stat(
        style,
        at,
        format!("{:.1}\u{b0}", slope_degrees(a, b)),
        style.color,
        Anchor::Center,
    ));
}

pub(super) fn regression(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (t0, t1) = (
        drawing.points[0].t.min(drawing.points[1].t),
        drawing.points[0].t.max(drawing.points[1].t),
    );
    let bars = proj.bars_between(t0, t1);
    let n = bars.len();
    if n < 2 {
        out.push(seg(pts[0], pts[1], style.color, 0.6, 1.0, Dash::Dashed));
        return;
    }
    let count = n as f64;
    let (mut sum_x, mut sum_xx, mut sum_y, mut sum_xy) = (0.0, 0.0, 0.0, 0.0);
    for (k, bar) in bars.iter().enumerate() {
        let x = k as f64;
        sum_x += x;
        sum_xx += x * x;
        sum_y += bar.close;
        sum_xy += x * bar.close;
    }
    let denominator = count * sum_xx - sum_x * sum_x;
    let slope = if denominator.abs() < 1e-12 {
        0.0
    } else {
        (count * sum_xy - sum_x * sum_y) / denominator
    };
    let intercept = (sum_y - slope * sum_x) / count;
    let variance = bars
        .iter()
        .enumerate()
        .map(|(k, bar)| (bar.close - (intercept + slope * k as f64)).powi(2))
        .sum::<f64>()
        / count;
    let deviation = variance.sqrt();

    let rect = proj.plot();
    let (first, last) = (&bars[0], &bars[n - 1]);
    let line_at = |offset: f64| -> Option<(P, P)> {
        let from = (first.x, proj.y_of(intercept + offset));
        let to = (
            last.x,
            proj.y_of(intercept + slope * (count - 1.0) + offset),
        );
        extended(from, to, style.extend_left, style.extend_right, rect)
    };
    let levels = visible_levels(drawing);
    let widest = levels.iter().map(|l| l.value).fold(0.0, f64::max);
    if widest > 0.0
        && let (Some((a0, a1)), Some((b0, b1)), Some((_, opacity))) = (
            line_at(widest * deviation),
            line_at(-widest * deviation),
            fill_of(style),
        )
    {
        out.push(Prim::Polygon {
            points: vec![a0, a1, b1, b0],
            fill: (style.fill_color(), opacity),
        });
    }
    for level in &levels {
        for sign in [1.0, -1.0] {
            if let Some((from, to)) = line_at(sign * level.value * deviation) {
                out.push(seg(
                    from,
                    to,
                    level.color,
                    0.9,
                    level.line_width(style.width),
                    level.line_dash(style.dash),
                ));
                if style.labels && sign > 0.0 {
                    out.push(label(
                        (to.0 + 4.0, to.1),
                        level_label(style, level.value, None),
                        level.color,
                        Anchor::Left,
                        style,
                    ));
                }
            }
        }
    }
    if let Some((from, to)) = line_at(0.0) {
        out.push(seg(from, to, style.color, 1.0, style.width, style.dash));
    }
}

pub(super) fn flat_top_bottom(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    out: &mut Vec<Prim>,
) {
    let style = &drawing.style;
    let (a, b, c) = (pts[0], pts[1], pts[2]);
    let (from, to) = stretch(a, b, style, proj.plot());
    let flat = ((from.0, c.1), (to.0, c.1));
    if let Some(fill) = fill_of(style) {
        out.push(Prim::Polygon {
            points: vec![from, to, flat.1, flat.0],
            fill,
        });
    }
    out.push(seg(from, to, style.color, 1.0, style.width, style.dash));
    out.push(seg(
        flat.0,
        flat.1,
        style.color,
        1.0,
        style.width,
        style.dash,
    ));
}

pub(super) fn disjoint_channel(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    out: &mut Vec<Prim>,
) {
    let style = &drawing.style;
    let rect = proj.plot();
    let (a, b) = left_to_right(pts[0], pts[1]);
    let (c, d) = left_to_right(pts[2], pts[3]);
    let (a, b) = stretch(a, b, style, rect);
    let (c, d) = stretch(c, d, style, rect);
    if let Some(fill) = fill_of(style) {
        out.push(Prim::Polygon {
            points: vec![a, b, d, c],
            fill,
        });
    }
    out.push(seg(a, b, style.color, 1.0, style.width, style.dash));
    out.push(seg(c, d, style.color, 1.0, style.width, style.dash));
    if style.middle {
        out.push(seg(
            mid(a, c),
            mid(b, d),
            style.color,
            0.6,
            1.0,
            Dash::Dashed,
        ));
    }
}

pub(super) fn pitchfan(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let (a, b, c) = (pts[0], pts[1], pts[2]);
    let through: Vec<(P, u32, String)> = visible_levels(drawing)
        .into_iter()
        .map(|l| {
            let p = add(b, scale(sub(c, b), l.value as f32));
            (p, l.color, level_text(l.value))
        })
        .collect();
    figures::fan(drawing, a, &through, proj.plot(), out);
    out.push(seg(b, c, drawing.style.color, 0.6, 1.0, Dash::Dashed));
}
