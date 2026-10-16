//! The shapes of each tool, drawn from its points.

use super::fib::{fib_prims, fill_of, stretch};
use super::measure::{measure_prims, position_prims};
use super::{Anchor, Prim, Projection, anchors, arrow_head, extended, label_size, lead, seg};
use crate::decor;
use crate::extras;
use crate::figures;
use crate::look::Face;
use crate::model::Drawing;
use crate::model::style::Dash;
use crate::model::tool::Tool;
use wyck_core::math::count_f32;

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
            let Some([a, b]) = lead(&pts) else {
                return out;
            };
            if let Some((a, b)) = extended(a, b, style.extend_left, style.extend_right, rect) {
                out.push(seg(a, b, color, 1.0, width, dash));
            }
        }
        Tool::Ray => {
            let Some([a, b]) = lead(&pts) else {
                return out;
            };
            if let Some((a, b)) = extended(a, b, style.extend_left, true, rect) {
                out.push(seg(a, b, color, 1.0, width, dash));
            }
        }
        Tool::ExtendedLine => {
            let Some([a, b]) = lead(&pts) else {
                return out;
            };
            if let Some((a, b)) = extended(a, b, true, true, rect) {
                out.push(seg(a, b, color, 1.0, width, dash));
            }
        }
        Tool::HorizontalLine => {
            let (Some(&(_, y)), Some(point)) = (pts.first(), drawing.points.first()) else {
                return out;
            };
            out.push(seg((rect.l, y), (rect.r, y), color, 1.0, width, dash));
            if style.labels {
                out.push(Prim::Label {
                    at: (rect.r - 6.0, y),
                    text: proj.format_price(point.p),
                    color: 0x0a_0a0a,
                    background: Some((color, 1.0)),
                    anchor: Anchor::Right,
                    size: label_size(),
                    bold: false,
                    face: Face::default(),
                });
            }
        }
        Tool::HorizontalRay => {
            let Some(&(x, y)) = pts.first() else {
                return out;
            };
            out.push(seg((x, y), (rect.r, y), color, 1.0, width, dash));
        }
        Tool::VerticalLine => {
            let Some(&(x, _)) = pts.first() else {
                return out;
            };
            out.push(seg((x, rect.t), (x, rect.b), color, 1.0, width, dash));
        }
        Tool::CrossLine => {
            let Some(&(x, y)) = pts.first() else {
                return out;
            };
            out.push(seg((rect.l, y), (rect.r, y), color, 1.0, width, dash));
            out.push(seg((x, rect.t), (x, rect.b), color, 1.0, width, dash));
        }
        Tool::Arrow => {
            let Some([a, b]) = lead(&pts) else {
                return out;
            };
            let start = extended(a, b, style.extend_left, false, rect).map_or(a, |(s, _)| s);
            out.push(seg(start, b, color, 1.0, width, dash));
            out.extend(arrow_head(a, b, color, width));
        }
        Tool::ArrowPath => {
            for pair in pts.windows(2) {
                if let &[from, to] = pair {
                    out.push(seg(from, to, color, 1.0, width, dash));
                }
            }
            // The head follows the last segment that has a length, so it stays put while the
            // pointer sits on the last point placed.
            let Some(&last) = pts.last() else {
                return out;
            };
            let from = pts.iter().rev().find(|p| {
                let (dx, dy) = (last.0 - p.0, last.1 - p.1);
                dx * dx + dy * dy > 1.0
            });
            if let Some(from) = from {
                out.extend(arrow_head(*from, last, color, width));
            }
        }
        Tool::ParallelChannel => {
            let Some([a, b, c]) = lead(&pts) else {
                return out;
            };
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
                    (f32::midpoint(a.0, a2.0), f32::midpoint(a.1, a2.1)),
                    (f32::midpoint(b.0, b2.0), f32::midpoint(b.1, b2.1)),
                );
                out.push(seg(m1, m2, color, 0.6, 1.0, Dash::Dashed));
            }
        }
        Tool::FibRetracement => {
            let Some([first, second]) = lead(&drawing.points) else {
                return out;
            };
            fib_prims(
                drawing,
                &pts,
                proj,
                |ratio| {
                    // 0 at the second point, 1 at the first.
                    second.p + (first.p - second.p) * ratio
                },
                &mut out,
            );
        }
        Tool::FibExtension => {
            let Some([first, second, third]) = lead(&drawing.points) else {
                return out;
            };
            fib_prims(
                drawing,
                &pts,
                proj,
                |ratio| {
                    // The move from the first to the second point, repeated from the third.
                    third.p + (second.p - first.p) * ratio
                },
                &mut out,
            );
        }
        Tool::Rectangle => {
            let Some([(x0, y0), (x1, y1)]) = lead(&pts) else {
                return out;
            };
            let (mut l, mut r) = (x0.min(x1), x0.max(x1));
            if style.extend_left {
                l = l.min(rect.l);
            }
            if style.extend_right {
                r = r.max(rect.r);
            }
            let (t, b) = (y0.min(y1), y0.max(y1));
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
        Tool::Ellipse => {
            let Some([(x0, y0), (x1, y1)]) = lead(&pts) else {
                return out;
            };
            out.push(Prim::Ellipse {
                center: (f32::midpoint(x0, x1), f32::midpoint(y0, y1)),
                rx: (x1 - x0).abs() / 2.0,
                ry: (y1 - y0).abs() / 2.0,
                fill,
                stroke: Some((color, width)),
            });
        }
        Tool::Triangle => {
            if let Some(fill) = fill {
                out.push(Prim::Polygon {
                    points: pts.clone(),
                    fill,
                });
            }
            let Some([a, b, c]) = lead(&pts) else {
                return out;
            };
            for (from, to) in [(a, b), (b, c), (c, a)] {
                out.push(seg(from, to, color, 1.0, width, dash));
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
            let Some(&(x, y)) = pts.first() else {
                return out;
            };
            let text = if drawing.text.is_empty() {
                "Text"
            } else {
                drawing.text.as_str()
            };
            // Each line of the words is its own label, one under the other.
            for (row, line) in text.lines().enumerate() {
                out.push(Prim::Label {
                    at: (x + 6.0, y + count_f32(row) * style.text_size * 1.4),
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
        Tool::PriceLabel => {
            let (Some(&at), Some(point)) = (pts.first(), drawing.points.first()) else {
                return out;
            };
            out.push(Prim::Label {
                at,
                text: proj.format_price(point.p),
                color: style.text_color.unwrap_or(0x0a_0a0a),
                background: Some((color, 1.0)),
                anchor: Anchor::Left,
                size: style.text_size,
                bold: style.bold,
                face: style.face(),
            });
        }
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
