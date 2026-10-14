//! The chart patterns and the Elliott waves.

use super::{add, point_label, ratio, ratio_label, scale, sub};
use crate::domain::drawings::geometry::fib::fill_of;
use crate::domain::drawings::geometry::{P, Prim, Rect, extended, seg};
use crate::domain::drawings::model::style::Dash;
use crate::domain::drawings::model::tool::Tool;
use crate::domain::drawings::model::{Drawing, wave_label};

/// The zigzag through every point.
fn zigzag(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    for pair in pts.windows(2) {
        out.push(seg(
            pair[0],
            pair[1],
            style.color,
            1.0,
            style.width,
            style.dash,
        ));
    }
}

pub(super) fn harmonic(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let p = &drawing.points;
    let names: &[&str] = match drawing.tool {
        Tool::Xabcd | Tool::Cypher => &["X", "A", "B", "C", "D"],
        Tool::Abcd => &["A", "B", "C", "D"],
        _ => &["", "1", "A", "2", "B", "3", ""],
    };
    // The two triangles of a five point pattern, the zones of a four point one.
    if let Some(fill) = fill_of(style) {
        let triangles: Vec<[usize; 3]> = match pts.len() {
            5 => vec![[0, 1, 2], [2, 3, 4]],
            4 => vec![[0, 1, 2], [1, 2, 3]],
            _ => vec![[1, 2, 3], [3, 4, 5]],
        };
        for [i, j, k] in triangles {
            out.push(Prim::Polygon {
                points: vec![pts[i], pts[j], pts[k]],
                fill,
            });
        }
    }
    zigzag(drawing, pts, out);
    if style.middle {
        // Which moves are compared, as (dashed line from, to, the ratio of the moves).
        let ratios: Vec<(usize, usize, Option<String>)> = match drawing.tool {
            Tool::Xabcd => vec![
                (0, 2, ratio(p[1], p[2], p[0], p[1])),
                (1, 3, ratio(p[2], p[3], p[1], p[2])),
                (2, 4, ratio(p[3], p[4], p[2], p[3])),
                (0, 4, ratio(p[1], p[4], p[0], p[1])),
            ],
            Tool::Cypher => vec![
                (0, 2, ratio(p[1], p[2], p[0], p[1])),
                (1, 3, ratio(p[0], p[3], p[0], p[1])),
                (0, 4, ratio(p[3], p[4], p[0], p[3])),
            ],
            Tool::Abcd => vec![
                (0, 2, ratio(p[1], p[2], p[0], p[1])),
                (1, 3, ratio(p[2], p[3], p[1], p[2])),
            ],
            _ => vec![
                (1, 3, ratio(p[2], p[3], p[1], p[2])),
                (3, 5, ratio(p[4], p[5], p[3], p[4])),
                (2, 4, ratio(p[3], p[4], p[2], p[3])),
            ],
        };
        for (from, to, text) in ratios {
            out.push(seg(pts[from], pts[to], style.color, 0.7, 1.0, Dash::Dashed));
            if style.labels
                && let Some(text) = text
            {
                out.push(ratio_label(pts[from], pts[to], text, drawing));
            }
        }
    }
    if style.labels {
        for (i, name) in names.iter().enumerate().filter(|(_, n)| !n.is_empty()) {
            let text = match drawing.tool {
                Tool::ThreeDrives
                    if name.len() == 1 && name.chars().all(|c| c.is_ascii_digit()) =>
                {
                    format!("Drive {name}")
                }
                _ => (*name).to_owned(),
            };
            out.push(point_label(pts, i, text, drawing, true));
        }
    }
}

pub(super) fn head_and_shoulders(drawing: &Drawing, pts: &[P], rect: Rect, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    if let Some(fill) = fill_of(style) {
        for [i, j, k] in [[0, 1, 2], [2, 3, 4], [4, 5, 6]] {
            out.push(Prim::Polygon {
                points: vec![pts[i], pts[j], pts[k]],
                fill,
            });
        }
    }
    zigzag(drawing, pts, out);
    // The neckline, through the two troughs, from the first point to the last.
    let (n1, n2) = (pts[2], pts[4]);
    if (n2.0 - n1.0).abs() > 1e-3 {
        let slope = (n2.1 - n1.1) / (n2.0 - n1.0);
        let at = |x: f32| (x, n1.1 + slope * (x - n1.0));
        let (mut start, mut end) = (at(pts[0].0.min(n1.0)), at(pts[6].0.max(n2.0)));
        if (style.extend_left || style.extend_right)
            && let Some((p, q)) = extended(start, end, style.extend_left, style.extend_right, rect)
        {
            (start, end) = (p, q);
        }
        out.push(seg(start, end, style.color, 1.0, style.width, Dash::Dashed));
    }
    if style.labels {
        for (i, name) in [(1, "Left shoulder"), (3, "Head"), (5, "Right shoulder")] {
            out.push(point_label(pts, i, name.to_owned(), drawing, true));
        }
    }
}

/// Where the lines `a` to `b` and `c` to `d` meet, if they are not parallel.
fn intersection(a: P, b: P, c: P, d: P) -> Option<P> {
    let (r, s) = (sub(b, a), sub(d, c));
    let denominator = r.0 * s.1 - r.1 * s.0;
    if denominator.abs() < 1e-6 {
        return None;
    }
    let t = ((c.0 - a.0) * s.1 - (c.1 - a.1) * s.0) / denominator;
    Some(add(a, scale(r, t)))
}

pub(super) fn triangle_pattern(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b, c, d) = (pts[0], pts[1], pts[2], pts[3]);
    let right = a.0.max(b.0).max(c.0).max(d.0);
    // The two sides run on to where they meet, when that is ahead; otherwise to the last point.
    let apex = intersection(a, c, b, d).filter(|p| p.0 > right);
    let side = |p: P, q: P| -> P {
        match apex {
            Some(apex) => apex,
            None if (q.0 - p.0).abs() > 1e-3 => {
                (right, p.1 + (q.1 - p.1) * (right - p.0) / (q.0 - p.0))
            }
            None => q,
        }
    };
    let (upper_end, lower_end) = (side(a, c), side(b, d));
    if let Some(fill) = fill_of(style) {
        out.push(Prim::Polygon {
            points: vec![a, upper_end, lower_end, b],
            fill,
        });
    }
    out.push(seg(a, upper_end, style.color, 1.0, style.width, style.dash));
    out.push(seg(b, lower_end, style.color, 1.0, style.width, style.dash));
    zigzag(drawing, pts, out);
    if style.labels {
        for (i, name) in ["A", "B", "C", "D"].iter().enumerate() {
            out.push(point_label(pts, i, (*name).to_owned(), drawing, true));
        }
    }
}

/// The names of the points of an Elliott wave, the first being its start.
pub fn wave_names(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::ElliottImpulse => &["0", "1", "2", "3", "4", "5"],
        Tool::ElliottCorrection => &["0", "A", "B", "C"],
        Tool::ElliottTriangle => &["0", "A", "B", "C", "D", "E"],
        Tool::ElliottDoubleCombo => &["0", "W", "X", "Y"],
        Tool::ElliottTripleCombo => &["0", "W", "X", "Y", "X", "Z"],
        _ => &[],
    }
}

pub(super) fn elliott(drawing: &Drawing, pts: &[P], out: &mut Vec<Prim>) {
    zigzag(drawing, pts, out);
    if !drawing.style.labels {
        return;
    }
    for (i, name) in wave_names(drawing.tool).iter().enumerate().skip(1) {
        if i < pts.len() {
            let text = wave_label(name, drawing.degree);
            out.push(point_label(pts, i, text, drawing, false));
        }
    }
}
