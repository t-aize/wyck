//! What the series of one run cost: the memory they hold now and the work of making them.
//!
//! Rhai counts the operations of the interpreter, but a function of the script that makes a series
//! is one operation however many bars it fills, and a script can keep many series alive. The meter
//! sees every series a run makes (each one asks for it when it is created) so that the run can be
//! stopped on memory, and so that the work done by native code counts in the budget.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// Bars a native function goes over for the cost of one operation of the interpreter.
const BARS_PER_OPERATION: usize = 128;

#[derive(Debug)]
pub(crate) struct Meter {
    live: AtomicUsize,
    limit: usize,
    work: AtomicU64,
    exceeded: AtomicBool,
}

impl Meter {
    pub(crate) fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            live: AtomicUsize::new(0),
            limit,
            work: AtomicU64::new(0),
            exceeded: AtomicBool::new(false),
        })
    }

    /// A series of `len` bars was made.
    pub(crate) fn made(&self, len: usize) {
        let bytes = len.saturating_mul(size_of::<f64>());
        let live = self
            .live
            .fetch_add(bytes, Ordering::Relaxed)
            .saturating_add(bytes);
        let ops = u64::try_from(len / BARS_PER_OPERATION + 1).unwrap_or(u64::MAX);
        self.work.fetch_add(ops, Ordering::Relaxed);
        if live > self.limit {
            self.exceeded.store(true, Ordering::Relaxed);
        }
    }

    /// A series of `len` bars is gone.
    pub(crate) fn dropped(&self, len: usize) {
        let bytes = len.saturating_mul(size_of::<f64>());
        self.live.fetch_sub(bytes, Ordering::Relaxed);
    }

    /// Whether more memory is held than the run may.
    pub(crate) fn exceeded(&self) -> bool {
        self.exceeded.load(Ordering::Relaxed)
    }

    /// The work of making series, in operations of the interpreter.
    pub(crate) fn work(&self) -> u64 {
        self.work.load(Ordering::Relaxed)
    }

    pub(crate) fn limit(&self) -> usize {
        self.limit
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Arc<Meter>>> = const { RefCell::new(None) };
}

/// The meter of the run that is going on on this thread, if one is.
pub(crate) fn current() -> Option<Arc<Meter>> {
    CURRENT.with(|cell| cell.try_borrow().ok().and_then(|slot| slot.clone()))
}

/// Puts the meter of a run in place for this thread, or takes it away.
pub(crate) fn set_current(meter: Option<Arc<Meter>>) {
    CURRENT.with(|cell| {
        if let Ok(mut slot) = cell.try_borrow_mut() {
            *slot = meter;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_meter_counts_what_is_held_and_says_when_it_is_too_much() {
        let meter = Meter::new(80);
        meter.made(5);
        assert!(!meter.exceeded());
        meter.made(10);
        assert!(meter.exceeded());
        meter.dropped(10);
        assert!(meter.exceeded(), "once over, the run is to be stopped");
    }

    #[test]
    fn making_a_series_costs_work_in_proportion_to_its_length() {
        let meter = Meter::new(usize::MAX);
        meter.made(1);
        let small = meter.work();
        meter.made(12_800);
        assert!(meter.work() - small >= 100);
    }
}
