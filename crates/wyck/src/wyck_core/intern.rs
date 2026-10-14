//! Names that live for the whole run of the program.
//!
//! The indicators the app ships describe themselves with `&'static str`, and the settings panels
//! hold those names in their fields. An indicator written by the user is described at run time, so
//! its names are put here once and handed out as `'static`.
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
    bytes: usize,
}

static TABLE: Mutex<Option<Table>> = Mutex::new(None);

/// `text` as a `'static` string. The same text always gives the same string.
pub fn name(text: &str) -> &'static str {
    let mut guard = TABLE.lock().unwrap_or_else(PoisonError::into_inner);
    let table = guard.get_or_insert_with(Table::default);
    if let Some(found) = table.names.get(text) {
        return found;
    }
    if table.bytes + text.len() > LIMIT {
        return FULL;
    }
    table.bytes += text.len();
    let kept: &'static str = Box::leak(text.to_owned().into_boxed_str());
    table.names.insert(kept);
    kept
}

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
}
