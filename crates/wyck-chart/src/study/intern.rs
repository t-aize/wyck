//! Names that live for the whole run of the program.
//!
//! The indicators the app ships describe themselves with `&'static str` and static slices, and
//! the settings panels hold those names in their fields. An indicator written by the user is
//! described at run time, so its names and lists are put here once and handed out as `'static`.
//!
//! What is stored is bounded: the same text is stored once however many times a script is
//! reloaded, so only text that is new costs memory, and past [`LIMIT`] bytes the table refuses
//! more (an indicator then shows [`FULL`] where a name would be) instead of growing.

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};

/// The most bytes of names kept, in all.
pub const LIMIT: usize = 4 * 1024 * 1024;

/// What stands in for a name once the table is full.
pub const FULL: &str = "(too many names)";

#[derive(Default)]
struct Table {
    names: HashSet<&'static str>,
    lists: Vec<&'static [&'static str]>,
    bytes: usize,
}

static TABLE: Mutex<Option<Table>> = Mutex::new(None);

fn with_table<R>(f: impl FnOnce(&mut Table) -> R) -> R {
    let mut guard = TABLE.lock().unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(Table::default))
}

/// `text` as a `'static` string. The same text always gives the same string.
pub fn name(text: &str) -> &'static str {
    with_table(|table| {
        if let Some(found) = table.names.get(text) {
            return *found;
        }
        if table.bytes + text.len() > LIMIT {
            return FULL;
        }
        table.bytes += text.len();
        let kept: &'static str = Box::leak(text.to_owned().into_boxed_str());
        table.names.insert(kept);
        kept
    })
}

/// A list of names as a `'static` slice. The same list always gives the same slice.
pub fn list(items: &[String]) -> &'static [&'static str] {
    let names: Vec<&'static str> = items.iter().map(|item| name(item)).collect();
    with_table(|table| {
        if let Some(found) = table.lists.iter().find(|known| **known == names.as_slice()) {
            return *found;
        }
        let cost = names.len() * std::mem::size_of::<&str>();
        if table.bytes + cost > LIMIT {
            return &[];
        }
        table.bytes += cost;
        let kept: &'static [&'static str] = Box::leak(names.into_boxed_slice());
        table.lists.push(kept);
        kept
    })
}

/// A table of slices of one type, each stored once however often it is asked for. It is bounded
/// by [`LIMIT`] in the number of slices it keeps.
pub struct Slices<T: 'static>(Mutex<Vec<&'static [T]>>);

impl<T: Clone + PartialEq + 'static> Slices<T> {
    pub const fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    /// `items` as a `'static` slice. Equal contents give the same slice.
    pub fn get(&self, items: &[T]) -> &'static [T] {
        let mut kept = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = kept.iter().find(|known| **known == items) {
            return found;
        }
        if kept.len() >= MAX_SLICES {
            return &[];
        }
        let leaked: &'static [T] = Box::leak(items.to_vec().into_boxed_slice());
        kept.push(leaked);
        leaked
    }
}

impl<T: Clone + PartialEq + 'static> Default for Slices<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// The most slices one [`Slices`] keeps.
const MAX_SLICES: usize = 20_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_text_is_stored_once() {
        let a = name("intern-test-name");
        let b = name(&String::from("intern-test-name"));
        assert!(std::ptr::eq(a, b));
        assert_eq!(a, "intern-test-name");
    }

    #[test]
    fn the_same_list_is_stored_once() {
        let a = list(&["Fast".to_owned(), "Slow".to_owned()]);
        let b = list(&["Fast".to_owned(), "Slow".to_owned()]);
        assert!(std::ptr::eq(a, b));
        assert_eq!(a, &["Fast", "Slow"]);
        assert!(!std::ptr::eq(a, list(&["Fast".to_owned()])));
    }
}
