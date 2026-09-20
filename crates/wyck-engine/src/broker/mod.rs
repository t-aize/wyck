//! The broker port: everything the engine needs from a trading server, in the engine's own
//! normalized terms.
//!
//! The engine never talks to `ctrader-mcp` directly. It talks to a [`Broker`], and the
//! adapters ([`RemoteBroker`], [`LocalBroker`], and the in-memory `MockBroker` behind the
//! `testing` feature) translate between this interface and each server's dialect: Remote's
//! integer pipettes and `symbolId`s, Local's floating point prices and ticker names, the
//! different ways each one expresses stop loss and take profit, and so on.
//!
//! # Contract
//!
//! Every adapter must satisfy the same behavior, exercised by one shared contract test
//! suite (`tests/broker_contract.rs`):
//!
//! - reads never mutate anything and may be repeated freely
//! - prices are display prices, volumes are [`Volume`] units, money is in account currency
//! - a mutating call is attempted **once**: adapters never retry it (a lost reply after the
//!   request reached the server must surface as an error, not be replayed)
//! - errors are [`EngineError`]s, never panics; secrets never appear in error text

mod local;
#[cfg(feature = "testing")]
mod mock;
mod remote;

use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use wyck_config::{ProfileId, WyckConfig};

pub use local::LocalBroker;
#[cfg(feature = "testing")]
pub use mock::{MockBroker, MockOp};
pub use remote::RemoteBroker;

use crate::config::AssumedSpecs;
use crate::domain::{
    AccountSnapshot, Instrument, PendingOrder, Position, Quote, Side, SymbolInfo, UnixMillis,
    Volume,
};
use crate::error::{EngineError, Result};
use crate::ids::{AccountId, OrderId, PositionId};

/// The default Remote endpoint.
pub const DEFAULT_REMOTE_ENDPOINT: &str = "https://mcp.ctrader.com/trading/mcp";
/// The default Local endpoint (the port is configurable in cTrader Desktop).
pub const DEFAULT_LOCAL_ENDPOINT: &str = "http://127.0.0.1:9876/mcp/";

/// Which kind of server a connection targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ServiceKind {
    /// cTrader's cloud MCP server (`mcp.ctrader.com`), bearer-token authenticated.
    CtraderRemote,
    /// The MCP server inside a running cTrader Desktop, no token.
    CtraderLocal,
}

impl ServiceKind {
    /// Parses a profile's `service` tag (`"ctrader-remote"`, `"ctrader-local"`).
    #[must_use]
    pub fn from_profile_tag(tag: &str) -> Option<Self> {
        match tag.trim().to_ascii_lowercase().as_str() {
            "ctrader-remote" => Some(Self::CtraderRemote),
            "ctrader-local" => Some(Self::CtraderLocal),
            _ => None,
        }
    }

    /// The endpoint used when a profile does not name one.
    #[must_use]
    pub fn default_endpoint(self) -> &'static str {
        match self {
            Self::CtraderRemote => DEFAULT_REMOTE_ENDPOINT,
            Self::CtraderLocal => DEFAULT_LOCAL_ENDPOINT,
        }
    }
}

/// What to connect to.
///
/// `Debug` never prints the token (a [`SecretString`] redacts itself).
#[derive(Debug, Clone)]
pub struct ConnectRequest {
    /// Remote or Local.
    pub service: ServiceKind,
    /// The server URL.
    pub endpoint: String,
    /// The bearer token, for services that need one.
    pub token: Option<SecretString>,
    /// A label for logs and the UI (the profile's display name, typically).
    pub label: String,
}

impl ConnectRequest {
    /// A request built by hand.
    #[must_use]
    pub fn new(
        service: ServiceKind,
        endpoint: impl Into<String>,
        token: Option<SecretString>,
    ) -> Self {
        let endpoint = endpoint.into();
        Self {
            service,
            label: endpoint.clone(),
            endpoint,
            token,
        }
    }

    /// Builds a request from a stored profile: its service tag picks the server family, a
    /// missing endpoint falls back to the service default, and the token comes from the
    /// secret store.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] for an unknown profile or an unrecognized service tag, and
    /// [`EngineError::Internal`] if the secret store fails.
    pub fn from_profile(config: &WyckConfig, id: &ProfileId) -> Result<Self> {
        let profile = config
            .profile(id)
            .ok_or_else(|| EngineError::Invalid(format!("no profile with id `{}`", id.as_str())))?;
        let service = ServiceKind::from_profile_tag(&profile.service).ok_or_else(|| {
            EngineError::Invalid(format!(
                "profile `{}` has unsupported service `{}`",
                profile.display_name, profile.service
            ))
        })?;
        let token = config
            .token_for(id)
            .map_err(|e| EngineError::Internal(format!("could not read the stored token: {e}")))?;
        Ok(Self {
            service,
            endpoint: profile
                .endpoint
                .clone()
                .unwrap_or_else(|| service.default_endpoint().to_owned()),
            token,
            label: profile.display_name.clone(),
        })
    }
}

/// A market order, in broker-neutral terms.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketOrder {
    /// The instrument's ticker.
    pub symbol: String,
    /// Buy or sell.
    pub side: Side,
    /// Volume, already validated against the instrument's rules.
    pub volume: Volume,
    /// Stop loss as a price distance from the entry (positive), if any.
    pub stop_loss_distance: Option<f64>,
    /// Take profit as a price distance from the entry (positive), if any.
    pub take_profit_distance: Option<f64>,
    /// Idempotency label attached to the order so it can be recognized afterwards.
    pub label: String,
    /// Slippage tolerance in points, where the server supports one.
    pub slippage_points: Option<i64>,
}

/// What a broker reported back for a submitted order.
///
/// Never proof of a fill: the pipeline confirms by re-reading positions.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlacedOrder {
    /// The position the order opened, when the server said so synchronously.
    pub position_id: Option<PositionId>,
    /// The order id, when the server reported one.
    pub order_id: Option<OrderId>,
}

/// Which broker call an operation is, for logging and the mock's failure injection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BrokerCall {
    /// `account`
    Account,
    /// `symbols`
    Symbols,
    /// `instrument`
    Instrument,
    /// `positions`
    Positions,
    /// `pending_orders`
    PendingOrders,
    /// `quotes`
    Quotes,
    /// `server_time`
    ServerTime,
    /// `ping`
    Ping,
    /// `place_market`
    PlaceMarket,
    /// `set_protection`
    SetProtection,
    /// `close_position`
    ClosePosition,
    /// `cancel_order`
    CancelOrder,
}

/// A trading server connection, in the engine's normalized terms. See the
/// [module docs](self) for the contract every implementation must meet.
#[async_trait]
pub trait Broker: Send + Sync + 'static {
    /// Which server family this is.
    fn service(&self) -> ServiceKind;

    /// The account this connection is bound to.
    fn account_id(&self) -> &AccountId;

    /// Whether this connection is allowed to place orders. `false` for a read-only
    /// (data profile) Remote session. Local cannot tell in advance and reports `true`.
    fn can_trade(&self) -> bool;

    /// The smallest price distance this server can express for a stop loss or take profit
    /// on `instrument`. The planner rounds stop distances up to a multiple of it, so what
    /// is sized is exactly what will be sent.
    fn stop_granularity(&self, instrument: &Instrument) -> f64;

    /// The prefix the engine should put on order labels for this session, when the server
    /// provides one (Remote does).
    fn label_prefix(&self) -> String;

    /// A fresh snapshot of the account.
    async fn account(&self) -> Result<AccountSnapshot>;

    /// Every symbol name this session can trade. Cheap: names only.
    async fn symbols(&self) -> Result<Vec<String>>;

    /// Every symbol this session can trade, with what the broker says about each: description,
    /// asset class, category, currencies. One call, no per-symbol details. The default knows
    /// names only.
    async fn catalog(&self) -> Result<Vec<SymbolInfo>> {
        Ok(self
            .symbols()
            .await?
            .into_iter()
            .map(SymbolInfo::named)
            .collect())
    }

    /// Full details for one symbol. Remote answers from its session cache; Local fetches
    /// on first use and caches, since listing details for every symbol would cost one call
    /// each.
    async fn instrument(&self, symbol: &str) -> Result<Instrument>;

    /// Open positions.
    async fn positions(&self) -> Result<Vec<Position>>;

    /// Working orders.
    async fn pending_orders(&self) -> Result<Vec<PendingOrder>>;

    /// Current quotes for `symbols`. A symbol with no quote is simply absent from the
    /// result (not an error): Remote in particular can empty a whole batch for one unknown
    /// id, and adapters must guard against that.
    async fn quotes(&self, symbols: &[String]) -> Result<Vec<Quote>>;

    /// The server's clock in Unix milliseconds.
    async fn server_time(&self) -> Result<UnixMillis>;

    /// A protocol-level liveness check.
    async fn ping(&self) -> Result<()>;

    /// Places a market order. Attempted once, never retried.
    async fn place_market(&self, order: &MarketOrder) -> Result<PlacedOrder>;

    /// Sets or changes a position's stop loss and take profit (absolute prices). A `None`
    /// leg is left as it is: adapters must not clear a protective leg the caller did not
    /// mention.
    async fn set_protection(
        &self,
        position: &Position,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) -> Result<()>;

    /// Closes `volume` of a position, or all of it when `None`.
    async fn close_position(&self, position: &Position, volume: Option<Volume>) -> Result<()>;

    /// Cancels a working order.
    async fn cancel_order(&self, order: OrderId) -> Result<()>;

    /// Ends the session. Idempotent.
    async fn close(&self) -> Result<()>;
}

/// Creates broker connections. The engine holds one so it can reconnect; tests substitute
/// their own.
#[async_trait]
pub trait Connector: Send + Sync + 'static {
    /// Connects and bootstraps a session for `request`.
    async fn connect(&self, request: &ConnectRequest) -> Result<Arc<dyn Broker>>;
}

/// The production [`Connector`]: real cTrader MCP connections.
#[derive(Debug, Clone, Default)]
pub struct CtraderConnector {
    assumed_specs: AssumedSpecs,
}

impl CtraderConnector {
    /// A connector using `assumed_specs` for symbols whose volume rules the server does not
    /// publish.
    #[must_use]
    pub fn new(assumed_specs: AssumedSpecs) -> Self {
        Self { assumed_specs }
    }
}

#[async_trait]
impl Connector for CtraderConnector {
    async fn connect(&self, request: &ConnectRequest) -> Result<Arc<dyn Broker>> {
        match request.service {
            ServiceKind::CtraderRemote => Ok(Arc::new(
                RemoteBroker::connect(request, &self.assumed_specs).await?,
            )),
            ServiceKind::CtraderLocal => Ok(Arc::new(
                LocalBroker::connect(request, &self.assumed_specs).await?,
            )),
        }
    }
}

/// Reads a server time out of the JSON a server returns.
///
/// Local's `get_server_time` answers `{"unixMs": 1789839318399, "utcTime":
/// "2026-09-19T17:35:18.399Z", "localTime": "2026-09-19T19:35:18.399+02:00"}` (checked
/// against a live server, 2026-09). Remote has no such tool. The field names tried are the
/// live ones first, then a few plausible others; each may hold epoch milliseconds, epoch
/// seconds or an RFC 3339 string. Returns `None` when nothing recognizable is present.
pub(crate) fn parse_server_time(value: &serde_json::Value) -> Option<UnixMillis> {
    for key in [
        "unixMs",
        "utcTime",
        "timestamp",
        "serverTime",
        "server_time",
        "time",
        "utc",
        "now",
    ] {
        let Some(field) = value.get(key) else {
            continue;
        };
        if let Some(n) = field.as_i64() {
            // Seconds since the epoch are below ~1e11; milliseconds are above.
            return Some(if n.abs() < 100_000_000_000 {
                n * 1000
            } else {
                n
            });
        }
        if let Some(text) = field.as_str()
            && let Ok(ms) = ctrader_mcp::time::local_iso8601_to_epoch_millis(text)
        {
            return Some(ms);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn service_tags_and_defaults() {
        assert_eq!(
            ServiceKind::from_profile_tag("ctrader-remote"),
            Some(ServiceKind::CtraderRemote)
        );
        assert_eq!(
            ServiceKind::from_profile_tag(" CTRADER-LOCAL "),
            Some(ServiceKind::CtraderLocal)
        );
        assert_eq!(ServiceKind::from_profile_tag("other"), None);
        assert!(
            ServiceKind::CtraderRemote
                .default_endpoint()
                .starts_with("https://")
        );
        assert!(
            ServiceKind::CtraderLocal
                .default_endpoint()
                .starts_with("http://127.0.0.1")
        );
    }

    #[test]
    fn connect_request_debug_never_shows_the_token() {
        let request = ConnectRequest::new(
            ServiceKind::CtraderRemote,
            "https://example.invalid/mcp",
            Some(SecretString::from("super-secret-token".to_owned())),
        );
        assert!(!format!("{request:?}").contains("super-secret-token"));
    }

    #[test]
    fn the_live_local_server_time_shape() {
        let live = json!({
            "localTime": "2026-09-19T19:35:18.3991894+02:00",
            "unixMs": 1_789_839_318_399_i64,
            "utcTime": "2026-09-19T17:35:18.3991893Z"
        });
        assert_eq!(parse_server_time(&live), Some(1_789_839_318_399));
        // With no `unixMs`, the UTC string is enough (fraction digits beyond ms are dropped).
        let text_only = json!({ "utcTime": "2026-09-19T17:35:18.3991893Z" });
        assert_eq!(parse_server_time(&text_only), Some(1_789_839_318_399));
    }

    #[test]
    fn server_time_shapes() {
        assert_eq!(
            parse_server_time(&json!({"timestamp": 1_700_000_000_123_i64})),
            Some(1_700_000_000_123)
        );
        assert_eq!(
            parse_server_time(&json!({"serverTime": 1_700_000_000})),
            Some(1_700_000_000_000)
        );
        assert_eq!(
            parse_server_time(&json!({"time": "2026-09-14T12:30:00Z"})),
            Some(1_789_389_000_000)
        );
        assert_eq!(parse_server_time(&json!({"nothing": 1})), None);
        assert_eq!(parse_server_time(&json!("x")), None);
    }
}
