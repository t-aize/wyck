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

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use rhai::{AST, Dynamic, Engine, EvalAltResult, ParseError, Position, Scope};

use super::super::intern;
use super::super::{InputKind, PlotKind, SOURCES, StudyInput, StudyOutput, ValueFormat};
use super::api;
use super::series::Series;
use crate::drawing::model::Dash;

/// How much a run may do before it is stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Operations of the interpreter: about one for each expression evaluated.
    pub operations: u64,
    /// Wall time.
    pub time: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            operations: 20_000_000,
            time: Duration::from_secs(4),
        }
    }
}

/// How serious a problem is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// Something wrong in a script, and where. Lines and columns count from 1; 0 means the place is
/// not known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub line: usize,
    pub column: usize,
    pub severity: Severity,
    pub message: String,
}

impl Problem {
    pub fn error(line: usize, column: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            column,
            severity: Severity::Error,
            message: message.into(),
        }
    }

    pub fn warning(line: usize, column: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            column,
            severity: Severity::Warning,
            message: message.into(),
        }
    }

    pub fn at(position: Position, message: impl Into<String>) -> Self {
        Self::error(
            position.line().unwrap_or(0),
            position.position().unwrap_or(0),
            message,
        )
    }
}

/// What kind of value an input takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    Int,
    Float,
    Bool,
    Source,
    Choice,
    Color,
}

/// An input a script asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct InputDecl {
    pub key: String,
    pub label: String,
    pub kind: InputType,
    pub default: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// The names, for a choice.
    pub options: Vec<String>,
}

impl InputDecl {
    /// The kind of input the panels know.
    pub fn input_kind(&self) -> InputKind {
        match self.kind {
            InputType::Int => InputKind::Int,
            InputType::Float => InputKind::Float,
            InputType::Bool => InputKind::Toggle,
            InputType::Source => InputKind::Source,
            InputType::Choice => InputKind::Choice(intern::list(&self.options)),
            InputType::Color => InputKind::Color,
        }
    }
}

/// A line, histogram or set of dots a script draws.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotDecl {
    pub key: String,
    pub label: String,
    pub kind: PlotKind,
    pub color: u32,
    pub width: f32,
    pub dash: Dash,
}

/// What the script says the indicator is.
#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    pub name: Option<String>,
    pub short: Option<String>,
    /// Drawn on the prices (true) or in a pane of its own.
    pub overlay: bool,
    pub format: ValueFormat,
    /// A pane whose scale is fixed.
    pub range: Option<(f64, f64)>,
    pub description: String,
    pub author: String,
    pub version: String,
    pub category: String,
}

impl Default for Meta {
    fn default() -> Self {
        Self {
            name: None,
            short: None,
            overlay: false,
            format: ValueFormat::Plain(2),
            range: None,
            description: String::new(),
            author: String::new(),
            version: String::new(),
            category: String::new(),
        }
    }
}

/// Everything a script says about itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Declaration {
    pub meta: Meta,
    pub inputs: Vec<InputDecl>,
    pub plots: Vec<PlotDecl>,
}

/// The most inputs and plots one indicator may have.
pub const MAX_INPUTS: usize = 32;
pub const MAX_PLOTS: usize = 16;

/// The bars a script reads, as series, with the day of each (for a volume weighted price that
/// starts over every day).
#[derive(Debug, Clone, Default)]
pub struct Columns {
    pub len: usize,
    pub open: Series,
    pub high: Series,
    pub low: Series,
    pub close: Series,
    pub volume: Series,
    pub time: Series,
    pub hl2: Series,
    pub hlc3: Series,
    pub ohlc4: Series,
    pub bar_index: Series,
    pub day: Arc<Vec<i64>>,
}

impl Columns {
    pub fn from_input(input: &StudyInput) -> Self {
        let n = input.len();
        Self {
            len: n,
            open: Series::new(input.open.clone()),
            high: Series::new(input.high.clone()),
            low: Series::new(input.low.clone()),
            close: Series::new(input.close.clone()),
            volume: Series::new(input.volume.clone()),
            time: Series::new(input.time.iter().map(|t| *t as f64).collect()),
            hl2: Series::new(input.source(4.0)),
            hlc3: Series::new(input.source(5.0)),
            ohlc4: Series::new(input.source(6.0)),
            bar_index: Series::new((0..n).map(|i| i as f64).collect()),
            day: Arc::new(input.day.clone()),
        }
    }

    /// The price a source input names, by its position in [`SOURCES`].
    pub fn source(&self, which: usize) -> Series {
        match which {
            0 => self.open.clone(),
            1 => self.high.clone(),
            2 => self.low.clone(),
            4 => self.hl2.clone(),
            5 => self.hlc3.clone(),
            6 => self.ohlc4.clone(),
            _ => self.close.clone(),
        }
    }
}

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
    pub declaration: Declaration,
    pub plots: Vec<PlotResult>,
    pub fills: Vec<FillResult>,
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
            declaration: Declaration::default(),
            plots: Vec::new(),
            fills: Vec::new(),
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

    fn finish(self) -> Run {
        let run = RUN
            .with(|cell| cell.borrow_mut().take())
            .expect("the run is installed until it finishes");
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
        let run = installed.finish();
        if let Err(error) = result {
            return Err(vec![runtime_problem(&error, &run)]);
        }
        let mut warnings = Vec::new();
        if run.declaration.plots.is_empty() {
            warnings.push(Problem::warning(
                0,
                0,
                "the script draws nothing: add a plot(...) call",
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
        let columns = Columns::from_input(input);
        let mut scope = globals(&columns, true);
        let mut run = Run::new(Mode::Compute, columns, values.clone(), limits);
        run.cancel = cancel;
        let started = Instant::now();
        let installed = Installed::new(run);
        let result = engine().run_ast_with_scope(&mut scope, &self.ast);
        let run = installed.finish();
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
        output.plots.push(super::super::PlotOut {
            key: intern::name(&plot.key),
            kind: decl.map_or(PlotKind::Line, |d| d.kind),
            values: plot.values.clone(),
            offset: plot.offset,
            up: plot.up.clone(),
        });
    }
    for fill in &run.fills {
        output.fills.push(super::super::FillOut {
            a: fill.a,
            b: fill.b,
            color: fill.color,
            alpha: fill.alpha,
            other: fill.other,
        });
    }
    output.levels = run.levels.clone();
    output.band = run.band;
    output
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(n: usize) -> StudyInput {
        let close: Vec<f64> = (0..n)
            .map(|i| 100.0 + (i as f64 * 0.7).sin() * 5.0)
            .collect();
        StudyInput {
            time: (0..n as i64).map(|i| i * 60_000).collect(),
            open: close.iter().map(|c| c - 0.5).collect(),
            high: close.iter().map(|c| c + 1.0).collect(),
            low: close.iter().map(|c| c - 1.0).collect(),
            volume: vec![10.0; n],
            day: vec![0; n],
            close,
        }
    }

    #[test]
    fn a_script_that_does_not_compile_says_where() {
        let problems = Script::compile("let a = 1;\nlet b = ;\n").unwrap_err();
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].line, 2);
    }

    #[test]
    fn an_unknown_name_is_found_before_the_script_runs() {
        let problems = Script::compile("plot(\"x\", clos);").unwrap_err();
        assert_eq!(problems[0].line, 1);
        assert!(
            problems[0].message.contains("clos"),
            "{}",
            problems[0].message
        );
    }

    #[test]
    fn the_declaration_run_learns_the_inputs_and_plots_without_bars() {
        let script = Script::compile(
            "indicator(#{ name: \"Two lines\", short: \"TL\", overlay: true });\n\
             let len = input_int(\"length\", 20, #{ min: 2, max: 200 });\n\
             plot(\"fast\", sma(close, len));\n\
             plot(\"slow\", sma(close, len * 2), #{ color: \"#ff9800\", width: 2 });",
        )
        .unwrap();
        let d = &script.declaration;
        assert_eq!(d.meta.name.as_deref(), Some("Two lines"));
        assert!(d.meta.overlay);
        assert_eq!(d.inputs.len(), 1);
        assert_eq!(
            (d.inputs[0].key.as_str(), d.inputs[0].default),
            ("length", 20.0)
        );
        assert_eq!(d.plots.len(), 2);
        assert_eq!(d.plots[1].color, 0xff9800);
    }

    #[test]
    fn computing_uses_the_values_the_user_chose() {
        let script = Script::compile(
            "let len = input_int(\"length\", 3, #{ min: 1, max: 50 });\n plot(\"ma\", sma(close, len));",
        )
        .unwrap();
        let input = bars(30);
        let mut values = BTreeMap::new();
        let with_default = script
            .compute(&input, &values, Limits::default(), None)
            .unwrap();
        values.insert("length".to_owned(), 5.0);
        let with_five = script
            .compute(&input, &values, Limits::default(), None)
            .unwrap();
        let (a, b) = (
            &with_default.output.plots[0].values,
            &with_five.output.plots[0].values,
        );
        assert!(a[1].is_nan() && a[2].is_finite());
        assert!(b[3].is_nan() && b[4].is_finite());
    }

    #[test]
    fn a_runaway_script_is_stopped() {
        let script = Script::compile("let i = 0;\n while i < n * 100_000_000 { i += 1; }").unwrap();
        let limits = Limits {
            operations: 50_000,
            time: Duration::from_secs(5),
        };
        let problems = script
            .compute(&bars(10), &BTreeMap::new(), limits, None)
            .unwrap_err();
        assert!(
            problems[0].message.contains("operations"),
            "{}",
            problems[0].message
        );
    }

    #[test]
    fn a_run_can_be_cancelled() {
        let script = Script::compile("let i = 0;\n while i < n * 100_000_000 { i += 1; }").unwrap();
        let cancel = Arc::new(AtomicBool::new(true));
        let limits = Limits {
            operations: u64::MAX,
            time: Duration::from_secs(60),
        };
        let started = Instant::now();
        let problems = script
            .compute(&bars(10), &BTreeMap::new(), limits, Some(cancel))
            .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(
            problems[0].message.contains("stopped"),
            "{}",
            problems[0].message
        );
    }

    #[test]
    fn a_mistake_that_the_bars_cause_says_the_line() {
        let script = Script::compile(
            "plot(\"a\", close);
let big = series(n * 1_000_000, 0.0);",
        )
        .unwrap();
        let problems = script
            .compute(&bars(10), &BTreeMap::new(), Limits::default(), None)
            .unwrap_err();
        assert_eq!(problems[0].line, 2);
        assert!(
            problems[0].message.contains("too long"),
            "{}",
            problems[0].message
        );
    }

    #[test]
    fn a_mistake_in_a_call_is_found_while_declaring() {
        let problems = Script::compile(
            "plot(\"a\", close);
let x = sma(close, 0);",
        )
        .unwrap_err();
        assert_eq!(problems[0].line, 2);
        assert!(
            problems[0].message.contains("period"),
            "{}",
            problems[0].message
        );
    }

    #[test]
    fn what_a_script_prints_is_kept_for_the_console() {
        let script = Script::compile("print(\"hello\");\nplot(\"a\", close);").unwrap();
        let done = script
            .compute(&bars(5), &BTreeMap::new(), Limits::default(), None)
            .unwrap();
        assert_eq!(done.log, vec!["hello".to_owned()]);
    }

    #[test]
    fn a_script_cannot_load_a_file_or_evaluate_text() {
        assert!(Script::compile("import \"other\" as o;").is_err());
        assert!(Script::compile("eval(\"1 + 1\");").is_err());
    }
}
