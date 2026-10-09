//! Placing, amending and cancelling orders, and closing positions, through [`TradingClient`] (see
//! [`crate::openapi::AccountClient::trading`]).
//!
//! These calls move money, simulated on a demo account but real on a live one, and they need a
//! token of the `trading` [`crate::openapi::auth::Scope`] (an `accounts` token is refused). Everything else
//! in this crate only reads; this module is the one place that writes.
//!
//! # Non-idempotency
//!
//! [`TradingClient::new_order`] and its siblings are not guaranteed idempotent. If a request times
//! out ([`crate::openapi::Error::Timeout`]) or the connection drops while it is in flight
//! ([`crate::openapi::Error::Closed`]), the order may still have reached the server: the answer was
//! lost, not necessarily the request. Sending the same order again on a bare timeout can double a
//! position. Instead, check what the account actually holds first:
//! [`crate::openapi::account::AccountDataClient::open_positions_and_orders`] (`ProtoOAReconcileReq`) for
//! what is open now, or [`crate::openapi::account::AccountDataClient::deals`] for what was executed
//! recently, using `label` or `client_order_id` to recognize the order you sent. This crate stays a
//! thin typed client, like Spotware's own SDKs (`OpenApiPy`, `OpenAPI.Net`): it does not add retry
//! or idempotency machinery of its own on top of trading calls.
//!
//! # Units
//!
//! Volumes are hundredths of a unit, like the read-only account calls (see
//! [`crate::openapi::account::volume_units`]). Prices (limit, stop, stop loss, take profit) are ordinary
//! decimals, like the prices on [`crate::openapi::account::Position`] and [`crate::openapi::account::Order`], unlike
//! the integer, [`crate::openapi::market::PRICE_SCALE`] prices of ticks and bars.

pub mod contract;
pub mod events;
pub mod requests;

pub use events::{ExecutionEvent, ExecutionType, OrderErrorEvent, TrailingSlChangedEvent};
pub use requests::{
    AmendOrderReq, AmendPositionSlTpReq, CancelOrderReq, ClosePositionReq, NewOrderReq,
    NewOrderType,
};

mod client;
pub use client::TradingClient;
