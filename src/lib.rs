//! # wyck
//!
//! A terminal trading panel for cTrader.
//!
//! - [`config`]: centralized application config and encrypted credential storage.
//! - [`openapi`]: a client for the cTrader Open API, over its JSON WebSocket.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod config;
pub mod openapi;
