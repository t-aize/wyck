use super::super::geometry::tests::{Linear, drawing};
use super::super::geometry::{hit, prims};
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
    assert_eq!(gann_text(1.0 / 3.0), "1/3");
    assert_eq!(gann_text(4.0), "4/1");
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
