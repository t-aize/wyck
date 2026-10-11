//! Indicators ("studies"): moving averages, bands, oscillators and the volume profile.
//!
//! - [`math`]: the arithmetic, series in and series out, tested against worked examples.
//! - `kind` and `spec`: what each indicator is ([`StudyKind`] and its [`Spec`]: inputs, plots,
//!   where it is drawn).
//! - `config` and `style`: what the user set ([`StudyConfig`], which is also what is saved) and how
//!   the plots, levels and fills look.
//! - `compute`: [`compute`], which turns bars into the lines to plot ([`StudyOutput`]).
//! - [`profile`]: the volume profile of the visible bars, which is not a line per bar.
//!
//! A config names its inputs and plots by stable keys (`length`, `upper`, `signal`), never by
//! position, and every one of them has a default. A file written by another version still loads:
//! an unknown indicator is dropped, a missing input takes its default, an unknown one is ignored.

pub mod atr_stop;
pub mod catalog;
pub mod custom;
mod extended;
pub mod higher;
pub mod intern;
pub mod math;
pub mod profile;

mod compute;
mod config;
mod kind;
mod spec;
mod style;

pub use compute::{
    DOWN_COLOR, FillOut, MA_METHODS, PlotOut, ScriptDrawing, StudyInput, StudyOutput, UP_COLOR,
    compute, moving_average,
};
pub use config::StudyConfig;
pub use kind::StudyKind;
pub use spec::{InputKind, InputSpec, Placement, PlotKind, PlotSpec, SOURCES, Spec, ValueFormat};
pub use style::{FillStyle, LevelStyle, PlotStyle};

// What the indicators of this folder share and keep to themselves.
use compute::plot;
use spec::{
    ATR_SMOOTHING, BLUE, DOWN, GRAY, OFFSET, ORANGE, PINK, PURPLE, SOURCE, TEAL, UP, choice, float,
    hidden_line, int, line,
};

#[cfg(test)]
#[path = "../../tests/unit/domain/indicators.rs"]
mod tests;
