//! The account calls of [`Client`]: balance, positions, orders, deals, catalogs, profile.
//!
//! All of them only read. They need the account to be authorized on the connection
//! ([`Client::authorize_account`]) except [`Client::ctid_profile`], which takes an access token.
//! Lists of deals and orders count against the historical rate limit, the conservative choice for
//! calls that read history; the rest count against the general one.

use crate::account::{
    AccountReq, Asset, AssetClass, AssetClassListRes, AssetListRes, CashFlowHistoryListReq,
    CashFlowHistoryListRes, CtidProfile, CtidProfileReq, CtidProfileRes, Deal,
    DealListByPositionIdReq, DealListByPositionIdRes, DealListReq, DealListRes, DealOffset,
    DealOffsetListReq, DealOffsetListRes, DepositWithdraw, Order, OrderDetailsReq, OrderDetailsRes,
    OrderListByPositionIdReq, OrderListByPositionIdRes, OrderListReq, OrderListRes, Position,
    PositionUnrealizedPnL, PositionUnrealizedPnLRes, ReconcileReq, ReconcileRes, SymbolCategory,
    SymbolCategoryListRes, Trader, TraderRes,
};
use crate::client::{Client, RateClass};
use crate::error::Result;
use crate::wire::payload;

impl Client {
    /// The account itself: balance, leverage, access rights, broker.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn trader(&self, account_id: i64) -> Result<Trader> {
        let response: TraderRes = self
            .call(
                payload::TRADER_REQ,
                payload::TRADER_RES,
                &AccountReq {
                    ctid_trader_account_id: account_id,
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
        account_id: i64,
        with_protection_orders: bool,
    ) -> Result<(Vec<Position>, Vec<Order>)> {
        let response: ReconcileRes = self
            .call(
                payload::RECONCILE_REQ,
                payload::RECONCILE_RES,
                &ReconcileReq {
                    ctid_trader_account_id: account_id,
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
        account_id: i64,
        from_ms: i64,
        to_ms: i64,
        max_rows: Option<i32>,
    ) -> Result<(Vec<Deal>, bool)> {
        let response: DealListRes = self
            .call(
                payload::DEAL_LIST_REQ,
                payload::DEAL_LIST_RES,
                &DealListReq {
                    ctid_trader_account_id: account_id,
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
    pub async fn orders(
        &self,
        account_id: i64,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<(Vec<Order>, bool)> {
        let response: OrderListRes = self
            .call(
                payload::ORDER_LIST_REQ,
                payload::ORDER_LIST_RES,
                &OrderListReq {
                    ctid_trader_account_id: account_id,
                    from_timestamp: Some(from_ms),
                    to_timestamp: Some(to_ms),
                },
                RateClass::Historical,
                "the order list",
            )
            .await?;
        Ok((response.order, response.has_more))
    }

    /// The assets (currencies and other units) of the broker.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn assets(&self, account_id: i64) -> Result<Vec<Asset>> {
        let response: AssetListRes = self
            .call(
                payload::ASSET_LIST_REQ,
                payload::ASSET_LIST_RES,
                &AccountReq {
                    ctid_trader_account_id: account_id,
                },
                RateClass::Standard,
                "the asset list",
            )
            .await?;
        Ok(response.asset)
    }

    /// The asset classes (forex, indices, metals, ...).
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn asset_classes(&self, account_id: i64) -> Result<Vec<AssetClass>> {
        let response: AssetClassListRes = self
            .call(
                payload::ASSET_CLASS_LIST_REQ,
                payload::ASSET_CLASS_LIST_RES,
                &AccountReq {
                    ctid_trader_account_id: account_id,
                },
                RateClass::Standard,
                "the asset class list",
            )
            .await?;
        Ok(response.asset_class)
    }

    /// The symbol categories (major pairs, cryptos, ...), each pointing to an asset class.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn symbol_categories(&self, account_id: i64) -> Result<Vec<SymbolCategory>> {
        let response: SymbolCategoryListRes = self
            .call(
                payload::SYMBOL_CATEGORY_REQ,
                payload::SYMBOL_CATEGORY_RES,
                &AccountReq {
                    ctid_trader_account_id: account_id,
                },
                RateClass::Standard,
                "the symbol category list",
            )
            .await?;
        Ok(response.symbol_category)
    }

    /// The profile of the cTrader ID an access token belongs to.
    ///
    /// # Errors
    ///
    /// A token error for a bad or expired token.
    pub async fn ctid_profile(&self, access_token: &str) -> Result<CtidProfile> {
        let response: CtidProfileRes = self
            .call(
                payload::GET_CTID_PROFILE_BY_TOKEN_REQ,
                payload::GET_CTID_PROFILE_BY_TOKEN_RES,
                &CtidProfileReq {
                    access_token: access_token.to_owned(),
                },
                RateClass::Standard,
                "the cTrader ID profile",
            )
            .await?;
        Ok(response.profile)
    }

    /// The deposits and withdrawals of the account in `[from_ms, to_ms]`, at most one week.
    ///
    /// # Errors
    ///
    /// `INCORRECT_BOUNDARIES` for a range over one week, and the usual account errors.
    pub async fn cash_flow_history(
        &self,
        account_id: i64,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<DepositWithdraw>> {
        let response: CashFlowHistoryListRes = self
            .call(
                payload::CASH_FLOW_HISTORY_LIST_REQ,
                payload::CASH_FLOW_HISTORY_LIST_RES,
                &CashFlowHistoryListReq {
                    ctid_trader_account_id: account_id,
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
        account_id: i64,
        position_id: i64,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
    ) -> Result<(Vec<Deal>, bool)> {
        let response: DealListByPositionIdRes = self
            .call(
                payload::DEAL_LIST_BY_POSITION_ID_REQ,
                payload::DEAL_LIST_BY_POSITION_ID_RES,
                &DealListByPositionIdReq {
                    ctid_trader_account_id: account_id,
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
        account_id: i64,
        position_id: i64,
        from_ms: Option<i64>,
        to_ms: Option<i64>,
    ) -> Result<(Vec<Order>, bool)> {
        let response: OrderListByPositionIdRes = self
            .call(
                payload::ORDER_LIST_BY_POSITION_ID_REQ,
                payload::ORDER_LIST_BY_POSITION_ID_RES,
                &OrderListByPositionIdReq {
                    ctid_trader_account_id: account_id,
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
    pub async fn order_details(
        &self,
        account_id: i64,
        order_id: i64,
    ) -> Result<(Order, Vec<Deal>)> {
        let response: OrderDetailsRes = self
            .call(
                payload::ORDER_DETAILS_REQ,
                payload::ORDER_DETAILS_RES,
                &OrderDetailsReq {
                    ctid_trader_account_id: account_id,
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
    pub async fn deal_offsets(
        &self,
        account_id: i64,
        deal_id: i64,
    ) -> Result<(Vec<DealOffset>, Vec<DealOffset>)> {
        let response: DealOffsetListRes = self
            .call(
                payload::DEAL_OFFSET_LIST_REQ,
                payload::DEAL_OFFSET_LIST_RES,
                &DealOffsetListReq {
                    ctid_trader_account_id: account_id,
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
    pub async fn position_unrealized_pnl(
        &self,
        account_id: i64,
    ) -> Result<Vec<PositionUnrealizedPnL>> {
        let response: PositionUnrealizedPnLRes = self
            .call(
                payload::GET_POSITION_UNREALIZED_PNL_REQ,
                payload::GET_POSITION_UNREALIZED_PNL_RES,
                &AccountReq {
                    ctid_trader_account_id: account_id,
                },
                RateClass::Standard,
                "the unrealized profit and loss of every position",
            )
            .await?;
        Ok(response.position_unrealized_pnl)
    }

    /// Logs an account out of this connection. Its subscriptions end; authorize it again to use it
    /// again.
    ///
    /// # Errors
    ///
    /// `ACCOUNT_NOT_AUTHORIZED` when the account was not authorized on this connection.
    pub async fn logout_account(&self, account_id: i64) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                payload::ACCOUNT_LOGOUT_REQ,
                payload::ACCOUNT_LOGOUT_RES,
                &AccountReq {
                    ctid_trader_account_id: account_id,
                },
                RateClass::Standard,
                "the account log out",
            )
            .await?;
        Ok(())
    }
}
