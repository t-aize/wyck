//! The events trading produces: the answer to every trading request, an error with no request to
//! attach it to, and a trailing stop loss moving with the price.

use serde::Deserialize;

use crate::account::{Deal, Order, Position};
use crate::number_enum;
use crate::transport::wire::flex;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
