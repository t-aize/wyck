#![cfg(test)]

mod creation;
mod editing;
mod history;
mod looks;
mod selection;

use super::super::geometry::tests::Linear;

use super::*;
use crate::domain::drawings::model::{MAX_NAMED_TEMPLATES, MAX_TEMPLATE_NAME};

const SYMBOL: &str = "US100.cash";

const TF: &str = "M5";

fn book() -> Book {
    Book::from_doc(DrawingsDoc::default())
}

/// Screen (x, y) for a time in seconds and a price on the test projection.
fn at(seconds: f32, price: f32) -> (f32, f32) {
    (seconds, 500.0 - price)
}

fn click(book: &mut Book, seconds: f32, price: f32) {
    let (x, y) = at(seconds, price);
    book.press(SYMBOL, TF, &Linear, x, y);
    book.release(SYMBOL, &Linear, x, y);
}

/// Two trend lines, far apart, the way a user would draw them.
fn apart_lines(book: &mut Book) {
    book.set_tool(Some(Tool::TrendLine));
    click(book, 60.0, 100.0);
    click(book, 240.0, 160.0);
    book.set_tool(Some(Tool::TrendLine));
    click(book, 60.0, 300.0);
    click(book, 240.0, 360.0);
    book.select(None);
}

/// Presses at `from`, moves to `to` in steps, and lets go there.
fn drag(book: &mut Book, from: (f32, f32), to: (f32, f32)) {
    let (x0, y0) = at(from.0, from.1);
    book.press(SYMBOL, TF, &Linear, x0, y0);
    for step in 1..=4 {
        let k = step as f32 / 4.0;
        let (x, y) = at(from.0 + (to.0 - from.0) * k, from.1 + (to.1 - from.1) * k);
        book.pointer_moved(SYMBOL, &Linear, x, y, false);
    }
    let (x1, y1) = at(to.0, to.1);
    book.release(SYMBOL, &Linear, x1, y1);
}

fn with_a_line() -> Book {
    let mut book = book();
    book.set_tool(Some(Tool::TrendLine));
    click(&mut book, 120.0, 100.0);
    click(&mut book, 480.0, 300.0);
    book
}

fn two_lines() -> (Book, u64, u64) {
    let mut book = book();
    for (a, b) in [(100.0, 200.0), (150.0, 250.0)] {
        book.set_tool(Some(Tool::TrendLine));
        click(&mut book, 60.0, a);
        click(&mut book, 300.0, b);
    }
    let ids: Vec<u64> = book.drawings(SYMBOL).iter().map(|d| d.id).collect();
    (book, ids[0], ids[1])
}
