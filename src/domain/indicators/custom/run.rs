//! Running a script: compiling it, asking it what it is, and computing it over bars.
//!
//! A script is one program that does everything: it says what the indicator is (`indicator`), what
//! the user can change (`input_*`), and what it draws (`plot`). Running it twice gives both halves:
//!
//! - The **declaration** run is on no bars at all. It only collects the calls that describe the
//!   indicator, so it is cheap and the settings panel knows the inputs and plots before any
//!   price is read. What a script declares must not depend on the data.
//! - The **compute** run is on the bars of a chart, with the values the user chose. The calls
//!   that describe the indicator do the same as before, and `plot` keeps its numbers.
//!
//! Every run is bounded: by a number of operations and by a time, and it can be told to stop.
//! Nothing a script can call reaches the file system, the network or the clock.

pub mod decl;

use super::super::compute::{ScriptDrawing, StudyInput, StudyOutput};
use super::super::intern;
use super::super::spec::{PlotKind, SOURCES};
use super::api;
use decl::{Columns, Declaration, Limits, Problem};
use rhai::{AST, Dynamic, Engine, EvalAltResult, ParseError, Position, Scope};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

/// Which of the two runs this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Declare,
    Compute,
}

/// A plot as a run made it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotResult {
    pub key: String,
    pub values: Vec<f64>,
    pub offset: i64,
    pub up: Option<Vec<bool>>,
}

/// The shaded area between two plots (by position), as a run made it.
#[derive(Debug, Clone, PartialEq)]
pub struct FillResult {
    pub a: usize,
    pub b: usize,
    pub color: u32,
    pub alpha: f32,
    pub other: Option<u32>,
}

/// The state of the run that is going on: what the functions of the script read (the bars, the
/// values of the inputs) and what they write (the declaration, the plots, the log).
pub struct Run {
    pub mode: Mode,
    pub columns: Columns,
    /// The values the user chose, by input key.
    pub values: BTreeMap<String, f64>,
    /// The text the user chose for the inputs that are text, by input key.
    pub texts: BTreeMap<String, String>,
    pub declaration: Declaration,
    pub plots: Vec<PlotResult>,
    pub fills: Vec<FillResult>,
    pub drawings: Vec<ScriptDrawing>,
    pub levels: Vec<f64>,
    pub band: Option<(f64, f64)>,
    pub log: Vec<String>,
    /// Whether `indicator` was called.
    pub declared: bool,
    limits: Limits,
    started: Instant,
    cancel: Option<Arc<AtomicBool>>,
    stopped: Option<Stop>,
}

/// Why a run was stopped from outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
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
    fn new(mode: Mode, columns: Columns, values: BTreeMap<String, f64>, limits: Limits) -> Self {
        Self {
            mode,
            columns,
            values,
            texts: BTreeMap::new(),
            declaration: Declaration::default(),
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
    pub fn say(&mut self, line: String) {
        if self.log.len() < MAX_LOG {
            self.log.push(line);
        } else if self.log.len() == MAX_LOG {
            self.log
                .push("(the log is full: the rest is not kept)".to_owned());
        }
    }
}

/// Runs `f` on the run that is going on, or fails when no script is running.
pub fn with_run<R>(f: impl FnOnce(&mut Run) -> R) -> Result<R, Box<EvalAltResult>> {
    RUN.with(|cell| match cell.try_borrow_mut() {
        Ok(mut slot) => slot
            .as_mut()
            .map(f)
            .ok_or_else(|| "this can only be called by a running script".into()),
        Err(_) => Err("this cannot be called from here".into()),
    })
}

/// Puts a run in place for the length of a script, and takes it back at the end.
struct Installed;

impl Installed {
    fn new(run: Run) -> Self {
        RUN.with(|cell| *cell.borrow_mut() = Some(run));
        Self
    }

    /// The run, or `None` when something took it from under the script (it is installed until
    /// this call, so that would be a bug).
    fn finish(self) -> Option<Run> {
        let run = RUN.with(|cell| cell.borrow_mut().take());
        std::mem::forget(self);
        run
    }
}

impl Drop for Installed {
    fn drop(&mut self) {
        // Only reached when the run was not taken back (a panic in a function of the script).
        RUN.with(|cell| *cell.borrow_mut() = None);
    }
}

/// The engine every script runs on: built once, shared by every thread.
pub fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut engine = Engine::new();
        engine.set_max_call_levels(32);
        engine.set_max_expr_depths(64, 32);
        engine.set_max_string_size(8 * 1024);
        engine.set_max_array_size(100_000);
        engine.set_max_map_size(512);
        engine.set_strict_variables(true);
        engine.set_fail_on_invalid_map_property(true);
        engine.set_optimization_level(rhai::OptimizationLevel::Simple);
        engine.disable_symbol("eval");
        engine.on_print(|text| {
            let _ = with_run(|run| run.say(text.to_owned()));
        });
        engine.on_debug(|text, _source, position| {
            let line = position
                .line()
                .map_or(String::new(), |l| format!("[line {l}] "));
            let _ = with_run(|run| run.say(format!("{line}{text}")));
        });
        // The budget: every operation asks whether the run must stop.
        engine.on_progress(|operations| {
            RUN.with(|cell| {
                let Ok(mut slot) = cell.try_borrow_mut() else {
                    return None;
                };
                let run = slot.as_mut()?;
                let stop = if operations > run.limits.operations {
                    Some(Stop::Operations)
                } else if operations % 1024 == 0 {
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
        });
        api::register(&mut engine);
        engine
    })
}

/// The names a script starts with: the bars, and how many there are. When the script is being
/// compiled they are plain variables: constants would be folded into the program, with the bars it
/// was compiled against.
fn globals(columns: &Columns, constant: bool) -> Scope<'static> {
    let mut scope = Scope::new();
    let mut put = |name: &'static str, value: Dynamic| {
        if constant {
            scope.push_constant_dynamic(name, value);
        } else {
            scope.push_dynamic(name, value);
        }
    };
    put("open", Dynamic::from(columns.open.clone()));
    put("high", Dynamic::from(columns.high.clone()));
    put("low", Dynamic::from(columns.low.clone()));
    put("close", Dynamic::from(columns.close.clone()));
    put("volume", Dynamic::from(columns.volume.clone()));
    put("time", Dynamic::from(columns.time.clone()));
    put("hl2", Dynamic::from(columns.hl2.clone()));
    put("hlc3", Dynamic::from(columns.hlc3.clone()));
    put("ohlc4", Dynamic::from(columns.ohlc4.clone()));
    put("hlcc4", Dynamic::from(columns.hlcc4.clone()));
    put("bar_index", Dynamic::from(columns.bar_index.clone()));
    put("n", Dynamic::from(columns.len as i64));
    put("na", Dynamic::from(f64::NAN));
    scope
}

/// The names a script starts with, for the editor.
#[cfg(test)]
pub fn global_names() -> Vec<String> {
    globals(&Columns::default(), false)
        .iter()
        .map(|(name, _, _)| name.to_owned())
        .collect()
}

/// What a run of a script gave.
#[derive(Debug, Clone, PartialEq)]
pub struct Computed {
    pub output: StudyOutput,
    /// What the script printed.
    pub log: Vec<String>,
    pub elapsed: Duration,
}

/// A script that compiles and whose declaration ran.
#[derive(Debug, Clone)]
pub struct Script {
    ast: AST,
    pub declaration: Declaration,
    /// Things worth knowing that do not stop it: it draws nothing, it has no `indicator` call.
    pub warnings: Vec<Problem>,
}

impl Script {
    /// Compiles `source` and runs it on no bars to learn what it declares.
    ///
    /// # Errors
    ///
    /// The problems found, when it does not compile or fails.
    pub fn compile(source: &str) -> Result<Self, Vec<Problem>> {
        Self::compile_with(source, Limits::default())
    }

    /// [`Self::compile`], with the limits the declaration run has.
    ///
    /// # Errors
    ///
    /// The problems found, when it does not compile or fails.
    pub fn compile_with(source: &str, limits: Limits) -> Result<Self, Vec<Problem>> {
        let scope = globals(&Columns::default(), false);
        let ast = engine()
            .compile_with_scope(&scope, source)
            .map_err(|error| vec![parse_problem(&error)])?;
        let run = Run::new(Mode::Declare, Columns::default(), BTreeMap::new(), limits);
        let installed = Installed::new(run);
        let mut scope = globals(&Columns::default(), true);
        let result = engine().run_ast_with_scope(&mut scope, &ast);
        let Some(run) = installed.finish() else {
            return Err(vec![lost_run()]);
        };
        if let Err(error) = result {
            return Err(vec![runtime_problem(&error, &run)]);
        }
        let mut warnings = Vec::new();
        let has_draw_call = source.match_indices("draw").any(|(at, _)| {
            source[at + 4..].trim_start().starts_with('(')
                && source[..at]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_')
        });
        if run.declaration.plots.is_empty() && !has_draw_call {
            warnings.push(Problem::warning(
                0,
                0,
                "the script draws nothing: add a plot(...) or draw(...) call",
            ));
        }
        if !run.declared {
            warnings.push(Problem::warning(
                0,
                0,
                "no indicator(#{ ... }) call: the file name is the name, and it is drawn in a pane",
            ));
        }
        Ok(Self {
            ast,
            declaration: run.declaration,
            warnings,
        })
    }

    /// Computes the script over `input`, with the values the user chose for the inputs (an input
    /// with no value uses its default).
    ///
    /// # Errors
    ///
    /// The problem that stopped it: a mistake of the script, or the limits.
    pub fn compute(
        &self,
        input: &StudyInput,
        values: &BTreeMap<String, f64>,
        limits: Limits,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<Computed, Vec<Problem>> {
        self.compute_with_texts(input, values, &BTreeMap::new(), limits, cancel)
    }

    /// [`Self::compute`] with the text the user chose for the inputs that are text.
    ///
    /// # Errors
    ///
    /// The problem that stopped it: a mistake of the script, or the limits.
    pub fn compute_with_texts(
        &self,
        input: &StudyInput,
        values: &BTreeMap<String, f64>,
        texts: &BTreeMap<String, String>,
        limits: Limits,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<Computed, Vec<Problem>> {
        let columns = Columns::from_input(input);
        let mut scope = globals(&columns, true);
        let mut run = Run::new(Mode::Compute, columns, values.clone(), limits);
        run.texts = texts.clone();
        run.cancel = cancel;
        let started = Instant::now();
        let installed = Installed::new(run);
        let result = engine().run_ast_with_scope(&mut scope, &self.ast);
        let Some(run) = installed.finish() else {
            return Err(vec![lost_run()]);
        };
        let elapsed = started.elapsed();
        if let Err(error) = result {
            return Err(vec![runtime_problem(&error, &run)]);
        }
        Ok(Computed {
            output: output_of(&run),
            log: run.log,
            elapsed,
        })
    }
}

fn output_of(run: &Run) -> StudyOutput {
    let mut output = StudyOutput::default();
    for plot in &run.plots {
        let decl = run.declaration.plots.iter().find(|d| d.key == plot.key);
        output.plots.push(super::super::compute::PlotOut {
            key: intern::name(&plot.key),
            kind: decl.map_or(PlotKind::Line, |d| d.kind),
            values: plot.values.clone(),
            offset: plot.offset,
            up: plot.up.clone(),
        });
    }
    for fill in &run.fills {
        output.fills.push(super::super::compute::FillOut {
            a: fill.a,
            b: fill.b,
            color: fill.color,
            alpha: fill.alpha,
            other: fill.other,
        });
    }
    output.levels = run.levels.clone();
    output.band = run.band;
    output.drawings = run.drawings.clone();
    output
}

/// What is reported when a run vanished while its script was running.
fn lost_run() -> Problem {
    Problem::error(
        0,
        0,
        "the run of the script was lost, this is a bug in Wyck",
    )
}

fn parse_problem(error: &ParseError) -> Problem {
    Problem::at(error.position(), error.err_type().to_string())
}

/// The message of an error without the position Rhai writes after it, and the innermost error of
/// a call inside a call.
fn message_of(error: &EvalAltResult) -> (String, Position) {
    match error {
        EvalAltResult::ErrorInFunctionCall(_, _, inner, _) => message_of(inner),
        EvalAltResult::ErrorRuntime(value, position) => (value.to_string(), *position),
        other => {
            let text = other.to_string();
            let cut = text.rfind(" (line ").unwrap_or(text.len());
            (text[..cut].to_owned(), other.position())
        }
    }
}

fn runtime_problem(error: &EvalAltResult, run: &Run) -> Problem {
    if let EvalAltResult::ErrorTerminated(_, position) = error {
        let message = match run.stopped {
            Some(Stop::Operations) => format!(
                "the script did more than {} operations and was stopped. A loop over the bars is slow: use the series functions (sma, highest, ...) instead",
                run.limits.operations
            ),
            Some(Stop::Time) => format!(
                "the script ran for more than {:.0} seconds and was stopped",
                run.limits.time.as_secs_f32()
            ),
            _ => "the script was stopped".to_owned(),
        };
        return Problem::at(*position, message);
    }
    let (message, position) = message_of(error);
    Problem::at(position, message)
}

/// The position of a source input in [`SOURCES`], by name (any case).
pub fn source_index(name: &str) -> Option<usize> {
    SOURCES.iter().position(|s| s.eq_ignore_ascii_case(name))
}
