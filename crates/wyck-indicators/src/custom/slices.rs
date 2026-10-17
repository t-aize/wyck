//! `'static` slices of what a script declared, each stored once, so that a spec (which holds
//! `'static` slices) can describe a script.

use crate::spec::{InputSpec, PlotSpec};
use std::sync::{Mutex, PoisonError};
use wyck_core::intern;

/// A table of slices of one type, each stored once however often it is asked for, so that a spec
/// can hold `'static` slices of what a script declared.
pub(super) struct Slices<T: 'static>(Mutex<Vec<&'static [T]>>);

/// The most slices one table keeps. Past it, an empty slice is handed out instead of growing.
const MAX_SLICES: usize = 20_000;

impl<T: Clone + PartialEq + 'static> Slices<T> {
    const fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    /// `items` as a `'static` slice. Equal contents give the same slice.
    pub(super) fn get(&self, items: &[T]) -> &'static [T] {
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

pub(super) static INPUTS: Slices<InputSpec> = Slices::new();
pub(super) static PLOTS: Slices<PlotSpec> = Slices::new();
static CHOICES: Slices<&'static str> = Slices::new();

/// The options of a choice input as a `'static` slice. The same options always give the same slice.
pub(super) fn choices(options: &[String]) -> &'static [&'static str] {
    let names: Vec<&'static str> = options.iter().map(|option| intern::name(option)).collect();
    CHOICES.get(&names)
}
