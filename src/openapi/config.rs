//! Where to connect, and with which credentials.
//!
//! The Open API has two separate worlds, **demo** and **live**, each behind its own host. An
//! application (and so a connection) works in one of them: accounts of the other kind cannot be
//! authorized on it, and an app that needs both opens two connections.
//!
//! This crate speaks **JSON over a WebSocket** (port `5036`). It needs no code generation and no
//! `.proto` files, and the messages are the same ones as in Protobuf (see [`crate::openapi::transport::wire`]).

use std::time::Duration;

use secrecy::SecretString;

/// The port of the JSON endpoint. TCP and WebSocket both work on it; this crate uses WebSocket.
pub const JSON_PORT: u16 = 5036;

/// Demo or live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Environment {
    /// Demo accounts: `demo.ctraderapi.com`.
    Demo,
    /// Live accounts: `live.ctraderapi.com`.
    Live,
}

impl Environment {
    /// The host of the environment.
    #[must_use]
    pub fn host(self) -> &'static str {
        match self {
            Self::Demo => "demo.ctraderapi.com",
            Self::Live => "live.ctraderapi.com",
        }
    }

    /// The secure WebSocket address of the JSON endpoint.
    #[must_use]
    pub fn url(self) -> String {
        format!("wss://{}:{JSON_PORT}", self.host())
    }
}

/// How the connection behaves. [`ConnectionConfig::new`] gives the settings the documentation
/// asks for; every field can be changed.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// The WebSocket address to connect to (`wss://...`, or `ws://...` for a local test server).
    pub url: String,
    /// How long the connection attempt may take.
    pub connect_timeout: Duration,
    /// How long a request waits for its answer.
    pub request_timeout: Duration,
    /// How often a heartbeat is sent. The server drops a connection that is silent for more
    /// than 10 seconds, so keep it well under that.
    pub heartbeat_interval: Duration,
    /// How many events may wait unread before the slowest reader starts to lose the oldest.
    pub event_capacity: usize,
    /// Requests per second for everything but history. The documented limit is 50; the default is
    /// 40, a margin under it so that requests spaced evenly cannot add up to more than 50 inside a
    /// window the server counts differently.
    pub standard_rate: u32,
    /// Requests per second for history. The documented limit is 5; the default is 4, for the same
    /// reason, and because a live run got a tick request blocked (`BLOCKED_PAYLOAD_TYPE`) after a
    /// burst of tick pages spaced at 5 per second.
    pub historical_rate: u32,
    /// How many times a request refused for its rate is sent again, after the wait the server asks
    /// for. 0 turns it off: the error is then returned at once.
    pub rate_limit_retries: u32,
    /// The longest wait before such a retry, whatever the server asks for.
    pub max_retry_wait: Duration,
}

impl ConnectionConfig {
    /// The settings for `environment`.
    #[must_use]
    pub fn new(environment: Environment) -> Self {
        Self::with_url(environment.url())
    }

    /// The settings for an explicit address, for a test server or a proxy.
    #[must_use]
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(5),
            event_capacity: 8192,
            standard_rate: 40,
            historical_rate: 4,
            rate_limit_retries: 3,
            max_retry_wait: Duration::from_secs(30),
        }
    }

    /// Checks the settings, so a mistake shows at connect time and not as a strange failure later.
    ///
    /// # Errors
    ///
    /// [`crate::openapi::OpenApiError::Config`] when the address is not a WebSocket URL, a rate is zero,
    /// or the heartbeat is not under the server's 10 second silence limit.
    pub fn validate(&self) -> crate::openapi::Result<()> {
        let bad = |what: &str| Err(crate::openapi::OpenApiError::Config(what.to_owned()));
        if !(self.url.starts_with("wss://") || self.url.starts_with("ws://")) {
            return bad("the address must start with wss:// or ws://");
        }
        if self.standard_rate == 0 || self.historical_rate == 0 {
            return bad("a request rate must be at least 1 per second");
        }
        if self.connect_timeout.is_zero() || self.request_timeout.is_zero() {
            return bad("connection and request timeouts must be positive");
        }
        if self.max_retry_wait < Duration::from_millis(250) {
            return bad("the maximum rate-limit retry wait must be at least 250 ms");
        }
        if self.heartbeat_interval.is_zero() || self.heartbeat_interval >= Duration::from_secs(10) {
            return bad("the heartbeat interval must be under 10 seconds");
        }
        if self.event_capacity == 0 {
            return bad("the event capacity must be at least 1");
        }
        Ok(())
    }
}

/// The credentials of a registered application: what the user gets from the Open API portal.
///
/// The secret is a [`SecretString`]: it is never shown by `Debug` and is wiped from memory when
/// dropped. Keep it out of files, logs and the repository.
#[derive(Debug, Clone)]
pub struct ClientCredentials {
    /// The application's client id.
    pub client_id: String,
    /// The application's client secret.
    pub client_secret: SecretString,
}

impl ClientCredentials {
    /// Credentials from an id and a secret.
    #[must_use]
    pub fn new(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: SecretString::from(client_secret.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environments_have_their_own_host_and_the_json_port() {
        assert_eq!(Environment::Demo.url(), "wss://demo.ctraderapi.com:5036");
        assert_eq!(Environment::Live.url(), "wss://live.ctraderapi.com:5036");
    }

    #[test]
    fn the_defaults_stay_under_the_documented_limits_and_validate() {
        let config = ConnectionConfig::new(Environment::Demo);
        assert!(config.standard_rate <= 50 && config.standard_rate > 0);
        assert!(config.historical_rate <= 5 && config.historical_rate > 0);
        assert!(config.rate_limit_retries > 0);
        assert!(config.heartbeat_interval < Duration::from_secs(10));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn bad_settings_are_refused_with_a_reason() {
        let mut config = ConnectionConfig::with_url("https://demo.ctraderapi.com");
        assert!(config.validate().is_err(), "not a websocket address");
        config = ConnectionConfig::with_url("ws://127.0.0.1:1");
        config.historical_rate = 0;
        assert!(config.validate().is_err());
        config = ConnectionConfig::with_url("ws://127.0.0.1:1");
        config.heartbeat_interval = Duration::from_secs(10);
        assert!(config.validate().is_err(), "too slow for the server");
        config.heartbeat_interval = Duration::ZERO;
        assert!(config.validate().is_err());
        config = ConnectionConfig::with_url("ws://127.0.0.1:1");
        config.max_retry_wait = Duration::ZERO;
        assert!(config.validate().is_err());
        config = ConnectionConfig::with_url("ws://127.0.0.1:1");
        config.request_timeout = Duration::ZERO;
        assert!(config.validate().is_err());
    }

    #[test]
    fn the_secret_never_shows_in_debug() {
        let credentials = ClientCredentials::new("id-123", "super-secret-value");
        let shown = format!("{credentials:?}");
        assert!(shown.contains("id-123"));
        assert!(!shown.contains("super-secret-value"));
    }
}
