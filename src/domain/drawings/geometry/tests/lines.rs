#![cfg(test)]

use super::*;

#[test]
fn a_point_is_measured_against_a_segment() {
    assert_eq!(
        distance_to_segment((5.0, 3.0), (0.0, 0.0), (10.0, 0.0)),
        3.0
    );
    assert_eq!(
        distance_to_segment((-4.0, 3.0), (0.0, 0.0), (10.0, 0.0)),
        5.0
    );
    assert_eq!(
        distance_to_segment((1.0, 1.0), (2.0, 2.0), (2.0, 2.0)),
        2f32.sqrt()
    );
}

#[test]
fn a_line_is_extended_to_the_edges_of_the_plot() {
    let rect = Rect::new(100.0, 100.0);
    let (a, b) = extended((40.0, 50.0), (60.0, 50.0), true, true, rect).unwrap();
    assert_eq!((a, b), ((0.0, 50.0), (100.0, 50.0)));
    let (a, b) = extended((40.0, 50.0), (60.0, 50.0), false, true, rect).unwrap();
    assert_eq!(
        (a, b),
        ((40.0, 50.0), (100.0, 50.0)),
        "a ray starts at its point"
    );
    let (a, b) = extended((40.0, 40.0), (60.0, 60.0), true, true, rect).unwrap();
    assert!(
        (a.0 - 0.0).abs() < 1e-3 && (b.0 - 100.0).abs() < 1e-3,
        "{a:?} {b:?}"
    );
    assert!(
        extended((0.0, 200.0), (10.0, 210.0), true, true, rect).is_none(),
        "misses the plot"
    );
}

#[test]
fn a_trend_line_is_one_segment_between_its_points() {
    let d = drawing(Tool::TrendLine, &[(100, 100.0), (400, 300.0)]);
    let shapes = prims(&d, &Linear);
    assert_eq!(shapes.len(), 1);
    assert!(matches!(
        shapes[0],
        Prim::Segment {
            a: (100.0, 400.0),
            b: (400.0, 200.0),
            ..
        }
    ));
}

#[test]
fn the_opacity_fades_the_lines_and_leaves_the_rest() {
    let mut d = drawing(Tool::TrendLine, &[(100, 100.0), (400, 300.0)]);
    d.style.opacity = 0.5;
    let Prim::Segment { alpha, .. } = prims(&d, &Linear)[0] else {
        panic!("a segment");
    };
    assert!((alpha - 0.5).abs() < 1e-6);

    // The outline of a box fades too, its fill has an opacity of its own.
    let mut rect = drawing(Tool::Rectangle, &[(100, 100.0), (400, 300.0)]);
    rect.style.opacity = 0.4;
    let faded = prims(&rect, &Linear).into_iter().any(
        |prim| matches!(prim, Prim::Rect { stroke: Some((_, alpha)), .. } if alpha <= 0.4 + 1e-6),
    );
    assert!(faded);
}

#[test]
fn a_horizontal_line_spans_the_plot_and_says_its_price() {
    let d = drawing(Tool::HorizontalLine, &[(300, 250.0)]);
    let shapes = prims(&d, &Linear);
    assert!(matches!(
        shapes[0],
        Prim::Segment {
            a: (0.0, 250.0),
            b: (1000.0, 250.0),
            ..
        }
    ));
    assert!(matches!(&shapes[1], Prim::Label { text, .. } if text == "250.0"));
}

#[test]
fn fib_levels_sit_between_the_two_prices() {
    let d = drawing(Tool::FibRetracement, &[(100, 100.0), (500, 200.0)]);
    let shapes = prims(&d, &Linear);
    let labels: Vec<&str> = shapes
        .iter()
        .filter_map(|s| match s {
            Prim::Label { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    // 0 at the second point (200), 1 at the first (100), 0.5 in between.
    assert!(labels.contains(&"0 (200.0)"), "{labels:?}");
    assert!(labels.contains(&"1 (100.0)"), "{labels:?}");
    assert!(labels.contains(&"0.5 (150.0)"), "{labels:?}");
    assert!(labels.contains(&"0.618 (138.2)"), "{labels:?}");
}

#[test]
fn a_fib_extension_repeats_the_first_move_from_the_third_point() {
    // The move is +100 (100 -> 200); from 150 the 1.618 level is 150 + 161.8.
    let d = drawing(
        Tool::FibExtension,
        &[(100, 100.0), (300, 200.0), (500, 150.0)],
    );
    let labels: Vec<String> = prims(&d, &Linear)
        .into_iter()
        .filter_map(|s| match s {
            Prim::Label { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert!(labels.contains(&"1.618 (311.8)".to_owned()), "{labels:?}");
    assert!(labels.contains(&"0 (150.0)".to_owned()), "{labels:?}");
}

#[test]
fn a_channel_has_two_parallel_lines_a_third_point_apart() {
    let d = drawing(
        Tool::ParallelChannel,
        &[(0, 100.0), (400, 200.0), (200, 250.0)],
    );
    let segments: Vec<(P, P)> = prims(&d, &Linear)
        .into_iter()
        .filter_map(|s| match s {
            Prim::Segment { a, b, alpha, .. } => (alpha == 1.0).then_some((a, b)),
            _ => None,
        })
        .collect();
    assert_eq!(segments.len(), 2);
    let slope = |(a, b): (P, P)| (b.1 - a.1) / (b.0 - a.0);
    assert!(
        (slope(segments[0]) - slope(segments[1])).abs() < 1e-6,
        "parallel"
    );
    // The second line passes through the third point (200 s, 250).
    let (a, b) = segments[1];
    let y_at_200 = a.1 + (b.1 - a.1) * (200.0 - a.0) / (b.0 - a.0);
    assert!((y_at_200 - 250.0_f32).abs() < 1e-3, "{y_at_200}");
}

#[test]
fn a_measure_reports_the_change_the_bars_and_the_time() {
    // 100 -> 150 over 10 minutes.
    let d = drawing(Tool::Measure, &[(0, 100.0), (600, 150.0)]);
    let text = prims(&d, &Linear)
        .into_iter()
        .find_map(|s| match s {
            Prim::Label { text, .. } => Some(text),
            _ => None,
        })
        .unwrap();
    assert_eq!(text, "+50.0 (+50.00%)   10 bars, 10m");
    let down = drawing(Tool::Measure, &[(0, 200.0), (120, 150.0)]);
    let text = prims(&down, &Linear)
        .into_iter()
        .find_map(|s| match s {
            Prim::Label { text, .. } => Some(text),
            _ => None,
        })
        .unwrap();
    assert!(text.starts_with("-50.0 (-25.00%)"), "{text}");
}

#[test]
fn a_measure_follows_its_look() {
    let mut d = drawing(Tool::Measure, &[(0, 100.0), (600, 150.0)]);
    d.style.color = 0x123456;
    d.style.measure.percent = false;
    d.style.measure.bars = false;
    let shapes = prims(&d, &Linear);
    let text = shapes.iter().find_map(|s| match s {
        Prim::Label { text, .. } => Some(text.clone()),
        _ => None,
    });
    assert_eq!(text.as_deref(), Some("+50.0   10m"));
    assert!(shapes.iter().any(|s| matches!(
        s,
        Prim::Rect {
            stroke: Some((0x123456, _)),
            ..
        }
    )));

    // A move down takes the other color; with nothing to say there is no tag.
    let mut down = drawing(Tool::Measure, &[(0, 200.0), (120, 150.0)]);
    down.style.measure.down_color = 0xabcdef;
    down.style.measure.price = false;
    down.style.measure.percent = false;
    down.style.measure.bars = false;
    down.style.measure.time = false;
    let shapes = prims(&down, &Linear);
    assert!(shapes.iter().any(|s| matches!(
        s,
        Prim::Rect {
            stroke: Some((0xabcdef, _)),
            ..
        }
    )));
    assert!(!shapes.iter().any(|s| matches!(s, Prim::Label { .. })));

    // The tag takes the text of the style.
    let mut styled = drawing(Tool::Measure, &[(0, 100.0), (600, 150.0)]);
    styled.style.text_size = 20.0;
    styled.style.bold = true;
    styled.style.text_color = Some(0x00ff00);
    assert!(prims(&styled, &Linear).iter().any(|s| matches!(
        s,
        Prim::Label { size, bold: true, color: 0x00ff00, .. } if *size == 20.0
    )));
}

#[test]
fn durations_read_in_their_two_largest_units() {
    assert_eq!(duration_text(30_000), "30s");
    assert_eq!(duration_text(45 * 60_000), "45m");
    assert_eq!(duration_text(135 * 60_000), "2h 15m");
    assert_eq!(duration_text(-(3 * 86_400_000 + 4 * 3_600_000)), "3d 4h");
}

#[test]
fn a_level_can_have_its_own_line_and_its_caption_follows_the_style() {
    use crate::domain::drawings::look::{LabelSide, LevelText};

    let mut d = drawing(Tool::FibRetracement, &[(100, 100.0), (400, 200.0)]);
    let mut levels = d.levels();
    levels[1].width = 4.0;
    levels[1].dash = Some(Dash::Dotted);
    let color = levels[1].color;
    d.levels = levels;
    let shapes = prims(&d, &Linear);
    assert!(shapes.iter().any(|s| matches!(
        s,
        Prim::Segment { color: c, width, dash: Dash::Dotted, .. } if *c == color && *width == 4.0
    )));
    // The others keep the line of the drawing.
    assert!(shapes.iter().any(|s| matches!(
        s, Prim::Segment { width, dash: Dash::Solid, .. } if *width == d.style.width
    )));

    let captions = |d: &Drawing| -> Vec<(String, P, Anchor)> {
        prims(d, &Linear)
            .into_iter()
            .filter_map(|s| match s {
                Prim::Label {
                    text, at, anchor, ..
                } => Some((text, at, anchor)),
                _ => None,
            })
            .collect()
    };
    assert!(
        captions(&d)
            .iter()
            .any(|(t, _, _)| t.starts_with("0.236 (")),
        "the ratio and the price, as before"
    );
    d.style.level_text = LevelText::Percent;
    assert!(captions(&d).iter().any(|(t, _, _)| t == "23.6%"));
    d.style.level_text = LevelText::Price;
    assert!(
        captions(&d)
            .iter()
            .all(|(t, _, _)| !t.contains('(') && !t.contains('%'))
    );
    d.style.level_text = LevelText::Ratio;
    assert!(captions(&d).iter().any(|(t, _, _)| t == "0.236"));

    // At the left of the levels by default, at the right when asked.
    assert!(
        captions(&d)
            .iter()
            .all(|(_, at, a)| at.0 < 100.0 && *a == Anchor::Right)
    );
    d.style.label_side = LabelSide::Right;
    assert!(
        captions(&d)
            .iter()
            .all(|(_, at, a)| at.0 > 400.0 && *a == Anchor::Left)
    );
}
