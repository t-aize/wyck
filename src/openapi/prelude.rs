//! A group import of the pieces most programs need. Purely additive: everything here is also
//! reachable by its own module path.
//!
//! ```
//! use wyck::openapi::prelude::*;
//! ```

pub use crate::openapi::account::TradeSide;
pub use crate::openapi::config::{ClientCredentials, ConnectionConfig, Environment};
pub use crate::openapi::market::{Period, QuoteType};
pub use crate::openapi::session::{Session, SessionConfig, SessionEvent};
pub use crate::openapi::trading::NewOrderReq;
pub use crate::openapi::{
    AccountClient, Client, ClientBuilder, ConnectionState, DisconnectReason, ErrorKind, Event,
    OpenApiError, Result,
};
