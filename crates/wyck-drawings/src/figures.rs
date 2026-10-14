//! The shapes of the tools built from several lines: pitchforks, Gann fans, boxes and squares,
//! the Fibonacci channel, time zones and fan, chart patterns and Elliott waves.
//!
//! Like the rest of [`super::geometry`], this is plain arithmetic on screen positions, tested
//! with the linear projection of the geometry tests.

pub(crate) mod gann;
pub(crate) mod patterns;

use super::geometry::fib::{fill_of, label, level_label, level_text, stretch, visible_levels};
use super::geometry::{Anchor, P, Prim, Projection, Rect, seg};
use super::model::style::Dash;
use super::model::tool::Tool;
use super::model::{Drawing, Point};

/// How far a label sits above or below the point it names.
const LABEL_GAP: f32 = 14.0;

/// The shapes of a drawing whose tool is one of the ones in this file. `pts` are its points on
/// the screen.
pub(crate) fn prims(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let rect = proj.plot();
    match drawing.tool {
        Tool::Pitchfork
        | Tool::SchiffPitchfork
        | Tool::ModifiedSchiffPitchfork
        | Tool::InsidePitchfork => pitchfork(drawing, pts, rect, out),
        Tool::FibChannel => fib_channel(drawing, pts, rect, out),
        Tool::FibTimeZone => fib_time_zone(drawing, pts, proj, out),
        Tool::FibFan => fib_fan(drawing, pts, rect, out),
        Tool::GannFan => gann::gann_fan(drawing, pts, rect, out),
        Tool::GannBox => gann::gann_box(drawing, pts, proj, out),
        Tool::GannSquare => gann::gann_square(drawing, pts, out),
        Tool::Xabcd | Tool::Cypher | Tool::Abcd | Tool::ThreeDrives => {
            patterns::harmonic(drawing, pts, out);
        }
        Tool::HeadAndShoulders => patterns::head_and_shoulders(drawing, pts, rect, out),
        Tool::TrianglePattern => patterns::triangle_pattern(drawing, pts, out),
        Tool::ElliottImpulse
        | Tool::ElliottCorrection
        | Tool::ElliottTriangle
        | Tool::ElliottDoubleCombo
        | Tool::ElliottTripleCombo => patterns::elliott(drawing, pts, out),
        _ => {}
    }
}

fn add(a: P, b: P) -> P {
    (a.0 + b.0, a.1 + b.1)
}

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}

fn scale(a: P, k: f32) -> P {
    (a.0 * k, a.1 * k)
}

fn mid(a: P, b: P) -> P {
    ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
}

/// How many times `d` fits between `from` and the farthest corner of the plot, forwards: the
/// parameter that takes a line from `from` along `d` past the plot's edge.
pub(super) fn reach(from: P, d: P, rect: Rect) -> f32 {
    let length_sq = d.0 * d.0 + d.1 * d.1;
    if length_sq < 1e-6 {
        return 1.0;
    }
    [
        (rect.l, rect.t),
        (rect.r, rect.t),
        (rect.l, rect.b),
        (rect.r, rect.b),
    ]
    .iter()
    .map(|c| ((c.0 - from.0) * d.0 + (c.1 - from.1) * d.1) / length_sq)
    .fold(1.0, f32::max)
}

/// The price ratio of the move `a` to `b` against the move `c` to `d`, as a label.
fn ratio(a: Point, b: Point, c: Point, d: Point) -> Option<String> {
    let base = (d.p - c.p).abs();
    (base > 0.0).then(|| level_text((b.p - a.p).abs() / base))
}

/// Whether point `i` is a top (above its neighbours) on the screen, where up is smaller y.
fn is_top(pts: &[P], i: usize) -> bool {
    let y = pts[i].1;
    let before = i.checked_sub(1).map(|j| pts[j].1);
    let after = pts.get(i + 1).map(|p| p.1);
    match (before, after) {
        (Some(b), Some(a)) => y <= b.min(a) || (y < b && y < a) || y <= (a + b) / 2.0,
        (Some(b), None) => y <= b,
        (None, Some(a)) => y <= a,
        (None, None) => true,
    }
}

/// A name over a top or under a bottom.
fn point_label(pts: &[P], i: usize, text: String, drawing: &Drawing, boxed: bool) -> Prim {
    let style = &drawing.style;
    let up = is_top(pts, i);
    let gap = LABEL_GAP + style.text_size * 0.3;
    let at = (pts[i].0, pts[i].1 + if up { -gap } else { gap });
    let mut prim = label(at, text, style.text_color(), Anchor::Center, style);
    if boxed
        && let Prim::Label {
            background, color, ..
        } = &mut prim
    {
        *background = Some((style.color, 0.9));
        *color = style.text_color.unwrap_or(0xffffff);
    }
    prim
}

/// A ratio written in the middle of the dashed line between two points.
fn ratio_label(a: P, b: P, text: String, drawing: &Drawing) -> Prim {
    let style = &drawing.style;
    let mut prim = label(mid(a, b), text, style.text_color(), Anchor::Center, style);
    if let Prim::Label { background, .. } = &mut prim {
        *background = Some((0x131722, 0.85));
    }
    prim
}

fn pitchfork(drawing: &Drawing, pts: &[P], rect: Rect, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b, c) = (pts[0], pts[1], pts[2]);
    let m = mid(b, c);
    // Where the median line starts, a point it passes through, and the step from it to a tine.
    let (origin, pivot, half) = match drawing.tool {
        Tool::SchiffPitchfork => (mid(a, b), m, sub(b, m)),
        Tool::ModifiedSchiffPitchfork => ((a.0, (a.1 + b.1) / 2.0), m, sub(b, m)),
        Tool::InsidePitchfork => (mid(a, b), c, sub(b, c)),
        _ => (a, m, sub(b, m)),
    };
    let d = sub(pivot, origin);
    if d.0.abs() < 1e-3 && d.1.abs() < 1e-3 {
        return;
    }
    // Every line runs parallel to the median, from its level on the base to past the plot.
    let lines: Vec<(f64, u32)> = visible_levels(drawing)
        .into_iter()
        .flat_map(|l| [(l.value, l.color), (-l.value, l.color)])
        .collect();
    let starts: Vec<P> = lines
        .iter()
        .map(|&(k, _)| add(pivot, scale(half, k as f32)))
        .chain([pivot])
        .collect();
    let far = if style.extend_right {
        starts
            .iter()
            .map(|s| reach(*s, d, rect))
            .fold(1.0, f32::max)
    } else {
        1.0
    };
    let near = if style.extend_left {
        -starts
            .iter()
            .map(|s| reach(*s, scale(d, -1.0), rect))
            .fold(0.0, f32::max)
    } else {
        0.0
    };
    let ends = |start: P| (add(start, scale(d, near)), add(start, scale(d, far)));

    // The zones between the lines, each in the color of its outer line.
    if let Some((_, opacity)) = fill_of(style) {
        for side in [1.0, -1.0] {
            let mut inner = pivot;
            let mut ordered: Vec<(f64, u32)> = lines
                .iter()
                .filter(|(k, _)| *k * side > 0.0)
                .copied()
                .collect();
            ordered.sort_by(|x, y| x.0.abs().total_cmp(&y.0.abs()));
            for (k, color) in ordered {
                let outer = add(pivot, scale(half, k as f32));
                let (i0, i1) = ends(inner);
                let (o0, o1) = ends(outer);
                out.push(Prim::Polygon {
                    points: vec![i0, i1, o1, o0],
                    fill: (color, opacity),
                });
                inner = outer;
            }
        }
    }
    // The handle, from the first point to the base, and the base between the other two.
    let median_color = style.color;
    let (m0, m1) = ends(pivot);
    out.push(seg(origin, m0, median_color, 1.0, style.width, style.dash));
    out.push(seg(m0, m1, median_color, 1.0, style.width, style.dash));
    if origin != a {
        out.push(seg(a, origin, median_color, 0.6, 1.0, Dash::Dashed));
    }
    out.push(seg(b, c, median_color, 0.8, 1.0, style.dash));
    for &(k, color) in &lines {
        let start = add(pivot, scale(half, k as f32));
        let (s0, s1) = ends(start);
        out.push(seg(s0, s1, color, 1.0, style.width, style.dash));
        if style.labels && k > 0.0 {
            out.push(label(
                (start.0 + 4.0, start.1 - 8.0),
                level_text(k),
                color,
                Anchor::Left,
                style,
            ));
        }
    }
}

fn fib_channel(drawing: &Drawing, pts: &[P], rect: Rect, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let (a, b, c) = (pts[0], pts[1], pts[2]);
    let dx = b.0 - a.0;
    let line_y_at_c = if dx.abs() < 1e-6 {
        a.1
    } else {
        a.1 + (b.1 - a.1) * (c.0 - a.0) / dx
    };
    let offset = c.1 - line_y_at_c;
    let (a, b) = stretch(a, b, style, rect);
    let mut previous: Option<(P, P)> = None;
    for level in visible_levels(drawing) {
        let k = if drawing.reverse {
            1.0 - level.value
        } else {
            level.value
        } as f32;
        let (p, q) = ((a.0, a.1 + offset * k), (b.0, b.1 + offset * k));
        if let (Some((pp, pq)), Some((_, opacity))) = (previous, fill_of(style)) {
            out.push(Prim::Polygon {
                points: vec![pp, pq, q, p],
                fill: (level.color, opacity),
            });
        }
        out.push(seg(
            p,
            q,
            level.color,
            0.9,
            level.line_width(style.width),
            level.line_dash(style.dash),
        ));
        if style.labels {
            let left = if p.0 <= q.0 { p } else { q };
            out.push(label(
                (left.0 + 4.0, left.1 - 8.0),
                level_label(style, level.value, None),
                level.color,
                Anchor::Left,
                style,
            ));
        }
        previous = Some((p, q));
    }
}

fn fib_time_zone(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    let style = &drawing.style;
    let rect = proj.plot();
    let (a, b) = (drawing.points[0], drawing.points[1]);
    let (Some(ia), Some(ib)) = (proj.index_of(a.t), proj.index_of(b.t)) else {
        return;
    };
    let bars = ib - ia;
    for level in visible_levels(drawing) {
        let x = if bars.abs() < 1e-9 {
            pts[0].0
        } else {
            let Some(t) = proj.shift_bars(a.t, bars * level.value) else {
                continue;
            };
            let Some(at) = proj.to_screen(Point { t, p: a.p }) else {
                continue;
            };
            at.0
        };
        out.push(seg(
            (x, rect.t),
            (x, rect.b),
            level.color,
            0.9,
            level.line_width(style.width),
            level.line_dash(style.dash),
        ));
        if style.labels {
            out.push(label(
                (x + 4.0, rect.b - 12.0),
                level_label(style, level.value, None),
                level.color,
                Anchor::Left,
                style,
            ));
        }
    }
    out.push(seg(pts[0], pts[1], style.color, 0.6, 1.0, Dash::Dashed));
}

/// Lines from `origin` through each of `through`, with the zones between them filled.
pub(super) fn fan(
    drawing: &Drawing,
    origin: P,
    through: &[(P, u32, String)],
    rect: Rect,
    out: &mut Vec<Prim>,
) {
    let style = &drawing.style;
    let far: Vec<P> = through
        .iter()
        .map(|(p, _, _)| {
            let d = sub(*p, origin);
            add(origin, scale(d, reach(origin, d, rect)))
        })
        .collect();
    if let Some((_, opacity)) = fill_of(style) {
        for i in 1..through.len() {
            out.push(Prim::Polygon {
                points: vec![origin, far[i - 1], far[i]],
                fill: (through[i].1, opacity),
            });
        }
    }
    for (i, (p, color, text)) in through.iter().enumerate() {
        out.push(seg(origin, far[i], *color, 1.0, style.width, style.dash));
        if style.labels && !text.is_empty() {
            out.push(label(
                (p.0 + 4.0, p.1),
                text.clone(),
                *color,
                Anchor::Left,
                style,
            ));
        }
    }
}

fn fib_fan(drawing: &Drawing, pts: &[P], rect: Rect, out: &mut Vec<Prim>) {
    let (a, b) = (pts[0], pts[1]);
    let through: Vec<(P, u32, String)> = visible_levels(drawing)
        .into_iter()
        .map(|l| {
            let y = b.1 + (a.1 - b.1) * l.value as f32;
            ((b.0, y), l.color, level_text(l.value))
        })
        .collect();
    fan(drawing, a, &through, rect, out);
    out.push(seg(
        a,
        b,
        drawing.style.color,
        1.0,
        drawing.style.width,
        drawing.style.dash,
    ));
}

#[cfg(test)]
mod tests {
    use super::super::geometry::hit::hit;
    use super::super::geometry::prims;
    use super::super::geometry::tests::{Linear, drawing};
    use super::*;

    fn labels(shapes: &[Prim]) -> Vec<String> {
        shapes
            .iter()
            .filter_map(|s| match s {
                Prim::Label { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn segments(shapes: &[Prim]) -> Vec<(P, P, u32)> {
        shapes
            .iter()
            .filter_map(|s| match s {
                Prim::Segment { a, b, color, .. } => Some((*a, *b, *color)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_pitchfork_has_a_median_through_the_middle_of_the_base_and_tines_through_its_ends() {
        // A at (100, 400), B at (300, 300), C at (300, 100): the base's middle is (300, 200).
        let mut d = drawing(Tool::Pitchfork, &[(100, 100.0), (300, 200.0), (300, 400.0)]);
        d.style.extend_right = false;
        d.style.fill = false;
        let shapes = prims(&d, &Linear);
        let segs = segments(&shapes);
        // The median runs from A to the base and on as far again.
        assert!(
            segs.iter()
                .any(|(a, b, _)| *a == (100.0, 400.0) && *b == (300.0, 200.0)),
            "{segs:?}"
        );
        // A tine (level 1) starts at B and C and runs parallel to the median.
        let tine = segs
            .iter()
            .find(|(a, b, _)| *a == (300.0, 300.0) && b.0 > a.0)
            .expect("a line from B");
        assert!(((tine.1.0 - tine.0.0) - 200.0).abs() < 1e-3);
        assert!(((tine.1.1 - tine.0.1) + 200.0).abs() < 1e-3);
        assert!(labels(&shapes).contains(&"1".to_owned()));
        assert!(labels(&shapes).contains(&"0.5".to_owned()));
    }

    #[test]
    fn the_schiff_pitchforks_move_the_handle_halfway_to_the_second_point() {
        let points = [(100, 100.0), (300, 200.0), (300, 400.0)];
        for (tool, origin) in [
            (Tool::SchiffPitchfork, (200.0, 350.0)),
            (Tool::ModifiedSchiffPitchfork, (100.0, 350.0)),
        ] {
            let mut d = drawing(tool, &points);
            d.style.extend_right = false;
            let segs = segments(&prims(&d, &Linear));
            assert!(
                segs.iter()
                    .any(|(a, b, _)| *a == origin && *b == (300.0, 200.0)),
                "{tool:?}: {segs:?}"
            );
        }
    }

    #[test]
    fn a_gann_fan_has_its_one_to_one_through_the_second_point() {
        let d = drawing(Tool::GannFan, &[(100, 100.0), (200, 200.0)]);
        let shapes = prims(&d, &Linear);
        let texts = labels(&shapes);
        for angle in ["1/1", "2/1", "1/2", "1/8", "8/1", "1/3"] {
            assert!(texts.contains(&angle.to_owned()), "{angle} in {texts:?}");
        }
        // The 1/1 line passes through (200, 300) on the way to the edge.
        let one = segments(&shapes).into_iter().find(|(a, b, _)| {
            *a == (100.0, 400.0) && ((b.1 - 400.0) / (b.0 - 100.0) + 1.0).abs() < 1e-3
        });
        assert!(one.is_some());
        assert_eq!(gann::gann_text(1.0 / 3.0), "1/3");
        assert_eq!(gann::gann_text(4.0), "4/1");
    }

    #[test]
    fn a_gann_box_divides_both_sides_at_its_levels() {
        let d = drawing(Tool::GannBox, &[(0, 100.0), (400, 300.0)]);
        let segs = segments(&prims(&d, &Linear));
        // The 0.5 level: a horizontal at price 200 and a vertical at 200 seconds.
        assert!(segs.iter().any(|(a, b, _)| a.1 == 300.0 && b.1 == 300.0));
        assert!(segs.iter().any(|(a, b, _)| a.0 == 200.0 && b.0 == 200.0));
    }

    #[test]
    fn a_fib_channel_repeats_its_base_at_each_level() {
        let d = drawing(Tool::FibChannel, &[(0, 100.0), (400, 100.0), (200, 200.0)]);
        let segs = segments(&prims(&d, &Linear));
        // Offset 100 in price: the 0.5 level is at price 150, screen y 350.
        assert!(
            segs.iter().any(|(a, b, _)| a.1 == 350.0 && b.1 == 350.0),
            "{segs:?}"
        );
    }

    #[test]
    fn fib_time_zones_fall_on_the_fibonacci_numbers_of_bars() {
        // One minute bars; the second point is 2 bars on, so zone 3 is 6 bars on (360 s).
        let d = drawing(Tool::FibTimeZone, &[(0, 100.0), (120, 100.0)]);
        let segs = segments(&prims(&d, &Linear));
        assert!(segs.iter().any(|(a, b, _)| a.0 == 360.0 && b.0 == 360.0));
        assert!(segs.iter().any(|(a, _, _)| a.0 == 0.0));
    }

    #[test]
    fn an_xabcd_pattern_writes_its_points_and_ratios() {
        // X 100, A 200, B 150 (half of XA), C 175 (half of AB), D 125.
        let d = drawing(
            Tool::Xabcd,
            &[
                (0, 100.0),
                (100, 200.0),
                (200, 150.0),
                (300, 175.0),
                (400, 125.0),
            ],
        );
        let texts = labels(&prims(&d, &Linear));
        for name in ["X", "A", "B", "C", "D", "0.5", "2", "0.75"] {
            assert!(texts.contains(&name.to_owned()), "{name} in {texts:?}");
        }
    }

    #[test]
    fn a_head_and_shoulders_names_its_parts_and_draws_a_neckline() {
        let d = drawing(
            Tool::HeadAndShoulders,
            &[
                (0, 100.0),
                (100, 200.0),
                (200, 150.0),
                (300, 250.0),
                (400, 150.0),
                (500, 200.0),
                (600, 100.0),
            ],
        );
        let shapes = prims(&d, &Linear);
        let texts = labels(&shapes);
        assert!(texts.contains(&"Head".to_owned()));
        assert!(texts.contains(&"Left shoulder".to_owned()));
        // The neckline is flat at price 150 (y 350) from the first to the last point.
        assert!(
            segments(&shapes)
                .iter()
                .any(|(a, b, _)| *a == (0.0, 350.0) && *b == (600.0, 350.0))
        );
    }

    #[test]
    fn a_triangle_patterns_sides_meet_at_its_apex() {
        let d = drawing(
            Tool::TrianglePattern,
            &[(0, 300.0), (100, 100.0), (200, 250.0), (300, 150.0)],
        );
        let segs = segments(&prims(&d, &Linear));
        // The upper side falls 50 in 200 s from 300, the lower rises 50 in 200 s from 100 (a
        // hundred seconds later): they meet at 450 s, price 187.5.
        assert!(
            segs.iter()
                .any(|(_, b, _)| (b.0 - 450.0).abs() < 1e-2 && (b.1 - 312.5).abs() < 1e-2),
            "{segs:?}"
        );
    }

    #[test]
    fn an_elliott_impulse_names_its_waves_at_its_degree() {
        let points = [
            (0, 100.0),
            (100, 200.0),
            (200, 150.0),
            (300, 300.0),
            (400, 250.0),
            (500, 350.0),
        ];
        let mut d = drawing(Tool::ElliottImpulse, &points);
        assert_eq!(labels(&prims(&d, &Linear)), vec!["1", "2", "3", "4", "5"]);
        d.degree = 3;
        assert_eq!(labels(&prims(&d, &Linear))[0], "(1)");
        let correction = drawing(
            Tool::ElliottCorrection,
            &[(0, 100.0), (100, 50.0), (200, 80.0), (300, 20.0)],
        );
        assert_eq!(labels(&prims(&correction, &Linear)), vec!["A", "B", "C"]);
    }

    #[test]
    fn a_label_goes_over_a_top_and_under_a_bottom() {
        let pts = [(0.0, 300.0), (100.0, 100.0), (200.0, 300.0)];
        assert!(is_top(&pts, 1));
        assert!(!is_top(&[(0.0, 100.0), (100.0, 300.0), (200.0, 100.0)], 1));
    }

    #[test]
    fn every_new_tool_draws_something_and_can_be_picked() {
        for tool in Tool::ALL {
            let points: Vec<(i64, f64)> = (0..tool.anchors().max(2))
                .map(|i| {
                    (
                        100 + i as i64 * 60,
                        if i % 2 == 0 { 150.0 } else { 250.0 } + i as f64,
                    )
                })
                .collect();
            let d = drawing(tool, &points[..tool.anchors().max(2)]);
            let shapes = prims(&d, &Linear);
            assert!(!shapes.is_empty(), "{tool:?} draws nothing");
            let first = super::super::geometry::anchors(&d, &Linear).unwrap()[0];
            assert!(
                hit(&d, &Linear, first, true).is_some(),
                "{tool:?} cannot be picked by its first point"
            );
        }
    }
}
