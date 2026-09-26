//! Margin, through [`MarginClient`] (see [`crate::AccountClient::margin`]): the expected cost of
//! an order before sending it, margin call thresholds, and dynamic leverage tiers.
//!
//! Reading these needs no trading permission; [`MarginClient::update_margin_call`] changes a
//! setting on the account and, like the trading calls, needs a token of the `trading`
//! [`crate::auth::Scope`].

pub use wyck_openapi_model::margin::{requests, types};

pub use requests::{
    ExpectedMarginReq, ExpectedMarginRes, GetDynamicLeverageReq, GetDynamicLeverageRes,
    MarginCallListRes, MarginCallUpdateReq,
};
pub use types::{
    DynamicLeverage, DynamicLeverageTier, ExpectedMargin, MarginCall, MarginCallTriggerEvent,
    MarginCallType, MarginCallUpdateEvent, MarginChangedEvent,
};

use requests::{ExpectedMarginReq as MarginReq, GetDynamicLeverageReq as LeverageReq};

use crate::error::Result;
use crate::transport::connection::{Client, RateClass};
use crate::transport::messages::AccountReq;
use crate::transport::wire::payload;

/// Margin bound to one account: expected margin, margin call thresholds, dynamic leverage. See
/// [`crate::AccountClient::margin`].
#[derive(Debug, Clone)]
pub struct MarginClient {
    client: Client,
    account_id: i64,
}

impl MarginClient {
    pub(crate) fn new(client: Client, account_id: i64) -> Self {
        Self { client, account_id }
    }

    /// The account id this client is bound to.
    #[must_use]
    pub fn account_id(&self) -> i64 {
        self.account_id
    }

    /// The connection underneath, for calls that are not about margin.
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// The margin a buy and a sell of each of `volumes` would use on `symbol_id`. Does not cover
    /// the `ACCORDING_TO_GSL` margin calculation type: with a guaranteed stop loss the margin is
    /// simply `(entry price - GSL price) * volume`, in the deposit currency.
    ///
    /// # Errors
    ///
    /// `SYMBOL_NOT_FOUND`, and the usual account errors.
    pub async fn expected_margin(
        &self,
        symbol_id: i64,
        volumes: &[i64],
    ) -> Result<Vec<ExpectedMargin>> {
        let response: ExpectedMarginRes = self
            .client
            .call(
                payload::EXPECTED_MARGIN_REQ,
                payload::EXPECTED_MARGIN_RES,
                &MarginReq {
                    ctid_trader_account_id: self.account_id,
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
    pub async fn margin_calls(&self) -> Result<Vec<MarginCall>> {
        let response: MarginCallListRes = self
            .client
            .call(
                payload::MARGIN_CALL_LIST_REQ,
                payload::MARGIN_CALL_LIST_RES,
                &AccountReq {
                    ctid_trader_account_id: self.account_id,
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
    pub async fn update_margin_call(&self, margin_call: MarginCall) -> Result<()> {
        let _: serde_json::Value = self
            .client
            .call(
                payload::MARGIN_CALL_UPDATE_REQ,
                payload::MARGIN_CALL_UPDATE_RES,
                &MarginCallUpdateReq {
                    ctid_trader_account_id: self.account_id,
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
    pub async fn dynamic_leverage(&self, leverage_id: i64) -> Result<DynamicLeverage> {
        let response: GetDynamicLeverageRes = self
            .client
            .call(
                payload::GET_DYNAMIC_LEVERAGE_REQ,
                payload::GET_DYNAMIC_LEVERAGE_RES,
                &LeverageReq {
                    ctid_trader_account_id: self.account_id,
                    leverage_id,
                },
                RateClass::Standard,
                "the dynamic leverage schedule",
            )
            .await?;
        Ok(response.leverage)
    }
}
