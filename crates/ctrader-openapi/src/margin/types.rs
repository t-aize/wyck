//! Margin data: the expected cost of an order before sending it, margin call thresholds, dynamic
//! leverage tiers, and the events that report a margin change.

use serde::{Deserialize, Serialize};

use crate::number_enum;
use crate::transport::wire::flex;

number_enum! {
    /// Which of the three supported margin call thresholds this is (`ProtoOANotificationType`).
    MarginCallType {
        /// The first threshold.
        First = 61 => "margin level threshold 1",
        /// The second threshold.
        Second = 62 => "margin level threshold 2",
        /// The third threshold.
        Third = 63 => "margin level threshold 3",
    }
}

/// One tier of a dynamic leverage schedule (`ProtoOADynamicLeverageTier`): the leverage applied up
/// to a volume; the last tier also covers everything above its volume.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicLeverageTier {
    /// The largest open volume, per side, this tier's leverage applies to, in hundredths of a unit.
    #[serde(deserialize_with = "flex::int")]
    pub volume: i64,
    /// The leverage applied up to `volume` (100 means 1:100).
    #[serde(deserialize_with = "flex::int")]
    pub leverage: i64,
}

/// A dynamic leverage schedule (`ProtoOADynamicLeverage`), referenced by `Symbol::leverage_id`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicLeverage {
    /// The id `Symbol::leverage_id` points to.
    #[serde(deserialize_with = "flex::int")]
    pub leverage_id: i64,
    /// The tiers, sorted by volume.
    #[serde(default)]
    pub tiers: Vec<DynamicLeverageTier>,
}

/// The margin an order of each volume would need (`ProtoOAExpectedMargin`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedMargin {
    /// The volume this estimate is for, in hundredths of a unit.
    #[serde(deserialize_with = "flex::int")]
    pub volume: i64,
    /// The margin a buy of `volume` would use, scaled by `10^moneyDigits`.
    #[serde(deserialize_with = "flex::int")]
    pub buy_margin: i64,
    /// The margin a sell of `volume` would use, scaled by `10^moneyDigits`.
    #[serde(deserialize_with = "flex::int")]
    pub sell_margin: i64,
}

/// A margin call threshold (`ProtoOAMarginCall`). Three exist per account, told apart by
/// `margin_call_type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginCall {
    /// Which of the three thresholds this is, as [`MarginCallType`]'s number.
    #[serde(deserialize_with = "flex::int")]
    pub margin_call_type: i64,
    /// The margin level (equity over used margin, in percent) that triggers it.
    pub margin_level_threshold: f64,
    /// When it was last changed, in Unix milliseconds.
    #[serde(
        default,
        deserialize_with = "flex::opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub utc_last_update_timestamp: Option<i64>,
}

impl MarginCall {
    /// Which threshold this is.
    #[must_use]
    pub fn kind(&self) -> Option<MarginCallType> {
        MarginCallType::from_number(self.margin_call_type)
    }
}

/// `ProtoOAMarginChangedEvent`: the margin used by a position changed.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginChangedEvent {
    /// The account.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    /// The position.
    #[serde(deserialize_with = "flex::int")]
    pub position_id: i64,
    /// The new margin used, scaled by `10^moneyDigits`.
    #[serde(deserialize_with = "flex::int")]
    pub used_margin: i64,
    /// Decimals of the money amounts.
    #[serde(default, deserialize_with = "flex::opt")]
    pub money_digits: Option<i64>,
}

/// `ProtoOAMarginCallUpdateEvent`: a margin call threshold was changed (by this call, or elsewhere,
/// for example the cTrader platform).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginCallUpdateEvent {
    /// The account.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    /// The threshold, after the change.
    pub margin_call: MarginCall,
}

/// `ProtoOAMarginCallTriggerEvent`: the account's margin level reached a threshold. Sent at most
/// once every ten minutes per threshold.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginCallTriggerEvent {
    /// The account.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    /// The threshold that triggered.
    pub margin_call: MarginCall,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn margin_call_types_round_trip_and_refuse_unknown_numbers() {
        assert_eq!(MarginCallType::from_number(61), Some(MarginCallType::First));
        assert_eq!(MarginCallType::from_number(64), None);
        let call = MarginCall {
            margin_call_type: 62,
            margin_level_threshold: 80.0,
            utc_last_update_timestamp: None,
        };
        assert_eq!(call.kind(), Some(MarginCallType::Second));
    }

    #[test]
    fn margin_events_are_read() {
        let changed: MarginChangedEvent = serde_json::from_value(json!({
            "positionId": 1, "usedMargin": 5000, "moneyDigits": 2
        }))
        .unwrap();
        assert_eq!(changed.used_margin, 5000);

        let triggered: MarginCallTriggerEvent = serde_json::from_value(json!({
            "ctidTraderAccountId": 1,
            "marginCall": {"marginCallType": 63, "marginLevelThreshold": 50.0}
        }))
        .unwrap();
        assert_eq!(triggered.margin_call.kind(), Some(MarginCallType::Third));
    }

    #[test]
    fn dynamic_leverage_tiers_are_read() {
        let leverage: DynamicLeverage = serde_json::from_value(json!({
            "leverageId": 9, "tiers": [
                {"volume": 100000000, "leverage": 100},
                {"volume": 500000000, "leverage": 50}
            ]
        }))
        .unwrap();
        assert_eq!(leverage.tiers.len(), 2);
        assert_eq!(leverage.tiers[1].leverage, 50);
    }
}
