//! The account's own data: balance, positions, orders, deals, and the unrealized profit or loss,
//! all read-only, reached through [`AccountDataClient`] (see [`crate::AccountClient::account_data`]).
//!
//! ```no_run
//! # async fn demo(account: wyck_openapi::AccountClient) -> wyck_openapi::Result<()> {
//! let data = account.account_data();
//! let balance = data.trader().await?.balance_amount();
//! # let _ = balance; Ok(()) }
//! ```

pub use wyck_openapi_model::account::{requests, types};

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

use requests::{
    CashFlowHistoryListReq as CashFlowReq, DealListByPositionIdReq as DealsByPositionReq,
    DealListReq as DealsReq, DealOffsetListReq as OffsetsReq, OrderDetailsReq as DetailsReq,
    OrderListByPositionIdReq as OrdersByPositionReq, OrderListReq as OrdersReq,
    ReconcileReq as PortfolioReq,
};

use crate::error::Result;
use crate::transport::connection::{Client, RateClass};
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

    /// The account id this client is bound to.
    #[must_use]
    pub fn account_id(&self) -> i64 {
        self.account_id
    }

    /// The connection underneath, for calls that are not about account data.
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
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

    /// The deposits and withdrawals of the account in `[from_ms, to_ms]`, at most one week.
    ///
    /// # Errors
    ///
    /// `INCORRECT_BOUNDARIES` for a range over one week, and the usual account errors.
    pub async fn cash_flow_history(
        &self,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<DepositWithdraw>> {
        let response: CashFlowHistoryListRes = self
            .client
            .call(
                payload::CASH_FLOW_HISTORY_LIST_REQ,
                payload::CASH_FLOW_HISTORY_LIST_RES,
                &CashFlowReq {
                    ctid_trader_account_id: self.account_id,
                    from_timestamp: from_ms,
                    to_timestamp: to_ms,
                },
                RateClass::Historical,
                "the cash flow history",
            )
            .await?;
        Ok(response.deposit_withdraw)
    }

    /// The deals of one position in `[from_ms, to_ms]`, and whether more exist in the range.
    ///
    /// # Errors
    ///
    /// `POSITION_NOT_FOUND`, and the usual account errors.
    pub async fn deals_by_position(
        &self,
        position_id: i64,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
    ) -> Result<(Vec<Deal>, bool)> {
        let response: DealListByPositionIdRes = self
            .client
            .call(
                payload::DEAL_LIST_BY_POSITION_ID_REQ,
                payload::DEAL_LIST_BY_POSITION_ID_RES,
                &DealsByPositionReq {
                    ctid_trader_account_id: self.account_id,
                    position_id,
                    from_timestamp: from_ms,
                    to_timestamp: to_ms,
                },
                RateClass::Historical,
                "the deal list of a position",
            )
            .await?;
        Ok((response.deal, response.has_more))
    }

    /// The orders of one position in `[from_ms, to_ms]`, newest first, and whether more exist.
    ///
    /// # Errors
    ///
    /// `POSITION_NOT_FOUND`, and the usual account errors.
    pub async fn orders_by_position(
        &self,
        position_id: i64,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
    ) -> Result<(Vec<Order>, bool)> {
        let response: OrderListByPositionIdRes = self
            .client
            .call(
                payload::ORDER_LIST_BY_POSITION_ID_REQ,
                payload::ORDER_LIST_BY_POSITION_ID_RES,
                &OrdersByPositionReq {
                    ctid_trader_account_id: self.account_id,
                    position_id,
                    from_timestamp: from_ms,
                    to_timestamp: to_ms,
                },
                RateClass::Historical,
                "the order list of a position",
            )
            .await?;
        Ok((response.order, response.has_more))
    }

    /// One order and every deal that filled it.
    ///
    /// # Errors
    ///
    /// `ORDER_NOT_FOUND`, and the usual account errors.
    pub async fn order_details(&self, order_id: i64) -> Result<(Order, Vec<Deal>)> {
        let response: OrderDetailsRes = self
            .client
            .call(
                payload::ORDER_DETAILS_REQ,
                payload::ORDER_DETAILS_RES,
                &DetailsReq {
                    ctid_trader_account_id: self.account_id,
                    order_id,
                },
                RateClass::Historical,
                "the order details",
            )
            .await?;
        Ok((response.order, response.deal))
    }

    /// The deals that offset a deal, and the deals it offsets in turn.
    ///
    /// # Errors
    ///
    /// The usual account errors.
    pub async fn deal_offsets(&self, deal_id: i64) -> Result<(Vec<DealOffset>, Vec<DealOffset>)> {
        let response: DealOffsetListRes = self
            .client
            .call(
                payload::DEAL_OFFSET_LIST_REQ,
                payload::DEAL_OFFSET_LIST_RES,
                &OffsetsReq {
                    ctid_trader_account_id: self.account_id,
                    deal_id,
                },
                RateClass::Historical,
                "the deal offset list",
            )
            .await?;
        Ok((response.offset_by, response.offsetting))
    }

    /// The unrealized profit or loss of every open position of the account.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn position_unrealized_pnl(&self) -> Result<Vec<PositionUnrealizedPnL>> {
        Ok(self.unrealized_pnl().await?.position_unrealized_pnl)
    }

    /// The whole answer about the unrealized profit or loss of the open positions, with the
    /// number of decimals of its amounts (see [`money`]).
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
