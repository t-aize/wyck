//! cTrader Open API: messages, client, session, OAuth and trading calculations.

pub mod account;
pub mod auth;
pub mod config;
pub mod error;
pub mod event;
mod handle;
pub mod margin;
pub mod market;
pub mod prelude;
pub mod session;
pub mod trading;
/// The wire format and the connection machinery.
#[doc(hidden)]
pub mod transport;

pub(crate) use account::types::number_enum;
pub use config::{ClientCredentials, ConnectionConfig, Environment};
pub use error::{Error, ErrorKind, Result};
pub use event::{DisconnectReason, Event};
pub use handle::AccountClient;
pub use transport::connection::{Client, ClientBuilder, ConnectionState};
pub use transport::messages::{AccountsRes, CtidProfile, RefreshTokenRes, TraderAccount};
