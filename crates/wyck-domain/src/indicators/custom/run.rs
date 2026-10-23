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

use super::api;
use super::context::{Installed, Mode, Run, Stop, budget, with_run};
use super::decl;
use super::docs;
use super::lexer::{self, Kind};
use super::other::{self, OtherSymbols};
use crate::base::intern;
use crate::base::math::count_i64;
use crate::indicators::data::{StudyContext, StudyInput, StudyOutput};
use crate::indicators::spec::PlotKind;
use rhai::{AST, Dynamic, Engine, EvalAltResult, ParseError, Position, Scope};
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

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
            // A `print` outside a running script has no log to go to: nothing to do.
            with_run(|run| run.say(text.to_owned())).unwrap_or_default();
        });
        engine.on_debug(|text, _source, position| {
            let line = position
                .line()
                .map_or(String::new(), |l| format!("[line {l}] "));
            with_run(|run| run.say(format!("{line}{text}"))).unwrap_or_default();
        });
        // The budget: every operation asks whether the run must stop.
        engine.on_progress(budget);
        api::register(&mut engine);
        engine
    })
}

/// The names a script starts with: the bars, and how many there are. When the script is being
/// compiled they are plain variables: constants would be folded into the program, with the bars it
/// was compiled against.
fn globals(columns: &decl::Columns, constant: bool) -> Scope<'static> {
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
    put("n", Dynamic::from(count_i64(columns.len)));
    put("na", Dynamic::from(f64::NAN));
    scope
}

/// The names a script starts with, for the editor.
#[cfg(test)]
pub(crate) fn global_names() -> Vec<String> {
    globals(&decl::Columns::default(), false)
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
    /// Things worth knowing that did not stop the run.
    pub warnings: Vec<decl::Problem>,
    pub elapsed: Duration,
}

/// Runs `ast` on the engine. A function of the script that panics is a bug of Wyck, not a reason
/// to take the app down: it ends the run with an error.
fn run_ast(scope: &mut Scope<'_>, ast: &AST) -> Result<(), Box<EvalAltResult>> {
    catch_unwind(AssertUnwindSafe(|| engine().run_ast_with_scope(scope, ast))).unwrap_or_else(
        |_| {
            Err(
                "a function of the script failed in a way it should not: this is a bug in Wyck"
                    .into(),
            )
        },
    )
}

/// Whether `source` calls the function `name`, outside comments and strings.
fn calls(source: &str, name: &str) -> bool {
    lexer::lex(source).windows(2).any(|pair| {
        let [word, open] = pair else {
            return false;
        };
        !matches!(word.kind, Kind::Comment | Kind::String)
            && source.get(word.range.clone()) == Some(name)
            && source.get(open.range.clone()) == Some("(")
    })
}

/// The first difference between what a script declared on no bars and what it declares on the
/// bars of a chart. The panels are built from the first, so the second must be the same: an input
/// or a plot behind an `if` on the bars would be drawn without being known.
fn declaration_changed(first: &decl::Declaration, now: &decl::Declaration) -> Option<String> {
    for (position, declared) in first.inputs.iter().enumerate() {
        match now.inputs.get(position) {
            Some(found) if found.key == declared.key && found.kind == declared.kind => {}
            _ => {
                return Some(format!(
                    "the input \"{}\" is not declared the same way when the script runs on bars",
                    declared.key
                ));
            }
        }
    }
    if let Some(extra) = now.inputs.get(first.inputs.len()) {
        return Some(format!(
            "the input \"{}\" is only declared when the script runs on bars",
            extra.key
        ));
    }
    for (position, declared) in first.plots.iter().enumerate() {
        match now.plots.get(position) {
            Some(found) if found.key == declared.key && found.kind == declared.kind => {}
            _ => {
                return Some(format!(
                    "the plot \"{}\" is not drawn the same way when the script runs on bars",
                    declared.key
                ));
            }
        }
    }
    now.plots.get(first.plots.len()).map(|extra| {
        format!(
            "the plot \"{}\" is only drawn when the script runs on bars",
            extra.key
        )
    })
}

/// What a run reads besides the bars: the values the user chose for the inputs, the bars of the
/// other symbols, and what the chart is.
pub struct Setup<'a> {
    pub values: &'a BTreeMap<String, f64>,
    pub texts: &'a BTreeMap<String, String>,
    pub others: &'a OtherSymbols,
    pub context: &'a StudyContext,
}

/// A script that compiles and whose declaration ran.
#[derive(Debug, Clone)]
pub struct Script {
    ast: AST,
    pub declaration: decl::Declaration,
    /// Things worth knowing that do not stop it: it draws nothing, it has no `indicator` call.
    pub warnings: Vec<decl::Problem>,
}

impl Script {
    /// Compiles `source` and runs it on no bars to learn what it declares.
    ///
    /// # Errors
    ///
    /// The problems found, when it does not compile or fails.
    pub fn compile(source: &str) -> Result<Self, Vec<decl::Problem>> {
        Self::compile_with(source, decl::Limits::default())
    }

    /// [`Self::compile`], with the limits the declaration run has.
    ///
    /// # Errors
    ///
    /// The problems found, when it does not compile or fails.
    pub(crate) fn compile_with(
        source: &str,
        limits: decl::Limits,
    ) -> Result<Self, Vec<decl::Problem>> {
        let scope = globals(&decl::Columns::default(), false);
        let ast = engine()
            .compile_with_scope(&scope, source)
            .map_err(|error| vec![parse_problem(&error)])?;
        let run = Run::new(
            Mode::Declare,
            decl::Columns::default(),
            BTreeMap::new(),
            limits,
        );
        let installed = Installed::new(run);
        let mut scope = globals(&decl::Columns::default(), true);
        let result = run_ast(&mut scope, &ast);
        let Some(run) = installed.finish() else {
            return Err(vec![lost_run()]);
        };
        if let Err(error) = result {
            return Err(vec![runtime_problem(&error, &run)]);
        }
        let mut warnings = run.warnings.clone();
        if run.declaration.plots.is_empty() && !calls(source, "draw") {
            warnings.push(decl::Problem::warning(
                0,
                0,
                "the script draws nothing: add a plot(...) or draw(...) call",
            ));
        }
        if !run.declared {
            warnings.push(decl::Problem::warning(
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

    /// The symbols the script reads the prices of: what the user chose for each input of type
    /// symbol (the default when nothing was chosen), as [`other::key_of`] their names. The symbol
    /// of the chart, which an empty name stands for, is not in the list.
    pub fn symbols_wanted(&self, texts: &BTreeMap<String, String>) -> Vec<String> {
        let mut wanted: Vec<String> = self
            .declaration
            .inputs
            .iter()
            .filter(|input| input.kind == decl::InputType::Symbol)
            .map(|input| {
                texts
                    .get(&input.key)
                    .map(|text| text.trim())
                    .filter(|text| text.chars().count() <= api::inputs::MAX_TEXT)
                    .unwrap_or(&input.text)
            })
            .map(other::key_of)
            .filter(|key| !key.is_empty())
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        wanted
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
        limits: decl::Limits,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<Computed, Vec<decl::Problem>> {
        self.compute_with_texts(input, values, &BTreeMap::new(), limits, cancel)
    }

    /// [`Self::compute`] with the text the user chose for the inputs that are text.
    ///
    /// # Errors
    ///
    /// The problem that stopped it: a mistake of the script, or the limits.
    pub(crate) fn compute_with_texts(
        &self,
        input: &StudyInput,
        values: &BTreeMap<String, f64>,
        texts: &BTreeMap<String, String>,
        limits: decl::Limits,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<Computed, Vec<decl::Problem>> {
        self.compute_with_others(input, values, texts, &OtherSymbols::new(), limits, cancel)
    }

    /// `compute_with_texts` with the bars of other symbols, which the script reads with
    /// `bars_of`. `others` holds them under [`other::key_of`] their names.
    ///
    /// # Errors
    ///
    /// The problem that stopped it: a mistake of the script, or the limits.
    pub fn compute_with_others(
        &self,
        input: &StudyInput,
        values: &BTreeMap<String, f64>,
        texts: &BTreeMap<String, String>,
        others: &OtherSymbols,
        limits: decl::Limits,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<Computed, Vec<decl::Problem>> {
        let context = StudyContext::default();
        let setup = Setup {
            values,
            texts,
            others,
            context: &context,
        };
        self.compute_in(input, &setup, limits, cancel)
    }

    /// Computes the script over `input` with everything a run may read besides the bars.
    ///
    /// # Errors
    ///
    /// The problem that stopped it: a mistake of the script, or the limits.
    pub fn compute_in(
        &self,
        input: &StudyInput,
        setup: &Setup<'_>,
        limits: decl::Limits,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<Computed, Vec<decl::Problem>> {
        let columns = decl::Columns::from_input(input);
        let mut scope = globals(&columns, true);
        let mut run = Run::new(Mode::Compute, columns, setup.values.clone(), limits);
        run.texts = setup.texts.clone();
        run.context = setup.context.clone();
        run.others = setup
            .others
            .iter()
            .map(|(name, bars)| (name.clone(), other::Aligned::on(&input.time, bars)))
            .collect();
        run.cancel = cancel;
        let started = Instant::now();
        let installed = Installed::new(run);
        let result = run_ast(&mut scope, &self.ast);
        let Some(run) = installed.finish() else {
            return Err(vec![lost_run()]);
        };
        let elapsed = started.elapsed();
        if let Err(error) = result {
            return Err(vec![runtime_problem(&error, &run)]);
        }
        if let Some(message) = declaration_changed(&self.declaration, &run.declaration) {
            return Err(vec![decl::Problem::error(0, 0, message)]);
        }
        Ok(Computed {
            output: output_of(&run),
            log: run.log,
            warnings: run.warnings,
            elapsed,
        })
    }
}

fn output_of(run: &Run) -> StudyOutput {
    let mut output = StudyOutput::default();
    for plot in &run.plots {
        let decl = run.declaration.plots.iter().find(|d| d.key == plot.key);
        output.plots.push(crate::indicators::data::PlotOut {
            key: intern::name(&plot.key),
            kind: decl.map_or(PlotKind::Line, |d| d.kind),
            values: plot.values.clone(),
            offset: plot.offset,
            up: plot.up.clone(),
        });
    }
    for fill in &run.fills {
        output.fills.push(crate::indicators::data::FillOut {
            a: fill.a,
            b: fill.b,
            color: fill.color,
            alpha: fill.alpha,
            other: fill.other,
        });
    }
    output.levels.clone_from(&run.levels);
    output.band = run.band;
    output.drawings.clone_from(&run.drawings);
    output
}

/// What is reported when a run vanished while its script was running.
fn lost_run() -> decl::Problem {
    decl::Problem::error(
        0,
        0,
        "the run of the script was lost, this is a bug in Wyck",
    )
}

fn parse_problem(error: &ParseError) -> decl::Problem {
    decl::Problem::at(error.position(), error.err_type().to_string())
}

/// The message of an error without the position Rhai writes after it, and the innermost error of
/// a call inside a call.
fn message_of(error: &EvalAltResult) -> (String, Position) {
    match error {
        EvalAltResult::ErrorInFunctionCall(_, _, inner, _) => message_of(inner),
        EvalAltResult::ErrorRuntime(value, position) => (value.to_string(), *position),
        // The usual cause is the wrong kind of argument: say how the function is called.
        EvalAltResult::ErrorFunctionNotFound(signature, position) => {
            let name = signature.split([' ', '(']).next().unwrap_or_default();
            let usage = docs::function(name)
                .map(|doc| format!(". Usage: {}", doc.signature))
                .unwrap_or_default();
            (format!("function not found: {signature}{usage}"), *position)
        }
        other => {
            let text = other.to_string();
            let cut = text.rfind(" (line ").unwrap_or(text.len());
            (
                text.get(..cut).unwrap_or_default().to_owned(),
                other.position(),
            )
        }
    }
}

fn runtime_problem(error: &EvalAltResult, run: &Run) -> decl::Problem {
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
            Some(Stop::Memory) => format!(
                "the script held more than {} MiB of series at once and was stopped: a series takes 8 bytes a bar, so keep fewer of them",
                run.meter.limit() / (1024 * 1024)
            ),
            _ => "the script was stopped".to_owned(),
        };
        return decl::Problem::at(*position, message);
    }
    let (message, position) = message_of(error);
    decl::Problem::at(position, message)
}
