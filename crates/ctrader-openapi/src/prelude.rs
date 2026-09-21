//! A group import of the pieces most programs need. Purely additive: everything here is also
//! reachable by its own module path.
//!
//! ```
//! use ctrader_openapi::prelude::*;
//! ```

pub use crate::account::TradeSide;
pub use crate::config::{ClientCredentials, ConnectionConfig, Environment};
pub use crate::market::{Period, QuoteType};
pub use crate::session::{Session, SessionConfig, SessionEvent};
pub use crate::trading::NewOrderReq;
pub use crate::{
    AccountClient, Client, ClientBuilder, ConnectionState, DisconnectReason, ErrorKind, Event,
    OpenApiError, Result,
};
