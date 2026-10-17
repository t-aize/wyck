//! What the chart types that look inside a bar are made of: volume candles and bars, the market
//! profile (TPO), the footprint, and the order flow the last two read.
//!
//! Plain arithmetic on bars, ticks and quotes; the drawing of each lives in `scene`.
//!
//! - `flow`: `Flow`, the order flow inferred from quotes (an estimate, not traded size);
//! - `footprint`: the settings of the footprint and the analysis of one bar;
//! - `tpo`: market profile sessions and their levels;
//! - `volume`: volume candles (width by volume) and volume bars.

pub(crate) mod flow;
pub(crate) mod footprint;
pub(crate) mod tpo;
pub(crate) mod volume;
