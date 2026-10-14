//! The account's own data: balance, positions, orders, deals, and the unrealized profit or loss,
//! all read-only, reached through [`AccountDataClient`] (see [`crate::infra::ctrader::AccountClient::account_data`]).
//!
//! ```no_run
//! # async fn demo(account: wyck::infra::ctrader::AccountClient) -> wyck::infra::ctrader::Result<()> {
//! let data = account.account_data();
//! let balance = data.trader().await?.balance_amount();
//! # let _ = balance; Ok(()) }
//! ```

pub mod requests;

pub use requests::{
    CashFlowHistoryListReq, CashFlowHistoryListRes, DealListByPositionIdReq,
    DealListByPositionIdRes, DealListReq, DealListRes, DealOffsetListReq, DealOffsetListRes,
    OrderDetailsReq, OrderDetailsRes, OrderListByPositionIdReq, OrderListByPositionIdRes,
    OrderListReq, OrderListRes, PositionUnrealizedPnLRes, ReconcileReq, ReconcileRes, TraderRes,
    TraderUpdatedEvent,
};

mod client;
pub use client::AccountDataClient;
