//! The request and response messages of the margin calls.

use serde::{Deserialize, Serialize};

use super::types::{DynamicLeverage, ExpectedMargin, MarginCall};
use crate::transport::wire::flex;

/// `ProtoOAExpectedMarginReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedMarginReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The symbol.
    pub symbol_id: i64,
    /// The volumes to estimate, in hundredths of a unit.
    pub volume: Vec<i64>,
}

/// `ProtoOAGetDynamicLeverageByIDReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDynamicLeverageReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The leverage schedule id (`Symbol::leverage_id`).
    pub leverage_id: i64,
}

/// `ProtoOAMarginCallUpdateReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginCallUpdateReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The threshold to change (its `margin_call_type` says which of the three).
    pub margin_call: MarginCall,
}

/// `ProtoOAExpectedMarginRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedMarginRes {
    /// One estimate per volume asked about.
    #[serde(default)]
    pub margin: Vec<ExpectedMargin>,
    /// Decimals of the money amounts.
    #[serde(default, deserialize_with = "flex::opt")]
    pub money_digits: Option<i64>,
}

/// `ProtoOAMarginCallListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginCallListRes {
    /// The three thresholds of the account.
    #[serde(default)]
    pub margin_call: Vec<MarginCall>,
}

/// `ProtoOAGetDynamicLeverageByIDRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDynamicLeverageRes {
    /// The schedule asked for.
    pub leverage: DynamicLeverage,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn expected_margin_and_dynamic_leverage_are_read() {
        let margin: ExpectedMarginRes = serde_json::from_value(json!({
            "margin": [{"volume": 100000, "buyMargin": 2000, "sellMargin": 2000}],
            "moneyDigits": 2
        }))
        .unwrap();
        assert_eq!(margin.margin[0].buy_margin, 2000);

        let leverage: GetDynamicLeverageRes = serde_json::from_value(json!({
            "leverage": {"leverageId": 9, "tiers": [
                {"volume": 100000000, "leverage": 100}
            ]}
        }))
        .unwrap();
        assert_eq!(leverage.leverage.tiers.len(), 1);
    }

    #[test]
    fn a_margin_call_update_request_carries_the_official_field_names() {
        let req = MarginCallUpdateReq {
            ctid_trader_account_id: 1,
            margin_call: MarginCall {
                margin_call_type: 61,
                margin_level_threshold: 100.0,
                utc_last_update_timestamp: None,
            },
        };
        assert_eq!(
            serde_json::to_value(&req).unwrap(),
            json!({"ctidTraderAccountId": 1, "marginCall": {"marginCallType": 61, "marginLevelThreshold": 100.0}})
        );
    }
}
