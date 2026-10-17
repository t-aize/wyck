//! The shapes of the tools that are not lines of the plain kind or the Fibonacci, Gann and pattern
//! families of [`super::figures`]: information lines and angles, regression, ranges, cycles,
//! circles and curves, forecasts, ghost bars, volume, notes, markers and tables.
//!
//! Like the rest of the drawing code this is arithmetic on screen positions. The tools that read
//! the chart's data (regression, bars, VWAP, volume) get it through [`Projection::bars_between`].

pub(crate) mod bars;
pub(crate) mod curves;
pub(crate) mod fib;
pub(crate) mod lines;
pub(crate) mod notes;
pub(crate) mod ranges;

use super::figures;
use super::look::MeasureLook;
use super::model::style::Style;
use super::model::tool::Tool;
use super::model::{Drawing, Point};
use super::prim::{Anchor, P, Prim, Projection, duration_text, lead};
use std::f32::consts::{PI, TAU};
use wyck_core::math::{count_f32, f64_to_i64};

/// The colors of a bar that closed above and below its open, as the position tools have them.
const UP: u32 = 0x00_d492;
const DOWN: u32 = 0xff_6467;
/// The color of the point of control of a volume profile.
const POC: u32 = 0xff_b900;
/// Most bars a bars pattern or a ghost feed copies.
const MAX_GHOST_BARS: usize = 400;
/// The height of a row of a volume profile, in pixels, and the fewest and most rows.
const ROW_PX: f32 = 6.0;
const MIN_ROWS: usize = 8;
const MAX_ROWS: usize = 120;
/// Most cycle lines drawn at once.
const MAX_CYCLES: i64 = 600;

/// The glyphs of an icon drawing: the name it is saved under, and what the settings call it.
pub const ICONS: [(&str, &str); 10] = [
    ("star", "Star"),
    ("circle", "Circle"),
    ("square", "Square"),
    ("diamond", "Diamond"),
    ("triangle_up", "Triangle up"),
    ("triangle_down", "Triangle down"),
    ("cross", "Cross"),
    ("check", "Check"),
    ("heart", "Heart"),
    ("bolt", "Bolt"),
];

/// The shapes of a drawing whose tool is one of the ones in this file. `pts` are its points on
/// the screen.
pub(crate) fn prims(drawing: &Drawing, pts: &[P], proj: &dyn Projection, out: &mut Vec<Prim>) {
    match drawing.tool {
        Tool::InfoLine => lines::info_line(drawing, pts, proj, out),
        Tool::TrendAngle => lines::trend_angle(drawing, pts, proj, out),
        Tool::RegressionTrend => lines::regression(drawing, pts, proj, out),
        Tool::FlatTopBottom => lines::flat_top_bottom(drawing, pts, proj, out),
        Tool::DisjointChannel => lines::disjoint_channel(drawing, pts, proj, out),
        Tool::Pitchfan => lines::pitchfan(drawing, pts, proj, out),
        Tool::FibTimeExtension => fib::fib_time_extension(drawing, pts, proj, out),
        Tool::FibCircles => fib::fib_circles(drawing, pts, out),
        Tool::FibSpiral => fib::fib_spiral(drawing, pts, out),
        Tool::FibArcs => fib::fib_arcs(drawing, pts, out),
        Tool::FibWedge => fib::fib_wedge(drawing, pts, out),
        Tool::GannSquareFixed => {
            if let Some([first, second]) = lead(pts) {
                let corner = fixed_square_corner(first, second);
                figures::gann::gann_square(drawing, &[first, corner], out);
            }
        }
        Tool::CyclicLines => curves::cyclic_lines(drawing, pts, proj, out),
        Tool::TimeCycles => curves::time_cycles(drawing, pts, proj, out),
        Tool::SineLine => curves::sine_line(drawing, pts, proj, out),
        Tool::PriceRange => ranges::price_range(drawing, pts, proj, out),
        Tool::DateRange => ranges::date_range(drawing, pts, proj, out),
        Tool::DatePriceRange => ranges::date_price_range(drawing, pts, proj, out),
        Tool::Forecast => ranges::forecast(drawing, pts, proj, out),
        Tool::BarsPattern => bars::bars_pattern(drawing, proj, out, false),
        Tool::GhostFeed => bars::bars_pattern(drawing, proj, out, true),
        Tool::AnchoredVwap => bars::anchored_vwap(drawing, pts, proj, out),
        Tool::FixedRangeVolumeProfile => bars::volume_profile(drawing, pts, proj, out, true),
        Tool::AnchoredVolumeProfile => bars::volume_profile(drawing, pts, proj, out, false),
        Tool::RotatedRectangle => curves::rotated_rectangle(drawing, pts, out),
        Tool::Circle => curves::circle(drawing, pts, out),
        Tool::Arc => curves::arc(drawing, pts, out),
        Tool::Curve => curves::curve(drawing, pts, out),
        Tool::DoubleCurve => curves::double_curve(drawing, pts, out),
        Tool::ArrowMarker => notes::arrow_marker(drawing, pts, out),
        Tool::ArrowMarkUp | Tool::ArrowMarkDown => {
            if let Some(&at) = pts.first() {
                notes::arrow_mark(drawing, at, drawing.tool == Tool::ArrowMarkUp, out);
            }
        }
        Tool::Note => notes::note(drawing, pts, out),
        Tool::PriceNote => notes::price_note(drawing, pts, proj, out),
        Tool::Callout => notes::callout(drawing, pts, out),
        Tool::Comment => notes::comment(drawing, pts, out),
        Tool::Pin => notes::pin(drawing, pts, out),
        Tool::Signpost => notes::signpost(drawing, pts, out),
        Tool::FlagMark => notes::flag_mark(drawing, pts, out),
        Tool::Table => notes::table(drawing, pts, out),
        Tool::Icon => notes::icon(drawing, pts, out),
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
    (f32::midpoint(a.0, b.0), f32::midpoint(a.1, b.1))
}

fn length(a: P) -> f32 {
    (a.0 * a.0 + a.1 * a.1).sqrt()
}

fn dist(a: P, b: P) -> f32 {
    length(sub(b, a))
}

/// The angle of the line from `a` to `b` to the horizontal, in degrees, up being positive and the
/// direction the line runs in not mattering: between -90 and 90.
fn slope_degrees(a: P, b: P) -> f32 {
    (-(b.1 - a.1)).atan2((b.0 - a.0).abs()).to_degrees()
}

/// `angle` brought into the range from -PI to PI.
fn wrap(angle: f32) -> f32 {
    let mut a = angle % TAU;
    if a > PI {
        a -= TAU;
    } else if a < -PI {
        a += TAU;
    }
    a
}

/// The two ends of a segment ordered left to right.
fn left_to_right(a: P, b: P) -> (P, P) {
    if a.0 <= b.0 { (a, b) } else { (b, a) }
}

/// A tag with a stat on it, in the color of the drawing.
fn stat(style: &Style, at: P, text: String, color: u32, anchor: Anchor) -> Prim {
    Prim::Label {
        at,
        text,
        color: style.text_color.unwrap_or(0xff_ffff),
        background: Some((color, 0.9)),
        anchor,
        size: style.text_size,
        bold: style.bold,
        face: style.face(),
    }
}

/// Tags one under the other, the first at `at`.
fn stat_rows(
    style: &Style,
    at: P,
    rows: Vec<String>,
    color: u32,
    anchor: Anchor,
    out: &mut Vec<Prim>,
) {
    for (i, text) in rows.into_iter().filter(|r| !r.is_empty()).enumerate() {
        out.push(stat(
            style,
            (at.0, at.1 + count_f32(i) * style.text_size * 1.8),
            text,
            color,
            anchor,
        ));
    }
}

/// The change from `a` to `b`: `+0.0012 (+0.12%)`, or the part of it the look asks for. Empty when
/// it asks for none.
pub(super) fn move_text(proj: &dyn Projection, a: Point, b: Point, look: &MeasureLook) -> String {
    let change = b.p - a.p;
    let percent = if a.p == 0.0 {
        0.0
    } else {
        change / a.p * 100.0
    };
    let sign = if change >= 0.0 { "+" } else { "" };
    let price = format!("{sign}{}", proj.format_price(change));
    let percent = format!("{sign}{percent:.2}%");
    match (look.price, look.percent) {
        (true, true) => format!("{price} ({percent})"),
        (true, false) => price,
        (false, true) => percent,
        (false, false) => String::new(),
    }
}

/// The time from `a` to `b`: `12 bars, 1h 0m`, or the part of it the look asks for. Empty when it
/// asks for none.
pub(super) fn span_text(proj: &dyn Projection, a: Point, b: Point, look: &MeasureLook) -> String {
    let bars = match (proj.index_of(a.t), proj.index_of(b.t)) {
        (Some(i), Some(j)) => f64_to_i64((j - i).abs().round()),
        _ => 0,
    };
    let time = duration_text(b.t - a.t);
    match (look.bars, look.time) {
        (true, true) => format!("{bars} bars, {time}"),
        (true, false) => format!("{bars} bars"),
        (false, true) => time,
        (false, false) => String::new(),
    }
}

/// A line of points `at(t)` for `t` from 0 to 1, in `steps` steps.
fn sample(steps: usize, at: impl Fn(f32) -> P) -> Vec<P> {
    (0..=steps)
        .map(|i| at(count_f32(i) / count_f32(steps)))
        .collect()
}

fn polyline(points: Vec<P>, color: u32, width: f32, alpha: f32) -> Prim {
    Prim::Polyline {
        points,
        color,
        width,
        alpha,
    }
}

/// The corner opposite `a` of the square a fixed Gann square makes towards `b`: as far as the
/// pointer goes on its longer side, so the square is square on the screen.
pub(crate) fn fixed_square_corner(a: P, b: P) -> P {
    let side = (b.0 - a.0).abs().max((b.1 - a.1).abs());
    let sx = if b.0 >= a.0 { 1.0 } else { -1.0 };
    let sy = if b.1 >= a.1 { 1.0 } else { -1.0 };
    (a.0 + sx * side, a.1 + sy * side)
}

#[cfg(test)]
mod tests {
    use crate::geometry::hit::{handles, hit};
    use crate::geometry::tests::{Linear, drawing};
    use crate::geometry::{Prim, prims as shapes_of};

    use super::*;
    use wyck_core::math::{count_f64, count_i64};

    /// `count` points spread over the plot, a few minutes and a few dozen units apart.
    fn spread(count: usize) -> Vec<(i64, f64)> {
        (0..count)
            .map(|i| {
                (
                    600 + 240 * count_i64(i),
                    120.0 + 35.0 * count_f64(i * 3 % 5),
                )
            })
            .collect()
    }

    fn segments(shapes: &[Prim]) -> Vec<(P, P)> {
        shapes
            .iter()
            .filter_map(|p| match p {
                Prim::Segment { a, b, .. } => Some((*a, *b)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_tool_draws_something_and_offers_a_grip_per_point() {
        for tool in Tool::ALL {
            let count = if tool.is_freehand() {
                6
            } else {
                tool.anchors()
            };
            let d = drawing(tool, &spread(count));
            assert!(d.is_valid(), "{tool:?} is valid with its points");
            let shapes = shapes_of(&d, &Linear);
            assert!(!shapes.is_empty(), "{tool:?} draws nothing");
            let grips = handles(&d, &Linear).len();
            if tool.is_freehand() {
                assert_eq!(grips, 0, "{tool:?}");
            } else if tool.is_box() {
                assert_eq!(grips, 8, "{tool:?}");
            } else {
                assert_eq!(grips, count, "{tool:?}");
            }
        }
    }

    #[test]
    fn every_tool_can_be_picked_on_what_it_draws() {
        for tool in Tool::ALL {
            let count = if tool.is_freehand() {
                6
            } else {
                tool.anchors()
            };
            let d = drawing(tool, &spread(count));
            let picked = handles(&d, &Linear)
                .first()
                .is_some_and(|grip| hit(&d, &Linear, *grip, true).is_some());
            let on_line = shapes_of(&d, &Linear).iter().any(|shape| match shape {
                Prim::Segment { a, b, .. } => hit(&d, &Linear, mid(*a, *b), false).is_some(),
                _ => false,
            });
            assert!(
                picked || on_line || tool.is_freehand(),
                "{tool:?} cannot be picked"
            );
        }
    }

    #[test]
    fn a_regression_runs_through_the_closes_of_its_range() {
        // On the test projection the closes are 101 plus the bar number: a perfect line.
        let d = drawing(Tool::RegressionTrend, &[(600, 100.0), (1_200, 100.0)]);
        let lines = segments(&shapes_of(&d, &Linear));
        let base = lines
            .iter()
            .find(|(a, _)| (a.0 - 600.0).abs() < 1e-3 && (a.1 - 389.0).abs() < 1e-2)
            .expect("a line from the first close");
        assert!((base.1.0 - 1_200.0).abs() < 1e-3);
        assert!((base.1.1 - 379.0).abs() < 1e-2, "ends on the last close");
    }

    #[test]
    fn cycle_lines_repeat_across_the_plot() {
        let d = drawing(Tool::CyclicLines, &[(60, 100.0), (180, 100.0)]);
        let verticals = segments(&shapes_of(&d, &Linear))
            .into_iter()
            .filter(|(a, b)| (a.0 - b.0).abs() < 1e-3)
            .count();
        assert!((8..=10).contains(&verticals), "{verticals} lines");
    }

    #[test]
    fn an_arc_ends_on_its_two_points_and_passes_the_third() {
        let d = drawing(Tool::Arc, &[(100, 200.0), (300, 200.0), (200, 260.0)]);
        let shapes = shapes_of(&d, &Linear);
        let Some(Prim::Polyline { points, .. }) =
            shapes.iter().find(|s| matches!(s, Prim::Polyline { .. }))
        else {
            panic!("an arc is a polyline");
        };
        let (first, last) = (points[0], points[points.len() - 1]);
        assert!(dist(first, (100.0, 300.0)) < 0.5, "{first:?}");
        assert!(dist(last, (300.0, 300.0)) < 0.5, "{last:?}");
        assert!(
            points.iter().any(|p| dist(*p, (200.0, 240.0)) < 3.0),
            "passes the third point"
        );
    }

    #[test]
    fn a_curve_passes_through_its_middle_point() {
        let d = drawing(Tool::Curve, &[(100, 100.0), (300, 250.0), (500, 100.0)]);
        let shapes = shapes_of(&d, &Linear);
        let Some(Prim::Polyline { points, .. }) = shapes.first() else {
            panic!("a curve is a polyline");
        };
        assert!(points.iter().any(|p| dist(*p, (300.0, 250.0)) < 1.0));
    }

    #[test]
    fn a_fixed_gann_square_is_square_on_the_screen() {
        assert_eq!(
            fixed_square_corner((100.0, 100.0), (150.0, 400.0)),
            (400.0, 400.0)
        );
        assert_eq!(
            fixed_square_corner((100.0, 100.0), (20.0, 60.0)),
            (20.0, 20.0)
        );
    }

    #[test]
    fn the_volume_profile_marks_its_point_of_control() {
        let d = drawing(
            Tool::FixedRangeVolumeProfile,
            &[(600, 100.0), (1_200, 100.0)],
        );
        let shapes = shapes_of(&d, &Linear);
        let rows = shapes
            .iter()
            .filter(|s| matches!(s, Prim::Rect { .. }))
            .count();
        assert!(rows >= MIN_ROWS, "{rows} rows");
        assert!(
            shapes
                .iter()
                .any(|s| matches!(s, Prim::Segment { color, .. } if *color == POC))
        );
    }

    #[test]
    fn a_volume_profile_is_cut_into_the_rows_asked_for_and_the_value_area_follows_its_share() {
        let mut d = drawing(
            Tool::FixedRangeVolumeProfile,
            &[(600, 100.0), (1_200, 100.0)],
        );
        let rects = |d: &Drawing| -> Vec<Option<(u32, f32)>> {
            shapes_of(d, &Linear)
                .into_iter()
                .filter_map(|s| match s {
                    Prim::Rect { fill, .. } => Some(fill),
                    _ => None,
                })
                .collect()
        };
        d.style.profile.rows = 10;
        let few = rects(&d).len();
        assert!((1..=10).contains(&few), "{few} rows of 10");
        d.style.profile.rows = 120;
        assert!(rects(&d).len() > few, "more rows when asked for more");

        // The rows inside the value area are the brighter ones.
        d.style.profile.rows = 40;
        let bright = |d: &Drawing| {
            rects(d)
                .into_iter()
                .flatten()
                .filter(|(_, alpha)| *alpha > d.style.fill_opacity * 0.75)
                .count()
        };
        d.style.profile.value_area = 1.0;
        let all = bright(&d);
        d.style.profile.value_area = 0.1;
        let few = bright(&d);
        assert!(few < all, "{few} inside a tenth, {all} inside all of it");
    }

    #[test]
    fn the_opacity_fades_the_fills_of_a_copy_of_bars_too() {
        let mut d = drawing(
            Tool::GhostFeed,
            &[(600, 100.0), (900, 100.0), (1_200, 150.0)],
        );
        let alphas = |d: &Drawing| -> Vec<f32> {
            shapes_of(d, &Linear)
                .into_iter()
                .filter_map(|s| match s {
                    Prim::Rect {
                        fill: Some((_, alpha)),
                        ..
                    }
                    | Prim::Polygon {
                        fill: (_, alpha), ..
                    } => Some(alpha),
                    _ => None,
                })
                .collect()
        };
        let solid = alphas(&d);
        assert_ne!(solid, [] as [f32; 0]);
        d.style.opacity = 0.5;
        let faded = alphas(&d);
        assert_eq!(solid.len(), faded.len());
        for (a, b) in solid.iter().zip(&faded) {
            assert!((b - a * 0.5).abs() < 1e-5, "{a} -> {b}");
        }
    }

    #[test]
    fn a_ghost_feed_is_the_same_every_time_it_is_drawn() {
        let d = drawing(
            Tool::GhostFeed,
            &[(600, 100.0), (900, 100.0), (1_200, 150.0)],
        );
        assert_eq!(shapes_of(&d, &Linear), shapes_of(&d, &Linear));
    }

    #[test]
    fn a_bars_pattern_copies_the_bars_of_its_range() {
        let d = drawing(
            Tool::BarsPattern,
            &[(600, 100.0), (900, 100.0), (1_200, 150.0)],
        );
        let bodies = shapes_of(&d, &Linear)
            .iter()
            .filter(|s| matches!(s, Prim::Rect { fill: Some((_, a)), .. } if *a > 0.5))
            .count();
        assert_eq!(bodies, 6, "the bars of minutes 10 to 15");
    }
}
