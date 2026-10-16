//! How the Fibonacci and level tools are drawn.

use super::{Anchor, P, Prim, Projection, Rect, extended, seg};
use crate::look::{LabelSide, LevelText};
use crate::model::style::{Dash, Level, Style};
use crate::model::{Drawing, Point};

pub(super) fn fib_prims(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    price_at: impl Fn(f64) -> f64,
    out: &mut Vec<Prim>,
) {
    let style = &drawing.style;
    let rect = proj.plot();
    let (mut left, mut right) = pts
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(l, r), p| {
            (l.min(p.0), r.max(p.0))
        });
    if style.extend_left {
        left = rect.l;
    }
    if style.extend_right {
        right = rect.r;
    }
    let Some(first_point) = drawing.points.first() else {
        return;
    };
    let mut previous: Option<(f32, u32)> = None;
    for level in visible_levels(drawing) {
        let ratio = if drawing.reverse {
            1.0 - level.value
        } else {
            level.value
        };
        let price = price_at(ratio);
        let Some(at) = proj.to_screen(Point {
            t: first_point.t,
            p: price,
        }) else {
            continue;
        };
        let y = at.1;
        if style.fill
            && let Some((previous_y, _)) = previous
        {
            out.push(Prim::Rect {
                a: (left, previous_y),
                b: (right, y),
                fill: Some((level.color, style.fill_opacity)),
                stroke: None,
            });
        }
        out.push(seg(
            (left, y),
            (right, y),
            level.color,
            0.9,
            level.line_width(style.width),
            level.line_dash(style.dash),
        ));
        if style.labels {
            let text = level_label(style, level.value, Some(&proj.format_price(price)));
            // At the left of the levels, or at the right. Lines that run to the edge of the chart
            // leave no room outside, so the words go inside, just above the line.
            let on_left = style.label_side == LabelSide::Left;
            let edge = if on_left { left } else { right };
            let run_out = if on_left {
                style.extend_left
            } else {
                style.extend_right
            };
            // The label sits inside the line when the line runs out on its own side, outside otherwise.
            let (x, anchor) = if on_left == run_out {
                (edge + 4.0, Anchor::Left)
            } else {
                (edge - 4.0, Anchor::Right)
            };
            let y = if run_out { y - 8.0 } else { y };
            out.push(label((x, y), text, level.color, anchor, style));
        }
        previous = Some((y, level.color));
    }
    // The line that joins the points the levels come from.
    for (&from, &to) in pts.iter().zip(pts.iter().skip(1)) {
        out.push(seg(from, to, style.color, 0.6, 1.0, Dash::Dashed));
    }
}

/// The fill of a drawing, when it has one: its color and opacity.
pub(crate) fn fill_of(style: &Style) -> Option<(u32, f32)> {
    (style.fill && style.fill_opacity > 0.0).then(|| (style.fill_color(), style.fill_opacity))
}

/// The segment `a` to `b` stretched to the plot's edges on the sides the style extends.
pub(crate) fn stretch(a: P, b: P, style: &Style, rect: Rect) -> (P, P) {
    if !(style.extend_left || style.extend_right) {
        return (a, b);
    }
    // Extend towards the left and right of the screen, whichever point is on which side.
    let (first, second, swapped) = if a.0 <= b.0 {
        (a, b, false)
    } else {
        (b, a, true)
    };
    let (p, q) = extended(first, second, style.extend_left, style.extend_right, rect)
        .unwrap_or((first, second));
    if swapped { (q, p) } else { (p, q) }
}

/// The levels of a drawing that show, in increasing order.
pub(crate) fn visible_levels(drawing: &Drawing) -> Vec<Level> {
    let mut levels: Vec<Level> = drawing.levels().into_iter().filter(|l| l.visible).collect();
    levels.sort_by(|a, b| a.value.total_cmp(&b.value));
    levels
}

/// A level's value as the chart writes it: up to three decimals, no trailing zeros.
pub(crate) fn level_text(value: f64) -> String {
    wyck_core::format::trim(value, 3)
}

/// The caption of a level as the style asks for it: `price` is the price of the level, for a tool
/// that knows one.
pub(crate) fn level_label(style: &Style, value: f64, price: Option<&str>) -> String {
    match (style.level_text, price) {
        (LevelText::Auto, Some(price)) => format!("{} ({price})", level_text(value)),
        (LevelText::Percent, _) => format!("{}%", level_text(value * 100.0)),
        (LevelText::Price, Some(price)) => price.to_owned(),
        _ => level_text(value),
    }
}

/// Words a drawing writes, in the size and weight its style asks for.
pub(crate) fn label(at: P, text: String, color: u32, anchor: Anchor, style: &Style) -> Prim {
    Prim::Label {
        at,
        text,
        color,
        background: None,
        anchor,
        size: style.text_size,
        bold: style.bold,
        face: style.face(),
    }
}
