//! What is put on a drawing after its own tool has drawn it: the words it carries, and what ends
//! its lines.
//!
//! Neither belongs to one tool. A trend line, a rectangle and a channel can all carry a label, and
//! any line can end in an arrow or a dot, so they are done here, once, from the style.

use super::geometry::{Anchor, P, Prim, Rect, arrow_head};
use super::look::{Cap, HAlign, TextLayout, VAlign};
use super::model::{Drawing, Tool};

/// How far the words sit from the end of a line or the edge of a shape.
const EDGE: f32 = 6.0;
/// The space between the words and the line they are above or below.
const GAP: f32 = 4.0;
/// The color of the tag behind the words when the style names none.
const TAG: u32 = 0x1b1d24;

/// Adds the caps and the label of `drawing`. `pts` are its points on the screen.
pub(super) fn decorate(drawing: &Drawing, pts: &[P], plot: Rect, out: &mut Vec<Prim>) {
    caps(drawing, pts, out);
    label(drawing, pts, plot, out);
}

// ---- what ends a line ----

fn caps(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let (tool, style) = (drawing.tool, &drawing.style);
    if pts.len() < 2 {
        return;
    }
    let width = style.width;
    if tool.has_start_cap() {
        push_cap(out, style.caps.start, pts[0], pts[1], style.color, width);
    }
    if tool.has_end_cap() {
        let last = pts.len() - 1;
        push_cap(
            out,
            style.caps.end,
            pts[last],
            pts[last - 1],
            style.color,
            width,
        );
    }
}

/// A cap at `at`, on a line that comes from `toward`.
fn push_cap(out: &mut Vec<Prim>, cap: Cap, at: P, toward: P, color: u32, width: f32) {
    match cap {
        Cap::None => {}
        Cap::Arrow => out.extend(arrow_head(toward, at, color, width)),
        Cap::Circle => {
            let radius = 3.0 + width * 1.2;
            out.push(Prim::Ellipse {
                center: at,
                rx: radius,
                ry: radius,
                fill: Some((color, 1.0)),
                stroke: None,
            });
        }
    }
}

// ---- the words ----

fn label(drawing: &Drawing, pts: &[P], plot: Rect, out: &mut Vec<Prim>) {
    let (tool, style) = (drawing.tool, &drawing.style);
    if !tool.takes_label() || pts.is_empty() {
        return;
    }
    let text = drawing.text.replace('\n', " ");
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let layout = &style.text_layout;
    let size = style.text_size;
    let (at, anchor) = place(tool, pts, plot, layout, size);
    out.push(Prim::Label {
        at,
        text: text.to_owned(),
        color: style.text_color(),
        background: layout
            .background
            .then(|| (layout.background_color.unwrap_or(TAG), 0.9)),
        anchor,
        size,
        bold: style.bold,
    });
}

/// Where the words of a drawing go, and which point of them sits there.
fn place(tool: Tool, pts: &[P], plot: Rect, layout: &TextLayout, size: f32) -> (P, Anchor) {
    let lift = size * 0.6 + GAP;
    // Above, on or below a line at height `y`.
    let across = |y: f32| match layout.valign {
        VAlign::Top => y - lift,
        VAlign::Middle => y,
        VAlign::Bottom => y + lift,
    };
    // Along a span from `from` to `to`: where the words stand and which side of them touches it.
    let along = |from: f32, to: f32| -> (f32, Anchor) {
        let (left, right) = (from.min(to), from.max(to));
        match layout.align {
            HAlign::Start => (left + EDGE, Anchor::Left),
            HAlign::Center => ((left + right) / 2.0, Anchor::Center),
            HAlign::End => (right - EDGE, Anchor::Right),
        }
    };

    match tool {
        Tool::HorizontalLine | Tool::CrossLine => {
            let (x, anchor) = along(plot.l, plot.r);
            ((x, across(pts[0].1)), anchor)
        }
        Tool::HorizontalRay => {
            let (x, anchor) = along(pts[0].0, plot.r);
            ((x, across(pts[0].1)), anchor)
        }
        Tool::VerticalLine => {
            let y = match layout.valign {
                VAlign::Top => plot.t + size + GAP * 2.0,
                VAlign::Middle => (plot.t + plot.b) / 2.0,
                VAlign::Bottom => plot.b - size - GAP * 2.0,
            };
            ((pts[0].0 + EDGE, y), Anchor::Left)
        }
        // A line between two points: the words follow it, wherever along it they stand.
        _ if pts.len() >= 2 && tool.anchors() == 2 && !tool.is_box() => {
            let (a, b) = if pts[0].0 <= pts[1].0 {
                (pts[0], pts[1])
            } else {
                (pts[1], pts[0])
            };
            let (x, anchor) = along(a.0, b.0);
            let t = if (b.0 - a.0).abs() < 1e-3 {
                0.5
            } else {
                ((x - a.0) / (b.0 - a.0)).clamp(0.0, 1.0)
            };
            ((x, across(a.1 + (b.1 - a.1) * t)), anchor)
        }
        // A shape, or a mark: inside the box its points make.
        _ => {
            let (mut left, mut right) = (f32::INFINITY, f32::NEG_INFINITY);
            let (mut top, mut bottom) = (f32::INFINITY, f32::NEG_INFINITY);
            for p in pts {
                left = left.min(p.0);
                right = right.max(p.0);
                top = top.min(p.1);
                bottom = bottom.max(p.1);
            }
            if right - left < 1.0 && bottom - top < 1.0 {
                // A mark is a point: the words stand clear of it.
                let (x, anchor) = along(left, right);
                let y = match layout.valign {
                    VAlign::Top => top - 30.0,
                    VAlign::Middle => top,
                    VAlign::Bottom => top + 30.0,
                };
                return ((x, y), anchor);
            }
            let (x, anchor) = along(left, right);
            let inset = size * 0.7 + GAP;
            let y = match layout.valign {
                VAlign::Top => top + inset,
                VAlign::Middle => (top + bottom) / 2.0,
                VAlign::Bottom => bottom - inset,
            };
            ((x, y), anchor)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::chart::drawing::look::Caps;
    use crate::app::chart::drawing::model::Point;

    const PLOT: Rect = Rect {
        l: 0.0,
        t: 0.0,
        r: 1000.0,
        b: 500.0,
    };

    fn drawing(tool: Tool, count: usize) -> Drawing {
        Drawing::new(1, tool, vec![Point { t: 0, p: 0.0 }; count])
    }

    fn labels(out: &[Prim]) -> Vec<(&str, P, Anchor)> {
        out.iter()
            .filter_map(|p| match p {
                Prim::Label {
                    text, at, anchor, ..
                } => Some((text.as_str(), *at, *anchor)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_line_carries_its_words_where_the_layout_puts_them() {
        let mut d = drawing(Tool::TrendLine, 2);
        d.text = "  pullback\nzone ".to_owned();
        let pts = [(100.0, 300.0), (500.0, 100.0)];

        let mut out = Vec::new();
        decorate(&d, &pts, PLOT, &mut out);
        let found = labels(&out);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "pullback zone", "trimmed, one line");
        assert_eq!(found[0].2, Anchor::Center);
        // In the middle of the line (300, 200), lifted above it.
        assert_eq!(found[0].1.0, 300.0);
        assert!(found[0].1.1 < 200.0);

        d.style.text_layout = TextLayout {
            align: HAlign::End,
            valign: VAlign::Bottom,
            background: true,
            background_color: Some(0x123456),
        };
        let mut out = Vec::new();
        decorate(&d, &pts, PLOT, &mut out);
        let found = labels(&out);
        assert_eq!(found[0].2, Anchor::Right);
        assert!(found[0].1.0 < 500.0 && found[0].1.0 > 480.0, "at the end");
        assert!(found[0].1.1 > 100.0, "below the line at its end");
        assert!(matches!(
            out.last(),
            Some(Prim::Label {
                background: Some((0x123456, _)),
                ..
            })
        ));
    }

    #[test]
    fn a_shape_carries_its_words_inside_and_a_horizontal_line_across_the_plot() {
        let mut rect = drawing(Tool::Rectangle, 2);
        rect.text = "range".to_owned();
        let pts = [(200.0, 100.0), (600.0, 300.0)];
        let mut out = Vec::new();
        decorate(&rect, &pts, PLOT, &mut out);
        let at = labels(&out)[0].1;
        assert_eq!(at.0, 400.0);
        assert!(at.1 > 100.0 && at.1 < 300.0, "inside the box");

        let mut line = drawing(Tool::HorizontalLine, 1);
        line.text = "support".to_owned();
        line.style.text_layout.align = HAlign::Start;
        let mut out = Vec::new();
        decorate(&line, &[(400.0, 250.0)], PLOT, &mut out);
        let found = labels(&out);
        assert_eq!(found[0].2, Anchor::Left);
        assert_eq!(found[0].1.0, PLOT.l + EDGE, "at the left of the plot");
    }

    #[test]
    fn a_tool_with_words_of_its_own_and_an_empty_label_add_nothing() {
        let mut note = drawing(Tool::Note, 1);
        note.text = "kept by the note itself".to_owned();
        let mut out = Vec::new();
        decorate(&note, &[(10.0, 10.0)], PLOT, &mut out);
        assert!(out.is_empty());

        let mut d = drawing(Tool::TrendLine, 2);
        d.text = "   ".to_owned();
        decorate(&d, &[(0.0, 0.0), (10.0, 10.0)], PLOT, &mut out);
        assert!(out.is_empty());

        let mut icon = drawing(Tool::Icon, 1);
        icon.text = "star".to_owned();
        decorate(&icon, &[(5.0, 5.0)], PLOT, &mut out);
        assert!(out.is_empty(), "an icon's text is which icon it is");
        for tool in Tool::ALL {
            assert!(
                !(tool.has_text() && tool.takes_label()),
                "{tool:?} has its own words"
            );
        }
    }

    #[test]
    fn the_ends_of_a_line_take_an_arrow_or_a_dot() {
        let mut d = drawing(Tool::TrendLine, 2);
        d.style.caps = Caps {
            start: Cap::Circle,
            end: Cap::Arrow,
        };
        let mut out = Vec::new();
        decorate(&d, &[(100.0, 100.0), (300.0, 100.0)], PLOT, &mut out);
        assert!(matches!(
            out[0],
            Prim::Ellipse {
                center: (100.0, 100.0),
                ..
            }
        ));
        let Prim::Polygon { points, .. } = &out[1] else {
            panic!("an arrow head");
        };
        assert_eq!(points[0], (300.0, 100.0), "the tip is on the end");

        // A ray has no far end to cap, and a tool with no caps gets none.
        let mut ray = drawing(Tool::Ray, 2);
        ray.style.caps = Caps {
            start: Cap::Circle,
            end: Cap::Circle,
        };
        let mut out = Vec::new();
        decorate(&ray, &[(100.0, 100.0), (300.0, 100.0)], PLOT, &mut out);
        assert_eq!(out.len(), 1);
        let mut rect = drawing(Tool::Rectangle, 2);
        rect.style.caps.start = Cap::Arrow;
        let mut out = Vec::new();
        decorate(&rect, &[(0.0, 0.0), (10.0, 10.0)], PLOT, &mut out);
        assert!(out.is_empty());
    }
}
