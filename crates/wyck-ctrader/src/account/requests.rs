//! The request and response messages of the account calls: what an the account client
//! sends and reads, and the event that reports a balance change.

use serde::{Deserialize, Serialize};

use wyck_core::flex;
use wyck_trading::types::{Deal, Order, Position, PositionUnrealizedPnL, Trader};

/// `ProtoOAReconcileReq`: what the account holds right now.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReconcileReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// Also return the protective orders (stop loss, take profit) of the positions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_protection_orders: Option<bool>,
}

/// `ProtoOADealListReq`: closed history, deals in a time range.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DealListReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// Start of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<i64>,
    /// End of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<i64>,
    /// The most rows to return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rows: Option<i32>,
}

/// `ProtoOAOrderListReq`: orders in a time range.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrderListReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// Start of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<i64>,
    /// End of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<i64>,
}

/// `ProtoOATraderRes`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TraderRes {
    pub trader: Trader,
}

/// `ProtoOAReconcileRes`: everything the account holds right now.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReconcileRes {
    /// The open positions.
    #[serde(default)]
    pub position: Vec<Position>,
    /// The working orders.
    #[serde(default)]
    pub order: Vec<Order>,
}

/// `ProtoOADealListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DealListRes {
    #[serde(default)]
    pub deal: Vec<Deal>,
    /// Whether more deals exist in the range than were returned.
    #[serde(default)]
    pub has_more: bool,
}

/// `ProtoOAOrderListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrderListRes {
    #[serde(default)]
    pub order: Vec<Order>,
    /// Whether more orders exist in the range than were returned.
    #[serde(default)]
    pub has_more: bool,
}

/// `ProtoOAGetPositionUnrealizedPnLRes`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionUnrealizedPnLRes {
    /// The unrealized profit or loss of every open position. Renamed explicitly: the server's
    /// field is `positionUnrealizedPnL` (capital `L`), which plain camelCase would not reproduce
    /// from this name.
    #[serde(default, rename = "positionUnrealizedPnL")]
    pub position_unrealized_pnl: Vec<PositionUnrealizedPnL>,
    /// Decimals of the money amounts.
    #[serde(default, deserialize_with = "flex::opt")]
    pub money_digits: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_answers_are_empty_lists_not_errors() {
        let reconcile: ReconcileRes = serde_json::from_value(json!({})).unwrap();
        assert!(reconcile.position.is_empty() && reconcile.order.is_empty());
        let deals: DealListRes = serde_json::from_value(json!({})).unwrap();
        assert!(deals.deal.is_empty() && !deals.has_more);
    }

    #[test]
    fn requests_use_the_official_names_and_skip_missing_options() {
        assert_eq!(
            serde_json::to_value(DealListReq {
                ctid_trader_account_id: 1,
                from_timestamp: Some(5),
                to_timestamp: None,
                max_rows: Some(100),
            })
            .unwrap(),
            json!({"ctidTraderAccountId": 1, "fromTimestamp": 5, "maxRows": 100})
        );
        assert_eq!(
            serde_json::to_value(ReconcileReq {
                ctid_trader_account_id: 1,
                return_protection_orders: Some(true),
            })
            .unwrap(),
            json!({"ctidTraderAccountId": 1, "returnProtectionOrders": true})
        );
    }

    #[test]
    fn unrealized_pnl_responses_are_read() {
        let pnl: PositionUnrealizedPnLRes = serde_json::from_value(json!({
            "positionUnrealizedPnL": [
                {"positionId": 1, "grossUnrealizedPnL": 500, "netUnrealizedPnL": 450}
            ],
            "moneyDigits": 2
        }))
        .unwrap();
        assert_eq!(pnl.position_unrealized_pnl[0].net_unrealized_pnl, 450);
    }
}
