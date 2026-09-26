//! Symbol lookup, live prices, the order book, price formatting, and history: everything about
//! market data, reached through [`MarketClient`] (see [`crate::AccountClient::market`]).
//!
//! ```no_run
//! # async fn demo(account: wyck_openapi::AccountClient) -> wyck_openapi::Result<()> {
//! let market = account.market();
//! let symbols = market.symbols().await?;
//! market.subscribe_spots(&[symbols[0].symbol_id]).await?;
//! # Ok(()) }
//! ```
//!
//! | Module | Role |
//! |---|---|
//! | [`client`] | [`MarketClient`] itself: one method per call |
//! | [`symbols`] | [`SymbolTable`], the symbol list and its details, the reference catalogs |
//! | [`quotes`] | [`SpotTracker`], [`Spot`], the live price event |
//! | [`depth`] | [`DepthBook`], the order book event |
//! | [`bars`] | [`Period`], [`Bar`], decoding the low-plus-offsets wire form |
//! | [`ticks`] | [`Tick`], [`QuoteType`], decoding the delta-encoded wire form |
//! | [`price`] | [`PRICE_SCALE`], [`to_price`], [`from_price`], [`format_price`] |
//! | [`live`] | [`LiveBarTracker`]: live bars with their real close (the server's is wrong) |
//! | [`history`] | [`history::fetch_bars`], [`history::fetch_ticks`]: whole ranges, paged |

pub use wyck_openapi_model::market::bars;
pub mod client;
pub use wyck_openapi_model::market::depth;
pub mod history;
pub use wyck_openapi_model::market::{hours, live, price, quotes, symbols, ticks};

pub use bars::{
    Bar, GetTrendbarsReq, GetTrendbarsRes, LiveTrendbarReq, Period, WireTrendbar, decode_bar,
    decode_bars,
};
pub use client::MarketClient;
pub use depth::{DepthBook, DepthEvent, DepthLevel, DepthQuote};
pub use history::{MAX_TICK_RANGE_MS, continuation, fetch_bars, fetch_ticks, tick_windows};
pub use hours::{Holiday, Interval, MarketStatus, TradingHours};
pub use live::{LiveBarTracker, with_true_close};
pub use price::{PRICE_SCALE, UNITS_PER_PRICE, format_price, from_price, to_price};
pub use quotes::{Spot, SpotEvent, SpotTracker, SubscribeSpotsReq};
pub use symbols::{
    Asset, AssetClass, AssetClassListRes, AssetListRes, LightSymbol, Symbol, SymbolByIdReq,
    SymbolByIdRes, SymbolCategory, SymbolCategoryListRes, SymbolChangedEvent, SymbolTable,
    SymbolsForConversionReq, SymbolsForConversionRes, SymbolsListReq, SymbolsListRes,
};
pub use ticks::{
    GetTickDataReq, GetTickDataRes, Quote, QuoteType, Tick, WireTick, decode_ticks, merge_sides,
};
