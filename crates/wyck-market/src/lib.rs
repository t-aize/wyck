//! Market data: symbols, bars, ticks, quotes, trading hours and timeframes.
//!
//! Pure code, no I/O. What the cTrader Open API sends (symbols, bars, ticks, price and depth
//! events) is read here as it arrives, so these types are also the wire format; the requests that
//! ask for them belong to the crate that talks to cTrader.
//!
//! Prices are the server's integers scaled by [`PRICE_SCALE`]: keep the integer for exact work and
//! convert with [`to_price`] only to display or to do arithmetic that tolerates rounding.

mod bars;
mod depth;
mod hours;
mod live;
mod price;
mod quotes;
mod symbols;
mod ticks;
mod timeframe;

pub use bars::{Bar, Period, WireTrendbar, decode_bars};
pub use depth::{DepthEvent, DepthQuote};
pub use hours::{Holiday, Interval, MarketStatus, TradingHours};
pub use live::LiveBarTracker;
pub use price::{PRICE_SCALE, format_price, from_price, pip_size, to_price};
pub use quotes::{Spot, SpotEvent};
pub use symbols::{Asset, AssetClass, LightSymbol, Symbol, SymbolCategory, SymbolChangedEvent};
pub use ticks::{Quote, QuoteType, Tick, WireTick, decode_ticks, merge_sides};
pub use timeframe::{GROUPS, QUICK, Timeframe, Unit};
