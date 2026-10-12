#![cfg(test)]

use super::*;

#[test]
fn an_arrow_path_takes_a_point_per_click_and_ends_on_a_double_click() {
    let mut book = book();
    book.set_tool(Some(Tool::ArrowPath));
    for (seconds, price) in [(60.0, 100.0), (180.0, 200.0), (300.0, 150.0)] {
        click(&mut book, seconds, price);
        let (x, y) = at(seconds + 40.0, price + 20.0);
        book.pointer_moved(SYMBOL, &Linear, x, y, false);
    }
    assert_eq!(book.count(SYMBOL), 0, "still being made");
    click(&mut book, 420.0, 250.0);
    click(&mut book, 420.0, 250.0);

    assert_eq!(book.count(SYMBOL), 1);
    let path = &book.drawings(SYMBOL)[0];
    assert_eq!(path.tool, Tool::ArrowPath);
    assert_eq!(path.points.len(), 4);
    assert!(path.is_valid());
}

#[test]
fn every_tool_can_be_made_with_the_pointer() {
    for tool in Tool::ALL {
        let mut book = book();
        book.set_tool(Some(tool));
        if tool.is_freehand() {
            let (x, y) = at(60.0, 100.0);
            book.press(SYMBOL, TF, &Linear, x, y);
            for i in 1..8 {
                let (x, y) = at(60.0 + 12.0 * i as f32, 100.0 + 6.0 * i as f32);
                book.pointer_moved(SYMBOL, &Linear, x, y, false);
            }
            book.release(SYMBOL, &Linear, x, y);
        } else if tool == Tool::ArrowPath {
            click(&mut book, 60.0, 100.0);
            click(&mut book, 240.0, 200.0);
            assert!(book.cancel());
        } else {
            for i in 0..tool.anchors().max(1) {
                click(
                    &mut book,
                    60.0 + 180.0 * i as f32,
                    100.0 + 40.0 * (i % 3) as f32,
                );
            }
        }
        assert_eq!(book.count(SYMBOL), 1, "{tool:?} is not made");
        assert!(book.drawings(SYMBOL)[0].is_valid(), "{tool:?} is not valid");
    }
}

#[test]
fn a_note_is_typed_at_once_and_a_marker_is_not() {
    let mut book = book();
    book.set_keep_tool(true);
    book.set_tool(Some(Tool::ArrowMarkUp));
    click(&mut book, 60.0, 100.0);
    click(&mut book, 180.0, 120.0);
    assert_eq!(book.count(SYMBOL), 2);
    assert_eq!(book.tool(), Some(Tool::ArrowMarkUp), "the tool stays");
    assert!(!book.wants_text_focus());

    book.set_tool(Some(Tool::Note));
    click(&mut book, 300.0, 150.0);
    assert!(book.wants_text_focus(), "a note asks for its words");
    assert_eq!(book.tool(), None, "and gives way to the pointer");
}

#[test]
fn escape_finishes_an_arrow_path_with_two_points() {
    let mut book = book();
    book.set_tool(Some(Tool::ArrowPath));
    click(&mut book, 60.0, 100.0);
    assert!(book.cancel());
    assert_eq!(book.count(SYMBOL), 0, "one point is not a path");

    click(&mut book, 60.0, 100.0);
    click(&mut book, 200.0, 180.0);
    assert!(book.cancel());
    assert_eq!(book.count(SYMBOL), 1);
    assert_eq!(book.drawings(SYMBOL)[0].points.len(), 2);
}

#[test]
fn a_trend_line_is_made_with_two_clicks_and_then_selected() {
    let mut book = book();
    book.set_tool(Some(Tool::TrendLine));
    click(&mut book, 120.0, 100.0);
    assert_eq!(book.count(SYMBOL), 0, "one click is not enough");
    assert!(book.creating(SYMBOL).is_some());
    // The second point follows the pointer until it is clicked.
    let (x, y) = at(360.0, 200.0);
    assert!(book.pointer_moved(SYMBOL, &Linear, x, y, false));
    assert_eq!(book.creating(SYMBOL).unwrap().points[1].p, 200.0);
    click(&mut book, 360.0, 200.0);

    assert_eq!(book.count(SYMBOL), 1);
    let line = &book.drawings(SYMBOL)[0];
    assert_eq!(line.tool, Tool::TrendLine);
    assert_eq!(
        line.points[0],
        Point {
            t: 120_000,
            p: 100.0
        }
    );
    assert_eq!(
        line.points[1],
        Point {
            t: 360_000,
            p: 200.0
        }
    );
    assert_eq!(book.selected(), Some(line.id));
    assert_eq!(book.tool(), None, "back to the pointer");
}

#[test]
fn dragging_from_the_first_point_finishes_a_two_point_drawing() {
    let mut book = book();
    book.set_tool(Some(Tool::Rectangle));
    let (x0, y0) = at(60.0, 100.0);
    book.press(SYMBOL, TF, &Linear, x0, y0);
    let (x1, y1) = at(300.0, 250.0);
    book.pointer_moved(SYMBOL, &Linear, x1, y1, false);
    book.release(SYMBOL, &Linear, x1, y1);
    assert_eq!(book.count(SYMBOL), 1);
    assert_eq!(
        book.drawings(SYMBOL)[0].points[1],
        Point {
            t: 300_000,
            p: 250.0
        }
    );
}

#[test]
fn a_small_wobble_on_the_first_click_is_not_a_drag() {
    let mut book = book();
    book.set_tool(Some(Tool::TrendLine));
    let (x, y) = at(60.0, 100.0);
    book.press(SYMBOL, TF, &Linear, x, y);
    book.pointer_moved(SYMBOL, &Linear, x + 2.0, y + 1.0, false);
    book.release(SYMBOL, &Linear, x + 2.0, y + 1.0);
    assert_eq!(book.count(SYMBOL), 0, "still waiting for the second click");
    assert!(book.creating(SYMBOL).is_some());
}

#[test]
fn every_point_of_a_many_point_drawing_can_be_a_drag_or_a_click() {
    // Two drags: the first places two points, the second one more.
    let mut book = book();
    book.set_tool(Some(Tool::ParallelChannel));
    drag(&mut book, (60.0, 100.0), (360.0, 200.0));
    assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(2));
    // The point is where the pointer is let go, not where it went down.
    drag(&mut book, (360.0, 200.0), (300.0, 40.0));
    assert_eq!(book.count(SYMBOL), 1);
    let channel = &book.drawings(SYMBOL)[0];
    assert_eq!(channel.points[1].t, 360_000);
    assert_eq!(
        channel.points[2],
        Point {
            t: 300_000,
            p: 40.0
        }
    );

    // A drag, then a click for the last point.
    let mut book = self::book();
    book.set_tool(Some(Tool::ParallelChannel));
    drag(&mut book, (60.0, 100.0), (360.0, 200.0));
    click(&mut book, 240.0, 30.0);
    assert_eq!(book.count(SYMBOL), 1);
    assert_eq!(book.drawings(SYMBOL)[0].points[2].p, 30.0);
}

#[test]
fn a_middle_point_is_not_placed_until_the_button_is_let_go() {
    let mut book = book();
    book.set_tool(Some(Tool::Abcd));
    click(&mut book, 60.0, 100.0);
    drag(&mut book, (150.0, 150.0), (200.0, 220.0));
    assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(2));
    assert_eq!(book.creating(SYMBOL).unwrap().points[1].p, 220.0);

    // Held down, the point has not landed yet.
    let (x, y) = at(300.0, 120.0);
    book.press(SYMBOL, TF, &Linear, x, y);
    assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(2));
    book.release(SYMBOL, &Linear, x, y);
    assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(3));
}

#[test]
fn dragging_places_the_points_of_an_arrow_path_too() {
    let mut book = book();
    book.set_tool(Some(Tool::ArrowPath));
    drag(&mut book, (60.0, 100.0), (200.0, 180.0));
    drag(&mut book, (200.0, 180.0), (320.0, 120.0));
    assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(3));
    assert!(book.finish());
    assert_eq!(book.count(SYMBOL), 1);
    assert_eq!(book.drawings(SYMBOL)[0].points.len(), 3);
}

#[test]
fn enter_ends_an_arrow_path_and_leaves_the_other_tools_waiting() {
    let mut book = book();
    book.set_tool(Some(Tool::TrendLine));
    click(&mut book, 60.0, 100.0);
    assert!(!book.finish(), "a line still needs its second point");
    assert!(book.creating(SYMBOL).is_some());

    book.set_tool(Some(Tool::ArrowPath));
    click(&mut book, 60.0, 100.0);
    click(&mut book, 200.0, 180.0);
    assert!(book.finish());
    assert_eq!(book.count(SYMBOL), 1);
    assert!(!book.is_creating());
    assert!(!book.finish(), "nothing left to end");
}

#[test]
fn backspace_takes_back_the_last_point_then_gives_up_the_drawing() {
    let mut book = book();
    book.set_tool(Some(Tool::ParallelChannel));
    assert!(!book.remove_last_point(), "nothing is being made");
    click(&mut book, 60.0, 100.0);
    click(&mut book, 360.0, 200.0);
    assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(2));
    assert!(book.remove_last_point());
    assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(1));
    // The drawing goes on from the first point.
    click(&mut book, 240.0, 150.0);
    click(&mut book, 300.0, 40.0);
    assert_eq!(book.count(SYMBOL), 1);
    assert_eq!(book.drawings(SYMBOL)[0].points[1].t, 240_000);

    book.set_tool(Some(Tool::ArrowPath));
    click(&mut book, 60.0, 100.0);
    click(&mut book, 200.0, 180.0);
    click(&mut book, 320.0, 120.0);
    assert!(book.remove_last_point());
    assert!(book.finish());
    assert_eq!(book.drawings(SYMBOL)[1].points.len(), 2);

    book.set_tool(Some(Tool::TrendLine));
    click(&mut book, 60.0, 100.0);
    assert!(book.remove_last_point(), "only the first point: gives up");
    assert!(!book.is_creating());
}

#[test]
fn a_right_click_drops_the_drawing_being_made_and_gives_up_the_tool() {
    let mut book = book();
    assert!(!book.abort(SYMBOL), "no tool armed: the menu is free");

    book.set_tool(Some(Tool::ArrowPath));
    assert!(
        book.abort(SYMBOL),
        "a tool armed but idle still gives it up"
    );
    assert_eq!(book.tool(), None);

    book.set_tool(Some(Tool::ArrowPath));
    click(&mut book, 60.0, 100.0);
    click(&mut book, 200.0, 180.0);
    assert!(
        !book.abort("EURUSD"),
        "another chart's drawing still needs the tool, so this one is left alone"
    );
    assert_eq!(book.tool(), Some(Tool::ArrowPath));
    assert!(book.abort(SYMBOL));
    assert!(!book.is_creating());
    assert_eq!(
        book.count(SYMBOL),
        0,
        "dropped, not finished like Escape does"
    );
    assert_eq!(
        book.tool(),
        None,
        "back to the normal cursor, like TradingView"
    );
    assert!(!book.abort(SYMBOL), "the next right click is free again");
}

#[test]
fn a_double_click_still_ends_an_arrow_path_made_by_dragging() {
    let mut book = book();
    book.set_tool(Some(Tool::ArrowPath));
    drag(&mut book, (60.0, 100.0), (200.0, 180.0));
    click(&mut book, 320.0, 120.0);
    click(&mut book, 320.0, 120.0);
    assert_eq!(book.count(SYMBOL), 1);
    assert_eq!(book.drawings(SYMBOL)[0].points.len(), 3);
}

#[test]
fn a_channel_takes_three_points() {
    let mut book = book();
    book.set_tool(Some(Tool::ParallelChannel));
    click(&mut book, 60.0, 100.0);
    click(&mut book, 360.0, 200.0);
    assert_eq!(book.count(SYMBOL), 0);
    click(&mut book, 180.0, 260.0);
    assert_eq!(book.count(SYMBOL), 1);
    assert_eq!(book.drawings(SYMBOL)[0].points.len(), 3);
}

#[test]
fn single_click_tools_finish_at_once_and_a_position_gets_sensible_defaults() {
    let mut book = book();
    book.set_tool(Some(Tool::HorizontalLine));
    click(&mut book, 60.0, 150.0);
    assert_eq!(book.count(SYMBOL), 1);

    book.set_tool(Some(Tool::LongPosition));
    click(&mut book, 120.0, 200.0);
    let long = &book.drawings(SYMBOL)[1];
    assert_eq!(long.points.len(), 4);
    let (entry, stop, target) = (long.points[0].p, long.points[1].p, long.points[2].p);
    assert!(stop < entry && target > entry, "{stop} {entry} {target}");
    assert!(
        ((target - entry) / (entry - stop) - 2.0).abs() < 1e-9,
        "reward is twice the risk"
    );
    assert!(long.points[3].t > long.points[0].t);

    book.set_tool(Some(Tool::ShortPosition));
    click(&mut book, 120.0, 200.0);
    let short = &book.drawings(SYMBOL)[2];
    assert!(short.points[1].p > short.points[0].p && short.points[2].p < short.points[0].p);
}

#[test]
fn a_brush_collects_a_stroke_while_dragging() {
    let mut book = book();
    book.set_tool(Some(Tool::Brush));
    let (x, y) = at(60.0, 100.0);
    book.press(SYMBOL, TF, &Linear, x, y);
    for step in 1..=10 {
        let (mx, my) = at(60.0 + step as f32 * 20.0, 100.0 + step as f32 * 5.0);
        book.pointer_moved(SYMBOL, &Linear, mx, my, false);
    }
    // A tiny move adds no point.
    let (tx, ty) = at(260.0, 150.0);
    let moved = book.pointer_moved(SYMBOL, &Linear, tx + 1.0, ty, false);
    assert!(!moved);
    book.release(SYMBOL, &Linear, tx, ty);
    assert_eq!(book.count(SYMBOL), 1);
    assert!(book.drawings(SYMBOL)[0].points.len() >= 8);
}

#[test]
fn a_brush_that_never_moved_is_thrown_away() {
    let mut book = book();
    book.set_tool(Some(Tool::Brush));
    let (x, y) = at(60.0, 100.0);
    book.press(SYMBOL, TF, &Linear, x, y);
    book.release(SYMBOL, &Linear, x, y);
    assert_eq!(book.count(SYMBOL), 0);
}

#[test]
fn escape_gives_up_one_level_at_a_time() {
    let mut book = book();
    book.set_tool(Some(Tool::TrendLine));
    click(&mut book, 60.0, 100.0);
    assert!(book.cancel(), "the half made line");
    assert!(book.creating(SYMBOL).is_none());
    assert_eq!(book.tool(), Some(Tool::TrendLine));
    assert!(book.cancel(), "the tool");
    assert_eq!(book.tool(), None);
    assert!(!book.cancel(), "nothing left");
}

#[test]
fn a_five_point_pattern_takes_five_clicks() {
    let mut book = book();
    book.set_tool(Some(Tool::Xabcd));
    for (i, price) in [100.0, 200.0, 150.0, 180.0, 120.0].iter().enumerate() {
        assert_eq!(book.count(SYMBOL), 0);
        click(&mut book, 60.0 * (i as f32 + 1.0), *price);
    }
    assert_eq!(book.count(SYMBOL), 1);
    let pattern = &book.drawings(SYMBOL)[0];
    assert_eq!(pattern.points.len(), 5);
    assert!(pattern.is_valid());
}

#[test]
fn with_the_tool_kept_several_lines_are_drawn_in_a_row() {
    let mut book = book();
    book.set_keep_tool(true);
    book.set_tool(Some(Tool::TrendLine));
    for n in 0..3 {
        click(&mut book, 60.0, 100.0 + n as f32 * 20.0);
        assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(1));
        click(&mut book, 300.0, 150.0 + n as f32 * 20.0);
    }
    assert_eq!(book.count(SYMBOL), 3);
    assert_eq!(
        book.tool(),
        Some(Tool::TrendLine),
        "the tool is still picked"
    );
    // A text gives way at once, so its words can be typed.
    book.set_tool(Some(Tool::Text));
    click(&mut book, 100.0, 100.0);
    assert_eq!(book.tool(), None);
}

#[test]
fn shift_keeps_a_line_to_45_degrees() {
    let mut book = book();
    book.set_tool(Some(Tool::TrendLine));
    click(&mut book, 60.0, 100.0);
    // Nearly flat: it lies flat.
    let (x, y) = at(300.0, 104.0);
    book.pointer_moved(SYMBOL, &Linear, x, y, true);
    let line = book.creating(SYMBOL).unwrap();
    assert!((line.points[1].p - 100.0).abs() < 1.0, "{:?}", line.points);
}
