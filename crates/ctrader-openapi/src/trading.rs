//! Placing, amending and cancelling orders, and closing positions.
//!
//! These calls move money, simulated on a demo account but real on a live one, and they need a
//! token of the `trading` [`crate::auth::Scope`] (an `accounts` token is refused). Everything else
//! in this crate only reads; this module is the one place that writes.
//!
//! # Non-idempotency
//!
//! [`Client::new_order`] and its siblings are not guaranteed idempotent. If a request times out
//! ([`crate::OpenApiError::Timeout`]) or the connection drops while it is in flight
//! ([`crate::OpenApiError::Closed`]), the order may still have reached the server: the answer was
//! lost, not necessarily the request. Sending the same order again on a bare timeout can double a
//! position. Instead, check what the account actually holds first: [`Client::open_positions_and_orders`]
//! (`ProtoOAReconcileReq`) for what is open now, or [`Client::deals`] for what was executed
//! recently, using `label` or `client_order_id` to recognize the order you sent. This crate stays a
//! thin typed client, like Spotware's own SDKs (`OpenApiPy`, `OpenAPI.Net`): it does not add retry
//! or idempotency machinery of its own on top of trading calls.
//!
//! # Units
//!
//! Volumes are hundredths of a unit, like the read-only account calls (see [`crate::account::volume_units`]).
//! Prices (limit, stop, stop loss, take profit) are ordinary decimals, like the prices on
//! [`crate::account::Position`] and [`crate::account::Order`], unlike the integer, [`crate::types::PRICE_SCALE`]
//! prices of ticks and bars.

use serde::{Deserialize, Serialize};

use crate::account::{Deal, Order, Position};
use crate::client::{Client, RateClass};
use crate::error::Result;
use crate::model::flex;
use crate::number_enum;
use crate::wire::payload;

number_enum! {
    /// The kind of an order request (`ProtoOAOrderType`, the subset a caller chooses between when
    /// placing one; `crate::account::OrderType` decodes any order the server sends back, including
    /// `StopLossTakeProfit`, which a caller never asks for directly).
    NewOrderType {
        /// At the current market price.
        Market = 1 => "market",
        /// At `limit_price` or better.
        Limit = 2 => "limit",
        /// Becomes a market order once `stop_price` is reached.
        Stop = 3 => "stop",
        /// At the market, within `slippage_in_points` of `base_slippage_price`.
        MarketRange = 5 => "market range",
        /// Becomes a limit order once `stop_price` is reached.
        StopLimit = 6 => "stop limit",
    }
}

number_enum! {
    /// What an execution event reports (`ProtoOAExecutionType`).
    ExecutionType {
        /// The order passed validation and is working.
        OrderAccepted = 2 => "order accepted",
        /// The order is fully filled.
        OrderFilled = 3 => "order filled",
        /// A pending order was replaced with a new one (an amend).
        OrderReplaced = 4 => "order replaced",
        /// The order was cancelled.
        OrderCancelled = 5 => "order cancelled",
        /// A good-till-date order ran out of time.
        OrderExpired = 6 => "order expired",
        /// The order was rejected.
        OrderRejected = 7 => "order rejected",
        /// A cancel request was itself rejected.
        OrderCancelRejected = 8 => "order cancel rejected",
        /// A swap was charged.
        Swap = 9 => "swap",
        /// A deposit or a withdrawal took place.
        DepositWithdraw = 10 => "deposit or withdrawal",
        /// The order was partially filled.
        OrderPartialFill = 11 => "order partially filled",
        /// A bonus deposit or withdrawal took place.
        BonusDepositWithdraw = 12 => "bonus deposit or withdrawal",
    }
}

// ---- requests ----

/// `ProtoOANewOrderReq`. Build with [`NewOrderReq::market`], [`NewOrderReq::limit`],
/// [`NewOrderReq::stop`] or [`NewOrderReq::stop_limit`], then set the optional fields.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewOrderReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The symbol.
    pub symbol_id: i64,
    /// Market, limit, stop, market range or stop limit.
    pub order_type: i32,
    /// Buy or sell, as [`crate::account::TradeSide`]'s number.
    pub trade_side: i32,
    /// The volume, in hundredths of a unit.
    pub volume: i64,
    /// The limit price. Only for [`NewOrderType::Limit`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<f64>,
    /// The stop price. Only for [`NewOrderType::Stop`] and [`NewOrderType::StopLimit`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<f64>,
    /// How long the order stays working, as [`crate::account::TimeInForce`]'s number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<i32>,
    /// When a good-till-date order expires, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_timestamp: Option<i64>,
    /// The absolute stop loss price. Not for market orders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<f64>,
    /// The absolute take profit price. Not for market orders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<f64>,
    /// A comment, at most 512 characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// The base price for a market range order's slippage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_slippage_price: Option<f64>,
    /// The slippage, in points, for a market range or stop limit order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slippage_in_points: Option<i32>,
    /// A label, at most 100 characters, to recognize the order later (in a deal list or a
    /// reconcile answer), which matters for the non-idempotency caveat above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The position this order should modify (for example a closing order).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_id: Option<i64>,
    /// Your own id for the order, at most 50 characters (like FIX `ClOrdID`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    /// A stop loss relative to the entry price, in [`crate::types::PRICE_SCALE`] units, instead of
    /// the absolute `stop_loss`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_stop_loss: Option<i64>,
    /// A take profit relative to the entry price, in [`crate::types::PRICE_SCALE`] units, instead
    /// of the absolute `take_profit`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_take_profit: Option<i64>,
    /// Whether the stop loss is guaranteed. Required on a limited risk account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guaranteed_stop_loss: Option<bool>,
    /// Whether the stop loss trails the price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailing_stop_loss: Option<bool>,
    /// What triggers a stop or stop limit order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_trigger_method: Option<i32>,
}

impl NewOrderReq {
    fn base(
        account_id: i64,
        symbol_id: i64,
        order_type: NewOrderType,
        side: crate::account::TradeSide,
        volume: i64,
    ) -> Self {
        Self {
            ctid_trader_account_id: account_id,
            symbol_id,
            order_type: order_type.number(),
            trade_side: side.number(),
            volume,
            limit_price: None,
            stop_price: None,
            time_in_force: None,
            expiration_timestamp: None,
            stop_loss: None,
            take_profit: None,
            comment: None,
            base_slippage_price: None,
            slippage_in_points: None,
            label: None,
            position_id: None,
            client_order_id: None,
            relative_stop_loss: None,
            relative_take_profit: None,
            guaranteed_stop_loss: None,
            trailing_stop_loss: None,
            stop_trigger_method: None,
        }
    }

    /// A market order: filled at once, at whatever the current price is.
    #[must_use]
    pub fn market(
        account_id: i64,
        symbol_id: i64,
        side: crate::account::TradeSide,
        volume: i64,
    ) -> Self {
        Self::base(account_id, symbol_id, NewOrderType::Market, side, volume)
    }

    /// A limit order: fills at `limit_price` or better.
    #[must_use]
    pub fn limit(
        account_id: i64,
        symbol_id: i64,
        side: crate::account::TradeSide,
        volume: i64,
        limit_price: f64,
    ) -> Self {
        let mut req = Self::base(account_id, symbol_id, NewOrderType::Limit, side, volume);
        req.limit_price = Some(limit_price);
        req
    }

    /// A stop order: becomes a market order once `stop_price` is reached.
    #[must_use]
    pub fn stop(
        account_id: i64,
        symbol_id: i64,
        side: crate::account::TradeSide,
        volume: i64,
        stop_price: f64,
    ) -> Self {
        let mut req = Self::base(account_id, symbol_id, NewOrderType::Stop, side, volume);
        req.stop_price = Some(stop_price);
        req
    }

    /// A stop limit order: becomes a limit order at `limit_price` once `stop_price` is reached.
    #[must_use]
    pub fn stop_limit(
        account_id: i64,
        symbol_id: i64,
        side: crate::account::TradeSide,
        volume: i64,
        stop_price: f64,
        limit_price: f64,
    ) -> Self {
        let mut req = Self::base(account_id, symbol_id, NewOrderType::StopLimit, side, volume);
        req.stop_price = Some(stop_price);
        req.limit_price = Some(limit_price);
        req
    }

    /// Sets the stop loss and take profit prices.
    #[must_use]
    pub fn with_protection(mut self, stop_loss: Option<f64>, take_profit: Option<f64>) -> Self {
        self.stop_loss = stop_loss;
        self.take_profit = take_profit;
        self
    }

    /// Sets the label used to recognize the order later.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// `ProtoOACancelOrderReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelOrderReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The pending order to cancel.
    pub order_id: i64,
}

/// `ProtoOAAmendOrderReq`. Every field but the ids is optional: only the ones set are changed.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AmendOrderReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The pending order to amend.
    pub order_id: i64,
    /// A new volume, in hundredths of a unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<i64>,
    /// A new limit price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<f64>,
    /// A new stop price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<f64>,
    /// A new expiration, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_timestamp: Option<i64>,
    /// A new absolute stop loss price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<f64>,
    /// A new absolute take profit price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<f64>,
    /// A new slippage, in points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slippage_in_points: Option<i32>,
    /// A new relative stop loss, in [`crate::types::PRICE_SCALE`] units.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_stop_loss: Option<i64>,
    /// A new relative take profit, in [`crate::types::PRICE_SCALE`] units.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_take_profit: Option<i64>,
    /// A new guaranteed stop loss setting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guaranteed_stop_loss: Option<bool>,
    /// A new trailing stop loss setting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailing_stop_loss: Option<bool>,
    /// A new stop trigger method.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_trigger_method: Option<i32>,
}

impl AmendOrderReq {
    /// A request that changes nothing yet: set fields on the result before sending it.
    #[must_use]
    pub fn new(account_id: i64, order_id: i64) -> Self {
        Self {
            ctid_trader_account_id: account_id,
            order_id,
            ..Self::default()
        }
    }
}

/// `ProtoOAClosePositionReq`. Volume equal to the position's own closes it in full; less closes it
/// in part.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosePositionReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The position to close.
    pub position_id: i64,
    /// The volume to close, in hundredths of a unit.
    pub volume: i64,
}

/// `ProtoOAAmendPositionSLTPReq`. Every field but the ids is optional: only the ones set are
/// changed.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AmendPositionSlTpReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The position to amend.
    pub position_id: i64,
    /// The new absolute stop loss price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<f64>,
    /// The new absolute take profit price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<f64>,
    /// Whether the stop loss is guaranteed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guaranteed_stop_loss: Option<bool>,
    /// Whether the stop loss trails the price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailing_stop_loss: Option<bool>,
    /// What triggers the stop loss or take profit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss_trigger_method: Option<i32>,
}

impl AmendPositionSlTpReq {
    /// A request that changes nothing yet: set fields on the result before sending it.
    #[must_use]
    pub fn new(account_id: i64, position_id: i64) -> Self {
        Self {
            ctid_trader_account_id: account_id,
            position_id,
            ..Self::default()
        }
    }
}

// ---- events ----

/// `ProtoOAExecutionEvent`: the answer to every trading request, and also sent for a deposit,
/// withdrawal or swap that was not asked for by one.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionEvent {
    /// The account the execution belongs to.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    /// What happened, as [`ExecutionType`]'s number.
    #[serde(deserialize_with = "flex::int")]
    pub execution_type: i64,
    /// The position the execution affected.
    #[serde(default)]
    pub position: Option<Position>,
    /// The order the execution answers.
    #[serde(default)]
    pub order: Option<Order>,
    /// The fill this execution reports.
    #[serde(default)]
    pub deal: Option<Deal>,
    /// The server's error code, for `ORDER_REJECTED` and `ORDER_CANCEL_REJECTED`.
    #[serde(default)]
    pub error_code: Option<String>,
    /// Whether the server generated this event by itself (for example a stop out), rather than
    /// answering a request.
    #[serde(default)]
    pub is_server_event: Option<bool>,
}

impl ExecutionEvent {
    /// What kind of execution this is.
    #[must_use]
    pub fn kind(&self) -> Option<ExecutionType> {
        ExecutionType::from_number(self.execution_type)
    }
}

/// `ProtoOAOrderErrorEvent`: a trading request failed without a matching answer of its own type
/// (for example the account moved out of margin between requests).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderErrorEvent {
    /// The account the error is about.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    /// The error code, for example `NOT_ENOUGH_MONEY`.
    pub error_code: String,
    /// The order it concerns, when there is one yet.
    #[serde(default, deserialize_with = "flex::opt")]
    pub order_id: Option<i64>,
    /// The position it concerns, when there is one.
    #[serde(default, deserialize_with = "flex::opt")]
    pub position_id: Option<i64>,
    /// The server's explanation.
    #[serde(default)]
    pub description: Option<String>,
}

/// `ProtoOATrailingSLChangedEvent`: a trailing stop loss moved with the price.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrailingSlChangedEvent {
    /// The account.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    /// The position.
    #[serde(deserialize_with = "flex::int")]
    pub position_id: i64,
    /// The protective order.
    #[serde(deserialize_with = "flex::int")]
    pub order_id: i64,
    /// The new stop price.
    pub stop_price: f64,
    /// When it moved, in Unix milliseconds.
    #[serde(deserialize_with = "flex::int")]
    pub utc_last_update_timestamp: i64,
}

impl Client {
    /// Places a new order. See the [module docs](self) for the non-idempotency caveat: on a
    /// timeout, check [`Client::open_positions_and_orders`] or [`Client::deals`] before retrying.
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
    pub async fn new_order(&self, request: &NewOrderReq) -> Result<ExecutionEvent> {
        self.call(
            payload::NEW_ORDER_REQ,
            payload::EXECUTION_EVENT,
            request,
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
    pub async fn cancel_order(&self, account_id: i64, order_id: i64) -> Result<ExecutionEvent> {
        self.call(
            payload::CANCEL_ORDER_REQ,
            payload::EXECUTION_EVENT,
            &CancelOrderReq {
                ctid_trader_account_id: account_id,
                order_id,
            },
            RateClass::Standard,
            "the order cancel",
        )
        .await
    }

    /// Amends a pending order: only the fields set on `request` change.
    ///
    /// # Errors
    ///
    /// `ORDER_NOT_FOUND`, `UNABLE_TO_AMEND_ORDER`, `TRADING_BAD_STOPS`, `PENDING_EXECUTION` (the
    /// order is already being filled), and the usual account errors.
    pub async fn amend_order(&self, request: &AmendOrderReq) -> Result<ExecutionEvent> {
        self.call(
            payload::AMEND_ORDER_REQ,
            payload::EXECUTION_EVENT,
            request,
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
    pub async fn close_position(
        &self,
        account_id: i64,
        position_id: i64,
        volume: i64,
    ) -> Result<ExecutionEvent> {
        self.call(
            payload::CLOSE_POSITION_REQ,
            payload::EXECUTION_EVENT,
            &ClosePositionReq {
                ctid_trader_account_id: account_id,
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
    ///
    /// # Errors
    ///
    /// `PROTECTION_IS_TOO_CLOSE_TO_MARKET`, `TRADING_BAD_STOPS`, `WORSE_GSL_NOT_ALLOWED`, and the
    /// usual account errors.
    pub async fn amend_position_sl_tp(
        &self,
        request: &AmendPositionSlTpReq,
    ) -> Result<ExecutionEvent> {
        self.call(
            payload::AMEND_POSITION_SLTP_REQ,
            payload::EXECUTION_EVENT,
            request,
            RateClass::Standard,
            "the position stop loss and take profit amend",
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_new_order_constructor_carries_the_right_fields() {
        let market = NewOrderReq::market(1, 2, crate::account::TradeSide::Buy, 10_000);
        assert_eq!(
            serde_json::to_value(&market).unwrap(),
            json!({"ctidTraderAccountId": 1, "symbolId": 2, "orderType": 1, "tradeSide": 1, "volume": 10000})
        );

        let limit = NewOrderReq::limit(1, 2, crate::account::TradeSide::Sell, 500, 1.2345)
            .with_protection(Some(1.24), Some(1.20))
            .with_label("wyck-test");
        let value = serde_json::to_value(&limit).unwrap();
        assert_eq!(value["orderType"], 2);
        assert_eq!(value["limitPrice"], 1.2345);
        assert_eq!(value["stopLoss"], 1.24);
        assert_eq!(value["label"], "wyck-test");

        let stop = NewOrderReq::stop(1, 2, crate::account::TradeSide::Buy, 500, 1.3);
        assert_eq!(serde_json::to_value(&stop).unwrap()["stopPrice"], 1.3);

        let stop_limit =
            NewOrderReq::stop_limit(1, 2, crate::account::TradeSide::Buy, 500, 1.3, 1.31);
        let value = serde_json::to_value(&stop_limit).unwrap();
        assert_eq!(value["stopPrice"], json!(1.3));
        assert_eq!(value["limitPrice"], json!(1.31));
    }

    #[test]
    fn amend_requests_only_send_the_fields_that_were_set() {
        let mut amend = AmendOrderReq::new(1, 9);
        amend.stop_loss = Some(1.1);
        assert_eq!(
            serde_json::to_value(&amend).unwrap(),
            json!({"ctidTraderAccountId": 1, "orderId": 9, "stopLoss": 1.1})
        );

        let sltp = AmendPositionSlTpReq::new(1, 77);
        assert_eq!(
            serde_json::to_value(&sltp).unwrap(),
            json!({"ctidTraderAccountId": 1, "positionId": 77})
        );
    }

    #[test]
    fn an_execution_event_decodes_its_kind_and_its_payload() {
        let event: ExecutionEvent = serde_json::from_value(json!({
            "ctidTraderAccountId": 1,
            "executionType": 3,
            "deal": {
                "dealId": 1, "orderId": 2, "positionId": 3, "volume": 1000, "filledVolume": 1000,
                "symbolId": 1, "createTimestamp": 1, "executionTimestamp": 2,
                "tradeSide": 1, "dealStatus": 2, "executionPrice": 1.1
            }
        }))
        .unwrap();
        assert_eq!(event.kind(), Some(ExecutionType::OrderFilled));
        assert!(event.deal.is_some());
        assert_eq!(event.position, None);
    }

    #[test]
    fn an_order_error_event_carries_its_code() {
        let event: OrderErrorEvent = serde_json::from_value(json!({
            "errorCode": "NOT_ENOUGH_MONEY", "orderId": 9, "description": "insufficient margin"
        }))
        .unwrap();
        assert_eq!(event.error_code, "NOT_ENOUGH_MONEY");
        assert_eq!(event.order_id, Some(9));
    }

    #[test]
    fn a_trailing_stop_loss_change_is_read() {
        let event: TrailingSlChangedEvent = serde_json::from_value(json!({
            "positionId": 1, "orderId": 2, "stopPrice": 1.234, "utcLastUpdateTimestamp": 999
        }))
        .unwrap();
        assert_eq!(event.stop_price, 1.234);
        assert_eq!(event.utc_last_update_timestamp, 999);
    }

    #[test]
    fn unknown_execution_types_are_none_not_an_error() {
        let event: ExecutionEvent = serde_json::from_value(json!({"executionType": 999})).unwrap();
        assert_eq!(event.kind(), None);
    }
}
