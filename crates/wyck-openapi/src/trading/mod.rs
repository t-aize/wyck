//! Placing, amending and cancelling orders, and closing positions, through [`TradingClient`] (see
//! [`crate::AccountClient::trading`]).
//!
//! These calls move money, simulated on a demo account but real on a live one, and they need a
//! token of the `trading` [`crate::auth::Scope`] (an `accounts` token is refused). Everything else
//! in this crate only reads; this module is the one place that writes.
//!
//! # Non-idempotency
//!
//! [`TradingClient::new_order`] and its siblings are not guaranteed idempotent. If a request times
//! out ([`crate::OpenApiError::Timeout`]) or the connection drops while it is in flight
//! ([`crate::OpenApiError::Closed`]), the order may still have reached the server: the answer was
//! lost, not necessarily the request. Sending the same order again on a bare timeout can double a
//! position. Instead, check what the account actually holds first:
//! [`crate::account::AccountDataClient::open_positions_and_orders`] (`ProtoOAReconcileReq`) for
//! what is open now, or [`crate::account::AccountDataClient::deals`] for what was executed
//! recently, using `label` or `client_order_id` to recognize the order you sent. This crate stays a
//! thin typed client, like Spotware's own SDKs (`OpenApiPy`, `OpenAPI.Net`): it does not add retry
//! or idempotency machinery of its own on top of trading calls.
//!
//! # Units
//!
//! Volumes are hundredths of a unit, like the read-only account calls (see
//! [`crate::account::volume_units`]). Prices (limit, stop, stop loss, take profit) are ordinary
//! decimals, like the prices on [`crate::account::Position`] and [`crate::account::Order`], unlike
//! the integer, [`crate::market::PRICE_SCALE`] prices of ticks and bars.

pub use wyck_openapi_model::trading::{events, requests};

pub use events::{ExecutionEvent, ExecutionType, OrderErrorEvent, TrailingSlChangedEvent};
pub use requests::{
    AmendOrderReq, AmendPositionSlTpReq, CancelOrderReq, ClosePositionReq, NewOrderReq,
    NewOrderType,
};

use crate::error::{OpenApiError, Result};
use crate::transport::connection::{Client, RateClass};
use crate::transport::wire::payload;

/// Trading bound to one account: places, amends and cancels orders, closes positions. See
/// [`crate::AccountClient::trading`] and the [module docs](self) for the non-idempotency caveat.
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

    /// Places a new order. `request`'s `ctid_trader_account_id` is overwritten with this account.
    /// See the [module docs](self) for the non-idempotency caveat: on a timeout, check
    /// [`crate::account::AccountDataClient::open_positions_and_orders`] or
    /// [`crate::account::AccountDataClient::deals`] before retrying.
    ///
    /// The answer is a [`crate::Event::Execution`], not a return value of this call: the server may
    /// accept the order (`ORDER_ACCEPTED`) and only fill it (`ORDER_FILLED`, `ORDER_PARTIAL_FILL`)
    /// moments later, both as separate execution events on [`Client::events`]. This call only
    /// confirms the request was sent and matched by `clientMsgId`; it returns the first
    /// [`ExecutionEvent`], which for a market order is usually the fill.
    ///
    /// # Errors
    ///
    /// `TRADING_BAD_VOLUME`, `TRADING_BAD_STOPS`, `TRADING_DISABLED`, `NOT_ENOUGH_MONEY`,
    /// `MAX_EXPOSURE_REACHED`, `SHORT_SELLING_NOT_ALLOWED`, and the usual account errors. A token of
    /// the `accounts` scope gets `ACCOUNT_NOT_AUTHORIZED` or a similar refusal: trading needs the
    /// `trading` [`crate::auth::Scope`].
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
    ///
    /// # Errors
    ///
    /// `ORDER_NOT_FOUND`, `UNABLE_TO_CANCEL_ORDER`, and the usual account errors.
    pub async fn cancel_order(&self, order_id: i64) -> Result<ExecutionEvent> {
        if order_id <= 0 {
            return Err(OpenApiError::Config("order id must be positive".into()));
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

    /// Amends a pending order: only the fields set on `request` change. `request`'s
    /// `ctid_trader_account_id` is overwritten with this account.
    ///
    /// # Errors
    ///
    /// `ORDER_NOT_FOUND`, `UNABLE_TO_AMEND_ORDER`, `TRADING_BAD_STOPS`, `PENDING_EXECUTION` (the
    /// order is already being filled), and the usual account errors.
    pub async fn amend_order(&self, mut request: AmendOrderReq) -> Result<ExecutionEvent> {
        request.ctid_trader_account_id = self.account_id;
        if request.order_id <= 0 {
            return Err(OpenApiError::Config("order id must be positive".into()));
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

    /// Closes a position in full or in part. See the [module docs](self) for the non-idempotency
    /// caveat.
    ///
    /// # Errors
    ///
    /// `POSITION_NOT_FOUND`, `POSITION_NOT_OPEN`, `POSITION_LOCKED`, `TRADING_BAD_VOLUME` for a
    /// close volume larger than the position, and the usual account errors.
    pub async fn close_position(&self, position_id: i64, volume: i64) -> Result<ExecutionEvent> {
        if position_id <= 0 || volume <= 0 {
            return Err(OpenApiError::Config(
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
    /// change. `request`'s `ctid_trader_account_id` is overwritten with this account.
    ///
    /// # Errors
    ///
    /// `PROTECTION_IS_TOO_CLOSE_TO_MARKET`, `TRADING_BAD_STOPS`, `WORSE_GSL_NOT_ALLOWED`, and the
    /// usual account errors.
    pub async fn amend_position_sl_tp(
        &self,
        mut request: AmendPositionSlTpReq,
    ) -> Result<ExecutionEvent> {
        request.ctid_trader_account_id = self.account_id;
        if request.position_id <= 0 {
            return Err(OpenApiError::Config("position id must be positive".into()));
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
        return Err(OpenApiError::Config(
            "trading prices must be finite and positive".into(),
        ));
    }
    Ok(())
}
