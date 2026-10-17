//! What an indicator is made of: where it is drawn, its inputs and its plots, and the fixed
//! description of each indicator ([`Spec`]).

use serde::{Deserialize, Serialize};
use wyck_core::math::count_f64;

use wyck_drawings::model::style::Dash;

/// Where an indicator is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// On the prices, in price units.
    Overlay,
    /// In a pane of its own under the prices, with its own scale.
    Pane,
}

/// How the values of a pane read on its axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueFormat {
    /// In the symbol's price units (raw prices), written like prices.
    Price,
    /// Plain numbers with this many decimals.
    Plain(u32),
    /// A percentage with this many decimals.
    Percent(u32),
    /// Large counts, shortened: 1.2K, 3.4M.
    Count,
}

/// What an input is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// A whole number.
    Int,
    /// A number with decimals.
    Float,
    /// Which price of a bar the indicator reads.
    Source,
    /// One of a few named choices, stored as its position.
    Choice(&'static [&'static str]),
    /// On or off, stored as 1 or 0.
    Toggle,
    /// A color, stored as its number `0xRRGGBB`.
    Color,
    /// The name of a symbol, kept as text. Empty means the symbol of the chart.
    Symbol,
    /// A timeframe code such as `15m` or `4h`, kept as text. Empty means the chart's own.
    Timeframe,
    /// A trading session such as `0930-1600`, kept as text.
    Session,
    /// Free text.
    Text,
}

impl InputKind {
    /// Whether the value is text, kept in [`super::config::StudyConfig::texts`], rather than a number.
    pub const fn is_text(self) -> bool {
        matches!(
            self,
            Self::Symbol | Self::Timeframe | Self::Session | Self::Text
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: InputKind,
    pub default: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// The heading this input sits under in the settings; empty for none.
    pub group: &'static str,
    /// A sentence shown beside the label; empty for none.
    pub tooltip: &'static str,
    /// The default of a text input (a symbol, a timeframe, a session); empty otherwise.
    pub text: &'static str,
}

pub(super) const fn int(
    key: &'static str,
    label: &'static str,
    default: f64,
    min: f64,
    max: f64,
) -> InputSpec {
    InputSpec {
        key,
        label,
        kind: InputKind::Int,
        default,
        min,
        max,
        step: 1.0,
        group: "",
        tooltip: "",
        text: "",
    }
}

pub(super) const fn float(
    key: &'static str,
    label: &'static str,
    default: f64,
    min: f64,
    max: f64,
    step: f64,
) -> InputSpec {
    InputSpec {
        key,
        label,
        kind: InputKind::Float,
        default,
        min,
        max,
        step,
        group: "",
        tooltip: "",
        text: "",
    }
}

pub(super) const fn choice(
    key: &'static str,
    label: &'static str,
    options: &'static [&'static str],
    default: f64,
) -> InputSpec {
    InputSpec {
        key,
        label,
        kind: InputKind::Choice(options),
        default,
        min: 0.0,
        max: count_f64(options.len() - 1),
        step: 1.0,
        group: "",
        tooltip: "",
        text: "",
    }
}

pub(super) const SOURCE: InputSpec = InputSpec {
    key: "source",
    label: "Source",
    kind: InputKind::Source,
    default: 3.0,
    min: 0.0,
    max: 7.0,
    step: 1.0,
    group: "",
    tooltip: "",
    text: "",
};

pub(super) const OFFSET: InputSpec = int("offset", "Offset", 0.0, -500.0, 500.0);

/// How a plot is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlotKind {
    Line,
    /// Columns from zero, colored up or down.
    Histogram,
    /// A dot per bar.
    Dots,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlotSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: PlotKind,
    pub color: u32,
    pub width: f32,
    pub dash: Dash,
    pub visible: bool,
}

pub(super) const fn line(
    key: &'static str,
    label: &'static str,
    color: u32,
    width: f32,
) -> PlotSpec {
    PlotSpec {
        key,
        label,
        kind: PlotKind::Line,
        color,
        width,
        dash: Dash::Solid,
        visible: true,
    }
}

pub(super) const fn hidden_line(key: &'static str, label: &'static str, color: u32) -> PlotSpec {
    PlotSpec {
        visible: false,
        ..line(key, label, color, 1.0)
    }
}

pub(super) const ATR_SMOOTHING: &[&str] = &["RMA (Wilder)", "SMA", "EMA", "WMA"];

/// Everything fixed about an indicator.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub label: &'static str,
    /// The short name shown in the legend: `SMA`, `RSI`.
    pub short: &'static str,
    pub placement: Placement,
    pub format: ValueFormat,
    pub inputs: &'static [InputSpec],
    pub plots: &'static [PlotSpec],
    /// A pane whose scale is fixed (an oscillator from 0 to 100).
    pub range: Option<(f64, f64)>,
}

/// The prices a source can read, in the order of [`InputKind::Source`] values.
pub const SOURCES: [&str; 8] = [
    "Open", "High", "Low", "Close", "HL2", "HLC3", "OHLC4", "HLCC4",
];

pub(super) const UP: u32 = 0x26_a69a;
pub(super) const DOWN: u32 = 0xef_5350;
pub(super) const BLUE: u32 = 0x29_62ff;
pub(super) const ORANGE: u32 = 0xff_9800;
pub(super) const PURPLE: u32 = 0x7e_57c2;
pub(super) const TEAL: u32 = 0x00_bcd4;
pub(super) const PINK: u32 = 0xe9_1e63;
pub(super) const GRAY: u32 = 0x78_7b86;

/// The kinds of moving average an indicator can offer, in the order of its `method` input.
pub(crate) const MA_METHODS: [&str; 5] = ["SMA", "EMA", "WMA", "RMA", "HMA"];
