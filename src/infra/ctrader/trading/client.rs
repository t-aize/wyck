//! [`TradingClient`]: the calls themselves.

use super::{AmendOrderReq, AmendPositionSlTpReq, CancelOrderReq, ClosePositionReq, NewOrderReq};
use crate::domain::trading::ExecutionEvent;
use crate::infra::ctrader::error::{Error, Result};
use crate::infra::ctrader::transport::connection::{Client, RateClass};
use crate::infra::ctrader::transport::wire::payload;

/// Trading bound to one account: places, amends and cancels orders, closes positions. See
/// [`crate::infra::ctrader::AccountClient::trading`] and the [module docs](crate::infra::ctrader::trading) for the non-idempotency caveat.
///
/// ```no_run
/// # async fn demo(account: wyck::infra::ctrader::AccountClient) -> wyck::infra::ctrader::Result<()> {
/// use wyck::domain::trading::TradeSide;
/// use wyck::infra::ctrader::trading::NewOrderReq;
///
/// // 0.01 lot of symbol 1 at market, with a stop loss and a take profit.
/// let order = NewOrderReq::market(1, TradeSide::Buy, 100_000)
///     .with_protection(Some(1.0750), Some(1.0950));
/// let execution = account.trading().new_order(order).await?;
/// println!("{:?}", execution.kind());
/// # Ok(()) }
/// ```
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
    /// See the [module docs](crate::infra::ctrader::trading) for the non-idempotency caveat: on a timeout, check
    /// [`crate::infra::ctrader::account::AccountDataClient::open_positions_and_orders`] or
    /// [`crate::infra::ctrader::account::AccountDataClient::deals`] before retrying.
    ///
    /// The answer is a [`crate::infra::ctrader::Event::Execution`], not a return value of this call: the server may
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
    /// `trading` [`crate::infra::ctrader::auth::Scope`].
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

    /// Closes a position in full or in part. See the [module docs](crate::infra::ctrader::trading) for the non-idempotency
    /// caveat.
    ///
    /// # Errors
    ///
    /// `POSITION_NOT_FOUND`, `POSITION_NOT_OPEN`, `POSITION_LOCKED`, `TRADING_BAD_VOLUME` for a
    /// close volume larger than the position, and the usual account errors.
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
