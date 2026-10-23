//! Trading types and arithmetic: accounts, orders, positions, deals, margin, contracts and the
//! account book that follows them.

pub mod amounts;
pub mod atr_stop;
pub mod book;
pub mod contract;
pub mod conversion;
pub mod doubt;
pub mod enums;
pub mod events;
pub mod follow;
pub mod guard;
pub mod margin;
pub mod plan;
pub mod profit;
pub mod risk;
pub mod summary;
pub mod ticket;
pub mod types;

pub use enums::{
    AccessRights, AccountType, DealStatus, NewOrderType, OrderStatus, OrderTriggerMethod,
    OrderType, PositionStatus, TimeInForce, TradeSide,
};
pub use events::{ExecutionEvent, ExecutionType, OrderErrorEvent, TrailingSlChangedEvent};
pub use margin::{
    ExpectedMargin, MarginCall, MarginCallTriggerEvent, MarginCallType, MarginCallUpdateEvent,
    MarginChangedEvent,
};
pub use types::{
    Deal, Order, Position, PositionUnrealizedPnL, TradeData, Trader, money, volume_units,
};
