//! The request and response messages of the margin calls.

use serde::{Deserialize, Serialize};

use wyck_core::flex;
use wyck_trading::margin::ExpectedMargin;

/// `ProtoOAExpectedMarginReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExpectedMarginReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    pub symbol_id: i64,
    /// The volumes to estimate, in hundredths of a unit.
    pub volume: Vec<i64>,
}

/// `ProtoOAExpectedMarginRes`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExpectedMarginRes {
    /// One estimate per volume asked about.
    #[serde(default)]
    pub margin: Vec<ExpectedMargin>,
    /// Decimals of the money amounts.
    #[serde(default, deserialize_with = "flex::opt")]
    pub money_digits: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn expected_margin_is_read() {
        let margin: ExpectedMarginRes = serde_json::from_value(json!({
            "margin": [{"volume": 100_000, "buyMargin": 2000, "sellMargin": 2000}],
            "moneyDigits": 2
        }))
        .unwrap();
        assert_eq!(margin.margin[0].buy_margin, 2000);
    }
}
