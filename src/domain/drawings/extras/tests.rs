use super::super::geometry::tests::{Linear, drawing};
use super::super::geometry::{Prim, handles, hit, prims as shapes_of};
use super::*;

/// `count` points spread over the plot, a few minutes and a few dozen units apart.
fn spread(count: usize) -> Vec<(i64, f64)> {
    (0..count)
        .map(|i| (600 + 240 * i as i64, 120.0 + 35.0 * ((i * 3 % 5) as f64)))
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
fn a_circle_goes_through_three_points() {
    let (center, radius) = circumcircle((0.0, 0.0), (10.0, 0.0), (5.0, 5.0)).unwrap();
    assert!((center.0 - 5.0).abs() < 1e-3 && center.1.abs() < 1e-3);
    assert!((radius - 5.0).abs() < 1e-3);
    assert!(circumcircle((0.0, 0.0), (5.0, 5.0), (10.0, 10.0)).is_none());
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
                } => Some(alpha),
                Prim::Polygon {
                    fill: (_, alpha), ..
                } => Some(alpha),
                _ => None,
            })
            .collect()
    };
    let solid = alphas(&d);
    assert!(!solid.is_empty());
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

#[test]
fn a_table_reads_cells_from_bars_and_lines() {
    let cells = table_cells("a | b\nc");
    assert_eq!(cells, vec![vec!["a", "b"], vec!["c", ""]]);
    assert_eq!(table_cells("  ").len(), 3, "an empty table shows a sample");
}

#[test]
fn an_unknown_icon_is_a_star() {
    assert_eq!(icon_key(""), "star");
    assert_eq!(icon_key("nonsense"), "star");
    assert_eq!(icon_key(" heart "), "heart");
}

#[test]
fn a_note_grows_with_its_words() {
    let mut d = drawing(Tool::Note, &[(300, 200.0)]);
    d.text = "one\nsecond line".to_owned();
    let Some(Prim::Board { rows, .. }) = shapes_of(&d, &Linear)
        .into_iter()
        .find(|s| matches!(s, Prim::Board { .. }))
    else {
        panic!("a note is a board");
    };
    assert_eq!(rows, vec!["one".to_owned(), "second line".to_owned()]);
}
