use super::*;
use crate::domain::drawings::model::{Drawing, Point, Tool};

#[test]
fn a_positions_points_keep_their_shape_when_typed() {
    let mut d = Drawing::new(
        1,
        Tool::LongPosition,
        vec![
            Point { t: 1_000, p: 100.0 },
            Point { t: 1_000, p: 90.0 },
            Point { t: 1_000, p: 130.0 },
            Point { t: 9_000, p: 100.0 },
        ],
    );
    set_point(&mut d, 0, Some(2_000), Some(105.0));
    assert_eq!(d.points[1].t, 2_000);
    assert_eq!(d.points[2].t, 2_000);
    assert_eq!(d.points[3], Point { t: 9_000, p: 105.0 });
    assert_eq!(point_fields(Tool::LongPosition, 3), (false, true));
    assert_eq!(point_name(Tool::HeadAndShoulders, 3), "Head");
    assert_eq!(point_name(Tool::TrendLine, 1), "Point 2");
}
