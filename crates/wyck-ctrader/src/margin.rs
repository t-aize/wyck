//! Margin, through [`MarginClient`] (see [`crate::AccountClient::margin`]): the expected cost of
//! an order before sending it, margin call thresholds, and dynamic leverage tiers.
//!
//! Reading these needs no trading permission; [`MarginClient::update_margin_call`] changes a
//! setting on the account and, like the trading calls, needs a token of the `trading`
//! [`crate::auth::Scope`].

mod requests;

pub(crate) use requests::ExpectedMarginRes;

mod client;
pub(crate) use client::MarginClient;
