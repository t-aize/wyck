//! Symbols, live prices, order book and price formatting.

pub mod client;
pub mod depth;
pub mod hours;
pub mod price;
pub mod quotes;
pub mod symbols;

pub use client::MarketClient;
pub use depth::{DepthBook, DepthEvent, DepthLevel, DepthQuote};
pub use hours::{Holiday, Interval, MarketStatus, TradingHours};
pub use price::{PRICE_SCALE, UNITS_PER_PRICE, format_price, from_price, pip_size, to_price};
pub use quotes::{Spot, SpotEvent, SpotTracker, SubscribeSpotsReq};
pub use symbols::{
    Asset, AssetClass, AssetClassListRes, AssetListRes, LightSymbol, Symbol, SymbolByIdReq,
    SymbolByIdRes, SymbolCategory, SymbolCategoryListRes, SymbolChangedEvent, SymbolTable,
    SymbolsForConversionReq, SymbolsForConversionRes, SymbolsListReq, SymbolsListRes,
};
