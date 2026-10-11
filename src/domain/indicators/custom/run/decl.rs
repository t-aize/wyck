//! What a script declares about itself and what it reads: limits, problems, inputs, plots and the columns of bars.

use crate::domain::drawings::model::Dash;
use crate::domain::indicators::custom::series::Series;
use crate::domain::indicators::intern;
use crate::domain::indicators::{InputKind, PlotKind, StudyInput, ValueFormat};
use rhai::Position;
use std::sync::Arc;
use std::time::Duration;

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
    pub fn input_kind(&self) -> InputKind {
        match self.kind {
            InputType::Int => InputKind::Int,
            InputType::Float => InputKind::Float,
            InputType::Bool => InputKind::Toggle,
            InputType::Source => InputKind::Source,
            InputType::Choice => InputKind::Choice(intern::list(&self.options)),
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
pub const MAX_INPUTS: usize = 32;
pub const MAX_PLOTS: usize = 16;
/// Maximum number of drawings generated by one indicator instance.
pub const MAX_SCRIPT_DRAWINGS: usize = 500;

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

    /// The price a source input names, by its position in [`crate::domain::indicators::SOURCES`].
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
