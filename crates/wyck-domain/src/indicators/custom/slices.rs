//! `'static` slices of what a script declared, each stored once, so that a spec (which holds
//! `'static` slices) can describe a script.

use crate::base::intern;
use crate::indicators::spec::{InputSpec, PlotSpec};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{LazyLock, Mutex, PoisonError};

/// The tables are full: this session has declared more different layouts than they keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TablesFull;

impl TablesFull {
    pub(crate) const MESSAGE: &'static str = "this session has seen too many different versions of indicator settings: restart Wyck to start over";
}

/// A table of slices of one type, each stored once however often it is asked for, so that a spec
/// can hold `'static` slices of what a script declared. A slice is found by its contents in one
/// lookup, and is never freed: only a change of the declaration (not of the code) adds one.
pub(super) struct Slices<T: 'static>(Mutex<HashMap<String, &'static [T]>>);

/// The most slices one table keeps.
const MAX_SLICES: usize = 20_000;

impl<T: Clone + Debug + 'static> Slices<T> {
    fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    /// `items` as a `'static` slice. Equal contents give the same slice.
    pub(super) fn get(&self, items: &[T]) -> Result<&'static [T], TablesFull> {
        if items.is_empty() {
            return Ok(&[]);
        }
        let key = format!("{items:?}");
        let mut kept = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = kept.get(&key) {
            return Ok(found);
        }
        if kept.len() >= MAX_SLICES {
            return Err(TablesFull);
        }
        let leaked: &'static [T] = Box::leak(items.to_vec().into_boxed_slice());
        kept.insert(key, leaked);
        Ok(leaked)
    }
}

pub(super) static INPUTS: LazyLock<Slices<InputSpec>> = LazyLock::new(Slices::new);
pub(super) static PLOTS: LazyLock<Slices<PlotSpec>> = LazyLock::new(Slices::new);
static CHOICES: LazyLock<Slices<&'static str>> = LazyLock::new(Slices::new);

/// The options of a choice input as a `'static` slice. The same options always give the same slice.
pub(super) fn choices(options: &[String]) -> Result<&'static [&'static str], TablesFull> {
    let names: Vec<&'static str> = options.iter().map(|option| intern::name(option)).collect();
    CHOICES.get(&names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_contents_give_the_same_slice_and_empty_gives_none_to_store() {
        let table: Slices<&'static str> = Slices::new();
        let a = table.get(&["a", "b"]).unwrap();
        let b = table.get(&["a", "b"]).unwrap();
        assert!(std::ptr::eq(a, b));
        assert!(!std::ptr::eq(a, table.get(&["a"]).unwrap()));
        assert_eq!(table.get(&[]).unwrap(), &[] as &[&str]);
        assert_eq!(table.0.lock().unwrap().len(), 2);
    }

    #[test]
    fn a_full_table_says_so_instead_of_handing_out_an_empty_slice() {
        let table: Slices<u32> = Slices::new();
        for i in 0..u32::try_from(MAX_SLICES).unwrap() {
            table.get(&[i]).unwrap();
        }
        assert_eq!(table.get(&[u32::MAX]), Err(TablesFull));
        assert!(table.get(&[0]).is_ok(), "what is stored is still found");
    }
}
