//! # wyck-engine
//!
//! The headless trading core of wyck. Work in progress: see `docs/ARCHITECTURE.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod broker;
pub mod config;
pub mod domain;
mod error;
mod ids;

pub use config::EngineConfig;
pub use error::{BrokerErrorKind, EngineError, ErrorKind, Result};
pub use ids::{AccountId, CommandId, OrderId, PositionId, Revision};
