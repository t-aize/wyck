//! The connection layer: the WebSocket itself, the envelope it carries, the rate limiter, and the
//! plain messages that sign the application and an account in.
//!
//! Everything above this module (the sub-clients under [`crate::infra::ctrader::market`], [`crate::infra::ctrader::account`],
//! [`crate::infra::ctrader::trading`] and [`crate::infra::ctrader::margin`]) is built on [`crate::infra::ctrader::Client`] and its
//! `pub(crate)` request machinery; nothing outside this crate needs to reach lower than
//! [`crate::infra::ctrader::Client`] itself.

pub mod connection;
pub mod messages;
pub mod rate_limit;
pub mod wire;
