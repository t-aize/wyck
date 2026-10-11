//! The shapes of each tool, drawn from its points.

use super::{
    Anchor, Prim, Projection, anchors, arrow_head, extended, fib_prims, fill_of, label_size,
    measure_prims, position_prims, seg, stretch,
};
use crate::domain::drawings::decor;
use crate::domain::drawings::extras;
use crate::domain::drawings::figures;
use crate::domain::drawings::model::{Dash, Drawing, Tool};

pub(super) fn shapes_of(drawing: &Drawing, proj: &dyn Projection, selected: bool) -> Vec<Prim> {
    let Some(pts) = anchors(drawing, proj) else {
        return Vec::new();
    };
    let rect = proj.plot();
    let style = &drawing.style;
    let (color, width, dash) = (style.color, style.width, style.dash);
    let fill = fill_of(style);
    let mut out = Vec::new();

    match drawing.tool {
        Tool::TrendLine => {
            if let Some((a, b)) =
                extended(pts[0], pts[1], style.extend_left, style.extend_right, rect)
            {
                out.push(seg(a, b, color, 1.0, width, dash));
            }
        }
        Tool::Ray => {
            if let Some((a, b)) = extended(pts[0], pts[1], style.extend_left, true, rect) {
                out.push(seg(a, b, color, 1.0, width, dash));
            }
        }
        Tool::ExtendedLine => {
            if let Some((a, b)) = extended(pts[0], pts[1], true, true, rect) {
                out.push(seg(a, b, color, 1.0, width, dash));
            }
        }
        Tool::HorizontalLine => {
            let y = pts[0].1;
            out.push(seg((rect.l, y), (rect.r, y), color, 1.0, width, dash));
            if style.labels {
                out.push(Prim::Label {
                    at: (rect.r - 6.0, y),
                    text: proj.format_price(drawing.points[0].p),
                    color: 0x0a0a0a,
                    background: Some((color, 1.0)),
                    anchor: Anchor::Right,
                    size: label_size(),
                    bold: false,
                    face: Default::default(),
                });
            }
        }
        Tool::HorizontalRay => {
            let y = pts[0].1;
            out.push(seg((pts[0].0, y), (rect.r, y), color, 1.0, width, dash));
        }
        Tool::VerticalLine => {
            let x = pts[0].0;
            out.push(seg((x, rect.t), (x, rect.b), color, 1.0, width, dash));
        }
        Tool::CrossLine => {
            let (x, y) = pts[0];
            out.push(seg((rect.l, y), (rect.r, y), color, 1.0, width, dash));
            out.push(seg((x, rect.t), (x, rect.b), color, 1.0, width, dash));
        }
        Tool::Arrow => {
            let (a, b) = (pts[0], pts[1]);
            let start = extended(a, b, style.extend_left, false, rect).map_or(a, |(s, _)| s);
            out.push(seg(start, b, color, 1.0, width, dash));
            out.extend(arrow_head(a, b, color, width));
        }
        Tool::ArrowPath => {
            for pair in pts.windows(2) {
                out.push(seg(pair[0], pair[1], color, 1.0, width, dash));
            }
            // The head follows the last segment that has a length, so it stays put while the
            // pointer sits on the last point placed.
            let last = pts[pts.len() - 1];
            let from = pts.iter().rev().find(|p| {
                let (dx, dy) = (last.0 - p.0, last.1 - p.1);
                dx * dx + dy * dy > 1.0
            });
            if let Some(from) = from {
                out.extend(arrow_head(*from, last, color, width));
            }
        }
        Tool::ParallelChannel => {
            let (a, b, c) = (pts[0], pts[1], pts[2]);
            // The second line is the first one moved vertically to pass through the third point.
            let dx = b.0 - a.0;
            let line_y_at_c = if dx.abs() < 1e-6 {
                a.1
            } else {
                a.1 + (b.1 - a.1) * (c.0 - a.0) / dx
            };
            let offset = c.1 - line_y_at_c;
            let (a, b) = stretch(a, b, style, rect);
            let (a2, b2) = ((a.0, a.1 + offset), (b.0, b.1 + offset));
            if let Some(fill) = fill {
                out.push(Prim::Polygon {
                    points: vec![a, b, b2, a2],
                    fill,
                });
            }
            out.push(seg(a, b, color, 1.0, width, dash));
            out.push(seg(a2, b2, color, 1.0, width, dash));
            if style.middle {
                let (m1, m2) = (
                    ((a.0 + a2.0) / 2.0, (a.1 + a2.1) / 2.0),
                    ((b.0 + b2.0) / 2.0, (b.1 + b2.1) / 2.0),
                );
                out.push(seg(m1, m2, color, 0.6, 1.0, Dash::Dashed));
            }
        }
        Tool::FibRetracement => fib_prims(
            drawing,
            &pts,
            proj,
            |ratio| {
                // 0 at the second point, 1 at the first.
                drawing.points[1].p + (drawing.points[0].p - drawing.points[1].p) * ratio
            },
            &mut out,
        ),
        Tool::FibExtension => fib_prims(
            drawing,
            &pts,
            proj,
            |ratio| {
                // The move from the first to the second point, repeated from the third.
                drawing.points[2].p + (drawing.points[1].p - drawing.points[0].p) * ratio
            },
            &mut out,
        ),
        Tool::Rectangle => {
            let (mut l, mut r) = (pts[0].0.min(pts[1].0), pts[0].0.max(pts[1].0));
            if style.extend_left {
                l = l.min(rect.l);
            }
            if style.extend_right {
                r = r.max(rect.r);
            }
            let (t, b) = (pts[0].1.min(pts[1].1), pts[0].1.max(pts[1].1));
            if dash == Dash::Solid {
                out.push(Prim::Rect {
                    a: (l, t),
                    b: (r, b),
                    fill,
                    stroke: Some((color, width)),
                });
            } else {
                if fill.is_some() {
                    out.push(Prim::Rect {
                        a: (l, t),
                        b: (r, b),
                        fill,
                        stroke: None,
                    });
                }
                for (p, q) in [
                    ((l, t), (r, t)),
                    ((r, t), (r, b)),
                    ((r, b), (l, b)),
                    ((l, b), (l, t)),
                ] {
                    out.push(seg(p, q, color, 1.0, width, dash));
                }
            }
        }
        Tool::Ellipse => out.push(Prim::Ellipse {
            center: ((pts[0].0 + pts[1].0) / 2.0, (pts[0].1 + pts[1].1) / 2.0),
            rx: (pts[1].0 - pts[0].0).abs() / 2.0,
            ry: (pts[1].1 - pts[0].1).abs() / 2.0,
            fill,
            stroke: Some((color, width)),
        }),
        Tool::Triangle => {
            if let Some(fill) = fill {
                out.push(Prim::Polygon {
                    points: pts.clone(),
                    fill,
                });
            }
            for i in 0..3 {
                out.push(seg(pts[i], pts[(i + 1) % 3], color, 1.0, width, dash));
            }
        }
        // A stroke has no ends to cap and nowhere for words: its points are handed over whole.
        Tool::Brush | Tool::Highlighter => {
            out.push(Prim::Polyline {
                points: pts,
                color,
                width,
                alpha: if drawing.tool == Tool::Highlighter {
                    HIGHLIGHT_ALPHA
                } else {
                    1.0
                },
            });
            fade_lines(&mut out, style.opacity);
            return out;
        }
        Tool::Measure => measure_prims(drawing, &pts, proj, &mut out),
        Tool::LongPosition | Tool::ShortPosition => {
            position_prims(drawing, &pts, proj, selected, &mut out);
        }
        Tool::Text => {
            let text = if drawing.text.is_empty() {
                "Text"
            } else {
                drawing.text.as_str()
            };
            // Each line of the words is its own label, one under the other.
            for (row, line) in text.lines().enumerate() {
                out.push(Prim::Label {
                    at: (
                        pts[0].0 + 6.0,
                        pts[0].1 + row as f32 * style.text_size * 1.4,
                    ),
                    text: line.to_owned(),
                    color: style.text_color(),
                    background: None,
                    anchor: Anchor::Left,
                    size: style.text_size,
                    bold: style.bold,
                    face: style.face(),
                });
            }
        }
        Tool::PriceLabel => out.push(Prim::Label {
            at: pts[0],
            text: proj.format_price(drawing.points[0].p),
            color: style.text_color.unwrap_or(0x0a0a0a),
            background: Some((color, 1.0)),
            anchor: Anchor::Left,
            size: style.text_size,
            bold: style.bold,
            face: style.face(),
        }),
        Tool::Unknown => {}
        _ => {
            figures::prims(drawing, &pts, proj, &mut out);
            extras::prims(drawing, &pts, proj, &mut out);
        }
    }
    decor::decorate(drawing, &pts, rect, &mut out);
    // A tag with nothing to say (every part of a measure switched off) is not drawn.
    out.retain(|prim| !matches!(prim, Prim::Label { text, .. } if text.is_empty()));
    fade_lines(&mut out, style.opacity);
    // A copy of bars is all fills: the opacity fades those too.
    if matches!(drawing.tool, Tool::BarsPattern | Tool::GhostFeed) && style.opacity < 1.0 {
        for prim in &mut out {
            if let Prim::Rect {
                fill: Some((_, alpha)),
                ..
            }
            | Prim::Polygon {
                fill: (_, alpha), ..
            } = prim
            {
                *alpha *= style.opacity;
            }
        }
    }
    out
}

/// Fades the lines of a drawing to the opacity of its style. Fills and words keep their own.
pub(super) fn fade_lines(out: &mut [Prim], opacity: f32) {
    if opacity >= 1.0 {
        return;
    }
    for prim in out {
        match prim {
            Prim::Segment { alpha, .. } | Prim::Polyline { alpha, .. } => *alpha *= opacity,
            Prim::Rect { stroke, .. } | Prim::Ellipse { stroke, .. } => {
                if let Some((_, alpha)) = stroke {
                    *alpha *= opacity;
                }
            }
            _ => {}
        }
    }
}

/// How opaque the stroke of a highlighter is.
pub(super) const HIGHLIGHT_ALPHA: f32 = 0.35;
