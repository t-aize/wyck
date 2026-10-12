#![cfg(test)]

use super::*;

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
    assert!(handles(&d, &Linear).is_empty());
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
    assert!(prims(&d, &Nowhere).is_empty());
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
