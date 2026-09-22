//! The request and response messages of the account calls: what an [`super::AccountDataClient`]
//! sends and reads, and the event that reports a balance change.

use serde::{Deserialize, Serialize};

use super::types::{
    Deal, DealOffset, DepositWithdraw, Order, Position, PositionUnrealizedPnL, Trader,
};
use crate::openapi::transport::wire::flex;

/// `ProtoOAReconcileReq`: what the account holds right now.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// Also return the protective orders (stop loss, take profit) of the positions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_protection_orders: Option<bool>,
}

/// `ProtoOADealListReq`: closed history, deals in a time range.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DealListReq {
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
pub struct OrderListReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// Start of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<i64>,
    /// End of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<i64>,
}

/// `ProtoOACashFlowHistoryListReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CashFlowHistoryListReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// Start of the range, in Unix milliseconds. The range may span at most one week.
    pub from_timestamp: i64,
    /// End of the range, in Unix milliseconds.
    pub to_timestamp: i64,
}

/// `ProtoOADealListByPositionIdReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DealListByPositionIdReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The position.
    pub position_id: i64,
    /// Start of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<i64>,
    /// End of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<i64>,
}

/// `ProtoOAOrderListByPositionIdReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderListByPositionIdReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The position.
    pub position_id: i64,
    /// Start of the range, in Unix milliseconds. Filters by the order's last update.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<i64>,
    /// End of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<i64>,
}

/// `ProtoOAOrderDetailsReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderDetailsReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The order.
    pub order_id: i64,
}

/// `ProtoOADealOffsetListReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DealOffsetListReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The deal.
    pub deal_id: i64,
}

/// `ProtoOATraderRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraderRes {
    /// The account.
    pub trader: Trader,
}

/// `ProtoOAReconcileRes`: everything the account holds right now.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileRes {
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
pub struct DealListRes {
    /// The deals.
    #[serde(default)]
    pub deal: Vec<Deal>,
    /// Whether more deals exist in the range than were returned.
    #[serde(default)]
    pub has_more: bool,
}

/// `ProtoOAOrderListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderListRes {
    /// The orders.
    #[serde(default)]
    pub order: Vec<Order>,
    /// Whether more orders exist in the range than were returned.
    #[serde(default)]
    pub has_more: bool,
}

/// `ProtoOATraderUpdatedEvent`: the account changed (a balance moved).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraderUpdatedEvent {
    /// The account after the change.
    pub trader: Trader,
}

/// `ProtoOACashFlowHistoryListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CashFlowHistoryListRes {
    /// The deposits and withdrawals of the range.
    #[serde(default)]
    pub deposit_withdraw: Vec<DepositWithdraw>,
}

/// `ProtoOADealListByPositionIdRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DealListByPositionIdRes {
    /// The deals of the position.
    #[serde(default)]
    pub deal: Vec<Deal>,
    /// Whether more deals exist in the range than were returned.
    #[serde(default)]
    pub has_more: bool,
}

/// `ProtoOAOrderListByPositionIdRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderListByPositionIdRes {
    /// The orders of the position, newest first.
    #[serde(default)]
    pub order: Vec<Order>,
    /// Whether more orders exist in the range than were returned.
    #[serde(default)]
    pub has_more: bool,
}

/// `ProtoOAOrderDetailsRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderDetailsRes {
    /// The order.
    pub order: Order,
    /// Every deal that filled it.
    #[serde(default)]
    pub deal: Vec<Deal>,
}

/// `ProtoOADealOffsetListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DealOffsetListRes {
    /// Deals that closed the one asked about.
    #[serde(default)]
    pub offset_by: Vec<DealOffset>,
    /// Deals that the one asked about closed.
    #[serde(default)]
    pub offsetting: Vec<DealOffset>,
}

/// `ProtoOAGetPositionUnrealizedPnLRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
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
    fn deal_offsets_and_unrealized_pnl_responses_are_read() {
        let offsets: DealOffsetListRes = serde_json::from_value(json!({
            "offsetBy": [{"dealId": 1, "volume": 100, "executionPrice": 1.1}],
            "offsetting": []
        }))
        .unwrap();
        assert_eq!(offsets.offset_by[0].deal_id, 1);
        assert!(offsets.offsetting.is_empty());

        let pnl: PositionUnrealizedPnLRes = serde_json::from_value(json!({
            "positionUnrealizedPnL": [
                {"positionId": 1, "grossUnrealizedPnL": 500, "netUnrealizedPnL": 450}
            ],
            "moneyDigits": 2
        }))
        .unwrap();
        assert_eq!(pnl.position_unrealized_pnl[0].net_unrealized_pnl, 450);
    }

    #[test]
    fn the_order_and_deal_by_position_and_offset_requests_use_the_official_names() {
        assert_eq!(
            serde_json::to_value(DealListByPositionIdReq {
                ctid_trader_account_id: 1,
                position_id: 77,
                from_timestamp: Some(1),
                to_timestamp: None,
            })
            .unwrap(),
            json!({"ctidTraderAccountId": 1, "positionId": 77, "fromTimestamp": 1})
        );
        assert_eq!(
            serde_json::to_value(OrderDetailsReq {
                ctid_trader_account_id: 1,
                order_id: 9,
            })
            .unwrap(),
            json!({"ctidTraderAccountId": 1, "orderId": 9})
        );
        assert_eq!(
            serde_json::to_value(DealOffsetListReq {
                ctid_trader_account_id: 1,
                deal_id: 4,
            })
            .unwrap(),
            json!({"ctidTraderAccountId": 1, "dealId": 4})
        );
    }
}
