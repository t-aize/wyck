//! cTrader Open API models and codecs without I/O.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Account and position messages.
pub mod account;
/// Errors shared with the client.
pub mod error;
/// Margin messages.
pub mod margin;
/// Market data and prices.
pub mod market;
/// Trading requests and events.
pub mod trading;
/// Wire envelopes and connection messages.
pub mod transport;

pub(crate) use account::types::number_enum;
pub use error::{ErrorKind, OpenApiError, Result};
