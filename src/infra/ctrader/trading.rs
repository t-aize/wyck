//! Placing, amending and cancelling orders, and closing positions, through [`TradingClient`] (see
//! [`crate::infra::ctrader::AccountClient::trading`]).
//!
//! These calls move money, simulated on a demo account but real on a live one, and they need a
//! token of the `trading` [`crate::infra::ctrader::auth::Scope`] (an `accounts` token is refused). Everything else
//! in this crate only reads; this module is the one place that writes.
//!
//! # Non-idempotency
//!
//! [`TradingClient::new_order`] and its siblings are not guaranteed idempotent. If a request times
//! out ([`crate::infra::ctrader::Error::Timeout`]) or the connection drops while it is in flight
//! ([`crate::infra::ctrader::Error::Closed`]), the order may still have reached the server: the answer was
//! lost, not necessarily the request. Sending the same order again on a bare timeout can double a
//! position. Instead, check what the account actually holds first:
//! [`crate::infra::ctrader::account::AccountDataClient::open_positions_and_orders`] (`ProtoOAReconcileReq`) for
//! what is open now, or [`crate::infra::ctrader::account::AccountDataClient::deals`] for what was executed
//! recently, using `label` or `client_order_id` to recognize the order you sent. This crate stays a
//! thin typed client, like Spotware's own SDKs (`OpenApiPy`, `OpenAPI.Net`): it does not add retry
//! or idempotency machinery of its own on top of trading calls.
//!
//! # Units
//!
//! Volumes are hundredths of a unit, like the read-only account calls (see
//! [`crate::domain::trading::volume_units`]). Prices (limit, stop, stop loss, take profit) are ordinary
//! decimals, like the prices on [`crate::domain::trading::Position`] and [`crate::domain::trading::Order`], unlike
//! the integer, [`crate::domain::market::PRICE_SCALE`] prices of ticks and bars.

pub mod requests;

pub use requests::{
    AmendOrderReq, AmendPositionSlTpReq, CancelOrderReq, ClosePositionReq, NewOrderReq,
    NewOrderType,
};

mod client;
pub use client::TradingClient;
