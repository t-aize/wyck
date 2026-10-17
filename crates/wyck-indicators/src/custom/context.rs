//! The run that is going on: what the functions of a script read (the bars, the inputs) and what
//! they write (the declaration, the plots, the log). A function of the script reaches it by
//! [`with_run`]; it is put in place for the length of a script and taken back at the end.

use super::decl;
use super::other;
use crate::data::ScriptDrawing;
use rhai::{Dynamic, EvalAltResult};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Which of the two runs this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Declare,
    Compute,
}

/// A plot as a run made it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlotResult {
    pub key: String,
    pub values: Vec<f64>,
    pub offset: i64,
    pub up: Option<Vec<bool>>,
}

/// The shaded area between two plots (by position), as a run made it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FillResult {
    pub a: usize,
    pub b: usize,
    pub color: u32,
    pub alpha: f32,
    pub other: Option<u32>,
}

/// The state of the run that is going on: what the functions of the script read (the bars, the
/// values of the inputs) and what they write (the declaration, the plots, the log).
pub(crate) struct Run {
    pub mode: Mode,
    pub columns: decl::Columns,
    /// The values the user chose, by input key.
    pub values: BTreeMap<String, f64>,
    /// The text the user chose for the inputs that are text, by input key.
    pub texts: BTreeMap<String, String>,
    /// The prices of the other symbols the script may read with `bars_of`, on the bars of the
    /// chart, by [`other::key_of`] their names.
    pub others: BTreeMap<String, other::Aligned>,
    pub declaration: decl::Declaration,
    pub plots: Vec<PlotResult>,
    pub fills: Vec<FillResult>,
    pub drawings: Vec<ScriptDrawing>,
    pub levels: Vec<f64>,
    pub band: Option<(f64, f64)>,
    pub log: Vec<String>,
    /// Whether `indicator` was called.
    pub declared: bool,
    pub(super) limits: decl::Limits,
    pub(super) started: Instant,
    pub(super) cancel: Option<Arc<AtomicBool>>,
    pub(super) stopped: Option<Stop>,
}

/// Why a run was stopped from outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stop {
    Operations,
    Time,
    Cancelled,
}

thread_local! {
    static RUN: RefCell<Option<Run>> = const { RefCell::new(None) };
}

/// The most lines a run may write to its log.
const MAX_LOG: usize = 500;

impl Run {
    pub(super) fn new(
        mode: Mode,
        columns: decl::Columns,
        values: BTreeMap<String, f64>,
        limits: decl::Limits,
    ) -> Self {
        Self {
            mode,
            columns,
            values,
            texts: BTreeMap::new(),
            others: BTreeMap::new(),
            declaration: decl::Declaration::default(),
            plots: Vec::new(),
            fills: Vec::new(),
            drawings: Vec::new(),
            levels: Vec::new(),
            band: None,
            log: Vec::new(),
            declared: false,
            limits,
            started: Instant::now(),
            cancel: None,
            stopped: None,
        }
    }

    /// Adds a line to the log, up to a limit.
    pub(crate) fn say(&mut self, line: String) {
        if self.log.len() < MAX_LOG {
            self.log.push(line);
        } else if self.log.len() == MAX_LOG {
            self.log
                .push("(the log is full: the rest is not kept)".to_owned());
        }
    }
}

/// Runs `f` on the run that is going on, or fails when no script is running.
pub(crate) fn with_run<R>(f: impl FnOnce(&mut Run) -> R) -> Result<R, Box<EvalAltResult>> {
    RUN.with(|cell| match cell.try_borrow_mut() {
        Ok(mut slot) => slot
            .as_mut()
            .map(f)
            .ok_or_else(|| "this can only be called by a running script".into()),
        Err(_) => Err("this cannot be called from here".into()),
    })
}

/// Puts a run in place for the length of a script, and takes it back at the end.
pub(super) struct Installed;

impl Installed {
    pub(super) fn new(run: Run) -> Self {
        RUN.with(|cell| *cell.borrow_mut() = Some(run));
        Self
    }

    /// The run, or `None` when something took it from under the script (it is installed until
    /// this call, so that would be a bug).
    pub(super) fn finish(self) -> Option<Run> {
        let run = RUN.with(|cell| cell.borrow_mut().take());
        // The run is out: the destructor, which clears the slot, must not run.
        let _kept = std::mem::ManuallyDrop::new(self);
        run
    }
}

impl Drop for Installed {
    fn drop(&mut self) {
        // Only reached when the run was not taken back (a panic in a function of the script).
        RUN.with(|cell| *cell.borrow_mut() = None);
    }
}

/// Asks whether the run that is going on must stop: past its budget of operations, cancelled from
/// outside, or out of time. Called by the engine at every operation.
pub(super) fn budget(operations: u64) -> Option<Dynamic> {
    RUN.with(|cell| {
        let Ok(mut slot) = cell.try_borrow_mut() else {
            return None;
        };
        let run = slot.as_mut()?;
        let stop = if operations > run.limits.operations {
            Some(Stop::Operations)
        } else if operations.is_multiple_of(1024) {
            if run
                .cancel
                .as_ref()
                .is_some_and(|c| c.load(Ordering::Relaxed))
            {
                Some(Stop::Cancelled)
            } else if run.started.elapsed() > run.limits.time {
                Some(Stop::Time)
            } else {
                None
            }
        } else {
            None
        };
        stop.map(|stop| {
            run.stopped = Some(stop);
            Dynamic::from(())
        })
    })
}
