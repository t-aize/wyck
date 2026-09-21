//! A client bound to one trading account.
//!
//! Almost every call of [`Client`] takes the account id first. [`AccountClient`] holds a client and
//! an account id and drops that argument, which reads better in code that works with one account
//! (the usual case) and rules out passing the wrong id.
//!
//! ```no_run
//! # async fn demo(client: ctrader_openapi::Client, account_id: i64) -> ctrader_openapi::Result<()> {
//! let account = client.account(account_id);
//! let symbols = account.symbols().await?;
//! account.subscribe_spots(&[symbols[0].symbol_id]).await?;
//! let balance = account.trader().await?.balance_amount();
//! # let _ = balance; Ok(()) }
//! ```

use crate::account::{
    Deal, DealOffset, DepositWithdraw, Order, Position, PositionUnrealizedPnL, Trader,
};
use crate::client::Client;
use crate::error::Result;
use crate::history;
use crate::margin::{DynamicLeverage, ExpectedMargin, MarginCall};
use crate::model::{LightSymbol, Symbol};
use crate::trading::{AmendOrderReq, AmendPositionSlTpReq, ExecutionEvent, NewOrderReq};
use crate::types::{Bar, Period, QuoteType, Tick};

impl Client {
    /// The client bound to `account_id`. The account must still be authorized on the connection
    /// ([`Client::authorize_account`]).
    #[must_use]
    pub fn account(&self, account_id: i64) -> AccountClient {
        AccountClient {
            client: self.clone(),
            account_id,
        }
    }
}

/// A [`Client`] and one account id. Cheap to clone. See the [module docs](self).
#[derive(Debug, Clone)]
pub struct AccountClient {
    client: Client,
    account_id: i64,
}

impl AccountClient {
    /// The account id.
    #[must_use]
    pub fn id(&self) -> i64 {
        self.account_id
    }

    /// The connection underneath, for the calls that are not about an account.
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Logs the account in on the connection with `access_token`.
    ///
    /// # Errors
    ///
    /// See [`Client::authorize_account`].
    pub async fn authorize(&self, access_token: &str) -> Result<()> {
        self.client
            .authorize_account(self.account_id, access_token)
            .await
            .map(|_| ())
    }

    /// See [`Client::trader`].
    ///
    /// # Errors
    ///
    /// See [`Client::trader`].
    pub async fn trader(&self) -> Result<Trader> {
        self.client.trader(self.account_id).await
    }

    /// See [`Client::open_positions_and_orders`].
    ///
    /// # Errors
    ///
    /// See [`Client::open_positions_and_orders`].
    pub async fn open_positions_and_orders(
        &self,
        with_protection_orders: bool,
    ) -> Result<(Vec<Position>, Vec<Order>)> {
        self.client
            .open_positions_and_orders(self.account_id, with_protection_orders)
            .await
    }

    /// See [`Client::deals`].
    ///
    /// # Errors
    ///
    /// See [`Client::deals`].
    pub async fn deals(
        &self,
        from_ms: i64,
        to_ms: i64,
        max_rows: Option<i32>,
    ) -> Result<(Vec<Deal>, bool)> {
        self.client
            .deals(self.account_id, from_ms, to_ms, max_rows)
            .await
    }

    /// The symbols of the account (archived ones left out).
    ///
    /// # Errors
    ///
    /// See [`Client::symbols`].
    pub async fn symbols(&self) -> Result<Vec<LightSymbol>> {
        self.client.symbols(self.account_id, false).await
    }

    /// See [`Client::symbol_details`].
    ///
    /// # Errors
    ///
    /// See [`Client::symbol_details`].
    pub async fn symbol_details(&self, symbol_ids: &[i64]) -> Result<Vec<Symbol>> {
        self.client
            .symbol_details(self.account_id, symbol_ids)
            .await
    }

    /// Follows the prices of some symbols, with the server's timestamp on each event.
    ///
    /// # Errors
    ///
    /// See [`Client::subscribe_spots`].
    pub async fn subscribe_spots(&self, symbol_ids: &[i64]) -> Result<()> {
        self.client
            .subscribe_spots(self.account_id, symbol_ids, true)
            .await
    }

    /// See [`Client::unsubscribe_spots`].
    ///
    /// # Errors
    ///
    /// See [`Client::unsubscribe_spots`].
    pub async fn unsubscribe_spots(&self, symbol_ids: &[i64]) -> Result<()> {
        self.client
            .unsubscribe_spots(self.account_id, symbol_ids)
            .await
    }

    /// See [`Client::subscribe_live_bars`].
    ///
    /// # Errors
    ///
    /// See [`Client::subscribe_live_bars`].
    pub async fn subscribe_live_bars(&self, symbol_id: i64, period: Period) -> Result<()> {
        self.client
            .subscribe_live_bars(self.account_id, symbol_id, period)
            .await
    }

    /// See [`Client::subscribe_depth`].
    ///
    /// # Errors
    ///
    /// See [`Client::subscribe_depth`].
    pub async fn subscribe_depth(&self, symbol_ids: &[i64]) -> Result<()> {
        self.client
            .subscribe_depth(self.account_id, symbol_ids)
            .await
    }

    /// Every bar of `period` in `[from_ms, to_ms]`, paged. See [`history::fetch_bars`].
    ///
    /// # Errors
    ///
    /// See [`history::fetch_bars`].
    pub async fn bars(
        &self,
        symbol_id: i64,
        period: Period,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<Bar>> {
        history::fetch_bars(
            &self.client,
            self.account_id,
            symbol_id,
            period,
            from_ms,
            to_ms,
        )
        .await
    }

    /// Every tick of one side in `[from_ms, to_ms]`, paged. See [`history::fetch_ticks`].
    ///
    /// # Errors
    ///
    /// See [`history::fetch_ticks`].
    pub async fn ticks(
        &self,
        symbol_id: i64,
        side: QuoteType,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<Tick>> {
        history::fetch_ticks(
            &self.client,
            self.account_id,
            symbol_id,
            side,
            from_ms,
            to_ms,
        )
        .await
    }

    /// The unrealized profit or loss of every open position.
    ///
    /// # Errors
    ///
    /// See [`Client::position_unrealized_pnl`].
    pub async fn position_unrealized_pnl(&self) -> Result<Vec<PositionUnrealizedPnL>> {
        self.client.position_unrealized_pnl(self.account_id).await
    }

    /// The deposits and withdrawals of the account in `[from_ms, to_ms]`.
    ///
    /// # Errors
    ///
    /// See [`Client::cash_flow_history`].
    pub async fn cash_flow_history(
        &self,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<DepositWithdraw>> {
        self.client
            .cash_flow_history(self.account_id, from_ms, to_ms)
            .await
    }

    /// The deals that offset a deal, and the deals it offsets in turn.
    ///
    /// # Errors
    ///
    /// See [`Client::deal_offsets`].
    pub async fn deal_offsets(&self, deal_id: i64) -> Result<(Vec<DealOffset>, Vec<DealOffset>)> {
        self.client.deal_offsets(self.account_id, deal_id).await
    }

    // ---- trading: places, amends and cancels orders, and closes positions ----
    //
    // See the crate::trading module docs for the non-idempotency caveat before retrying a timed
    // out call.

    /// Places a new order. `request`'s `ctid_trader_account_id` is overwritten with this account.
    ///
    /// # Errors
    ///
    /// See [`Client::new_order`].
    pub async fn new_order(&self, mut request: NewOrderReq) -> Result<ExecutionEvent> {
        request.ctid_trader_account_id = self.account_id;
        self.client.new_order(&request).await
    }

    /// Cancels a pending order.
    ///
    /// # Errors
    ///
    /// See [`Client::cancel_order`].
    pub async fn cancel_order(&self, order_id: i64) -> Result<ExecutionEvent> {
        self.client.cancel_order(self.account_id, order_id).await
    }

    /// Amends a pending order. `request`'s `ctid_trader_account_id` is overwritten with this
    /// account.
    ///
    /// # Errors
    ///
    /// See [`Client::amend_order`].
    pub async fn amend_order(&self, mut request: AmendOrderReq) -> Result<ExecutionEvent> {
        request.ctid_trader_account_id = self.account_id;
        self.client.amend_order(&request).await
    }

    /// Closes a position in full or in part.
    ///
    /// # Errors
    ///
    /// See [`Client::close_position`].
    pub async fn close_position(&self, position_id: i64, volume: i64) -> Result<ExecutionEvent> {
        self.client
            .close_position(self.account_id, position_id, volume)
            .await
    }

    /// Amends the stop loss and take profit of an open position. `request`'s
    /// `ctid_trader_account_id` is overwritten with this account.
    ///
    /// # Errors
    ///
    /// See [`Client::amend_position_sl_tp`].
    pub async fn amend_position_sl_tp(
        &self,
        mut request: AmendPositionSlTpReq,
    ) -> Result<ExecutionEvent> {
        request.ctid_trader_account_id = self.account_id;
        self.client.amend_position_sl_tp(&request).await
    }

    // ---- margin ----

    /// The margin a buy and a sell of each of `volumes` would use on `symbol_id`.
    ///
    /// # Errors
    ///
    /// See [`Client::expected_margin`].
    pub async fn expected_margin(
        &self,
        symbol_id: i64,
        volumes: &[i64],
    ) -> Result<Vec<ExpectedMargin>> {
        self.client
            .expected_margin(self.account_id, symbol_id, volumes)
            .await
    }

    /// The account's three margin call thresholds.
    ///
    /// # Errors
    ///
    /// See [`Client::margin_calls`].
    pub async fn margin_calls(&self) -> Result<Vec<MarginCall>> {
        self.client.margin_calls(self.account_id).await
    }

    /// The dynamic leverage schedule `leverage_id`.
    ///
    /// # Errors
    ///
    /// See [`Client::dynamic_leverage`].
    pub async fn dynamic_leverage(&self, leverage_id: i64) -> Result<DynamicLeverage> {
        self.client
            .dynamic_leverage(self.account_id, leverage_id)
            .await
    }
}
