//! Connection configuration shared by [`crate::local::LocalClient`] and
//! [`crate::remote::RemoteClient`].

use secrecy::SecretString;
use std::time::Duration;

use crate::retry::RetryPolicy;

/// Endpoint and authentication settings for a single cTrader MCP session.
///
/// Both the Local server (bound to the cTrader Desktop application, enabled and
/// configured from cTrader Desktop's own Advanced -> MCP Server settings page, which
/// defaults to `http://127.0.0.1:9876/mcp/`: the port shown there is user-changeable, so
/// do not assume 9876 without reading it back from that page) and the Remote server
/// (`rest-proxy`, typically `https://mcp.ctrader.com/trading/mcp` or a self-hosted
/// equivalent) speak the same streamable-HTTP + SSE MCP transport, so they share this
/// configuration shape. Local does not require a bearer token by default (leave
/// [`Self::bearer_token`] unset); Remote does.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// The MCP endpoint URI, e.g. `"http://127.0.0.1:9876/mcp/"` (Local, default port) or
    /// `"https://mcp.ctrader.com/trading/mcp"` (Remote).
    pub uri: String,

    /// The bearer token sent as `Authorization: Bearer <token>`, if the endpoint requires
    /// authentication. Kept in a secret wrapper so `Debug` output redacts it. Use
    /// [`Self::with_bearer_token`] to set this.
    ///
    /// Stored as the bare token, NOT the full header value: [`crate::transport`] hands
    /// this to `rmcp`'s `StreamableHttpClientTransportConfig::auth_header`, which itself
    /// adds the `Bearer ` prefix (via reqwest's `bearer_auth`). Prefixing it here too
    /// would send `Authorization: Bearer Bearer <token>`, which cTrader's remote MCP
    /// endpoint rejects with `AuthRequired(invalid_token)`.
    pub bearer_token: Option<SecretString>,

    /// Timeout applied to each individual control request (initialize, `tools/call`,
    /// etc.). Does not bound long-lived SSE streams. Defaults to 30 seconds: generous
    /// enough for Remote's historical endpoints under the 5 req/s rate limit, but still
    /// bounded so a hung connection surfaces as an error rather than hanging a workflow
    /// indefinitely.
    pub control_request_timeout: Duration,

    /// Timeout allowed for session recovery after a dropped SSE stream. Defaults to 10
    /// seconds.
    pub session_recovery_timeout: Duration,

    /// How [`crate::transport::McpSession::connect`] and
    /// [`crate::transport::McpSession::call_idempotent`],
    /// `call_no_args_idempotent`, and `call_raw_idempotent`
    /// retry on transient failures. Defaults to [`RetryPolicy::default`]. Never applied
    /// to a mutating `tools/call`: see [`crate::retry`]'s module doc comment for why.
    pub retry_policy: RetryPolicy,
}

impl ConnectionConfig {
    /// Creates a configuration pointed at `uri` with no authentication and the crate's
    /// default timeouts.
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            bearer_token: None,
            control_request_timeout: Duration::from_secs(30),
            session_recovery_timeout: Duration::from_secs(10),
            retry_policy: RetryPolicy::default(),
        }
    }

    /// Sets `Authorization: Bearer <token>` for every request on this session.
    #[must_use]
    pub fn with_bearer_token(mut self, token: impl Into<SecretString>) -> Self {
        self.bearer_token = Some(token.into());
        self
    }

    /// Overrides the per-request control timeout.
    #[must_use]
    pub fn with_control_request_timeout(mut self, timeout: Duration) -> Self {
        self.control_request_timeout = timeout;
        self
    }

    /// Overrides the session-recovery timeout.
    #[must_use]
    pub fn with_session_recovery_timeout(mut self, timeout: Duration) -> Self {
        self.session_recovery_timeout = timeout;
        self
    }

    /// Overrides the retry policy (e.g. [`RetryPolicy::none`] for tests that want
    /// deterministic, immediate failure).
    #[must_use]
    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_bearer_token() {
        let config = ConnectionConfig::new("https://example.test/mcp")
            .with_bearer_token("test-secret-token");
        let debug = format!("{config:?}");
        assert!(!debug.contains("test-secret-token"));
        assert!(debug.contains("bearer_token"));
    }
}
