//! Margin: the expected cost of an order before sending it, margin call thresholds, dynamic
//! leverage tiers, and the events that report a margin change.
//!
//! Reading these needs no trading permission; [`Client::update_margin_call`] changes a setting on
//! the account and, like the trading calls, needs a token of the `trading` [`crate::auth::Scope`].

use serde::{Deserialize, Serialize};

use crate::client::{Client, RateClass};
use crate::error::Result;
use crate::model::flex;
use crate::number_enum;
use crate::wire::payload;

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

// ---- data ----

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

// ---- requests ----

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

// ---- responses ----

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

// ---- events ----

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

impl Client {
    /// The margin a buy and a sell of each of `volumes` would use on `symbol_id`. Does not cover
    /// the `ACCORDING_TO_GSL` margin calculation type: with a guaranteed stop loss the margin is
    /// simply `(entry price - GSL price) * volume`, in the deposit currency.
    ///
    /// # Errors
    ///
    /// `SYMBOL_NOT_FOUND`, and the usual account errors.
    pub async fn expected_margin(
        &self,
        account_id: i64,
        symbol_id: i64,
        volumes: &[i64],
    ) -> Result<Vec<ExpectedMargin>> {
        let response: ExpectedMarginRes = self
            .call(
                payload::EXPECTED_MARGIN_REQ,
                payload::EXPECTED_MARGIN_RES,
                &ExpectedMarginReq {
                    ctid_trader_account_id: account_id,
                    symbol_id,
                    volume: volumes.to_vec(),
                },
                RateClass::Standard,
                "the expected margin",
            )
            .await?;
        Ok(response.margin)
    }

    /// The account's three margin call thresholds.
    ///
    /// # Errors
    ///
    /// The usual account errors.
    pub async fn margin_calls(&self, account_id: i64) -> Result<Vec<MarginCall>> {
        let response: MarginCallListRes = self
            .call(
                payload::MARGIN_CALL_LIST_REQ,
                payload::MARGIN_CALL_LIST_RES,
                &crate::account::AccountReq {
                    ctid_trader_account_id: account_id,
                },
                RateClass::Standard,
                "the margin call list",
            )
            .await?;
        Ok(response.margin_call)
    }

    /// Changes the level of one margin call threshold.
    ///
    /// # Errors
    ///
    /// A server error for an out of range threshold, and the usual account errors. Needs a token of
    /// the `trading` [`crate::auth::Scope`].
    pub async fn update_margin_call(&self, account_id: i64, margin_call: MarginCall) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                payload::MARGIN_CALL_UPDATE_REQ,
                payload::MARGIN_CALL_UPDATE_RES,
                &MarginCallUpdateReq {
                    ctid_trader_account_id: account_id,
                    margin_call,
                },
                RateClass::Standard,
                "the margin call update",
            )
            .await?;
        Ok(())
    }

    /// The dynamic leverage schedule `leverage_id` (see `Symbol::leverage_id`).
    ///
    /// # Errors
    ///
    /// A server error for an unknown `leverage_id`, and the usual account errors.
    pub async fn dynamic_leverage(
        &self,
        account_id: i64,
        leverage_id: i64,
    ) -> Result<DynamicLeverage> {
        let response: GetDynamicLeverageRes = self
            .call(
                payload::GET_DYNAMIC_LEVERAGE_REQ,
                payload::GET_DYNAMIC_LEVERAGE_RES,
                &GetDynamicLeverageReq {
                    ctid_trader_account_id: account_id,
                    leverage_id,
                },
                RateClass::Standard,
                "the dynamic leverage schedule",
            )
            .await?;
        Ok(response.leverage)
    }
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
    fn expected_margin_and_dynamic_leverage_are_read() {
        let margin: ExpectedMarginRes = serde_json::from_value(json!({
            "margin": [{"volume": 100000, "buyMargin": 2000, "sellMargin": 2000}],
            "moneyDigits": 2
        }))
        .unwrap();
        assert_eq!(margin.margin[0].buy_margin, 2000);

        let leverage: GetDynamicLeverageRes = serde_json::from_value(json!({
            "leverage": {"leverageId": 9, "tiers": [
                {"volume": 100000000, "leverage": 100},
                {"volume": 500000000, "leverage": 50}
            ]}
        }))
        .unwrap();
        assert_eq!(leverage.leverage.tiers.len(), 2);
        assert_eq!(leverage.leverage.tiers[1].leverage, 50);
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
}
