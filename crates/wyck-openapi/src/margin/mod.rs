//! Margin, through [`MarginClient`] (see [`crate::AccountClient::margin`]): the expected cost of
//! an order before sending it, margin call thresholds, and dynamic leverage tiers.
//!
//! Reading these needs no trading permission; [`MarginClient::update_margin_call`] changes a
//! setting on the account and, like the trading calls, needs a token of the `trading`
//! [`crate::auth::Scope`].

pub mod requests;
pub mod types;

pub use requests::{
    ExpectedMarginReq, ExpectedMarginRes, GetDynamicLeverageReq, GetDynamicLeverageRes,
    MarginCallListRes, MarginCallUpdateReq,
};
pub use types::{
    DynamicLeverage, DynamicLeverageTier, ExpectedMargin, MarginCall, MarginCallTriggerEvent,
    MarginCallType, MarginCallUpdateEvent, MarginChangedEvent,
};

#[cfg(feature = "client")]
mod client;
#[cfg(feature = "client")]
pub use client::MarginClient;
