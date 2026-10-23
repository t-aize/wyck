//! Which part of a drawing is under the pointer, and where its handles are.

use super::{
    Anchor, HANDLE_REACH, LINE_REACH, P, Part, Prim, Projection, anchors, board_size,
    distance_to_segment, lead, prims, prims_with,
};
use crate::base::math::{count_f32, f32_of};
use crate::drawings::extras;
use crate::drawings::model::Drawing;
use crate::drawings::model::tool::Tool;

/// The grips of a drawing: where its points are on the screen.
pub fn handles(drawing: &Drawing, proj: &dyn Projection) -> Vec<P> {
    match drawing.tool {
        // A brush has too many points for grips: it is moved as a whole.
        tool if tool.is_freehand() => Vec::new(),
        // The far corner of the square is where the pointer would put it on the grid.
        Tool::GannSquareFixed => anchors(drawing, proj)
            .and_then(|pts| {
                let [first, second] = lead(&pts)?;
                Some(vec![first, extras::fixed_square_corner(first, second)])
            })
            .unwrap_or_default(),
        tool if tool.is_box() => anchors(drawing, proj)
            .and_then(|pts| lead(&pts).map(|[first, second]| box_handles(first, second)))
            .unwrap_or_default(),
        tool if grips_may_leave_shape(tool) => grips_on_shape(drawing, proj),
        _ => anchors(drawing, proj).unwrap_or_default(),
    }
}

/// Tools whose last point only sets a size or an offset, so the shape does not pass through it
/// (the second line of a channel is only drawn between the first two points, for one).
pub(crate) fn grips_may_leave_shape(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::ParallelChannel
            | Tool::FlatTopBottom
            | Tool::FibChannel
            | Tool::RotatedRectangle
            | Tool::FibWedge
            | Tool::RegressionTrend
            | Tool::TimeCycles
    )
}

/// The grips of a tool that has [`grips_may_leave_shape`]: each one sits on a line that is drawn,
/// at the place where taking it and not moving changes nothing. A grip that sat on the raw point
/// would be off the drawing whenever that point is out of the span the lines are drawn over.
fn grips_on_shape(drawing: &Drawing, proj: &dyn Projection) -> Vec<P> {
    let Some(mut grips) = anchors(drawing, proj) else {
        return Vec::new();
    };
    match (drawing.tool, grips.as_mut_slice()) {
        // The second line is the first one moved to pass through the third point, and is drawn
        // between the first two: the grip is where that line is, over the same span.
        (Tool::ParallelChannel | Tool::FibChannel | Tool::FlatTopBottom, [from, to, offset]) => {
            let (low, high) = (from.0.min(to.0), from.0.max(to.0));
            let across = offset.0.clamp(low, high);
            let flat = drawing.tool == Tool::FlatTopBottom || (to.0 - from.0).abs() < 1e-6;
            let height = if flat {
                offset.1
            } else {
                let on_line = |at: f32| from.1 + (to.1 - from.1) * (at - from.0) / (to.0 - from.0);
                on_line(across) + (offset.1 - on_line(offset.0))
            };
            *offset = (across, height);
        }
        // The far side stands as far from the first as the third point is: its grip is the far
        // corner on the second point's side.
        (Tool::RotatedRectangle, [from, to, height_point]) => {
            let side = (to.0 - from.0, to.1 - from.1);
            let run = side.0.hypot(side.1);
            if run >= 1e-3 {
                let normal = (-side.1 / run, side.0 / run);
                let height =
                    (height_point.0 - from.0) * normal.0 + (height_point.1 - from.1) * normal.1;
                *height_point = (to.0 + normal.0 * height, to.1 + normal.1 * height);
            }
        }
        // The third point only gives the direction of the second ray: its grip is the end of it.
        (Tool::FibWedge, [center, edge, direction]) => {
            let widest = f32_of(
                drawing
                    .levels()
                    .iter()
                    .filter(|level| level.visible)
                    .map(|level| level.value)
                    .fold(0.0, f64::max),
            );
            let base = (edge.0 - center.0).hypot(edge.1 - center.1);
            let toward = (direction.1 - center.1).atan2(direction.0 - center.0);
            if base >= 2.0 && widest > 0.0 {
                *direction = (
                    center.0 + base * widest * toward.cos(),
                    center.1 + base * widest * toward.sin(),
                );
            }
        }
        // The line runs through the closes between the two times: each grip is the end of it
        // nearest to its time. With the line extended its ends are the edges of the plot, which
        // are no place for a grip, so the points stay where they are.
        (Tool::RegressionTrend, [first, second])
            if !(drawing.style.extend_left || drawing.style.extend_right) =>
        {
            let line = prims(drawing, proj)
                .into_iter()
                .rev()
                .find_map(|shape| match shape {
                    Prim::Segment { a, b, .. } => Some((a, b)),
                    _ => None,
                });
            if let Some((from, to)) = line {
                for grip in [first, second] {
                    *grip = if (grip.0 - from.0).abs() <= (grip.0 - to.0).abs() {
                        from
                    } else {
                        to
                    };
                }
            }
        }
        // The arches stand on the height of the first point, whatever the second one says.
        (Tool::TimeCycles, [first, second]) => second.1 = first.1,
        _ => {}
    }
    grips
}

/// The grips of a shape drawn in the box between two opposite corners: those two, the other two
/// corners, then the middle of each side (the side through the first corner's height, the second
/// corner's height, the first corner's time, the second's).
pub(crate) fn box_handles(a: P, b: P) -> Vec<P> {
    let (mx, my) = (f32::midpoint(a.0, b.0), f32::midpoint(a.1, b.1));
    vec![
        a,
        b,
        (a.0, b.1),
        (b.0, a.1),
        (mx, a.1),
        (mx, b.1),
        (a.0, my),
        (b.0, my),
    ]
}

pub(super) fn label_box(at: P, text: &str, anchor: Anchor, size: f32) -> (P, P) {
    let width = count_f32(text.chars().count()) * size * 0.6 + 14.0;
    let half = size * 0.8;
    let (l, r) = match anchor {
        Anchor::Left => (at.0, at.0 + width),
        Anchor::Right => (at.0 - width, at.0),
        Anchor::Center => (at.0 - width / 2.0, at.0 + width / 2.0),
    };
    ((l, at.1 - half), (r, at.1 + half))
}

pub(super) fn inside_rect(p: P, a: P, b: P) -> bool {
    p.0 >= a.0.min(b.0) && p.0 <= a.0.max(b.0) && p.1 >= a.1.min(b.1) && p.1 <= a.1.max(b.1)
}

/// What part of the drawing a screen position is on, if any. Grips win over the body, so a
/// drawing can be picked up by its ends even where lines cross.
pub(crate) fn hit(
    drawing: &Drawing,
    proj: &dyn Projection,
    at: P,
    with_handles: bool,
) -> Option<Part> {
    if with_handles && !drawing.locked {
        let grips = handles(drawing, proj);
        let mut best: Option<(usize, f32)> = None;
        for (index, grip) in grips.iter().enumerate() {
            let distance = ((at.0 - grip.0).powi(2) + (at.1 - grip.1).powi(2)).sqrt();
            if distance <= HANDLE_REACH && best.is_none_or(|(_, d)| distance < d) {
                best = Some((index, distance));
            }
        }
        if let Some((index, _)) = best {
            return Some(Part::Handle(index));
        }
    }
    if drawing.tool.is_position() {
        // The tags show while the pointer is over the position, so they are part of it: a tag
        // that let the pointer through would lose the hover, and itself with it. A tag beside
        // the position (see `position_prims`) is not: it is only there once the pointer is.
        let pts = anchors(drawing, proj)?;
        let &[p0, p1, p2, p3] = pts.as_slice() else {
            return None;
        };
        let (left, right) = (p0.0, p3.0.max(p0.0 + 1.0));
        let over_tag = prims_with(drawing, proj, true).iter().any(|prim| {
            if let Prim::Label {
                at: position,
                text,
                anchor,
                size,
                ..
            } = prim
            {
                let (a, b) = label_box(*position, text, *anchor, *size);
                b.0 >= left && a.0 <= right && inside_rect(at, a, b)
            } else {
                false
            }
        });
        if over_tag {
            return Some(Part::Body);
        }
        let cfg = &drawing.style.position;
        // A zone that is not painted is not a thing to pick up either; and the user can ask for
        // the zones to let a press through to the chart, leaving the lines and the edges.
        let painted = drawing.style.fill
            && [cfg.target_opacity, cfg.stop_opacity]
                .into_iter()
                .any(|own| own.unwrap_or(drawing.style.fill_opacity) > 0.0);
        if cfg.grab_zones && painted {
            return (inside_rect(at, (left, p0.1), (right, p1.1))
                || inside_rect(at, (left, p0.1), (right, p2.1)))
            .then_some(Part::Body);
        }
        let reach = LINE_REACH + drawing.style.width / 2.0;
        let (top, bottom) = (p1.1.min(p2.1).min(p0.1), p1.1.max(p2.1).max(p0.1));
        let lines = [
            ((left, p0.1), (right, p0.1)),
            ((left, p1.1), (right, p1.1)),
            ((left, p2.1), (right, p2.1)),
            ((left, top), (left, bottom)),
            ((right, top), (right, bottom)),
        ];
        return lines
            .iter()
            .any(|(a, b)| distance_to_segment(at, *a, *b) <= reach)
            .then_some(Part::Body);
    }
    let reach = LINE_REACH + drawing.style.width / 2.0;
    for prim in prims(drawing, proj) {
        let on = match &prim {
            Prim::Segment { a, b, .. } => distance_to_segment(at, *a, *b) <= reach,
            Prim::Rect { a, b, fill, stroke } => {
                let edges = [
                    (*a, (b.0, a.1)),
                    ((b.0, a.1), *b),
                    (*b, (a.0, b.1)),
                    ((a.0, b.1), *a),
                ];
                (stroke.is_some()
                    && edges
                        .iter()
                        .any(|(p, q)| distance_to_segment(at, *p, *q) <= reach))
                    || (fill.is_some_and(|(_, alpha)| alpha > 0.1) && inside_rect(at, *a, *b))
            }
            Prim::Ellipse {
                center,
                rx,
                ry,
                fill,
                ..
            } => {
                if *rx < 1.0 || *ry < 1.0 {
                    false
                } else {
                    let (nx, ny) = ((at.0 - center.0) / rx, (at.1 - center.1) / ry);
                    let radius = (nx * nx + ny * ny).sqrt();
                    let edge = (reach / rx.min(*ry)).max(0.05);
                    (radius - 1.0).abs() <= edge || (fill.is_some() && radius < 1.0)
                }
            }
            Prim::Polygon { points, .. } => points.len() >= 3 && point_in_polygon(at, points),
            Prim::Polyline { points, .. } => points.windows(2).any(|pair| {
                let &[from, to] = pair else {
                    return false;
                };
                distance_to_segment(at, from, to) <= reach
            }),
            Prim::Label {
                at: position,
                text,
                anchor,
                size,
                ..
            } => {
                let (a, b) = label_box(*position, text, *anchor, *size);
                inside_rect(at, a, b)
            }
            Prim::FittedLabel { bounds, .. } => {
                inside_rect(at, (bounds.l, bounds.t), (bounds.r, bounds.b))
            }
            Prim::Board { tl, rows, size, .. } => {
                let (w, h) = board_size(rows, *size);
                inside_rect(at, *tl, (tl.0 + w, tl.1 + h))
            }
            Prim::Handle { .. } => false,
        };
        // The dashed guide lines of a fib or a channel are not part of what is clickable, but
        // they are segments like any other; they sit inside the levels' boxes anyway.
        if on {
            return Some(Part::Body);
        }
    }
    None
}

pub(super) fn point_in_polygon(p: P, polygon: &[P]) -> bool {
    let mut inside = false;
    let Some(&last) = polygon.last() else {
        return false;
    };
    let mut previous = last;
    for &current in polygon {
        let (a, b) = (current, previous);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        previous = current;
    }
    inside
}
