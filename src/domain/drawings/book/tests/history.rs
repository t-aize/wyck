#![cfg(test)]

use super::*;

#[test]
fn undo_and_redo_walk_through_every_change() {
    let mut book = with_a_line();
    assert_eq!(book.count(SYMBOL), 1);
    assert!(book.set_color(SYMBOL, 0xff6467));
    assert_eq!(book.drawings(SYMBOL)[0].style.color, 0xff6467);
    assert!(book.undo());
    assert_eq!(
        book.drawings(SYMBOL)[0].style.color,
        Tool::TrendLine.default_style().color
    );
    assert!(book.undo());
    assert_eq!(book.count(SYMBOL), 0);
    assert_eq!(book.selected(), None, "the selected drawing is gone");
    assert!(!book.undo());
    assert!(book.redo());
    assert_eq!(book.count(SYMBOL), 1);
    assert!(book.redo());
    assert_eq!(book.drawings(SYMBOL)[0].style.color, 0xff6467);
    assert!(!book.redo());
    // A new change forgets what could have been redone.
    book.undo();
    book.selected = book.drawings(SYMBOL).first().map(|d| d.id);
    assert!(book.set_width(SYMBOL, 4.0));
    assert!(!book.can_redo());
}

#[test]
fn a_move_is_one_undo_step_however_long_the_drag() {
    let mut book = with_a_line();
    let steps_before = book.undo.len();
    let (x, y) = at(300.0, 200.0);
    book.press(SYMBOL, TF, &Linear, x, y);
    for step in 1..=5 {
        let (nx, ny) = at(300.0 + step as f32 * 60.0, 200.0);
        book.pointer_moved(SYMBOL, &Linear, nx, ny, false);
    }
    let (nx, ny) = at(600.0, 200.0);
    book.release(SYMBOL, &Linear, nx, ny);
    assert_eq!(book.undo.len(), steps_before + 1);
    book.undo();
    assert_eq!(
        book.drawings(SYMBOL)[0].points[0],
        Point {
            t: 120_000,
            p: 100.0
        }
    );
}

#[test]
fn a_press_and_release_without_moving_records_nothing() {
    let mut book = with_a_line();
    let steps_before = book.undo.len();
    let revision = book.revision();
    let (x, y) = at(300.0, 200.0);
    book.press(SYMBOL, TF, &Linear, x, y);
    assert!(!book.release(SYMBOL, &Linear, x, y));
    assert_eq!(book.undo.len(), steps_before);
    assert_eq!(book.revision(), revision, "nothing to save");
}

#[test]
fn deleting_clearing_and_duplicating() {
    let mut book = with_a_line();
    assert!(book.duplicate(SYMBOL, &Linear));
    assert_eq!(book.count(SYMBOL), 2);
    let ids: Vec<u64> = book.drawings(SYMBOL).iter().map(|d| d.id).collect();
    assert_ne!(ids[0], ids[1]);
    assert_eq!(book.selected(), Some(ids[1]), "the copy is selected");
    assert!(book.delete_selected(SYMBOL));
    assert_eq!(book.count(SYMBOL), 1);
    assert!(book.clear(SYMBOL));
    assert_eq!(book.count(SYMBOL), 0);
    assert!(book.undo(), "clearing is one step");
    assert_eq!(book.count(SYMBOL), 1);
}

#[test]
fn drawings_belong_to_their_symbol() {
    let book = with_a_line();
    assert_eq!(book.count("EURUSD"), 0);
    assert!(book.creating("EURUSD").is_none());
    let (x, y) = at(300.0, 200.0);
    assert_eq!(book.hover("EURUSD", TF, &Linear, x, y), None);
    assert!(book.hover(SYMBOL, TF, &Linear, x, y).is_some());
}

#[test]
fn text_is_edited_without_undo_steps_and_asks_for_the_keyboard() {
    let mut book = book();
    book.set_tool(Some(Tool::Text));
    click(&mut book, 120.0, 200.0);
    assert!(book.take_text_focus());
    assert!(!book.take_text_focus(), "asked once");
    let steps = book.undo.len();
    assert!(book.set_text(SYMBOL, "Breakout"));
    assert!(!book.set_text(SYMBOL, "Breakout"));
    assert_eq!(book.undo.len(), steps);
    assert_eq!(book.drawings(SYMBOL)[0].text, "Breakout");
}

#[test]
fn the_book_round_trips_through_its_document() {
    let mut book = with_a_line();
    book.set_tool(Some(Tool::HorizontalLine));
    click(&mut book, 60.0, 150.0);
    let doc = book.to_doc();
    let text = toml::to_string_pretty(&doc).unwrap();
    let mut reloaded = Book::from_doc(toml::from_str(&text).unwrap());
    assert_eq!(reloaded.drawings(SYMBOL), book.drawings(SYMBOL));
    // New drawings do not reuse an id.
    reloaded.set_tool(Some(Tool::HorizontalLine));
    click(&mut reloaded, 60.0, 50.0);
    let ids: Vec<u64> = reloaded.drawings(SYMBOL).iter().map(|d| d.id).collect();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(ids.len(), unique.len());
}

#[test]
fn a_symbol_cannot_hold_more_than_the_limit() {
    let mut book = book();
    book.set_drawing_limit(3);
    for _ in 0..5 {
        book.set_tool(Some(Tool::HorizontalLine));
        click(&mut book, 60.0, 150.0);
    }
    assert_eq!(book.count(SYMBOL), 3);
    let mut loaded = Book::from_doc(book.to_doc());
    loaded.set_drawing_limit(2);
    assert_eq!(
        loaded.count(SYMBOL),
        3,
        "lowering a limit keeps saved drawings"
    );
}

#[test]
fn a_settings_dialog_previews_then_keeps_as_one_step_or_puts_back() {
    let (mut book, first, _) = two_lines();
    let before = book.drawings(SYMBOL).to_vec();
    let steps = book.undo.len();
    let mut changed = book.get(SYMBOL, first).unwrap().clone();
    changed.style.width = 4.0;
    assert!(book.preview(SYMBOL, changed.clone()));
    changed.style.color = 0x123456;
    assert!(book.preview(SYMBOL, changed));
    assert_eq!(book.undo.len(), steps, "previews are not undo steps");
    assert!(book.settle(SYMBOL, before.clone()));
    assert_eq!(book.undo.len(), steps + 1, "keeping is one step");
    assert!(book.undo());
    assert_eq!(book.drawings(SYMBOL), before.as_slice());

    let mut changed = book.get(SYMBOL, first).unwrap().clone();
    changed.style.width = 4.0;
    book.preview(SYMBOL, changed);
    assert!(book.revert(SYMBOL, before.clone()));
    assert_eq!(book.drawings(SYMBOL), before.as_slice());
}

#[test]
fn a_drawing_moves_up_and_down_the_stack() {
    let (mut book, first, second) = two_lines();
    assert!(
        !book.reorder(SYMBOL, second, Order::Front),
        "already on top"
    );
    assert!(book.reorder(SYMBOL, first, Order::Front));
    let ids: Vec<u64> = book.drawings(SYMBOL).iter().map(|d| d.id).collect();
    assert_eq!(ids, vec![second, first]);
    assert!(book.reorder(SYMBOL, first, Order::Backward));
    assert_eq!(book.drawings(SYMBOL)[0].id, first);
    assert!(book.undo());
    assert_eq!(book.drawings(SYMBOL)[1].id, first);
}

#[test]
fn hiding_locking_and_deleting_by_id() {
    let (mut book, first, second) = two_lines();
    assert!(book.set_all_hidden(SYMBOL, true));
    assert!(book.drawings(SYMBOL).iter().all(|d| d.hidden));
    assert!(!book.drawings(SYMBOL)[0].shows_on(TF));
    assert!(book.set_hidden(SYMBOL, first, false));
    assert!(book.set_locked(SYMBOL, first, true));
    assert!(!book.delete(SYMBOL, first), "a locked drawing stays");
    assert!(book.delete(SYMBOL, second));
    assert_eq!(book.count(SYMBOL), 1);
}
