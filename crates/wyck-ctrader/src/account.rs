//! The account's own data: balance, positions, orders, deals, and the unrealized profit or loss,
//! all read-only, reached through [`AccountDataClient`] (see [`crate::AccountClient::account_data`]).
//!
//! ```no_run
//! # async fn demo(account: wyck_ctrader::AccountClient) -> wyck_ctrader::Result<()> {
//! let data = account.account_data();
//! let balance = data.trader().await?.balance_amount();
//! # let _ = balance; Ok(()) }
//! ```

mod requests;

pub(crate) use requests::{
    DealListRes, OrderListRes, PositionUnrealizedPnLRes, ReconcileRes, TraderRes,
};

mod client;
pub(crate) use client::AccountDataClient;
