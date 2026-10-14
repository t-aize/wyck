//! Symbol lookup, live prices, the order book, and history: everything about market data, reached
//! through [`MarketClient`] (see [`crate::AccountClient::market`]).
//!
//! ```no_run
//! # async fn demo(account: wyck_ctrader::AccountClient) -> wyck_ctrader::Result<()> {
//! let market = account.market();
//! let symbols = market.symbols().await?;
//! market.subscribe_spots(&[symbols[0].symbol_id]).await?;
//! # Ok(()) }
//! ```
//!
//! | Module | Role |
//! |---|---|
//! | [`client`] | [`MarketClient`] itself: one method per call |
//! | [`requests`] | the request and response messages of those calls |
//! | [`history`] | [`history::fetch_bars`], [`history::fetch_ticks`]: whole ranges, paged |
//!
//! What the calls return (symbols, bars, ticks, price and depth events) is defined by the
//! `wyck_market` crate.

pub mod client;
pub mod history;
pub mod requests;

pub use client::MarketClient;
pub use history::{MAX_TICK_RANGE_MS, continuation, fetch_bars, fetch_ticks, tick_windows};
pub use requests::{
    AssetClassListRes, AssetListRes, GetTickDataReq, GetTickDataRes, GetTrendbarsReq,
    GetTrendbarsRes, LiveTrendbarReq, SubscribeSpotsReq, SymbolByIdReq, SymbolByIdRes,
    SymbolCategoryListRes, SymbolsForConversionReq, SymbolsForConversionRes, SymbolsListReq,
    SymbolsListRes,
};
