//! Margin, through `MarginClient` (see `crate::openapi::AccountClient::margin`): the expected
//! cost of an order before sending it, margin call thresholds, and dynamic leverage tiers.

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

mod client;
pub use client::MarginClient;
