//! Turning a drawing into shapes on the screen, and finding out what is under the pointer.
//!
//! Everything works through a [`Projection`], which knows how the chart maps time and price to
//! screen positions. That is the only thing tied to the chart, so this is plain arithmetic and is
//! tested with a simple linear projection. The shapes themselves and the helpers they are built
//! from are in `prim`; this module picks the shapes of each tool (`shapes`), finds what
//! is under the pointer (`hit`) and adds the measures a tool shows (`measure`).
//!
//! Positions are in the plot's own coordinates, the top left of the plot being (0, 0).

pub(crate) mod hit;
pub(crate) mod measure;
pub(crate) mod shapes;

pub use hit::handles;

pub use crate::prim::{
    Anchor, BarView, LINE_REACH, P, Part, Prim, Projection, Rect, anchors, board_metrics,
    board_size, duration_text, extended, label_size,
};
pub(crate) use crate::prim::{HANDLE_REACH, arrow_head, distance_to_segment, lead, seg};

use super::model::Drawing;

/// The shapes of a drawing (without grips), as they show when it is selected.
pub(crate) fn prims(drawing: &Drawing, proj: &dyn Projection) -> Vec<Prim> {
    prims_with(drawing, proj, true)
}

/// The shapes of a drawing (without grips). `selected` says whether it is the one selected: a
/// position can keep its stats for then.
pub fn prims_with(drawing: &Drawing, proj: &dyn Projection, selected: bool) -> Vec<Prim> {
    prims_at(drawing, proj, selected, wyck_core::text_scale::factor())
}

/// [`prims_with`] with the text at `factor` times its size. The text of a drawing follows the
/// scale of the text (see [`wyck_core::text_scale`]); the size that is saved does not change.
fn prims_at(drawing: &Drawing, proj: &dyn Projection, selected: bool, factor: f32) -> Vec<Prim> {
    if (factor - 1.0).abs() > f32::EPSILON {
        let mut scaled = drawing.clone();
        scaled.style.text_size *= factor;
        return shapes::shapes_of(&scaled, proj, selected);
    }
    shapes::shapes_of(drawing, proj, selected)
}

#[cfg(test)]
pub(super) mod tests {
    use super::hit::*;

    use super::*;
    use crate::model::style::{Dash, Style};
    use crate::model::tool::Tool;
    use crate::model::{Drawing, Point};
    use wyck_core::math::{f32_of, f64_to_i64, i64_to_f64};

    /// A projection where time is 1 pixel per second from 0, and price is 1 pixel per unit
    /// upwards from the bottom of a 1000 by 500 plot. Bars are one minute long.
    pub(crate) struct Linear;

    impl Projection for Linear {
        fn plot(&self) -> Rect {
            Rect::new(1_000.0, 500.0)
        }
        fn to_screen(&self, point: Point) -> Option<P> {
            Some((f32_of(i64_to_f64(point.t / 1_000)), 500.0 - f32_of(point.p)))
        }
        fn point_at(&self, x: f32, y: f32, _magnet: bool) -> Option<Point> {
            // Snap the time to whole minutes, like snapping to a bar.
            let seconds = f64_to_i64(f64::from((x / 60.0).round())) * 60;
            Some(Point {
                t: seconds * 1_000,
                p: f64::from(500.0 - y),
            })
        }
        fn index_of(&self, time_ms: i64) -> Option<f64> {
            Some(i64_to_f64(time_ms) / 60_000.0)
        }
        fn shift_bars(&self, time_ms: i64, bars: f64) -> Option<i64> {
            Some(time_ms + f64_to_i64((bars * 60_000.0).round()))
        }
        fn format_price(&self, price: f64) -> String {
            format!("{price:.1}")
        }
        fn price_span(&self) -> f64 {
            500.0
        }
        fn bars_between(&self, from_ms: i64, to_ms: i64) -> Vec<BarView> {
            // A bar a minute, opening at that minute at a price of 100 plus its number.
            let first = (from_ms.max(0) + 59_999) / 60_000;
            let last = (to_ms / 60_000).min(first + 1_999);
            (first..=last)
                .map(|i| BarView {
                    time: i * 60_000,
                    x: f32_of(i64_to_f64(i * 60)),
                    open: 100.0 + i64_to_f64(i),
                    high: 103.0 + i64_to_f64(i),
                    low: 98.0 + i64_to_f64(i),
                    close: 101.0 + i64_to_f64(i),
                    volume: 10.0,
                })
                .collect()
        }
        fn y_of(&self, price: f64) -> f32 {
            500.0 - f32_of(price)
        }
        fn pip(&self) -> f64 {
            0.01
        }
    }

    pub(crate) fn drawing(tool: Tool, points: &[(i64, f64)]) -> Drawing {
        let mut drawing = Drawing::new(
            1,
            tool,
            points
                .iter()
                .map(|&(t, p)| Point { t: t * 1_000, p })
                .collect(),
        );
        drawing.style = Style {
            width: 1.0,
            ..tool.default_style()
        };
        drawing
    }

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
    #[expect(
        clippy::float_cmp,
        reason = "the values compared are copied from the style into the shapes unchanged, so they are exact"
    )]
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
    #[expect(
        clippy::float_cmp,
        reason = "the values compared are copied from the style into the shapes unchanged, so they are exact"
    )]
    fn a_measure_follows_its_look() {
        let mut d = drawing(Tool::Measure, &[(0, 100.0), (600, 150.0)]);
        d.style.color = 0x12_3456;
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
                stroke: Some((0x12_3456, _)),
                ..
            }
        )));

        // A move down takes the other color; with nothing to say there is no tag.
        let mut down = drawing(Tool::Measure, &[(0, 200.0), (120, 150.0)]);
        down.style.measure.down_color = 0xab_cdef;
        down.style.measure.price = false;
        down.style.measure.percent = false;
        down.style.measure.bars = false;
        down.style.measure.time = false;
        let shapes = prims(&down, &Linear);
        assert!(shapes.iter().any(|s| matches!(
            s,
            Prim::Rect {
                stroke: Some((0xab_cdef, _)),
                ..
            }
        )));
        assert!(!shapes.iter().any(|s| matches!(s, Prim::Label { .. })));

        // The tag takes the text of the style.
        let mut styled = drawing(Tool::Measure, &[(0, 100.0), (600, 150.0)]);
        styled.style.text_size = 20.0;
        styled.style.bold = true;
        styled.style.text_color = Some(0x00_ff00);
        assert!(prims(&styled, &Linear).iter().any(|s| matches!(
            s,
            Prim::Label { size, bold: true, color: 0x00_ff00, .. } if *size == 20.0
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
    #[expect(
        clippy::float_cmp,
        reason = "the values compared are copied from the style into the shapes unchanged, so they are exact"
    )]
    fn a_level_can_have_its_own_line_and_its_caption_follows_the_style() {
        use crate::look::{LabelSide, LevelText};

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

    #[test]
    fn a_pointer_on_a_line_hits_the_body_and_near_an_end_hits_the_grip() {
        let d = drawing(Tool::TrendLine, &[(100, 100.0), (500, 300.0)]);
        // Screen: (100, 400) to (500, 200). The middle is (300, 300).
        assert_eq!(hit(&d, &Linear, (300.0, 302.0), true), Some(Part::Body));
        assert_eq!(hit(&d, &Linear, (300.0, 330.0), true), None);
        assert_eq!(
            hit(&d, &Linear, (104.0, 396.0), true),
            Some(Part::Handle(0))
        );
        assert_eq!(
            hit(&d, &Linear, (503.0, 199.0), true),
            Some(Part::Handle(1))
        );
        // Without grips the end is just the body.
        assert_eq!(hit(&d, &Linear, (104.0, 396.0), false), Some(Part::Body));
    }

    #[test]
    fn a_locked_drawing_has_no_grips_but_can_still_be_picked() {
        let mut d = drawing(Tool::TrendLine, &[(100, 100.0), (500, 300.0)]);
        d.locked = true;
        assert_eq!(hit(&d, &Linear, (100.0, 400.0), true), Some(Part::Body));
    }

    #[test]
    fn a_filled_shape_is_picked_from_inside_and_an_empty_one_only_by_its_edge() {
        let mut d = drawing(Tool::Rectangle, &[(100, 100.0), (400, 300.0)]);
        // Screen rect: x 100..400, y 200..400.
        assert_eq!(hit(&d, &Linear, (250.0, 300.0), false), Some(Part::Body));
        d.style.fill = false;
        assert_eq!(hit(&d, &Linear, (250.0, 300.0), false), None);
        assert_eq!(hit(&d, &Linear, (250.0, 201.0), false), Some(Part::Body));
        assert_eq!(hit(&d, &Linear, (900.0, 300.0), false), None);
    }

    #[test]
    fn an_ellipse_is_picked_inside_and_on_its_rim() {
        let d = drawing(Tool::Ellipse, &[(100, 100.0), (500, 300.0)]);
        // Center (300, 300), radii 200 and 100.
        assert_eq!(hit(&d, &Linear, (300.0, 300.0), false), Some(Part::Body));
        assert_eq!(hit(&d, &Linear, (150.0, 300.0), false), Some(Part::Body));
        assert_eq!(
            hit(&d, &Linear, (120.0, 210.0), false),
            None,
            "the corner of the box is outside"
        );
    }

    #[test]
    fn a_brush_is_picked_along_its_stroke_and_has_no_grips() {
        let d = drawing(Tool::Brush, &[(0, 100.0), (100, 200.0), (200, 100.0)]);
        assert_eq!(handles(&d, &Linear), [] as [(f32, f32); 0]);
        assert_eq!(hit(&d, &Linear, (50.0, 350.0), true), Some(Part::Body));
        assert_eq!(hit(&d, &Linear, (50.0, 100.0), true), None);
    }

    #[test]
    fn a_drawing_with_no_way_to_place_it_draws_nothing() {
        struct Nowhere;
        impl Projection for Nowhere {
            fn plot(&self) -> Rect {
                Rect::new(10.0, 10.0)
            }
            fn to_screen(&self, _: Point) -> Option<P> {
                None
            }
            fn point_at(&self, _: f32, _: f32, _: bool) -> Option<Point> {
                None
            }
            fn index_of(&self, _: i64) -> Option<f64> {
                None
            }
            fn shift_bars(&self, _: i64, _: f64) -> Option<i64> {
                None
            }
            fn format_price(&self, _: f64) -> String {
                String::new()
            }
            fn price_span(&self) -> f64 {
                1.0
            }
        }
        let d = drawing(Tool::TrendLine, &[(0, 1.0), (1, 2.0)]);
        assert_eq!(prims(&d, &Nowhere), [] as [Prim; 0]);
        assert_eq!(hit(&d, &Nowhere, (1.0, 1.0), true), None);
    }

    #[test]
    fn the_text_of_a_drawing_follows_the_scale_of_the_text_and_the_saved_size_stays() {
        let mut styled = drawing(Tool::Measure, &[(0, 100.0), (600, 150.0)]);
        styled.style.text_size = 20.0;
        let sizes = |factor: f32| -> Vec<f32> {
            prims_at(&styled, &Linear, true, factor)
                .into_iter()
                .filter_map(|p| match p {
                    Prim::Label { size, .. } => Some(size),
                    _ => None,
                })
                .collect()
        };
        assert!(sizes(1.0).contains(&20.0));
        assert!(sizes(1.5).iter().any(|s| (*s - 30.0).abs() < 1e-4));
        assert!(sizes(0.8).iter().any(|s| (*s - 16.0).abs() < 1e-4));
        assert_eq!(styled.style.text_size, 20.0, "what is saved is not touched");
    }

    #[test]
    fn a_marker_grows_with_its_scale() {
        let height = |scale: f32| {
            let mut d = drawing(Tool::ArrowMarkUp, &[(100, 100.0)]);
            d.style.scale = scale;
            let Some(Prim::Polygon { points, .. }) = prims(&d, &Linear).into_iter().next() else {
                panic!("a polygon");
            };
            let ys = points.iter().map(|p| p.1);
            ys.clone().fold(f32::MIN, f32::max) - ys.fold(f32::MAX, f32::min)
        };
        assert!((height(2.0) - 2.0 * height(1.0)).abs() < 1e-3);
        assert!(height(0.5) < height(1.0));
    }

    /// The words of the labels of a position, from top to bottom of the list.
    fn position_labels(d: &Drawing, selected: bool) -> Vec<String> {
        prims_with(d, &Linear, selected)
            .into_iter()
            .filter_map(|s| match s {
                Prim::Label { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }

    fn long_position() -> Drawing {
        // Entry 100, stop 90, target 130: with the defaults, 1% of 10 000 risked over 10 is 10
        // units, so the stop loses 100 and the target gains 300.
        drawing(
            Tool::LongPosition,
            &[(100, 100.0), (100, 90.0), (100, 130.0), (400, 100.0)],
        )
    }

    #[test]
    fn a_position_shows_its_risk_and_reward() {
        // Long: entry 100, stop 90, target 130.
        let mut d = drawing(
            Tool::LongPosition,
            &[(100, 100.0), (100, 90.0), (100, 130.0), (400, 100.0)],
        );
        d.style.position.compact = false;
        let labels: Vec<String> = prims_with(&d, &Linear, true)
            .into_iter()
            .filter_map(|s| match s {
                Prim::Label { text, .. } => Some(text),
                _ => None,
            })
            .collect();
        assert!(labels.iter().any(|t| t.contains("R/R 3.00")), "{labels:?}");
        assert!(
            labels
                .iter()
                .any(|t| t.starts_with("Target 130.0 (+30.00%)")),
            "{labels:?}"
        );
        assert!(
            labels.iter().any(|t| t.starts_with("Stop 90.0 (-10.00%)")),
            "{labels:?}"
        );

        // A long with the stop above the entry is said to be wrong, not given a ratio.
        let mut wrong = drawing(
            Tool::LongPosition,
            &[(100, 100.0), (100, 120.0), (100, 130.0), (400, 100.0)],
        );
        wrong.style.position.compact = false;
        assert!(prims_with(&wrong, &Linear, true).iter().any(|s| matches!(
            s, Prim::Label { text, .. } if text.contains("wrong sides")
        )));
        // A short mirrors it.
        let mut short = drawing(
            Tool::ShortPosition,
            &[(100, 100.0), (100, 110.0), (100, 80.0), (400, 100.0)],
        );
        short.style.position.compact = false;
        assert!(prims_with(&short, &Linear, true).iter().any(|s| matches!(
            s, Prim::Label { text, .. } if text.starts_with("Short | Qty") && text.contains("R/R 2.00")
        )));
    }

    #[test]
    fn a_position_writes_its_size_and_its_money() {
        let mut d = long_position();
        d.style.position.compact = false;
        let labels = position_labels(&d, true);
        assert!(
            labels
                .iter()
                .any(|t| t == "Long | Qty 10 | R/R 3.00 | Risk 100.00"),
            "{labels:?}"
        );
        assert!(labels.iter().any(|t| t.ends_with("+300.00")), "{labels:?}");
        assert!(labels.iter().any(|t| t.ends_with("-100.00")), "{labels:?}");
    }

    #[test]
    fn the_switches_choose_what_a_position_writes() {
        let mut d = long_position();
        d.style.position.compact = false;
        d.style.position.show_qty = false;
        d.style.position.show_risk = false;
        d.style.position.show_amounts = false;
        d.style.position.show_price = false;
        let labels = position_labels(&d, true);
        assert!(labels.iter().any(|t| t == "Long | R/R 3.00"), "{labels:?}");
        assert!(labels.iter().any(|t| t == "Target (+30.00%)"), "{labels:?}");

        // Compact: one short figure per tag.
        let mut d = long_position();
        d.style.position.compact = true;
        let labels = position_labels(&d, true);
        assert!(labels.iter().any(|t| t == "Long | 10 | 3.0R"), "{labels:?}");
        assert!(labels.iter().any(|t| t == "+300.00"), "{labels:?}");
        assert!(labels.iter().any(|t| t == "-100.00"), "{labels:?}");

        // A currency follows the amounts.
        let mut d = long_position();
        d.style.position.currency = "USD".to_owned();
        assert!(
            position_labels(&d, true)
                .iter()
                .any(|t| t.ends_with("+300.00 USD"))
        );
    }

    #[test]
    fn visible_tags_show_the_full_stats_when_requested() {
        let mut d = long_position();
        d.style.position.always_stats = true;
        d.style.position.compact = false;
        let idle = position_labels(&d, false);
        assert!(idle.iter().any(|t| t.contains("Qty 10")), "{idle:?}");
        let picked = position_labels(&d, true);
        assert_eq!(picked, idle);
    }

    #[test]
    fn position_tags_show_only_when_selected_by_default() {
        for tool in [Tool::LongPosition, Tool::ShortPosition] {
            let mut d = long_position();
            d.tool = tool;
            assert_eq!(position_labels(&d, false), [] as [String; 0]);
            assert!(
                prims_with(&d, &Linear, false)
                    .iter()
                    .any(|prim| matches!(prim, Prim::Rect { .. }))
            );
            assert_eq!(position_labels(&d, true).len(), 3);
            d.style.position.always_stats = true;
            assert_eq!(position_labels(&d, false).len(), 3);
            d.style.labels = false;
            assert_eq!(position_labels(&d, true), [] as [String; 0]);
        }
    }

    #[test]
    fn positions_can_show_pips_in_compact_and_full_tags() {
        for (tool, stop, target) in [
            (Tool::LongPosition, 99.9, 100.3),
            (Tool::ShortPosition, 100.1, 99.7),
        ] {
            let mut d = drawing(
                tool,
                &[(100, 100.0), (100, stop), (100, target), (400, 100.0)],
            );
            d.style.position.show_pips = true;
            let compact = position_labels(&d, true);
            assert!(
                compact.iter().any(|text| text.contains("30.0 pips")),
                "{compact:?}"
            );
            assert!(
                compact.iter().any(|text| text.contains("10.0 pips")),
                "{compact:?}"
            );
            d.style.position.compact = false;
            let full = position_labels(&d, true);
            assert!(
                full.iter().any(|text| text.contains("30.0 pips")),
                "{full:?}"
            );
            assert!(
                full.iter().any(|text| text.contains("10.0 pips")),
                "{full:?}"
            );
        }
    }

    #[test]
    fn positions_move_from_the_zones_but_not_their_tags() {
        for (tool, stop, target, stop_y, target_y) in [
            (Tool::LongPosition, 90.0, 130.0, 405.0, 385.0),
            (Tool::ShortPosition, 110.0, 80.0, 395.0, 415.0),
        ] {
            let mut d = drawing(
                tool,
                &[(100, 100.0), (100, stop), (100, target), (400, 100.0)],
            );
            assert_eq!(hit(&d, &Linear, (150.0, stop_y), true), Some(Part::Body));
            assert_eq!(hit(&d, &Linear, (150.0, target_y), true), Some(Part::Body));
            assert_eq!(hit(&d, &Linear, (250.0, 400.0), true), None);
            assert_eq!(
                hit(&d, &Linear, (100.0, 400.0), true),
                Some(Part::Handle(0))
            );
            d.style.position.always_stats = true;
            assert_eq!(hit(&d, &Linear, (250.0, 400.0), false), None);
        }
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "the values compared are copied from the style into the shapes unchanged, so they are exact"
    )]
    fn a_position_takes_its_colors_and_lines_from_its_settings() {
        let mut d = long_position();
        d.style.position.target_color = 0x12_3456;
        d.style.position.stop_color = 0x65_4321;
        d.style.width = 3.0;
        d.style.fill_opacity = 0.5;
        let shapes = prims(&d, &Linear);
        let fills: Vec<(u32, f32)> = shapes
            .iter()
            .filter_map(|s| match s {
                Prim::Rect {
                    fill: Some(fill), ..
                } => Some(*fill),
                _ => None,
            })
            .collect();
        assert_eq!(fills, vec![(0x12_3456, 0.5), (0x65_4321, 0.5)]);
        assert!(shapes.iter().any(|s| matches!(
            s, Prim::Segment { color: 0x12_3456, width, .. } if *width == 3.0
        )));

        // No background, no tags.
        d.style.fill = false;
        d.style.labels = false;
        let bare = prims(&d, &Linear);
        assert!(
            !bare
                .iter()
                .any(|s| matches!(s, Prim::Rect { .. } | Prim::Label { .. }))
        );
        assert_eq!(bare.len(), 3, "the three lines are left");
    }
}
