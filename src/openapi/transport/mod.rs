//! The connection layer: the WebSocket itself, the envelope it carries, the rate limiter, and the
//! plain messages that sign the application and an account in.

pub mod connection;
pub mod messages;
pub mod rate_limit;
pub mod wire;
