//! The request messages of the trading calls: placing, amending and cancelling orders, closing
//! positions, and amending a position's protection.
//!
//! `ctid_trader_account_id` is a public field of every request here (serde needs it), initialized
//! to `0` by the constructors below: [`super::TradingClient`] always overwrites it with the account
//! it is bound to before sending, so a caller building one of these directly never has to set it.

use serde::Serialize;

use crate::account::TradeSide;
use crate::number_enum;

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

/// `ProtoOANewOrderReq`. Build with [`NewOrderReq::market`], [`NewOrderReq::limit`],
/// [`NewOrderReq::stop`] or [`NewOrderReq::stop_limit`], then set the optional fields.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewOrderReq {
    /// The trading account id. Always overwritten by [`super::TradingClient::new_order`] with the
    /// account it is bound to.
    pub ctid_trader_account_id: i64,
    /// The symbol.
    pub symbol_id: i64,
    /// Market, limit, stop, market range or stop limit.
    pub order_type: i32,
    /// Buy or sell, as [`TradeSide`]'s number.
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
    /// reconcile answer), which matters for the non-idempotency caveat in the [module docs](self).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The position this order should modify (for example a closing order).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_id: Option<i64>,
    /// Your own id for the order, at most 50 characters (like FIX `ClOrdID`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    /// A stop loss relative to the entry price, in [`crate::market::PRICE_SCALE`] units, instead of
    /// the absolute `stop_loss`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_stop_loss: Option<i64>,
    /// A take profit relative to the entry price, in [`crate::market::PRICE_SCALE`] units, instead
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
    /// Checks values that would otherwise encode incorrectly or produce a known refusal.
    /// The server still decides whether the symbol, account and price are tradable.
    pub fn validate(&self) -> crate::Result<()> {
        let bad = |message: &str| Err(crate::OpenApiError::Config(message.to_owned()));
        if self.symbol_id <= 0 || self.volume <= 0 {
            return bad("an order needs a positive symbol id and volume");
        }
        for price in [
            self.limit_price,
            self.stop_price,
            self.stop_loss,
            self.take_profit,
            self.base_slippage_price,
        ] {
            if price.is_some_and(|value| !value.is_finite() || value <= 0.0) {
                return bad("order prices must be finite and positive");
            }
        }
        if self.relative_stop_loss.is_some_and(|value| value <= 0)
            || self.relative_take_profit.is_some_and(|value| value <= 0)
        {
            return bad("relative protection distances must be positive");
        }
        if self.slippage_in_points.is_some_and(|value| value < 0) {
            return bad("slippage must not be negative");
        }
        if (self.stop_loss.is_some() && self.relative_stop_loss.is_some())
            || (self.take_profit.is_some() && self.relative_take_profit.is_some())
        {
            return bad("absolute and relative protection cannot be combined for one leg");
        }
        if self.order_type == NewOrderType::Market.number()
            && (self.stop_loss.is_some() || self.take_profit.is_some())
        {
            return bad("market orders need relative rather than absolute protection");
        }
        if self.order_type == NewOrderType::Limit.number() && self.limit_price.is_none() {
            return bad("limit orders need a limit price");
        }
        if self.order_type == NewOrderType::Stop.number() && self.stop_price.is_none() {
            return bad("stop orders need a stop price");
        }
        if self.order_type == NewOrderType::StopLimit.number()
            && (self.stop_price.is_none() || self.limit_price.is_none())
        {
            return bad("stop limit orders need a stop and a limit price");
        }
        if self.time_in_force == Some(1) && self.expiration_timestamp.is_none() {
            return bad("good till date orders need an expiration timestamp");
        }
        if self
            .label
            .as_ref()
            .is_some_and(|value| value.chars().count() > 100)
            || self
                .comment
                .as_ref()
                .is_some_and(|value| value.chars().count() > 512)
            || self
                .client_order_id
                .as_ref()
                .is_some_and(|value| value.chars().count() > 50)
        {
            return bad("an order label, comment or client order id is too long");
        }
        Ok(())
    }

    fn base(symbol_id: i64, order_type: NewOrderType, side: TradeSide, volume: i64) -> Self {
        Self {
            ctid_trader_account_id: 0,
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
    pub fn market(symbol_id: i64, side: TradeSide, volume: i64) -> Self {
        Self::base(symbol_id, NewOrderType::Market, side, volume)
    }

    /// A limit order: fills at `limit_price` or better.
    #[must_use]
    pub fn limit(symbol_id: i64, side: TradeSide, volume: i64, limit_price: f64) -> Self {
        let mut req = Self::base(symbol_id, NewOrderType::Limit, side, volume);
        req.limit_price = Some(limit_price);
        req
    }

    /// A stop order: becomes a market order once `stop_price` is reached.
    #[must_use]
    pub fn stop(symbol_id: i64, side: TradeSide, volume: i64, stop_price: f64) -> Self {
        let mut req = Self::base(symbol_id, NewOrderType::Stop, side, volume);
        req.stop_price = Some(stop_price);
        req
    }

    /// A stop limit order: becomes a limit order at `limit_price` once `stop_price` is reached.
    #[must_use]
    pub fn stop_limit(
        symbol_id: i64,
        side: TradeSide,
        volume: i64,
        stop_price: f64,
        limit_price: f64,
    ) -> Self {
        let mut req = Self::base(symbol_id, NewOrderType::StopLimit, side, volume);
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
    /// The trading account id. Always overwritten by [`super::TradingClient::amend_order`].
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
    /// A new relative stop loss, in [`crate::market::PRICE_SCALE`] units.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_stop_loss: Option<i64>,
    /// A new relative take profit, in [`crate::market::PRICE_SCALE`] units.
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
    pub fn new(order_id: i64) -> Self {
        Self {
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
    /// The trading account id. Always overwritten by
    /// [`super::TradingClient::amend_position_sl_tp`].
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
    pub fn new(position_id: i64) -> Self {
        Self {
            position_id,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_new_order_constructor_carries_the_right_fields() {
        let market = NewOrderReq::market(2, TradeSide::Buy, 10_000);
        assert_eq!(
            serde_json::to_value(&market).unwrap(),
            json!({"ctidTraderAccountId": 0, "symbolId": 2, "orderType": 1, "tradeSide": 1, "volume": 10000})
        );

        let limit = NewOrderReq::limit(2, TradeSide::Sell, 500, 1.2345)
            .with_protection(Some(1.24), Some(1.20))
            .with_label("wyck-test");
        let value = serde_json::to_value(&limit).unwrap();
        assert_eq!(value["orderType"], 2);
        assert_eq!(value["limitPrice"], 1.2345);
        assert_eq!(value["stopLoss"], 1.24);
        assert_eq!(value["label"], "wyck-test");

        let stop = NewOrderReq::stop(2, TradeSide::Buy, 500, 1.3);
        assert_eq!(serde_json::to_value(&stop).unwrap()["stopPrice"], 1.3);

        let stop_limit = NewOrderReq::stop_limit(2, TradeSide::Buy, 500, 1.3, 1.31);
        let value = serde_json::to_value(&stop_limit).unwrap();
        assert_eq!(value["stopPrice"], json!(1.3));
        assert_eq!(value["limitPrice"], json!(1.31));
    }

    #[test]
    fn amend_requests_only_send_the_fields_that_were_set() {
        let mut amend = AmendOrderReq::new(9);
        amend.ctid_trader_account_id = 1;
        amend.stop_loss = Some(1.1);
        assert_eq!(
            serde_json::to_value(&amend).unwrap(),
            json!({"ctidTraderAccountId": 1, "orderId": 9, "stopLoss": 1.1})
        );

        let mut sltp = AmendPositionSlTpReq::new(77);
        sltp.ctid_trader_account_id = 1;
        assert_eq!(
            serde_json::to_value(&sltp).unwrap(),
            json!({"ctidTraderAccountId": 1, "positionId": 77})
        );
    }

    #[test]
    fn unsafe_order_values_are_rejected_before_serialization() {
        let mut request = NewOrderReq::market(2, TradeSide::Buy, 100);
        request.stop_loss = Some(f64::NAN);
        assert!(request.validate().is_err());
        request.stop_loss = None;
        request.volume = 0;
        assert!(request.validate().is_err());
        request.volume = 100;
        request.relative_stop_loss = Some(100);
        assert!(request.validate().is_ok());
    }
}
