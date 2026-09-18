//! Connection configuration shared by [`crate::local::LocalClient`] and
//! [`crate::remote::RemoteClient`].

use std::time::Duration;

/// Endpoint and authentication settings for a single cTrader MCP session.
///
/// Both the Local server (bound to the cTrader Desktop application, typically
/// `http://127.0.0.1:<port>/mcp`) and the Remote server (`rest-proxy`, typically
/// `https://mcp.spotware.com/mcp` or a self-hosted equivalent) speak the same
/// streamable-HTTP + SSE MCP transport, so they share this configuration shape.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// The MCP endpoint URI, e.g. `"http://127.0.0.1:9000/mcp"` (Local) or
    /// `"https://mcp.spotware.com/mcp"` (Remote).
    pub uri: String,

    /// The full `Authorization` header value, if the endpoint requires one
    /// (e.g. `"Bearer <token>"`). Use [`Self::with_bearer_token`] to set this from a
    /// bare token.
    pub auth_header: Option<String>,

    /// Timeout applied to each individual control request (initialize, `tools/call`,
    /// etc.). Does not bound long-lived SSE streams. Defaults to 30 seconds — generous
    /// enough for Remote's historical endpoints under the 5 req/s rate limit, but still
    /// bounded so a hung connection surfaces as an error rather than hanging a workflow
    /// indefinitely.
    pub control_request_timeout: Duration,

    /// Timeout allowed for session recovery after a dropped SSE stream. Defaults to 10
    /// seconds.
    pub session_recovery_timeout: Duration,
}

impl ConnectionConfig {
    /// Creates a configuration pointed at `uri` with no authentication and the crate's
    /// default timeouts.
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            auth_header: None,
            control_request_timeout: Duration::from_secs(30),
            session_recovery_timeout: Duration::from_secs(10),
        }
    }

    /// Sets `Authorization: Bearer <token>` for every request on this session.
    #[must_use]
    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth_header = Some(format!("Bearer {}", token.into()));
        self
    }

    /// Sets a raw `Authorization` header value (use this if the endpoint expects a
    /// non-`Bearer` scheme).
    #[must_use]
    pub fn with_auth_header(mut self, value: impl Into<String>) -> Self {
        self.auth_header = Some(value.into());
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
}
