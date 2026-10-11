//! Which part of a drawing is under the pointer, and where its handles are.

use super::{
    Anchor, HANDLE_REACH, LINE_REACH, P, Part, Prim, Projection, anchors, board_size,
    distance_to_segment, prims, prims_with,
};
use crate::domain::drawings::extras;
use crate::domain::drawings::model::{Drawing, Tool};

/// The grips of a drawing: where its points are on the screen.
pub fn handles(drawing: &Drawing, proj: &dyn Projection) -> Vec<P> {
    match drawing.tool {
        // A brush has too many points for grips: it is moved as a whole.
        tool if tool.is_freehand() => Vec::new(),
        // The far corner of the square is where the pointer would put it on the grid.
        Tool::GannSquareFixed => anchors(drawing, proj)
            .map(|pts| vec![pts[0], extras::fixed_square_corner(pts[0], pts[1])])
            .unwrap_or_default(),
        tool if tool.is_box() => anchors(drawing, proj)
            .map(|pts| box_handles(pts[0], pts[1]))
            .unwrap_or_default(),
        _ => anchors(drawing, proj).unwrap_or_default(),
    }
}

/// The grips of a shape drawn in the box between two opposite corners: those two, the other two
/// corners, then the middle of each side (the side through the first corner's height, the second
/// corner's height, the first corner's time, the second's).
pub fn box_handles(a: P, b: P) -> Vec<P> {
    let (mx, my) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
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
    let width = text.chars().count() as f32 * size * 0.6 + 14.0;
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
pub fn hit(drawing: &Drawing, proj: &dyn Projection, at: P, with_handles: bool) -> Option<Part> {
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
        // Position tags are information, not handles for moving the trade. The same rule applies
        // when a tag overlaps a zone. The four grips above remain available when selected.
        let over_tag = prims_with(
            drawing,
            proj,
            with_handles || drawing.style.position.always_stats,
        )
        .iter()
        .any(|prim| {
            if let Prim::Label {
                at: position,
                text,
                anchor,
                size,
                ..
            } = prim
            {
                let (a, b) = label_box(*position, text, *anchor, *size);
                inside_rect(at, a, b)
            } else {
                false
            }
        });
        if over_tag {
            return None;
        }
        let pts = anchors(drawing, proj)?;
        if pts.len() != 4 {
            return None;
        }
        let left = pts[0].0;
        let right = pts[3].0.max(left + 1.0);
        return (inside_rect(at, (left, pts[0].1), (right, pts[1].1))
            || inside_rect(at, (left, pts[0].1), (right, pts[2].1)))
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
            Prim::Polyline { points, .. } => points
                .windows(2)
                .any(|pair| distance_to_segment(at, pair[0], pair[1]) <= reach),
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
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}
