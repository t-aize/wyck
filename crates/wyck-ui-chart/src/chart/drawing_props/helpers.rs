//! Names and edits of the points of a drawing.

use wyck_core::math::i64_to_f64;
use wyck_drawings::model::tool::Tool;
use wyck_drawings::model::{Drawing, Point};
use wyck_drawings::wave_names;
use wyck_market::GROUPS;
use wyck_market::PRICE_SCALE;

/// Every timeframe's code.
pub(super) fn every_timeframe() -> Vec<String> {
    GROUPS
        .iter()
        .flat_map(|(_, frames)| frames.iter().map(|f| f.code()))
        .collect()
}

/// What a point of a drawing is called in the list of coordinates.
pub(super) fn point_name(tool: Tool, index: usize) -> String {
    let names: &[&str] = match tool {
        Tool::LongPosition | Tool::ShortPosition => &["Entry", "Stop", "Target", "End"],
        Tool::Xabcd | Tool::Cypher => &["X", "A", "B", "C", "D"],
        Tool::Abcd | Tool::TrianglePattern => &["A", "B", "C", "D"],
        Tool::HeadAndShoulders => &[
            "Start",
            "Left shoulder",
            "Neckline 1",
            "Head",
            "Neckline 2",
            "Right shoulder",
            "End",
        ],
        tool if tool.is_elliott() => wave_names(tool),
        tool if tool.is_pitchfork() => &["Handle", "Tine 1", "Tine 2"],
        _ => &[],
    };
    names
        .get(index)
        .map_or_else(|| format!("Point {}", index + 1), |n| (*n).to_owned())
}

/// Which of a point's price and time can be typed (a position's stop and target sit at the
/// entry's time, its end at the entry's price).
pub(super) fn point_fields(tool: Tool, index: usize) -> (bool, bool) {
    if tool.is_position() {
        match index {
            0 => (true, true),
            1 | 2 => (true, false),
            _ => (false, true),
        }
    } else {
        (true, true)
    }
}

/// Moves point `index` to a new time and/or price, keeping a position's shape.
pub(super) fn set_point(
    drawing: &mut Drawing,
    index: usize,
    time: Option<i64>,
    price: Option<f64>,
) {
    let Some(point) = drawing.points.get_mut(index) else {
        return;
    };
    if let Some(t) = time {
        point.t = t;
    }
    if let Some(p) = price.filter(|p| p.is_finite()) {
        point.p = p;
        if drawing.tool.is_position() && index == 1 {
            drawing.style.position.atr_stop = None;
        }
        if drawing.tool.is_position() && index == 2 {
            drawing.style.position.target_rr = None;
        }
    }
    if drawing.tool.is_position()
        && let [entry, stop, target, last] = drawing.points.as_mut_slice()
    {
        stop.t = entry.t;
        target.t = entry.t;
        *last = Point {
            t: last.t.max(entry.t + 1),
            p: entry.p,
        };
    }
}

/// A raw price written as a real one with `digits` decimals.
pub(super) fn format_real(raw: f64, digits: u32) -> String {
    format!("{:.*}", digits as usize, raw / i64_to_f64(PRICE_SCALE))
}

#[cfg(test)]
mod tests {

    use super::*;
    use wyck_drawings::model::tool::Tool;
    use wyck_drawings::model::{Drawing, Point};

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
}
