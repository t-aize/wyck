//! Trading types and arithmetic: accounts, orders, positions, deals, margin, contracts and the
//! account book that follows them.

pub mod book;
pub mod contract;
pub mod events;
pub mod guard;
pub mod margin;
pub mod math;
pub mod plan;
pub mod types;

pub use events::{ExecutionEvent, ExecutionType, OrderErrorEvent, TrailingSlChangedEvent};
pub use margin::{
    DynamicLeverage, DynamicLeverageTier, ExpectedMargin, MarginCall, MarginCallTriggerEvent,
    MarginCallType, MarginCallUpdateEvent, MarginChangedEvent,
};
pub use types::{
    AccessRights, AccountType, Deal, DealOffset, DealStatus, DepositWithdraw, Order, OrderStatus,
    OrderTriggerMethod, OrderType, Position, PositionStatus, PositionUnrealizedPnL, TimeInForce,
    TradeData, TradeSide, Trader, money, volume_units,
};
