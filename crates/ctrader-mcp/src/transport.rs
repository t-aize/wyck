//! The underlying [`rmcp`] session and the generic, typed `tools/call` helper shared by
//! [`crate::local::LocalClient`] and [`crate::remote::RemoteClient`].
//!
//! Neither server-specific client talks to [`rmcp`] directly, both hold an
//! [`McpSession`] and express every tool as a call to [`McpSession::call`],
//! [`McpSession::call_no_args`], or (for tools this crate does not yet model with a typed
//! DTO, see the per-category notes in [`crate::local`] and [`crate::remote`])
//! [`McpSession::call_raw`]. This keeps the encoding/decoding, error-classification
//! (`self-healing-playbook.md` §3), and schema-fields-only enforcement (§1.5, implicit
//! since only declared DTO fields are ever serialized) in exactly one place.

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, ClientRequest,
    Implementation, JsonObject, PingRequest, ProtocolVersion,
};
use rmcp::service::RunningService;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{ClientHandler, ClientLifecycleMode, ClientServiceExt, Peer, RoleClient};
use secrecy::ExposeSecret;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::config::ConnectionConfig;
use crate::error::CTraderError;
use crate::retry::{RetryPolicy, retry_with_backoff};

/// The [`rmcp::ClientHandler`] identity this crate presents to every cTrader MCP server
/// during the `initialize` handshake. It declines every server-initiated capability
/// (sampling, roots, elicitation) because this crate is a pure tool-calling client, not
/// an interactive agent host: cTrader's servers do not currently call back into the
/// client for any of those capabilities.
#[derive(Debug, Clone, Copy, Default)]
struct ClientIdentity;

impl ClientHandler for ClientIdentity {
    fn get_info(&self) -> ClientConfig {
        // `Implementation` is `#[non_exhaustive]`, so it cannot be built with a struct
        // literal from outside `rmcp`: start from its `Default` impl and overwrite the
        // fields this crate cares about.
        let mut implementation = Implementation::default();
        implementation.name = "ctrader-mcp".to_string();
        implementation.title = Some("wyck cTrader MCP client".to_string());
        implementation.version = env!("CARGO_PKG_VERSION").to_string();
        implementation.description =
            Some("Rust MCP client for cTrader's Local and Remote MCP servers".to_string());
        ClientConfig::new(ClientCapabilities::default(), implementation)
    }
}

/// A live MCP session against exactly one cTrader server endpoint (either a Local
/// desktop instance or a Remote `rest-proxy` deployment: this type is server-family
/// agnostic; [`crate::local::LocalClient`] and [`crate::remote::RemoteClient`] are the
/// server-family-aware layers built on top of it).
pub struct McpSession {
    running: RunningService<RoleClient, ClientIdentity>,
    diagnostics: McpSessionDiagnostics,
    /// Copied from [`ConnectionConfig::retry_policy`] at connect time, and used by
    /// [`Self::call_idempotent`], [`Self::call_no_args_idempotent`], and
    /// [`Self::call_raw_idempotent`]. `connect` itself is
    /// also retried against this same policy: see this method's body.
    retry_policy: RetryPolicy,
}

/// The lifecycle selected while opening an MCP session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpLifecycle {
    /// Stateless discovery and per-request metadata from MCP 2026-07-28.
    Modern,
    /// The `initialize` and `notifications/initialized` handshake.
    Legacy,
}

/// Protocol details negotiated with the connected server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSessionDiagnostics {
    /// Lifecycle selected by automatic discovery.
    pub lifecycle: McpLifecycle,
    /// MCP protocol version reported by the server.
    pub protocol_version: String,
}

impl McpSession {
    /// Opens a streamable-HTTP + SSE MCP session against `config.uri`. The client first
    /// tries MCP 2026-07-28 discovery, then falls back to the MCP 2025-11-25 legacy
    /// initialization handshake when the server does not implement discovery. Retried
    /// per `config.retry_policy`: always safe to retry, since nothing has been sent to
    /// any tool yet at this point.
    ///
    /// # Errors
    ///
    /// Returns [`CTraderError::Connect`] if the transport cannot be established or the
    /// lifecycle negotiation fails (unreachable host, TLS failure, auth rejection,
    /// protocol-version mismatch) and every retry attempt is exhausted.
    pub async fn connect(config: &ConnectionConfig) -> Result<Self, CTraderError> {
        let mut transport_config =
            StreamableHttpClientTransportConfig::with_uri(config.uri.clone())
                .control_request_timeout(config.control_request_timeout)
                .session_recovery_timeout(config.session_recovery_timeout);
        if let Some(token) = &config.bearer_token {
            transport_config = transport_config.auth_header(token.expose_secret().to_owned());
        }

        let running = retry_with_backoff(&config.retry_policy, || {
            let transport_config = transport_config.clone();
            async {
                let transport = StreamableHttpClientTransport::from_config(transport_config);
                ClientIdentity
                    .serve_with_lifecycle(
                        transport,
                        ClientLifecycleMode::Auto {
                            preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                            legacy_version: Some(ProtocolVersion::V_2025_11_25),
                        },
                    )
                    .await
                    .map_err(|source| CTraderError::Connect {
                        uri: config.uri.clone(),
                        // `{:?}` (Debug), not `{}` (Display): rmcp's `ClientInitializeError`
                        // doesn't wire every variant's inner error through `std::error::Error::
                        // source()` (its `TransportError.error` field has no `#[source]`
                        // attribute), so walking `.source()` here would silently stop one level
                        // too early and drop the actual transport-layer cause (DNS failure, TLS
                        // handshake failure, connection reset, ...). Every error in this chain
                        // still derives/implements `Debug`, and each layer's own `Debug` impl
                        // recursively embeds its `source`'s `Debug` (this is true of
                        // `reqwest::Error` in particular, whose `Debug` includes `kind`, `url`,
                        // AND `source`), so formatting with `{:?}` surfaces the full chain
                        // regardless of which attribute rmcp did or didn't add.
                        message: format!("{source:?}"),
                    })
            }
        })
        .await?;

        let peer_info = running
            .peer()
            .peer_info()
            .ok_or_else(|| CTraderError::Connect {
                uri: config.uri.clone(),
                message: "lifecycle negotiation completed without server information".into(),
            })?;
        let protocol_version = peer_info.protocol_version.clone();
        let diagnostics = McpSessionDiagnostics {
            lifecycle: if protocol_version >= ProtocolVersion::V_2026_07_28 {
                McpLifecycle::Modern
            } else {
                McpLifecycle::Legacy
            },
            protocol_version: protocol_version.to_string(),
        };

        Ok(Self {
            running,
            diagnostics,
            retry_policy: config.retry_policy.clone(),
        })
    }

    /// Returns the lifecycle and protocol version selected at connection time.
    pub fn diagnostics(&self) -> &McpSessionDiagnostics {
        &self.diagnostics
    }

    /// The MCP peer handle for this session, used for direct protocol operations
    /// ([`Peer::list_tools`], etc.) that fall outside the typed `tools/call` helpers.
    pub fn peer(&self) -> &Peer<RoleClient> {
        self.running.peer()
    }

    /// Calls `tool` with `params` serialized as the JSON `arguments` object, and decodes
    /// the response into `R`.
    ///
    /// `params` must serialize to a JSON object (every request DTO in [`crate::local`]
    /// and [`crate::remote`] does) or `Null` (equivalent to no arguments). Response
    /// decoding prefers `structured_content` when the server populates it, falling back
    /// to parsing the first text content block as JSON: cTrader's servers have been
    /// observed to use either shape depending on build and tool.
    ///
    /// # Errors
    ///
    /// See [`CTraderError`]: transport failures, schema mismatches, server rejections,
    /// upstream broker failures, and decode failures are all distinguished.
    pub async fn call<P, R>(&self, tool: &'static str, params: P) -> Result<R, CTraderError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let arguments = Self::encode_arguments(tool, params)?;
        self.call_with_arguments(tool, arguments).await
    }

    /// Calls `tool` with no arguments (e.g. `ping`, `get_server_time`,
    /// `get_accounts_list`), and decodes the response into `R`.
    ///
    /// Sends `"arguments": {}` rather than omitting the field entirely: cTrader's Remote
    /// `rest-proxy` has been observed to validate some "no-arg" tools' input against a
    /// Zod object schema (e.g. `get_balance`), which rejects an omitted/`undefined`
    /// arguments field with `invalid_type` even though the object has no required
    /// properties.
    pub async fn call_no_args<R: DeserializeOwned>(
        &self,
        tool: &'static str,
    ) -> Result<R, CTraderError> {
        self.call_with_arguments(tool, Some(JsonObject::default()))
            .await
    }

    /// Like [`Self::call`], but retried per this session's
    /// [`crate::config::ConnectionConfig::retry_policy`] on transient failures.
    ///
    /// Only ever call this for a tool that is safe to run more than once for a single
    /// logical request: i.e. a read-only getter. Never wrap a mutating call
    /// (`create_order`, `amend_order`, `cancel_order`, `amend_position`,
    /// `close_position`, `place_*_order`, ...) in this: see [`crate::retry`]'s module doc
    /// comment for why a lost response cannot be told apart from a lost request.
    pub async fn call_idempotent<P, R>(
        &self,
        tool: &'static str,
        params: P,
    ) -> Result<R, CTraderError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let arguments = Self::encode_arguments(tool, params)?;
        retry_with_backoff(&self.retry_policy, || {
            self.call_with_arguments(tool, arguments.clone())
        })
        .await
    }

    /// The no-argument counterpart to [`Self::call_idempotent`]: see that method's doc
    /// comment for which tools this is safe to use for.
    pub async fn call_no_args_idempotent<R: DeserializeOwned>(
        &self,
        tool: &'static str,
    ) -> Result<R, CTraderError> {
        retry_with_backoff(&self.retry_policy, || {
            self.call_with_arguments(tool, Some(JsonObject::default()))
        })
        .await
    }

    /// Shared by [`Self::call`] and [`Self::call_idempotent`]: serializes `params` into
    /// the `arguments` object a `tools/call` request expects, or `None` for a params type
    /// that serializes to `Null` (equivalent to no arguments).
    fn encode_arguments<P: Serialize>(
        tool: &'static str,
        params: P,
    ) -> Result<Option<JsonObject>, CTraderError> {
        let value = serde_json::to_value(&params).map_err(|source| CTraderError::Encode {
            tool: tool.into(),
            source,
        })?;
        match value {
            Value::Object(map) => Ok(Some(map)),
            Value::Null => Ok(None),
            other => {
                // A DTO that serializes to a JSON scalar/array instead of an object is a
                // programming error in this crate, not a runtime condition callers can
                // recover from: surface it the same way a serialization failure would
                // be surfaced, rather than silently dropping the arguments.
                Err(CTraderError::Encode {
                    tool: tool.into(),
                    source: serde::de::Error::custom(format!(
                        "request DTO for `{tool}` serialized to a non-object JSON value: {other}"
                    )),
                })
            }
        }
    }

    /// Escape hatch for tools this crate does not (yet) model with a typed DTO: see the
    /// "inferred tool name" notes on [`crate::local::LocalClient`] methods for
    /// categories (watchlists, workspaces, chart templates, price alerts) where the
    /// skill's reference documentation names the *capability* but not always the exact
    /// wire tool name. Returns the decoded JSON payload without imposing a Rust type on
    /// it; validate the live `tools/list` schema before depending on a specific shape in
    /// production.
    pub async fn call_raw(
        &self,
        tool: &'static str,
        arguments: Option<JsonObject>,
    ) -> Result<Value, CTraderError> {
        self.call_with_arguments(tool, arguments).await
    }

    /// Retries a read-only raw tool call on transient failures. Mutating tools must use
    /// [`Self::call_raw`] so a lost response cannot submit the action twice.
    pub async fn call_raw_idempotent(
        &self,
        tool: &'static str,
        arguments: Option<JsonObject>,
    ) -> Result<Value, CTraderError> {
        retry_with_backoff(&self.retry_policy, || {
            self.call_with_arguments(tool, arguments.clone())
        })
        .await
    }

    /// Lists every tool the connected server currently advertises. Used by workflow W0
    /// (session bootstrap) to fingerprint the server family per `SKILL.md`'s routing
    /// table (Local: `get_accounts_list`; Remote: `get_version`) and to detect whether
    /// the bound connection is `data`-only or also exposes the `trading` profile's
    /// mutating tools (`references/remote-http-server.md` "Profile distinction").
    ///
    /// `"ping"` is deliberately not part of either fingerprint despite appearing in
    /// older versions of this crate's own documentation: neither server advertises it
    /// here: see [`Self::ping`].
    pub async fn list_tool_names(&self) -> Result<Vec<String>, CTraderError> {
        let tools = self
            .peer()
            .list_all_tools()
            .await
            .map_err(|source| CTraderError::from_service_error("tools/list", source))?;
        Ok(tools
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect())
    }

    /// Confirms round-trip liveness with the connected server via a native MCP protocol
    /// `ping` (`ClientRequest::PingRequest`), NOT a `tools/call`.
    ///
    /// Both `RemoteClient::ping` and `LocalClient::ping` used to send this as a tool
    /// call named `"ping"`, which was silently broken: a live probe against both
    /// server families (`examples/probe_remote.rs`, `examples/probe_local.rs`) found
    /// that neither one advertises `"ping"` in `tools/list`: MCP's protocol-level ping
    /// exists precisely so a client doesn't need a tool for this.
    pub async fn ping(&self) -> Result<(), CTraderError> {
        self.peer()
            .send_request(ClientRequest::PingRequest(PingRequest::default()))
            .await
            .map(|_| ())
            .map_err(|source| CTraderError::from_service_error("ping", source))
    }

    async fn call_with_arguments<R: DeserializeOwned>(
        &self,
        tool: &'static str,
        arguments: Option<JsonObject>,
    ) -> Result<R, CTraderError> {
        let mut request = CallToolRequestParams::new(tool);
        if let Some(arguments) = arguments {
            request = request.with_arguments(arguments);
        }

        let result = self
            .peer()
            .call_tool(request)
            .await
            .map_err(|source| CTraderError::from_service_error(tool, source))?;

        if result.is_error == Some(true) {
            return Err(CTraderError::classify_tool_error(tool, &result));
        }

        decode_result(tool, result)
    }

    /// Gracefully shuts down the MCP session, closing the underlying transport.
    ///
    /// Dropping an [`McpSession`] without calling this also closes the transport (the
    /// underlying [`RunningService`] closes on drop), but calling it explicitly lets a
    /// caller observe and handle a graceful-shutdown failure instead of it happening
    /// silently during drop.
    pub async fn shutdown(self) -> Result<(), CTraderError> {
        self.running
            .cancel()
            .await
            .map(|_quit_reason| ())
            .map_err(|join_error| {
                CTraderError::Other(format!("MCP session shutdown task panicked: {join_error}"))
            })
    }
}

fn decode_result<R: DeserializeOwned>(
    tool: &'static str,
    result: CallToolResult,
) -> Result<R, CTraderError> {
    if let Some(structured) = result.structured_content {
        return serde_json::from_value(structured).map_err(|source| CTraderError::Decode {
            tool: tool.into(),
            source,
        });
    }

    let text = result
        .content
        .iter()
        .find_map(|block| block.as_text())
        .ok_or(CTraderError::EmptyResponse { tool: tool.into() })?;

    serde_json::from_str(&text.text).map_err(|source| CTraderError::Decode {
        tool: tool.into(),
        source,
    })
}
