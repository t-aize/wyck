//! [`AccountDataClient`]: the calls themselves.

use super::requests::{
    DealListReq as DealsReq, OrderListReq as OrdersReq, ReconcileReq as PortfolioReq,
};
use super::{DealListRes, OrderListRes, PositionUnrealizedPnLRes, ReconcileRes, TraderRes};
use wyck_domain::trading::{Deal, Order, Position, Trader};

use crate::connection::{Client, RateClass};
use crate::error::Result;
use crate::transport::messages::AccountReq;
use crate::transport::wire::payload;

/// An account's own data, read-only: balance, positions, orders, deals. See
/// [`crate::AccountClient::account_data`].
#[derive(Debug, Clone)]
pub struct AccountDataClient {
    client: Client,
    account_id: i64,
}

impl AccountDataClient {
    pub(crate) fn new(client: Client, account_id: i64) -> Self {
        Self { client, account_id }
    }

    /// The account itself: balance, leverage, access rights, broker.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn trader(&self) -> Result<Trader> {
        let response: TraderRes = self
            .client
            .call(
                payload::TRADER_REQ,
                payload::TRADER_RES,
                &AccountReq {
                    ctid_trader_account_id: self.account_id,
                },
                RateClass::Standard,
                "the account details",
            )
            .await?;
        Ok(response.trader)
    }

    /// What the account holds right now: the open positions and the working orders. With
    /// `with_protection_orders` the protective orders (stop loss, take profit) of the positions are
    /// listed too.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn open_positions_and_orders(
        &self,
        with_protection_orders: bool,
    ) -> Result<(Vec<Position>, Vec<Order>)> {
        let response: ReconcileRes = self
            .client
            .call(
                payload::RECONCILE_REQ,
                payload::RECONCILE_RES,
                &PortfolioReq {
                    ctid_trader_account_id: self.account_id,
                    return_protection_orders: with_protection_orders.then_some(true),
                },
                RateClass::Standard,
                "the open positions and orders",
            )
            .await?;
        Ok((response.position, response.order))
    }

    /// The deals (executions) of the account in `[from_ms, to_ms]`, at most `max_rows` of them, and
    /// whether more exist in the range.
    ///
    /// # Errors
    ///
    /// `INCORRECT_BOUNDARIES` for a range the server refuses, and the usual account errors.
    pub async fn deals(
        &self,
        from_ms: i64,
        to_ms: i64,
        max_rows: Option<i32>,
    ) -> Result<(Vec<Deal>, bool)> {
        let response: DealListRes = self
            .client
            .call(
                payload::DEAL_LIST_REQ,
                payload::DEAL_LIST_RES,
                &DealsReq {
                    ctid_trader_account_id: self.account_id,
                    from_timestamp: Some(from_ms),
                    to_timestamp: Some(to_ms),
                    max_rows,
                },
                RateClass::Historical,
                "the deal list",
            )
            .await?;
        Ok((response.deal, response.has_more))
    }

    /// The orders of the account in `[from_ms, to_ms]`, and whether more exist in the range.
    ///
    /// # Errors
    ///
    /// `INCORRECT_BOUNDARIES` for a range the server refuses, and the usual account errors.
    pub async fn orders(&self, from_ms: i64, to_ms: i64) -> Result<(Vec<Order>, bool)> {
        let response: OrderListRes = self
            .client
            .call(
                payload::ORDER_LIST_REQ,
                payload::ORDER_LIST_RES,
                &OrdersReq {
                    ctid_trader_account_id: self.account_id,
                    from_timestamp: Some(from_ms),
                    to_timestamp: Some(to_ms),
                },
                RateClass::Historical,
                "the order list",
            )
            .await?;
        Ok((response.order, response.has_more))
    }

    /// The whole answer about the unrealized profit or loss of the open positions, with the
    /// number of decimals of its amounts (see [`wyck_domain::trading::money`]).
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn unrealized_pnl(&self) -> Result<PositionUnrealizedPnLRes> {
        self.client
            .call(
                payload::GET_POSITION_UNREALIZED_PNL_REQ,
                payload::GET_POSITION_UNREALIZED_PNL_RES,
                &AccountReq {
                    ctid_trader_account_id: self.account_id,
                },
                RateClass::Standard,
                "the unrealized profit and loss of every position",
            )
            .await
    }
}
