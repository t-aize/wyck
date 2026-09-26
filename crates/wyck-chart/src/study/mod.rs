//! Indicators ("studies"): moving averages, bands, oscillators and the volume profile.
//!
//! - [`math`]: the arithmetic, series in and series out, tested against worked examples.
//! - This file: what each indicator is ([`StudyKind`] and its [`Spec`]: inputs, plots, where it
//!   is drawn), what the user set ([`StudyConfig`], which is also what is saved), and
//!   [`compute`], which turns bars into the lines to plot ([`StudyOutput`]).
//! - [`profile`]: the volume profile of the visible bars, which is not a line per bar.
//!
//! A config names its inputs and plots by stable keys (`length`, `upper`, `signal`), never by
//! position, and every one of them has a default. A file written by another version still loads:
//! an unknown indicator is dropped, a missing input takes its default, an unknown one is ignored.

pub mod catalog;
pub mod custom;
pub mod intern;
pub mod math;
pub mod profile;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::drawing::model::Dash;

/// The indicators on offer. The names are written in saved files and never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudyKind {
    Sma,
    Ema,
    Wma,
    Hma,
    Vwap,
    Bollinger,
    Keltner,
    Donchian,
    Ichimoku,
    ParabolicSar,
    VolumeProfile,
    Volume,
    Rsi,
    Macd,
    Stochastic,
    Atr,
    Cci,
    WilliamsR,
    Momentum,
    Obv,
    Dmi,
    /// An indicator written as a script: which one is in [`StudyConfig::script`].
    Custom,
    /// An indicator a newer version wrote. Dropped on load.
    #[serde(other)]
    Unknown,
}

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
#[derive(Debug, Clone, Copy, PartialEq)]
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
}

const fn int(
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
    }
}

const fn float(
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
    }
}

const SOURCE: InputSpec = InputSpec {
    key: "source",
    label: "Source",
    kind: InputKind::Source,
    default: 3.0,
    min: 0.0,
    max: 6.0,
    step: 1.0,
};

const OFFSET: InputSpec = int("offset", "Offset", 0.0, -500.0, 500.0);

/// How a plot is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

const fn line(key: &'static str, label: &'static str, color: u32, width: f32) -> PlotSpec {
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

const fn hidden_line(key: &'static str, label: &'static str, color: u32) -> PlotSpec {
    PlotSpec {
        visible: false,
        ..line(key, label, color, 1.0)
    }
}

const ATR_SMOOTHING: &[&str] = &["RMA (Wilder)", "SMA", "EMA", "WMA"];

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
pub const SOURCES: [&str; 7] = ["Open", "High", "Low", "Close", "HL2", "HLC3", "OHLC4"];

const UP: u32 = 0x26a69a;
const DOWN: u32 = 0xef5350;
const BLUE: u32 = 0x2962ff;
const ORANGE: u32 = 0xff9800;
const PURPLE: u32 = 0x7e57c2;
const TEAL: u32 = 0x00bcd4;
const PINK: u32 = 0xe91e63;
const GRAY: u32 = 0x787b86;

impl StudyKind {
    /// Every indicator a user can add, in menu order.
    pub const ALL: [Self; 21] = [
        Self::Sma,
        Self::Ema,
        Self::Wma,
        Self::Hma,
        Self::Vwap,
        Self::Bollinger,
        Self::Keltner,
        Self::Donchian,
        Self::Ichimoku,
        Self::ParabolicSar,
        Self::VolumeProfile,
        Self::Volume,
        Self::Rsi,
        Self::Macd,
        Self::Stochastic,
        Self::Atr,
        Self::Cci,
        Self::WilliamsR,
        Self::Momentum,
        Self::Obv,
        Self::Dmi,
    ];

    pub fn spec(self) -> Spec {
        use Placement::{Overlay, Pane};
        match self {
            Self::Sma => {
                const {
                    Spec {
                        label: "Moving average (simple)",
                        short: "SMA",
                        placement: Overlay,
                        format: ValueFormat::Price,
                        inputs: &[int("length", "Length", 20.0, 1.0, 1000.0), SOURCE, OFFSET],
                        plots: &[line("ma", "Average", BLUE, 1.5)],
                        range: None,
                    }
                }
            }
            Self::Ema => {
                const {
                    Spec {
                        label: "Moving average (exponential)",
                        short: "EMA",
                        placement: Overlay,
                        format: ValueFormat::Price,
                        inputs: &[int("length", "Length", 50.0, 1.0, 1000.0), SOURCE, OFFSET],
                        plots: &[line("ma", "Average", ORANGE, 1.5)],
                        range: None,
                    }
                }
            }
            Self::Wma => {
                const {
                    Spec {
                        label: "Moving average (weighted)",
                        short: "WMA",
                        placement: Overlay,
                        format: ValueFormat::Price,
                        inputs: &[int("length", "Length", 20.0, 1.0, 1000.0), SOURCE, OFFSET],
                        plots: &[line("ma", "Average", PURPLE, 1.5)],
                        range: None,
                    }
                }
            }
            Self::Hma => {
                const {
                    Spec {
                        label: "Hull moving average",
                        short: "HMA",
                        placement: Overlay,
                        format: ValueFormat::Price,
                        inputs: &[int("length", "Length", 21.0, 2.0, 1000.0), SOURCE, OFFSET],
                        plots: &[line("ma", "Average", TEAL, 1.5)],
                        range: None,
                    }
                }
            }
            Self::Vwap => {
                const {
                    Spec {
                        label: "VWAP (daily)",
                        short: "VWAP",
                        placement: Overlay,
                        format: ValueFormat::Price,
                        inputs: &[InputSpec {
                            default: 5.0,
                            ..SOURCE
                        }],
                        plots: &[line("vwap", "VWAP", PINK, 1.5)],
                        range: None,
                    }
                }
            }
            Self::Bollinger => {
                const {
                    Spec {
                        label: "Bollinger bands",
                        short: "BB",
                        placement: Overlay,
                        format: ValueFormat::Price,
                        inputs: &[
                            int("length", "Length", 20.0, 1.0, 1000.0),
                            float("mult", "Deviations", 2.0, 0.1, 10.0, 0.1),
                            SOURCE,
                        ],
                        plots: &[
                            line("basis", "Basis", ORANGE, 1.0),
                            line("upper", "Upper", BLUE, 1.0),
                            line("lower", "Lower", BLUE, 1.0),
                        ],
                        range: None,
                    }
                }
            }
            Self::Keltner => {
                const {
                    Spec {
                        label: "Keltner channels",
                        short: "KC",
                        placement: Overlay,
                        format: ValueFormat::Price,
                        inputs: &[
                            int("length", "Length", 20.0, 1.0, 1000.0),
                            float("mult", "Multiplier", 2.0, 0.1, 10.0, 0.1),
                            int("atr", "ATR length", 10.0, 1.0, 1000.0),
                        ],
                        plots: &[
                            line("basis", "Basis", BLUE, 1.0),
                            line("upper", "Upper", BLUE, 1.0),
                            line("lower", "Lower", BLUE, 1.0),
                        ],
                        range: None,
                    }
                }
            }
            Self::Donchian => {
                const {
                    Spec {
                        label: "Donchian channels",
                        short: "DC",
                        placement: Overlay,
                        format: ValueFormat::Price,
                        inputs: &[int("length", "Length", 20.0, 1.0, 1000.0)],
                        plots: &[
                            line("basis", "Basis", ORANGE, 1.0),
                            line("upper", "Upper", BLUE, 1.0),
                            line("lower", "Lower", BLUE, 1.0),
                        ],
                        range: None,
                    }
                }
            }
            Self::Ichimoku => {
                const {
                    Spec {
                        label: "Ichimoku cloud",
                        short: "Ichimoku",
                        placement: Overlay,
                        format: ValueFormat::Price,
                        inputs: &[
                            int("conversion", "Conversion line", 9.0, 1.0, 500.0),
                            int("base", "Base line", 26.0, 1.0, 500.0),
                            int("span_b", "Leading span B", 52.0, 1.0, 500.0),
                            int("displacement", "Displacement", 26.0, 1.0, 500.0),
                        ],
                        plots: &[
                            line("conversion", "Conversion line", BLUE, 1.0),
                            line("base", "Base line", 0xb71c1c, 1.0),
                            line("lagging", "Lagging span", 0x43a047, 1.0),
                            line("span_a", "Leading span A", 0xa5d6a7, 1.0),
                            line("span_b", "Leading span B", 0xef9a9a, 1.0),
                        ],
                        range: None,
                    }
                }
            }
            Self::ParabolicSar => {
                const {
                    Spec {
                        label: "Parabolic SAR",
                        short: "SAR",
                        placement: Overlay,
                        format: ValueFormat::Price,
                        inputs: &[
                            float("start", "Start", 0.02, 0.001, 1.0, 0.001),
                            float("step", "Increment", 0.02, 0.001, 1.0, 0.001),
                            float("max", "Maximum", 0.2, 0.01, 1.0, 0.01),
                        ],
                        plots: &[PlotSpec {
                            key: "sar",
                            label: "SAR",
                            kind: PlotKind::Dots,
                            color: BLUE,
                            width: 2.0,
                            dash: Dash::Solid,
                            visible: true,
                        }],
                        range: None,
                    }
                }
            }
            Self::VolumeProfile => {
                const {
                    Spec {
                        label: "Volume profile (visible range)",
                        short: "VPVR",
                        placement: Overlay,
                        format: ValueFormat::Count,
                        inputs: &[
                            int("rows", "Rows", 24.0, 4.0, 200.0),
                            int("value_area", "Value area (%)", 70.0, 1.0, 100.0),
                            int("width", "Width (% of the chart)", 30.0, 5.0, 100.0),
                            InputSpec {
                                key: "side",
                                label: "Placement",
                                kind: InputKind::Choice(&["Right", "Left"]),
                                default: 0.0,
                                min: 0.0,
                                max: 1.0,
                                step: 1.0,
                            },
                            InputSpec {
                                key: "highlight",
                                label: "Highlight the value area",
                                kind: InputKind::Toggle,
                                default: 1.0,
                                min: 0.0,
                                max: 1.0,
                                step: 1.0,
                            },
                        ],
                        plots: &[
                            line("up", "Up volume", UP, 1.0),
                            line("down", "Down volume", DOWN, 1.0),
                            line("poc", "Point of control", 0xffeb3b, 1.0),
                        ],
                        range: None,
                    }
                }
            }
            Self::Volume => {
                const {
                    Spec {
                        label: "Volume (ticks)",
                        short: "Vol",
                        placement: Pane,
                        format: ValueFormat::Count,
                        inputs: &[int("ma", "Average length", 20.0, 1.0, 1000.0)],
                        plots: &[
                            PlotSpec {
                                key: "volume",
                                label: "Volume",
                                kind: PlotKind::Histogram,
                                color: GRAY,
                                width: 1.0,
                                dash: Dash::Solid,
                                visible: true,
                            },
                            line("ma", "Average", BLUE, 1.0),
                        ],
                        range: None,
                    }
                }
            }
            Self::Rsi => {
                const {
                    Spec {
                        label: "Relative strength index",
                        short: "RSI",
                        placement: Pane,
                        format: ValueFormat::Plain(2),
                        inputs: &[
                            int("length", "Length", 14.0, 1.0, 1000.0),
                            SOURCE,
                            float("upper", "Overbought", 70.0, 0.0, 100.0, 1.0),
                            float("lower", "Oversold", 30.0, 0.0, 100.0, 1.0),
                        ],
                        plots: &[line("rsi", "RSI", PURPLE, 1.5)],
                        range: Some((0.0, 100.0)),
                    }
                }
            }
            Self::Macd => {
                const {
                    Spec {
                        label: "MACD",
                        short: "MACD",
                        placement: Pane,
                        format: ValueFormat::Price,
                        inputs: &[
                            int("fast", "Fast length", 12.0, 1.0, 500.0),
                            int("slow", "Slow length", 26.0, 1.0, 500.0),
                            int("signal", "Signal length", 9.0, 1.0, 500.0),
                            SOURCE,
                        ],
                        plots: &[
                            PlotSpec {
                                key: "histogram",
                                label: "Histogram",
                                kind: PlotKind::Histogram,
                                color: GRAY,
                                width: 1.0,
                                dash: Dash::Solid,
                                visible: true,
                            },
                            line("macd", "MACD", BLUE, 1.5),
                            line("signal", "Signal", ORANGE, 1.5),
                        ],
                        range: None,
                    }
                }
            }
            Self::Stochastic => {
                const {
                    Spec {
                        label: "Stochastic",
                        short: "Stoch",
                        placement: Pane,
                        format: ValueFormat::Plain(2),
                        inputs: &[
                            int("length", "%K length", 14.0, 1.0, 500.0),
                            int("smooth", "%K smoothing", 3.0, 1.0, 100.0),
                            int("d", "%D smoothing", 3.0, 1.0, 100.0),
                            float("upper", "Upper band", 80.0, 0.0, 100.0, 1.0),
                            float("lower", "Lower band", 20.0, 0.0, 100.0, 1.0),
                        ],
                        plots: &[line("k", "%K", BLUE, 1.5), line("d", "%D", ORANGE, 1.5)],
                        range: Some((0.0, 100.0)),
                    }
                }
            }
            Self::Atr => {
                const {
                    Spec {
                        label: "Average true range",
                        short: "ATR",
                        placement: Pane,
                        format: ValueFormat::Price,
                        inputs: &[
                            int("length", "ATR length", 14.0, 1.0, 1000.0),
                            InputSpec {
                                key: "smoothing",
                                label: "ATR smoothing",
                                kind: InputKind::Choice(ATR_SMOOTHING),
                                default: 0.0,
                                min: 0.0,
                                max: 3.0,
                                step: 1.0,
                            },
                            InputSpec {
                                key: "unit",
                                label: "Display unit",
                                kind: InputKind::Choice(&["Price", "% of close"]),
                                default: 0.0,
                                min: 0.0,
                                max: 1.0,
                                step: 1.0,
                            },
                            int("percent_decimals", "Percent decimals", 2.0, 0.0, 6.0),
                            int("signal_length", "Signal length", 14.0, 1.0, 1000.0),
                            InputSpec {
                                key: "signal_smoothing",
                                label: "Signal smoothing",
                                kind: InputKind::Choice(ATR_SMOOTHING),
                                default: 1.0,
                                min: 0.0,
                                max: 3.0,
                                step: 1.0,
                            },
                        ],
                        plots: &[
                            line("atr", "ATR", 0xb71c1c, 1.5),
                            hidden_line("signal", "Signal average", ORANGE),
                            hidden_line("tr", "True range", GRAY),
                        ],
                        range: None,
                    }
                }
            }
            Self::Cci => {
                const {
                    Spec {
                        label: "Commodity channel index",
                        short: "CCI",
                        placement: Pane,
                        format: ValueFormat::Plain(2),
                        inputs: &[
                            int("length", "Length", 20.0, 1.0, 1000.0),
                            float("upper", "Upper band", 100.0, -1000.0, 1000.0, 1.0),
                            float("lower", "Lower band", -100.0, -1000.0, 1000.0, 1.0),
                        ],
                        plots: &[line("cci", "CCI", TEAL, 1.5)],
                        range: None,
                    }
                }
            }
            Self::WilliamsR => {
                const {
                    Spec {
                        label: "Williams %R",
                        short: "%R",
                        placement: Pane,
                        format: ValueFormat::Plain(2),
                        inputs: &[
                            int("length", "Length", 14.0, 1.0, 1000.0),
                            float("upper", "Upper band", -20.0, -100.0, 0.0, 1.0),
                            float("lower", "Lower band", -80.0, -100.0, 0.0, 1.0),
                        ],
                        plots: &[line("r", "%R", PURPLE, 1.5)],
                        range: Some((-100.0, 0.0)),
                    }
                }
            }
            Self::Momentum => {
                const {
                    Spec {
                        label: "Momentum",
                        short: "Mom",
                        placement: Pane,
                        format: ValueFormat::Price,
                        inputs: &[int("length", "Length", 10.0, 1.0, 1000.0), SOURCE],
                        plots: &[line("mom", "Momentum", BLUE, 1.5)],
                        range: None,
                    }
                }
            }
            Self::Obv => {
                const {
                    Spec {
                        label: "On balance volume (ticks)",
                        short: "OBV",
                        placement: Pane,
                        format: ValueFormat::Count,
                        inputs: &[],
                        plots: &[line("obv", "OBV", BLUE, 1.5)],
                        range: None,
                    }
                }
            }
            Self::Dmi => {
                const {
                    Spec {
                        label: "Directional movement (ADX)",
                        short: "DMI",
                        placement: Pane,
                        format: ValueFormat::Plain(2),
                        inputs: &[
                            int("length", "DI length", 14.0, 1.0, 500.0),
                            int("smoothing", "ADX smoothing", 14.0, 1.0, 500.0),
                        ],
                        plots: &[
                            line("adx", "ADX", 0xff5252, 1.5),
                            line("plus", "+DI", BLUE, 1.0),
                            line("minus", "-DI", ORANGE, 1.0),
                        ],
                        range: None,
                    }
                }
            }
            Self::Unknown | Self::Custom => {
                const {
                    Spec {
                        label: "Unknown",
                        short: "?",
                        placement: Overlay,
                        format: ValueFormat::Plain(2),
                        inputs: &[],
                        plots: &[],
                        range: None,
                    }
                }
            }
        }
    }
}

/// How one plot of an indicator looks.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlotStyle {
    pub color: u32,
    pub width: f32,
    #[serde(default = "yes")]
    pub visible: bool,
    /// How opaque the plot is, from 0 to 1.
    #[serde(default = "opaque")]
    pub opacity: f32,
    /// The line style of a line plot.
    #[serde(default)]
    pub dash: Dash,
}

fn yes() -> bool {
    true
}

fn opaque() -> f32 {
    1.0
}

/// One indicator on a chart, as the user set it. Also its saved form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudyConfig {
    pub kind: StudyKind,
    /// The id of the script, for [`StudyKind::Custom`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    #[serde(default)]
    pub inputs: BTreeMap<String, f64>,
    #[serde(default)]
    pub plots: BTreeMap<String, PlotStyle>,
    #[serde(default = "yes")]
    pub visible: bool,
    /// How tall its pane is, relative to the prices (only for an indicator with a pane).
    #[serde(default = "pane_weight")]
    pub weight: f32,
}

fn pane_weight() -> f32 {
    1.0
}

impl StudyConfig {
    /// An indicator with every input and plot at its default.
    pub fn new(kind: StudyKind) -> Self {
        Self {
            kind,
            script: None,
            inputs: BTreeMap::new(),
            plots: BTreeMap::new(),
            visible: true,
            weight: pane_weight(),
        }
        .normalized()
    }

    /// An indicator written as a script, with every input and plot at its default.
    pub fn for_script(id: &str) -> Self {
        Self {
            kind: StudyKind::Custom,
            script: Some(id.to_owned()),
            inputs: BTreeMap::new(),
            plots: BTreeMap::new(),
            visible: true,
            weight: pane_weight(),
        }
        .normalized()
    }

    /// The same indicator with every input and plot at its default.
    #[must_use]
    pub fn fresh(&self) -> Self {
        match (&self.kind, self.script.as_deref()) {
            (StudyKind::Custom, Some(id)) => Self::for_script(id),
            (kind, _) => Self::new(*kind),
        }
    }

    /// Whether this is an indicator written as a script.
    pub fn is_script(&self) -> bool {
        self.kind == StudyKind::Custom
    }

    /// Everything fixed about the indicator: what the app ships, or what the script declared. A
    /// script that is gone gives an empty spec.
    pub fn spec(&self) -> Spec {
        match self.kind {
            StudyKind::Custom => custom::spec_for(self.script.as_deref()),
            kind => kind.spec(),
        }
    }

    /// Whether the script this indicator holds is known and works. True for the indicators the
    /// app ships.
    pub fn is_ready(&self) -> bool {
        self.kind != StudyKind::Custom
            || self
                .script
                .as_deref()
                .and_then(custom::library::registry::get)
                .is_some_and(|entry| entry.is_ready())
    }

    /// The config repaired: every input of the spec present and inside its range (whole where it
    /// must be), unknown keys dropped, every plot styled.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        // A script that is not there (yet, or any more), or that does not work at the moment (a
        // mistake being typed), keeps what was saved for it: the file may come back, and the
        // settings with it.
        if self.kind == StudyKind::Custom
            && !self
                .script
                .as_deref()
                .and_then(custom::library::registry::get)
                .is_some_and(|entry| entry.is_ready())
        {
            if !(self.weight.is_finite() && self.weight > 0.0) {
                self.weight = pane_weight();
            }
            self.weight = self.weight.clamp(0.1, 20.0);
            return self;
        }
        let spec = self.spec();
        let mut inputs = BTreeMap::new();
        for input in spec.inputs {
            let raw = self
                .inputs
                .get(input.key)
                .copied()
                .filter(|v| v.is_finite())
                .unwrap_or(input.default);
            let mut value = raw.clamp(input.min, input.max);
            if !matches!(input.kind, InputKind::Float) {
                value = value.round();
            }
            inputs.insert(input.key.to_owned(), value);
        }
        self.inputs = inputs;
        let mut plots = BTreeMap::new();
        for plot in spec.plots {
            let style = self
                .plots
                .get(plot.key)
                .copied()
                .filter(|s| s.width.is_finite() && s.width > 0.0)
                .unwrap_or(PlotStyle {
                    color: plot.color,
                    width: plot.width,
                    visible: plot.visible,
                    opacity: opaque(),
                    dash: Dash::Solid,
                });
            let style = PlotStyle {
                opacity: if style.opacity.is_finite() {
                    style.opacity.clamp(0.05, 1.0)
                } else {
                    opaque()
                },
                ..style
            };
            plots.insert(plot.key.to_owned(), style);
        }
        self.plots = plots;
        if !(self.weight.is_finite() && self.weight > 0.0) {
            self.weight = pane_weight();
        }
        self.weight = self.weight.clamp(0.1, 20.0);
        self
    }

    /// How tall its pane is, relative to the prices.
    pub fn pane_weight(&self) -> f32 {
        self.weight
    }

    pub fn value_format(&self) -> ValueFormat {
        if self.kind == StudyKind::Atr && self.input("unit") == 1.0 {
            ValueFormat::Percent(self.input("percent_decimals").clamp(0.0, 6.0) as u32)
        } else {
            self.spec().format
        }
    }

    /// An input's value (its default if it is somehow missing).
    pub fn input(&self, key: &str) -> f64 {
        self.inputs.get(key).copied().unwrap_or_else(|| {
            self.kind
                .spec()
                .inputs
                .iter()
                .find(|i| i.key == key)
                .map_or(0.0, |i| i.default)
        })
    }

    fn length(&self, key: &str) -> usize {
        self.input(key).max(1.0) as usize
    }

    pub fn plot_style(&self, key: &str) -> PlotStyle {
        self.plots.get(key).copied().unwrap_or_else(|| {
            let spec = self.spec();
            let plot = spec.plots.iter().find(|p| p.key == key);
            PlotStyle {
                color: plot.map_or(GRAY, |p| p.color),
                width: plot.map_or(1.0, |p| p.width),
                visible: plot.is_none_or(|p| p.visible),
                opacity: opaque(),
                dash: Dash::Solid,
            }
        })
    }

    /// The legend's title: the short name and the main inputs, `SMA 20 close`.
    pub fn title(&self) -> String {
        if self.kind == StudyKind::Atr {
            let mut title = format!("ATR {}", self.length("length"));
            if self.input("smoothing") != 0.0 {
                title.push_str(&format!(
                    " {}",
                    ATR_SMOOTHING[(self.input("smoothing") as usize).min(ATR_SMOOTHING.len() - 1)]
                ));
            }
            if self.input("unit") == 1.0 {
                title.push_str(" %");
            }
            return title;
        }
        let spec = self.spec();
        let mut parts = vec![spec.short.to_owned()];
        for input in spec.inputs {
            let value = self.input(input.key);
            match input.kind {
                InputKind::Int => parts.push(format!("{value:.0}")),
                InputKind::Float => parts.push(trim_float(value)),
                InputKind::Source => {
                    parts.push(SOURCES[(value as usize).min(SOURCES.len() - 1)].to_lowercase());
                }
                InputKind::Choice(_) | InputKind::Toggle | InputKind::Color => {}
            }
            if input.key == "offset" && value == 0.0 {
                parts.pop();
            }
        }
        parts.join(" ")
    }
}

fn trim_float(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// The bars an indicator reads, as columns of numbers. Prices are raw (the server's integers as
/// real numbers), so an overlay shares the price scale of the chart.
#[derive(Debug, Clone, Default)]
pub struct StudyInput {
    pub time: Vec<i64>,
    pub open: Vec<f64>,
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    pub volume: Vec<f64>,
    /// Which day each bar is in (for anchoring VWAP), in the chart's time zone.
    pub day: Vec<i64>,
}

impl StudyInput {
    pub fn len(&self) -> usize {
        self.close.len()
    }

    pub fn is_empty(&self) -> bool {
        self.close.is_empty()
    }

    /// The price a source input names.
    pub fn source(&self, which: f64) -> Vec<f64> {
        let n = self.len();
        match which as usize {
            0 => self.open.clone(),
            1 => self.high.clone(),
            2 => self.low.clone(),
            4 => (0..n).map(|i| (self.high[i] + self.low[i]) / 2.0).collect(),
            5 => (0..n)
                .map(|i| (self.high[i] + self.low[i] + self.close[i]) / 3.0)
                .collect(),
            6 => (0..n)
                .map(|i| (self.open[i] + self.high[i] + self.low[i] + self.close[i]) / 4.0)
                .collect(),
            _ => self.close.clone(),
        }
    }
}

/// One line (or histogram) of an indicator, a value per bar.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotOut {
    pub key: &'static str,
    pub kind: PlotKind,
    pub values: Vec<f64>,
    /// How many bars later than its bar each value is drawn (negative: earlier). The leading
    /// spans of Ichimoku are drawn ahead of the prices.
    pub offset: i64,
    /// For a histogram: whether each column is up (colored `UP`) or down.
    pub up: Option<Vec<bool>>,
}

/// The shaded area between two plots.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillOut {
    pub a: usize,
    pub b: usize,
    pub color: u32,
    pub alpha: f32,
    /// Colored by which plot is on top: `color` when `a` is above, this one otherwise.
    pub other: Option<u32>,
}

/// What an indicator draws.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StudyOutput {
    pub plots: Vec<PlotOut>,
    pub fills: Vec<FillOut>,
    /// Horizontal lines: overbought and oversold, the zero line.
    pub levels: Vec<f64>,
    /// A shaded band between two levels.
    pub band: Option<(f64, f64)>,
}

/// The up and down colors of histograms and profiles.
pub const UP_COLOR: u32 = UP;
pub const DOWN_COLOR: u32 = DOWN;

fn plot(key: &'static str, values: Vec<f64>) -> PlotOut {
    PlotOut {
        key,
        kind: PlotKind::Line,
        values,
        offset: 0,
        up: None,
    }
}

/// Computes an indicator over the bars. The volume profile is not computed here (it depends on
/// what is on screen, see [`profile`]); it gives an empty output.
pub fn compute(config: &StudyConfig, input: &StudyInput) -> StudyOutput {
    let n = input.len();
    let mut out = StudyOutput::default();
    if n == 0 {
        return out;
    }
    let source = || input.source(config.input("source"));
    let offset = config.input("offset") as i64;
    match config.kind {
        StudyKind::Sma | StudyKind::Ema | StudyKind::Wma | StudyKind::Hma => {
            let length = config.length("length");
            let src = source();
            let values = match config.kind {
                StudyKind::Sma => math::sma(&src, length),
                StudyKind::Ema => math::ema(&src, length),
                StudyKind::Wma => math::wma(&src, length),
                _ => math::hma(&src, length),
            };
            out.plots.push(PlotOut {
                offset,
                ..plot("ma", values)
            });
        }
        StudyKind::Vwap => {
            let src = source();
            let mut values = vec![f64::NAN; n];
            let (mut pv, mut vol, mut day) = (0.0, 0.0, None);
            for i in 0..n {
                if day != Some(input.day[i]) {
                    day = Some(input.day[i]);
                    pv = 0.0;
                    vol = 0.0;
                }
                let v = input.volume[i].max(0.0);
                pv += src[i] * v;
                vol += v;
                values[i] = if vol > 0.0 { pv / vol } else { src[i] };
            }
            out.plots.push(plot("vwap", values));
        }
        StudyKind::Bollinger => {
            let (length, mult) = (config.length("length"), config.input("mult"));
            let src = source();
            let basis = math::sma(&src, length);
            let dev = math::stdev(&src, length);
            let upper = basis.iter().zip(&dev).map(|(b, d)| b + mult * d).collect();
            let lower = basis.iter().zip(&dev).map(|(b, d)| b - mult * d).collect();
            out.plots.push(plot("basis", basis));
            out.plots.push(plot("upper", upper));
            out.plots.push(plot("lower", lower));
            out.fills.push(FillOut {
                a: 1,
                b: 2,
                color: config.plot_style("upper").color,
                alpha: 0.08,
                other: None,
            });
        }
        StudyKind::Keltner => {
            let (length, mult) = (config.length("length"), config.input("mult"));
            let basis = math::ema(&input.close, length);
            let range = math::atr(&input.high, &input.low, &input.close, config.length("atr"));
            let upper = basis
                .iter()
                .zip(&range)
                .map(|(b, r)| b + mult * r)
                .collect();
            let lower = basis
                .iter()
                .zip(&range)
                .map(|(b, r)| b - mult * r)
                .collect();
            out.plots.push(plot("basis", basis));
            out.plots.push(plot("upper", upper));
            out.plots.push(plot("lower", lower));
            out.fills.push(FillOut {
                a: 1,
                b: 2,
                color: config.plot_style("upper").color,
                alpha: 0.06,
                other: None,
            });
        }
        StudyKind::Donchian => {
            let length = config.length("length");
            let upper = math::highest(&input.high, length);
            let lower = math::lowest(&input.low, length);
            let basis = upper
                .iter()
                .zip(&lower)
                .map(|(u, l)| (u + l) / 2.0)
                .collect();
            out.plots.push(plot("basis", basis));
            out.plots.push(plot("upper", upper));
            out.plots.push(plot("lower", lower));
            out.fills.push(FillOut {
                a: 1,
                b: 2,
                color: config.plot_style("upper").color,
                alpha: 0.06,
                other: None,
            });
        }
        StudyKind::Ichimoku => {
            let middle = |length: usize| -> Vec<f64> {
                let (h, l) = (
                    math::highest(&input.high, length),
                    math::lowest(&input.low, length),
                );
                h.iter().zip(&l).map(|(h, l)| (h + l) / 2.0).collect()
            };
            let conversion = middle(config.length("conversion"));
            let base = middle(config.length("base"));
            let span_a: Vec<f64> = conversion
                .iter()
                .zip(&base)
                .map(|(c, b)| (c + b) / 2.0)
                .collect();
            let span_b = middle(config.length("span_b"));
            let shift = config.length("displacement") as i64 - 1;
            out.plots.push(plot("conversion", conversion));
            out.plots.push(plot("base", base));
            out.plots.push(PlotOut {
                offset: -shift,
                ..plot("lagging", input.close.clone())
            });
            out.plots.push(PlotOut {
                offset: shift,
                ..plot("span_a", span_a)
            });
            out.plots.push(PlotOut {
                offset: shift,
                ..plot("span_b", span_b)
            });
            out.fills.push(FillOut {
                a: 3,
                b: 4,
                color: 0x43a047,
                alpha: 0.12,
                other: Some(0xf44336),
            });
        }
        StudyKind::ParabolicSar => {
            let values = math::parabolic_sar(
                &input.high,
                &input.low,
                config.input("start"),
                config.input("step"),
                config.input("max"),
            );
            out.plots.push(PlotOut {
                kind: PlotKind::Dots,
                ..plot("sar", values)
            });
        }
        StudyKind::VolumeProfile | StudyKind::Unknown | StudyKind::Custom => {}
        StudyKind::Volume => {
            let up = (0..n).map(|i| input.close[i] >= input.open[i]).collect();
            out.plots.push(PlotOut {
                kind: PlotKind::Histogram,
                up: Some(up),
                ..plot("volume", input.volume.clone())
            });
            out.plots
                .push(plot("ma", math::sma(&input.volume, config.length("ma"))));
        }
        StudyKind::Rsi => {
            out.plots
                .push(plot("rsi", math::rsi(&source(), config.length("length"))));
            let (upper, lower) = (config.input("upper"), config.input("lower"));
            out.levels = vec![upper, 50.0, lower];
            out.band = Some((lower, upper));
        }
        StudyKind::Macd => {
            let (line, signal, hist) = math::macd(
                &source(),
                config.length("fast"),
                config.length("slow"),
                config.length("signal"),
            );
            let up = hist.iter().map(|h| *h >= 0.0).collect();
            out.plots.push(PlotOut {
                kind: PlotKind::Histogram,
                up: Some(up),
                ..plot("histogram", hist)
            });
            out.plots.push(plot("macd", line));
            out.plots.push(plot("signal", signal));
            out.levels = vec![0.0];
        }
        StudyKind::Stochastic => {
            let (k, d) = math::stochastic(
                &input.high,
                &input.low,
                &input.close,
                config.length("length"),
                config.length("smooth"),
                config.length("d"),
            );
            out.plots.push(plot("k", k));
            out.plots.push(plot("d", d));
            let (upper, lower) = (config.input("upper"), config.input("lower"));
            out.levels = vec![upper, lower];
            out.band = Some((lower, upper));
        }
        StudyKind::Atr => {
            let tr = math::true_range(&input.high, &input.low, &input.close);
            let smooth = |values: &[f64], length: usize, method: usize| match method {
                1 => math::sma(values, length),
                2 => math::ema(values, length),
                3 => math::wma(values, length),
                _ => math::rma(values, length),
            };
            let mut atr = smooth(
                &tr,
                config.length("length"),
                config.input("smoothing") as usize,
            );
            let mut tr = tr;
            if config.input("unit") == 1.0 {
                for (i, close) in input.close.iter().enumerate() {
                    let scale = if close.is_finite() && *close != 0.0 {
                        100.0 / close.abs()
                    } else {
                        f64::NAN
                    };
                    atr[i] *= scale;
                    tr[i] *= scale;
                }
            }
            let signal = smooth(
                &atr,
                config.length("signal_length"),
                config.input("signal_smoothing") as usize,
            );
            out.plots.push(plot("atr", atr));
            out.plots.push(plot("signal", signal));
            out.plots.push(plot("tr", tr));
        }
        StudyKind::Cci => {
            let typical = input.source(5.0);
            out.plots
                .push(plot("cci", math::cci(&typical, config.length("length"))));
            let (upper, lower) = (config.input("upper"), config.input("lower"));
            out.levels = vec![upper, 0.0, lower];
            out.band = Some((lower.min(upper), upper.max(lower)));
        }
        StudyKind::WilliamsR => {
            out.plots.push(plot(
                "r",
                math::williams_r(
                    &input.high,
                    &input.low,
                    &input.close,
                    config.length("length"),
                ),
            ));
            let (upper, lower) = (config.input("upper"), config.input("lower"));
            out.levels = vec![upper, lower];
            out.band = Some((lower.min(upper), upper.max(lower)));
        }
        StudyKind::Momentum => {
            out.plots.push(plot(
                "mom",
                math::momentum(&source(), config.length("length")),
            ));
            out.levels = vec![0.0];
        }
        StudyKind::Obv => {
            out.plots
                .push(plot("obv", math::obv(&input.close, &input.volume)));
        }
        StudyKind::Dmi => {
            let (plus, minus, adx) = math::dmi(
                &input.high,
                &input.low,
                &input.close,
                config.length("length"),
                config.length("smoothing"),
            );
            out.plots.push(plot("adx", adx));
            out.plots.push(plot("plus", plus));
            out.plots.push(plot("minus", minus));
            out.levels = vec![25.0];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(n: usize) -> StudyInput {
        let close: Vec<f64> = (0..n)
            .map(|i| 100_000.0 + (i as f64 * 0.7).sin() * 500.0 + i as f64 * 10.0)
            .collect();
        StudyInput {
            time: (0..n as i64).map(|i| i * 60_000).collect(),
            open: close.iter().map(|c| c - 20.0).collect(),
            high: close.iter().map(|c| c + 80.0).collect(),
            low: close.iter().map(|c| c - 90.0).collect(),
            volume: (0..n).map(|i| 1.0 + (i % 5) as f64).collect(),
            day: (0..n as i64).map(|i| i / 100).collect(),
            close,
        }
    }

    #[test]
    fn every_indicator_has_a_unique_label_and_computes_aligned_output() {
        let data = input(300);
        let mut labels = Vec::new();
        for kind in StudyKind::ALL {
            let spec = kind.spec();
            labels.push(spec.label);
            let config = StudyConfig::new(kind);
            let out = compute(&config, &data);
            for plot in &out.plots {
                assert_eq!(plot.values.len(), data.len(), "{kind:?} {}", plot.key);
                assert!(
                    spec.plots.iter().any(|p| p.key == plot.key),
                    "{kind:?}: plot {} has no spec",
                    plot.key
                );
                assert!(
                    plot.values.iter().rev().take(5).all(|v| v.is_finite()),
                    "{kind:?} {}: the newest values are defined",
                    plot.key
                );
            }
            if kind != StudyKind::VolumeProfile {
                assert!(!out.plots.is_empty(), "{kind:?}");
            }
            for fill in &out.fills {
                assert!(fill.a < out.plots.len() && fill.b < out.plots.len());
            }
        }
        let total = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), total);
    }

    #[test]
    fn a_config_is_repaired_to_its_spec() {
        let mut config = StudyConfig::new(StudyKind::Rsi);
        config.inputs.insert("length".into(), 1e9);
        config.inputs.insert("bogus".into(), 3.0);
        config.inputs.insert("upper".into(), f64::NAN);
        config.inputs.insert("lower".into(), 29.6);
        let config = config.normalized();
        assert_eq!(config.input("length"), 1000.0);
        assert_eq!(config.input("upper"), 70.0);
        assert_eq!(config.input("lower"), 29.6, "a float keeps its decimals");
        assert!(!config.inputs.contains_key("bogus"));
        assert_eq!(config.plots.len(), 1);
    }

    #[test]
    fn a_saved_indicator_reads_back_and_a_future_one_is_recognized() {
        let mut config = StudyConfig::new(StudyKind::Macd);
        config.inputs.insert("fast".into(), 8.0);
        config.plots.get_mut("macd").unwrap().color = 0x123456;
        #[derive(Serialize, Deserialize)]
        struct Holder {
            studies: Vec<StudyConfig>,
        }
        let text = toml::to_string(&Holder {
            studies: vec![config.clone()],
        })
        .unwrap();
        let back: Holder = toml::from_str(&text).unwrap();
        assert_eq!(back.studies[0], config);
        let future: Holder =
            toml::from_str("[[studies]]\nkind = \"super_trend\"\n[studies.inputs]\nx = 1.0\n")
                .unwrap();
        assert_eq!(future.studies[0].kind, StudyKind::Unknown);
    }

    #[test]
    fn titles_name_the_main_inputs() {
        assert_eq!(StudyConfig::new(StudyKind::Sma).title(), "SMA 20 close");
        assert_eq!(
            StudyConfig::new(StudyKind::Bollinger).title(),
            "BB 20 2 close"
        );
        assert_eq!(
            StudyConfig::new(StudyKind::Macd).title(),
            "MACD 12 26 9 close"
        );
        assert_eq!(StudyConfig::new(StudyKind::Obv).title(), "OBV");
    }

    #[test]
    fn vwap_restarts_every_day() {
        let data = input(250);
        let out = compute(&StudyConfig::new(StudyKind::Vwap), &data);
        let values = &out.plots[0].values;
        let typical = data.source(5.0);
        // The first bar of a day is its own VWAP.
        assert!((values[100] - typical[100]).abs() < 1e-9);
        assert!((values[200] - typical[200]).abs() < 1e-9);
    }

    #[test]
    fn ichimoku_leads_and_lags() {
        let out = compute(&StudyConfig::new(StudyKind::Ichimoku), &input(200));
        let offsets: Vec<i64> = out.plots.iter().map(|p| p.offset).collect();
        assert_eq!(offsets, vec![0, 0, -25, 25, 25]);
    }

    #[test]
    fn atr_methods_optional_plots_and_percent_use_the_same_bars() {
        let data = StudyInput {
            high: vec![12.0, 13.0, 17.0, 16.0],
            low: vec![8.0, 9.0, 11.0, 10.0],
            close: vec![10.0, 12.0, 12.0, 15.0],
            ..StudyInput::default()
        };
        let mut config = StudyConfig::new(StudyKind::Atr);
        config.inputs.insert("length".into(), 2.0);
        config.inputs.insert("signal_length".into(), 2.0);
        assert!(!config.plot_style("signal").visible);
        assert!(!config.plot_style("tr").visible);
        let out = compute(&config, &data);
        assert_eq!(
            out.plots.iter().map(|p| p.key).collect::<Vec<_>>(),
            ["atr", "signal", "tr"]
        );
        assert_eq!(out.plots[2].values, [4.0, 4.0, 6.0, 6.0]);
        assert!(out.plots[0].values[0].is_nan());
        assert_eq!(&out.plots[0].values[1..], &[4.0, 5.0, 5.5]);
        assert!(out.plots[1].values[1].is_nan());
        assert_eq!(&out.plots[1].values[2..], &[4.5, 5.25]);

        for (method, expected) in [(1.0, 6.0), (2.0, 5.777777777777778), (3.0, 6.0)] {
            config.inputs.insert("smoothing".into(), method);
            let out = compute(&config, &data);
            assert!((out.plots[0].values[3] - expected).abs() < 1e-10);
        }

        config.inputs.insert("smoothing".into(), 0.0);
        config.inputs.insert("unit".into(), 1.0);
        let out = compute(&config, &data);
        assert_eq!(config.value_format(), ValueFormat::Percent(2));
        config.inputs.insert("percent_decimals".into(), 4.0);
        assert_eq!(config.value_format(), ValueFormat::Percent(4));
        assert!((out.plots[0].values[3] - 5.5 / 15.0 * 100.0).abs() < 1e-10);
        assert!((out.plots[2].values[3] - 6.0 / 15.0 * 100.0).abs() < 1e-10);
    }

    #[test]
    fn old_atr_settings_keep_wilder_and_one_visible_plot() {
        let config: StudyConfig = toml::from_str("kind = 'atr'\n[inputs]\nlength = 21\n").unwrap();
        let config = config.normalized();
        assert_eq!(config.input("smoothing"), 0.0);
        assert_eq!(config.input("unit"), 0.0);
        assert_eq!(config.value_format(), ValueFormat::Price);
        assert!(config.plot_style("atr").visible);
        assert!(!config.plot_style("signal").visible);
        assert!(!config.plot_style("tr").visible);
    }

    #[test]
    fn a_plot_saved_before_opacity_and_line_style_loads_opaque_and_solid() {
        let old: PlotStyle = toml::from_str(
            "color = 255
width = 2.0
",
        )
        .unwrap();
        assert_eq!(old.opacity, 1.0);
        assert_eq!(old.dash, Dash::Solid);
        assert!(old.visible);

        let mut config = StudyConfig::new(StudyKind::Sma);
        let key = config.spec().plots[0].key;
        config.plots.insert(
            key.to_owned(),
            PlotStyle {
                opacity: 9.0,
                ..old
            },
        );
        assert_eq!(config.normalized().plot_style(key).opacity, 1.0);
    }
}
