//! The one door from the screens to the broker connection.
//!
//! Screens do not name `infra` directly: they take the connection types from here. Where the
//! connection types become plain domain types (market data, orders, positions) they come from
//! `domain` instead.

pub use crate::infra::ctrader::*;
