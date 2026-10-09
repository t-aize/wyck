//! Margin, through [`MarginClient`] (see [`crate::infra::ctrader::AccountClient::margin`]): the expected cost of
//! an order before sending it, margin call thresholds, and dynamic leverage tiers.
//!
//! Reading these needs no trading permission; [`MarginClient::update_margin_call`] changes a
//! setting on the account and, like the trading calls, needs a token of the `trading`
//! [`crate::infra::ctrader::auth::Scope`].

pub mod requests;

pub use requests::{
    ExpectedMarginReq, ExpectedMarginRes, GetDynamicLeverageReq, GetDynamicLeverageRes,
    MarginCallListRes, MarginCallUpdateReq,
};

mod client;
pub use client::MarginClient;
