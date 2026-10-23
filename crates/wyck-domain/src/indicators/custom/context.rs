//! The run that is going on: what the functions of a script read (the bars, the inputs) and what
//! they write (the declaration, the plots, the log). A function of the script reaches it by
//! [`with_run`]; it is put in place for the length of a script and taken back at the end.

use super::decl;
use super::meter::{self, Meter};
use super::other;
use crate::indicators::data::{BandStyle, ScriptDrawing, StudyContext};
use crate::indicators::spec::ValueFormat;
use crate::indicators::style::LevelStyle;
use rhai::{Dynamic, EvalAltResult, Position};
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
    pub follows: bool,
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
    /// What the chart is: its time zone, timeframe and symbol.
    pub context: StudyContext,
    pub declaration: decl::Declaration,
    pub plots: Vec<PlotResult>,
    pub fills: Vec<FillResult>,
    pub drawings: Vec<ScriptDrawing>,
    pub levels: Vec<f64>,
    /// The look the script gave each level, parallel to `levels`.
    pub level_styles: Vec<LevelStyle>,
    pub band: Option<(f64, f64)>,
    pub band_style: BandStyle,
    /// The way the values read, when the script said so with `set_format`.
    pub format: Option<ValueFormat>,
    pub log: Vec<String>,
    /// Things worth knowing that do not stop the run.
    pub warnings: Vec<decl::Problem>,
    /// Whether `indicator` was called.
    pub declared: bool,
    pub(super) limits: decl::Limits,
    pub(super) started: Instant,
    pub(super) cancel: Option<Arc<AtomicBool>>,
    pub(super) stopped: Option<Stop>,
    /// What the series of the run cost, in memory and in work.
    pub(super) meter: Arc<Meter>,
    /// The operations the interpreter counted, as of its last report.
    operations_seen: u64,
    /// The work native functions announced beyond the series they make (see [`Run::charge`]).
    charged: u64,
}

/// Why a run was stopped from outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stop {
    Operations,
    Time,
    Cancelled,
    Memory,
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
            context: StudyContext::default(),
            declaration: decl::Declaration::default(),
            plots: Vec::new(),
            fills: Vec::new(),
            drawings: Vec::new(),
            levels: Vec::new(),
            level_styles: Vec::new(),
            band: None,
            band_style: BandStyle::default(),
            format: None,
            log: Vec::new(),
            warnings: Vec::new(),
            declared: false,
            meter: Meter::new(limits.memory),
            limits,
            started: Instant::now(),
            cancel: None,
            stopped: None,
            operations_seen: 0,
            charged: 0,
        }
    }

    /// The work done so far, in operations of the interpreter: what it counted, the series made
    /// and what native functions announced.
    fn work(&self) -> u64 {
        self.operations_seen
            .saturating_add(self.meter.work())
            .saturating_add(self.charged)
    }

    /// Announces that a native function is about to do the work of `operations`. Refuses, ending
    /// the run, when that is over the budget or the time: a function that goes over every bar for
    /// every bar of its window is one operation to the interpreter, so it says what it costs.
    pub(crate) fn charge(&mut self, operations: u64) -> Result<(), Box<EvalAltResult>> {
        self.charged = self.charged.saturating_add(operations);
        let stop = if self.work() > self.limits.operations {
            Some(Stop::Operations)
        } else {
            self.stop_from_outside()
        };
        match stop {
            Some(stop) => {
                self.stopped = Some(stop);
                Err(Box::new(EvalAltResult::ErrorTerminated(
                    Dynamic::UNIT,
                    Position::NONE,
                )))
            }
            None => Ok(()),
        }
    }

    /// Whether the run was cancelled, is out of time or holds too much memory.
    fn stop_from_outside(&self) -> Option<Stop> {
        if self.meter.exceeded() {
            Some(Stop::Memory)
        } else if self
            .cancel
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
        {
            Some(Stop::Cancelled)
        } else if self.started.elapsed() > self.limits.time {
            Some(Stop::Time)
        } else {
            None
        }
    }

    /// Adds a warning that does not stop the run.
    pub(crate) fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(decl::Problem::warning(0, 0, message));
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
        meter::set_current(Some(Arc::clone(&run.meter)));
        RUN.with(|cell| *cell.borrow_mut() = Some(run));
        Self
    }

    /// The run, or `None` when something took it from under the script (it is installed until
    /// this call, so that would be a bug).
    pub(super) fn finish(self) -> Option<Run> {
        let run = RUN.with(|cell| cell.borrow_mut().take());
        meter::set_current(None);
        // The run is out: the destructor, which clears the slot, must not run.
        let _kept = std::mem::ManuallyDrop::new(self);
        run
    }
}

impl Drop for Installed {
    fn drop(&mut self) {
        // Only reached when the run was not taken back (a panic in a function of the script).
        RUN.with(|cell| *cell.borrow_mut() = None);
        meter::set_current(None);
    }
}

/// Asks whether the run that is going on must stop: past its budget of operations, over its memory,
/// cancelled from outside, or out of time. Called by the engine at every operation; the clock and
/// the cancel flag are read once in a thousand.
pub(super) fn budget(operations: u64) -> Option<Dynamic> {
    RUN.with(|cell| {
        let Ok(mut slot) = cell.try_borrow_mut() else {
            return None;
        };
        let run = slot.as_mut()?;
        run.operations_seen = operations;
        let stop = if run.work() > run.limits.operations {
            Some(Stop::Operations)
        } else if run.meter.exceeded() {
            Some(Stop::Memory)
        } else if operations.is_multiple_of(1024) {
            run.stop_from_outside()
        } else {
            None
        };
        stop.map(|stop| {
            run.stopped = Some(stop);
            Dynamic::from(())
        })
    })
}
