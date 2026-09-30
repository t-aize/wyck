//! The account's own data: balance, positions, orders, deals, and the unrealized profit or loss,
//! all read-only, reached through `AccountDataClient` (see
//! `crate::openapi::AccountClient::account_data`).

pub mod book;
pub mod requests;
pub mod types;

pub use requests::{
    CashFlowHistoryListReq, CashFlowHistoryListRes, DealListByPositionIdReq,
    DealListByPositionIdRes, DealListReq, DealListRes, DealOffsetListReq, DealOffsetListRes,
    OrderDetailsReq, OrderDetailsRes, OrderListByPositionIdReq, OrderListByPositionIdRes,
    OrderListReq, OrderListRes, PositionUnrealizedPnLRes, ReconcileReq, ReconcileRes, TraderRes,
    TraderUpdatedEvent,
};
pub use types::{
    AccessRights, AccountType, Deal, DealOffset, DealStatus, DepositWithdraw, Order, OrderStatus,
    OrderTriggerMethod, OrderType, Position, PositionStatus, PositionUnrealizedPnL, TimeInForce,
    TradeData, TradeSide, Trader, money, volume_units,
};

mod client;
pub use client::AccountDataClient;
