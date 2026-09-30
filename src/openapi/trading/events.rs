//! The events trading produces: the answer to every trading request, an error with no request to
//! attach it to, and a trailing stop loss moving with the price.

use serde::Deserialize;

use crate::openapi::account::{Deal, Order, Position};
use crate::openapi::number_enum;
use crate::openapi::transport::wire::flex;

number_enum! {
    /// What an execution event reports (`ProtoOAExecutionType`).
    ExecutionType {
        OrderAccepted = 2 => "order accepted",
        OrderFilled = 3 => "order filled",
        OrderReplaced = 4 => "order replaced",
        OrderCancelled = 5 => "order cancelled",
        OrderExpired = 6 => "order expired",
        OrderRejected = 7 => "order rejected",
        OrderCancelRejected = 8 => "order cancel rejected",
        Swap = 9 => "swap",
        DepositWithdraw = 10 => "deposit or withdrawal",
        OrderPartialFill = 11 => "order partially filled",
        BonusDepositWithdraw = 12 => "bonus deposit or withdrawal",
    }
}

/// `ProtoOAExecutionEvent`: the answer to every trading request, and also sent for a deposit,
/// withdrawal or swap that was not asked for by one.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ExecutionEvent {
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    #[serde(deserialize_with = "flex::int")]
    pub execution_type: i64,
    #[serde(default)]
    pub position: Option<Position>,
    #[serde(default)]
    pub order: Option<Order>,
    #[serde(default)]
    pub deal: Option<Deal>,
    #[serde(default)]
    pub error_code: Option<String>,
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
#[non_exhaustive]
pub struct OrderErrorEvent {
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    pub error_code: String,
    #[serde(default, deserialize_with = "flex::opt")]
    pub order_id: Option<i64>,
    #[serde(default, deserialize_with = "flex::opt")]
    pub position_id: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
}

/// `ProtoOATrailingSLChangedEvent`: a trailing stop loss moved with the price.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TrailingSlChangedEvent {
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    #[serde(deserialize_with = "flex::int")]
    pub position_id: i64,
    #[serde(deserialize_with = "flex::int")]
    pub order_id: i64,
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
