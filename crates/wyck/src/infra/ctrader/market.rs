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
//! | [`wyck_market::symbols`] | the symbol list and its details, the reference catalogs |
//! | [`wyck_market::quotes`] | [`wyck_market::Spot`], the live price event |
//! | [`wyck_market::depth`] | the order book event |
//! | [`wyck_market::bars`] | [`wyck_market::Period`], [`wyck_market::Bar`], decoding the low-plus-offsets wire form |
//! | [`wyck_market::ticks`] | [`wyck_market::Tick`], [`wyck_market::QuoteType`], decoding the delta-encoded wire form |
//! | [`wyck_market::price`] | [`wyck_market::PRICE_SCALE`], [`wyck_market::to_price`], [`wyck_market::from_price`], [`wyck_market::format_price`] |
//! | [`wyck_market::live`] | [`wyck_market::LiveBarTracker`]: live bars with their real close (the server's is wrong) |
//! | [`history`] | [`history::fetch_bars`], [`history::fetch_ticks`]: whole ranges, paged |

pub mod client;
pub mod history;

pub use client::MarketClient;
pub use history::{MAX_TICK_RANGE_MS, continuation, fetch_bars, fetch_ticks, tick_windows};
