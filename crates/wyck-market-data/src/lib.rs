//! Local historical market data catalog for Wyck's Replay and Backtesting features.
//!
//! cTrader's Open API historical endpoints are rate-limited (5 requests/second), cap
//! ticks at a 7-day span and bars at 14,000 per response, and document no fixed
//! retention depth (it is broker- and symbol-dependent). Replaying or backtesting over
//! anything but a short range would otherwise mean re-fetching the same data, slowly and
//! repeatedly, from a rate-limited network endpoint. [`catalog::Catalog`] is a local,
//! persistent cache that both the backfill scheduler and the app's normal chart loading
//! read through, so live charts, Replay and Backtesting all see one consistent history.
//!
//! Storage is deliberately not Arrow/Parquet: this is a single-machine desktop app with
//! strictly sequential, forward-in-time reads, never analytic scans across symbols.
//! [`catalog::manifest`] is a small SQLite index (which `(symbol, series, month)` chunks
//! are complete, and backfill job resume state), and the bars/ticks themselves live in
//! flat, fixed-record binary files, one per `(symbol, series, calendar month)`
//! ([`catalog::chunks`]).

pub mod backfill;
pub mod catalog;
pub mod error;
pub mod replay;

pub use catalog::Catalog;
pub use error::{MarketDataError, Result};
