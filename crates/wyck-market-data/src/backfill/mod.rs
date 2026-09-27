//! Rate-limited, resumable backfill of the local catalog from an upstream historical
//! data source: the real cTrader `MarketClient` in the app, or a scriptable fake in
//! tests. See [`scheduler`] for the chunked fetch-and-store logic and [`probe`] for
//! finding how far back a symbol's history actually goes.

pub mod probe;
pub mod scheduler;

pub use scheduler::{BackfillRange, RateLimiter, Upstream};
