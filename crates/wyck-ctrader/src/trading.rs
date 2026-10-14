//! Placing, amending and cancelling orders, and closing positions, through [`TradingClient`] (see
//! [`crate::AccountClient::trading`]).
//!
//! These calls move money, simulated on a demo account but real on a live one, and they need a
//! token of the `trading` [`crate::auth::Scope`] (an `accounts` token is refused). Everything else
//! in this crate only reads; this module is the one place that writes.
//!
//! # Non-idempotency
//!
//! [`TradingClient::new_order`] and its siblings are not guaranteed idempotent. If a request times
//! out ([`crate::Error::Timeout`]) or the connection drops while it is in flight
//! ([`crate::Error::Closed`]), the order may still have reached the server: the answer was
//! lost, not necessarily the request. Sending the same order again on a bare timeout can double a
//! position. Instead, check what the account actually holds first:
//! `crate::account::AccountDataClient::open_positions_and_orders` (`ProtoOAReconcileReq`) for
//! what is open now, or `crate::account::AccountDataClient::deals` for what was executed
//! recently, using `label` or `client_order_id` to recognize the order you sent. This crate stays a
//! thin typed client, like Spotware's own SDKs (`OpenApiPy`, `OpenAPI.Net`): it does not add retry
//! or idempotency machinery of its own on top of trading calls.
//!
//! # Units
//!
//! Volumes are hundredths of a unit, like the read-only account calls (see
//! [`wyck_trading::volume_units`]). Prices (limit, stop, stop loss, take profit) are ordinary
//! decimals, like the prices on [`wyck_trading::Position`] and [`wyck_trading::Order`], unlike
//! the integer, [`wyck_market::PRICE_SCALE`] prices of ticks and bars.

mod requests;

pub use requests::{AmendOrderReq, AmendPositionSlTpReq, NewOrderReq};
pub(crate) use requests::{CancelOrderReq, ClosePositionReq};

mod client;
pub use client::TradingClient;
