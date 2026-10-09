//! A group import of the pieces most programs need. Purely additive: everything here is also
//! reachable by its own module path.
//!
//! ```
//! use wyck::infra::ctrader::prelude::*;
//! ```

pub use crate::domain::market::{Period, QuoteType};
pub use crate::domain::trading::TradeSide;
pub use crate::infra::ctrader::config::{ClientCredentials, ConnectionConfig, Environment};
pub use crate::infra::ctrader::session::{Session, SessionConfig, SessionEvent};
pub use crate::infra::ctrader::trading::NewOrderReq;
pub use crate::infra::ctrader::{
    AccountClient, Client, ClientBuilder, ConnectionState, DisconnectReason, Error, ErrorKind,
    Event, Result,
};
