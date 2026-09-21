//! The connection layer: the WebSocket itself, the envelope it carries, the rate limiter, and the
//! plain messages that sign the application and an account in.
//!
//! Everything above this module (the sub-clients under [`crate::market`], [`crate::account`],
//! [`crate::trading`] and [`crate::margin`]) is built on [`connection::Client`] and its
//! `pub(crate)` request machinery; nothing outside this crate needs to reach lower than
//! [`connection::Client`] itself.

pub mod connection;
pub mod messages;
pub mod rate_limit;
pub mod wire;
