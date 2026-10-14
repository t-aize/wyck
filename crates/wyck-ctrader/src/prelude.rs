//! A group import of the pieces most programs need. Purely additive: everything here is also
//! reachable by its own module path.
//!
//! ```
//! use wyck_ctrader::prelude::*;
//! ```

pub use crate::config::{ClientCredentials, ConnectionConfig, Environment};
pub use crate::session::{Session, SessionConfig, SessionEvent};
pub use crate::trading::NewOrderReq;
pub use crate::{
    AccountClient, Client, ClientBuilder, ConnectionState, DisconnectReason, Error, ErrorKind,
    Event, Result,
};
pub use wyck_market::{Period, QuoteType};
pub use wyck_trading::TradeSide;
