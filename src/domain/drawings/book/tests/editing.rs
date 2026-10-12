#![cfg(test)]

use super::*;

#[test]
fn a_press_on_a_drawing_selects_it_and_dragging_moves_it_whole() {
    let mut book = with_a_line();
    book.selected = None;
    // The middle of the line is at time 300 s and price 200.
    let (x, y) = at(300.0, 200.0);
    assert_eq!(book.press(SYMBOL, TF, &Linear, x, y), Press::Taken);
    assert_eq!(book.selected(), Some(book.drawings(SYMBOL)[0].id));
    let (nx, ny) = at(420.0, 260.0);
    assert!(book.pointer_moved(SYMBOL, &Linear, nx, ny, false));
    book.release(SYMBOL, &Linear, nx, ny);
    let line = &book.drawings(SYMBOL)[0];
    // Moved by 120 s (two bars) and +60 in price.
    assert_eq!(
        line.points[0],
        Point {
            t: 240_000,
            p: 160.0
        }
    );
    assert_eq!(
        line.points[1],
        Point {
            t: 600_000,
            p: 360.0
        }
    );
}

#[test]
fn dragging_a_grip_reshapes_without_moving_the_other_end() {
    let mut book = with_a_line();
    let (gx, gy) = at(480.0, 300.0);
    book.press(SYMBOL, TF, &Linear, gx, gy);
    let (nx, ny) = at(660.0, 340.0);
    book.pointer_moved(SYMBOL, &Linear, nx, ny, false);
    book.release(SYMBOL, &Linear, nx, ny);
    let line = &book.drawings(SYMBOL)[0];
    assert_eq!(
        line.points[0],
        Point {
            t: 120_000,
            p: 100.0
        },
        "the first end stayed"
    );
    assert_eq!(
        line.points[1],
        Point {
            t: 660_000,
            p: 340.0
        }
    );
}

#[test]
fn a_press_on_nothing_clears_the_selection_and_is_left_to_the_chart() {
    let mut book = with_a_line();
    assert!(book.selected().is_some());
    let (x, y) = at(900.0, 20.0);
    assert_eq!(book.press(SYMBOL, TF, &Linear, x, y), Press::Ignored);
    assert_eq!(book.selected(), None);
}

#[test]
fn a_locked_drawing_is_selected_but_not_moved_or_deleted() {
    let mut book = with_a_line();
    assert!(book.toggle_lock(SYMBOL));
    let (x, y) = at(300.0, 200.0);
    book.press(SYMBOL, TF, &Linear, x, y);
    let (nx, ny) = at(400.0, 300.0);
    assert!(!book.pointer_moved(SYMBOL, &Linear, nx, ny, false));
    book.release(SYMBOL, &Linear, nx, ny);
    assert_eq!(
        book.drawings(SYMBOL)[0].points[0],
        Point {
            t: 120_000,
            p: 100.0
        }
    );
    assert!(!book.delete_selected(SYMBOL));
    assert!(!book.clear(SYMBOL), "clearing leaves locked drawings alone");
    assert_eq!(book.count(SYMBOL), 1);
}

#[test]
fn position_grips_keep_their_shape() {
    let mut book = book();
    book.set_tool(Some(Tool::LongPosition));
    click(&mut book, 120.0, 200.0);
    let before = book.drawings(SYMBOL)[0].clone();
    // Drag the stop grip sideways and down: only the price changes.
    let (sx, sy) = at(120.0, before.points[1].p as f32);
    book.press(SYMBOL, TF, &Linear, sx, sy);
    let (nx, ny) = at(300.0, 60.0);
    book.pointer_moved(SYMBOL, &Linear, nx, ny, false);
    book.release(SYMBOL, &Linear, nx, ny);
    let after = &book.drawings(SYMBOL)[0];
    assert_eq!(
        after.points[1],
        Point {
            t: before.points[0].t,
            p: 60.0
        }
    );
    assert_eq!(after.points[0], before.points[0]);
    assert_eq!(after.points[2], before.points[2]);
}

#[test]
fn a_right_click_finds_the_drawing_under_the_pointer() {
    let (book, _, second) = two_lines();
    // The second line runs from (60 s, 150) to (300 s, 250); its middle is (180, 200).
    let (x, y) = at(180.0, 200.0);
    assert_eq!(book.drawing_at(SYMBOL, TF, &Linear, x, y), Some(second));
    let (x, y) = at(180.0, 50.0);
    assert_eq!(book.drawing_at(SYMBOL, TF, &Linear, x, y), None);
}

#[test]
fn a_position_can_be_flipped_to_the_other_side() {
    let mut book = book();
    book.set_tool(Some(Tool::LongPosition));
    click(&mut book, 300.0, 200.0);
    let id = book.selected().unwrap();
    let before = book.get(SYMBOL, id).unwrap().clone();
    assert!(book.flip_position(SYMBOL, id));
    let flipped = book.get(SYMBOL, id).unwrap();
    assert_eq!(flipped.tool, Tool::ShortPosition);
    assert_eq!(flipped.points[0], before.points[0], "the entry stays");
    let entry = before.points[0].p;
    assert!((flipped.points[1].p - (2.0 * entry - before.points[1].p)).abs() < 1e-9);
    assert!((flipped.points[2].p - (2.0 * entry - before.points[2].p)).abs() < 1e-9);
    assert!(book.flip_position(SYMBOL, id));
    assert_eq!(book.get(SYMBOL, id).unwrap().points, before.points);
    assert!(book.undo() && book.undo());
    assert_eq!(book.get(SYMBOL, id).unwrap().tool, Tool::LongPosition);
    let line = {
        book.set_tool(Some(Tool::TrendLine));
        click(&mut book, 60.0, 100.0);
        click(&mut book, 180.0, 160.0);
        book.selected().unwrap()
    };
    assert!(!book.flip_position(SYMBOL, line), "only positions flip");
}
