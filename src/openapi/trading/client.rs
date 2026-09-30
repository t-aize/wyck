//! `TradingClient`: the calls themselves.

use super::*;
use crate::openapi::error::{Error, Result};
use crate::openapi::transport::connection::{Client, RateClass};
use crate::openapi::transport::wire::payload;

/// Trading bound to one account: places, amends and cancels orders, closes positions.
#[derive(Debug, Clone)]
pub struct TradingClient {
    client: Client,
    account_id: i64,
}

impl TradingClient {
    pub(crate) fn new(client: Client, account_id: i64) -> Self {
        Self { client, account_id }
    }

    /// The account id this client is bound to.
    #[must_use]
    pub fn account_id(&self) -> i64 {
        self.account_id
    }

    /// The connection underneath, for calls that are not about trading.
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Places a new order.
    pub async fn new_order(&self, mut request: NewOrderReq) -> Result<ExecutionEvent> {
        request.ctid_trader_account_id = self.account_id;
        request.validate()?;
        self.client
            .call(
                payload::NEW_ORDER_REQ,
                payload::EXECUTION_EVENT,
                &request,
                RateClass::Standard,
                "the new order",
            )
            .await
    }

    /// Cancels a pending order.
    pub async fn cancel_order(&self, order_id: i64) -> Result<ExecutionEvent> {
        if order_id <= 0 {
            return Err(Error::Config("order id must be positive".into()));
        }
        self.client
            .call(
                payload::CANCEL_ORDER_REQ,
                payload::EXECUTION_EVENT,
                &CancelOrderReq {
                    ctid_trader_account_id: self.account_id,
                    order_id,
                },
                RateClass::Standard,
                "the order cancel",
            )
            .await
    }

    /// Amends a pending order: only the fields set on `request` change.
    pub async fn amend_order(&self, mut request: AmendOrderReq) -> Result<ExecutionEvent> {
        request.ctid_trader_account_id = self.account_id;
        if request.order_id <= 0 {
            return Err(Error::Config("order id must be positive".into()));
        }
        check_prices(&[
            request.limit_price,
            request.stop_price,
            request.stop_loss,
            request.take_profit,
        ])?;
        self.client
            .call(
                payload::AMEND_ORDER_REQ,
                payload::EXECUTION_EVENT,
                &request,
                RateClass::Standard,
                "the order amend",
            )
            .await
    }

    /// Closes a position in full or in part.
    pub async fn close_position(&self, position_id: i64, volume: i64) -> Result<ExecutionEvent> {
        if position_id <= 0 || volume <= 0 {
            return Err(Error::Config(
                "position id and close volume must be positive".into(),
            ));
        }
        self.client
            .call(
                payload::CLOSE_POSITION_REQ,
                payload::EXECUTION_EVENT,
                &ClosePositionReq {
                    ctid_trader_account_id: self.account_id,
                    position_id,
                    volume,
                },
                RateClass::Standard,
                "the position close",
            )
            .await
    }

    /// Amends the stop loss and take profit of an open position: only the fields set on `request`
    /// change.
    pub async fn amend_position_sl_tp(
        &self,
        mut request: AmendPositionSlTpReq,
    ) -> Result<ExecutionEvent> {
        request.ctid_trader_account_id = self.account_id;
        if request.position_id <= 0 {
            return Err(Error::Config("position id must be positive".into()));
        }
        check_prices(&[request.stop_loss, request.take_profit])?;
        self.client
            .call(
                payload::AMEND_POSITION_SLTP_REQ,
                payload::EXECUTION_EVENT,
                &request,
                RateClass::Standard,
                "the position stop loss and take profit amend",
            )
            .await
    }
}

fn check_prices(prices: &[Option<f64>]) -> Result<()> {
    if prices
        .iter()
        .flatten()
        .any(|price| !price.is_finite() || *price <= 0.0)
    {
        return Err(Error::Config(
            "trading prices must be finite and positive".into(),
        ));
    }
    Ok(())
}
