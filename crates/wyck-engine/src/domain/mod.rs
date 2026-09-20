//! The engine's normalized trading model.
//!
//! cTrader's two servers describe the same things in incompatible ways: Remote uses
//! integer pipettes, cents and numeric `symbolId`s, Local uses floating point prices, lots
//! and ticker names. Everything in this module is the *common* shape the adapters
//! translate into, so the rest of the engine and every front end deal with one model:
//!
//! - prices and money are `f64` display values (as in `ctrader-mcp`)
//! - volumes are exact integer [`Volume`] units
//! - identifiers are distinct newtypes

mod account;
mod instrument;
mod position;
mod volume;

pub use account::{AccountKind, AccountSnapshot};
pub use instrument::{Instrument, SpecsSource, SymbolInfo, VolumeSpecs};
pub use position::{OrderKind, PendingOrder, Position, Quote, Side};
pub use volume::Volume;

/// Milliseconds since the Unix epoch, UTC.
pub type UnixMillis = i64;

/// The current time as [`UnixMillis`].
#[must_use]
pub fn now_millis() -> UnixMillis {
    ctrader_mcp::time::now_epoch_millis()
}
