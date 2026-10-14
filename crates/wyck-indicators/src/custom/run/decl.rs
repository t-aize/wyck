//! What a script declares about itself and what it reads: limits, problems, inputs, plots and the columns of bars.

use crate::compute::StudyInput;
use crate::custom::library::choices;
use crate::custom::series::Series;
use crate::spec::{InputKind, PlotKind, ValueFormat};
use rhai::Position;
use std::sync::Arc;
use std::time::Duration;
use wyck_drawings::model::style::Dash;

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
    Symbol,
    Timeframe,
    Session,
    Text,
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
    /// The settings tab requested by the script, when set.
    pub section: Option<InputSection>,
    /// The default of a text input; empty otherwise.
    pub text: String,
    /// The heading the input sits under, when the script gave one.
    pub group: Option<String>,
    /// A sentence shown beside the label, when the script gave one.
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSection {
    Inputs,
    Style,
}

impl InputDecl {
    pub fn in_style(&self) -> bool {
        match self.section {
            Some(InputSection::Inputs) => false,
            Some(InputSection::Style) => true,
            None => {
                if self.kind == InputType::Color {
                    return true;
                }
                let name = format!("{} {}", self.key, self.label).to_ascii_lowercase();
                [
                    "opacity",
                    "width",
                    "thickness",
                    "text size",
                    "font size",
                    "text_size",
                    "font_size",
                ]
                .iter()
                .any(|word| name.contains(word))
            }
        }
    }
    /// The kind of input the panels know.
    pub(crate) fn input_kind(&self) -> InputKind {
        match self.kind {
            InputType::Int => InputKind::Int,
            InputType::Float => InputKind::Float,
            InputType::Bool => InputKind::Toggle,
            InputType::Source => InputKind::Source,
            InputType::Choice => InputKind::Choice(choices(&self.options)),
            InputType::Color => InputKind::Color,
            InputType::Symbol => InputKind::Symbol,
            InputType::Timeframe => InputKind::Timeframe,
            InputType::Session => InputKind::Session,
            InputType::Text => InputKind::Text,
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
pub(crate) const MAX_INPUTS: usize = 32;
pub(crate) const MAX_PLOTS: usize = 16;
/// Maximum number of drawings generated by one indicator instance.
pub(crate) const MAX_SCRIPT_DRAWINGS: usize = 500;

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
    /// Exact timestamps for points created from bar indices.
    pub raw_time: Arc<Vec<i64>>,
    pub hl2: Series,
    pub hlc3: Series,
    pub ohlc4: Series,
    pub hlcc4: Series,
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
            raw_time: Arc::new(input.time.clone()),
            hl2: Series::new(input.source(4.0)),
            hlc3: Series::new(input.source(5.0)),
            ohlc4: Series::new(input.source(6.0)),
            hlcc4: Series::new(input.source(7.0)),
            bar_index: Series::new((0..n).map(|i| i as f64).collect()),
            day: Arc::new(input.day.clone()),
        }
    }

    /// The price a source input names, by its position in [`crate::spec::SOURCES`].
    pub fn source(&self, which: usize) -> Series {
        match which {
            0 => self.open.clone(),
            1 => self.high.clone(),
            2 => self.low.clone(),
            4 => self.hl2.clone(),
            5 => self.hlc3.clone(),
            6 => self.ohlc4.clone(),
            7 => self.hlcc4.clone(),
            _ => self.close.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::run::*;

    #[test]
    fn style_inputs_follow_kind_name_and_explicit_section() {
        let mut input = InputDecl {
            key: "bullish_fvg".to_owned(),
            label: "Bullish FVG".to_owned(),
            kind: InputType::Color,
            default: 0.0,
            min: 0.0,
            max: 1.0,
            step: 1.0,
            options: Vec::new(),
            section: None,
            text: String::new(),
            group: None,
            tooltip: None,
        };
        assert!(input.in_style());
        input.section = Some(InputSection::Inputs);
        assert!(!input.in_style());
        input.key = "fill_opacity".to_owned();
        input.kind = InputType::Float;
        input.section = None;
        assert!(input.in_style());
        input.section = Some(InputSection::Inputs);
        assert!(!input.in_style());
        input.section = Some(InputSection::Style);
        assert!(input.in_style());
    }

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
