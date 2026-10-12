#![cfg(test)]

use super::*;

#[test]
fn shift_and_a_click_builds_a_selection_and_a_second_click_takes_one_out() {
    let mut book = book();
    apart_lines(&mut book);
    let ids: Vec<u64> = book.drawings(SYMBOL).iter().map(|d| d.id).collect();
    let (x, y) = at(150.0, 130.0);
    book.press_adding(SYMBOL, TF, &Linear, x, y, true, &|d| Some(d.clone()));
    book.release(SYMBOL, &Linear, x, y);
    let (x, y) = at(150.0, 330.0);
    book.press_adding(SYMBOL, TF, &Linear, x, y, true, &|d| Some(d.clone()));
    book.release(SYMBOL, &Linear, x, y);
    assert_eq!(book.selected_count(), 2);
    assert!(ids.iter().all(|id| book.is_selected(*id)));

    book.press_adding(SYMBOL, TF, &Linear, x, y, true, &|d| Some(d.clone()));
    book.release(SYMBOL, &Linear, x, y);
    assert_eq!(book.selected_count(), 1);
    assert!(!book.is_selected(ids[1]));
}

#[test]
fn dragging_one_of_a_group_moves_all_of_it_as_one_undo_step() {
    let mut book = book();
    apart_lines(&mut book);
    assert!(book.select_all(SYMBOL, TF));
    let before: Vec<f64> = book
        .drawings(SYMBOL)
        .iter()
        .map(|d| d.points[0].p)
        .collect();
    let (x, y) = at(150.0, 130.0);
    book.press(SYMBOL, TF, &Linear, x, y);
    let (x2, y2) = at(150.0, 150.0);
    book.pointer_moved(SYMBOL, &Linear, x2, y2, false);
    book.release(SYMBOL, &Linear, x2, y2);
    let after: Vec<f64> = book
        .drawings(SYMBOL)
        .iter()
        .map(|d| d.points[0].p)
        .collect();
    let moved: Vec<f64> = before.iter().zip(&after).map(|(a, b)| b - a).collect();
    assert!(moved[0].abs() > 0.0);
    assert!((moved[0] - moved[1]).abs() < 1e-6, "both moved the same");
    assert_eq!(book.selected_count(), 2, "the group is kept");

    assert!(book.undo());
    let restored: Vec<f64> = book
        .drawings(SYMBOL)
        .iter()
        .map(|d| d.points[0].p)
        .collect();
    assert_eq!(restored, before);
}

#[test]
fn a_box_selects_what_has_a_point_inside_it() {
    let mut book = book();
    apart_lines(&mut book);
    let low = book.select_in_box(SYMBOL, TF, &Linear, at(0.0, 80.0), at(300.0, 200.0), false);
    assert!(low);
    assert_eq!(book.selected_count(), 1);
    book.select_in_box(SYMBOL, TF, &Linear, at(0.0, 280.0), at(300.0, 400.0), true);
    assert_eq!(book.selected_count(), 2, "added to the selection");
}

#[test]
fn delete_and_style_changes_reach_every_selected_drawing() {
    let mut book = book();
    apart_lines(&mut book);
    book.select_all(SYMBOL, TF);
    assert!(book.set_color(SYMBOL, 0x123456));
    assert!(
        book.drawings(SYMBOL)
            .iter()
            .all(|d| d.style.color == 0x123456)
    );
    assert!(book.delete_selected(SYMBOL));
    assert_eq!(book.count(SYMBOL), 0);
    assert!(book.undo());
    assert_eq!(book.count(SYMBOL), 2);
}

#[test]
fn a_copy_pastes_on_another_symbol_and_each_paste_lands_further() {
    let mut book = book();
    apart_lines(&mut book);
    book.select_all(SYMBOL, TF);
    assert!(book.copy_selection(SYMBOL));
    assert!(book.paste("EURUSD", &Linear));
    assert_eq!(book.count("EURUSD"), 2);
    assert_eq!(book.selected_count(), 2, "the copies are selected");
    assert!(book.paste("EURUSD", &Linear));
    let list = book.drawings("EURUSD");
    assert_eq!(list.len(), 4);
    assert_ne!(list[0].points[0], list[2].points[0]);
    assert_eq!(book.count(SYMBOL), 2, "the originals stay");
}

#[test]
fn the_arrows_move_the_selection_but_not_a_locked_drawing() {
    let mut book = book();
    apart_lines(&mut book);
    let id = book.drawings(SYMBOL)[0].id;
    book.select(Some(id));
    let start = book.drawings(SYMBOL)[0].points[0];
    assert!(book.nudge(SYMBOL, &Linear, 1.0, 1.0));
    let moved = book.drawings(SYMBOL)[0].points[0];
    assert!(moved.t > start.t && moved.p > start.p);
    assert!(book.toggle_lock(SYMBOL));
    assert!(
        !book.nudge(SYMBOL, &Linear, 1.0, 1.0),
        "a locked drawing stays"
    );
}
