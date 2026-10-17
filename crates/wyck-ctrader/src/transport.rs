//! The wire under the connection: the envelope a message travels in, the rate limiter, and the
//! plain messages that sign the application and an account in. The connection itself (the
//! WebSocket and its task) is [`crate::connection`], above this module.
//!
//! Everything above this module (the sub-clients under [`crate::market`], [`crate::account`],
//! [`crate::trading`] and [`crate::margin`]) is built on [`crate::Client`] and its
//! `pub(crate)` request machinery; nothing outside this crate needs to reach lower than
//! [`crate::Client`] itself.

pub(crate) mod messages;
pub(crate) mod rate_limit;
pub(crate) mod wire;
