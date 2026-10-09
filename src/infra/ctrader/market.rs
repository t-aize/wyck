//! Symbol lookup, live prices, the order book, price formatting, and history: everything about
//! market data, reached through [`MarketClient`] (see [`crate::infra::ctrader::AccountClient::market`]).
//!
//! ```no_run
//! # async fn demo(account: wyck::infra::ctrader::AccountClient) -> wyck::infra::ctrader::Result<()> {
//! let market = account.market();
//! let symbols = market.symbols().await?;
//! market.subscribe_spots(&[symbols[0].symbol_id]).await?;
//! # Ok(()) }
//! ```
//!
//! | Module | Role |
//! |---|---|
//! | [`client`] | [`MarketClient`] itself: one method per call |
//! | [`crate::domain::market::symbols`] | the symbol list and its details, the reference catalogs |
//! | [`crate::domain::market::quotes`] | [`crate::domain::market::Spot`], the live price event |
//! | [`crate::domain::market::depth`] | the order book event |
//! | [`crate::domain::market::bars`] | [`crate::domain::market::Period`], [`crate::domain::market::Bar`], decoding the low-plus-offsets wire form |
//! | [`crate::domain::market::ticks`] | [`crate::domain::market::Tick`], [`crate::domain::market::QuoteType`], decoding the delta-encoded wire form |
//! | [`crate::domain::market::price`] | [`crate::domain::market::PRICE_SCALE`], [`crate::domain::market::to_price`], [`crate::domain::market::from_price`], [`crate::domain::market::format_price`] |
//! | [`crate::domain::market::live`] | [`crate::domain::market::LiveBarTracker`]: live bars with their real close (the server's is wrong) |
//! | [`history`] | [`history::fetch_bars`], [`history::fetch_ticks`]: whole ranges, paged |

pub mod client;
pub mod history;

pub use client::MarketClient;
pub use history::{MAX_TICK_RANGE_MS, continuation, fetch_bars, fetch_ticks, tick_windows};
