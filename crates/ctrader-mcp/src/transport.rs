//! The underlying [`rmcp`] session and the generic, typed `tools/call` helper shared by
//! [`crate::local::LocalClient`] and [`crate::remote::RemoteClient`].
//!
//! Neither server-specific client talks to [`rmcp`] directly — both hold an
//! [`McpSession`] and express every tool as a call to [`McpSession::call`],
//! [`McpSession::call_no_args`], or (for tools this crate does not yet model with a typed
//! DTO — see the per-category notes in [`crate::local`] and [`crate::remote`])
//! [`McpSession::call_raw`]. This keeps the encoding/decoding, error-classification
//! (`self-healing-playbook.md` §3), and schema-fields-only enforcement (§1.5, implicit
//! since only declared DTO fields are ever serialized) in exactly one place.

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation,
    JsonObject,
};
use rmcp::service::RunningService;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{ClientHandler, Peer, RoleClient, ServiceExt};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::config::ConnectionConfig;
use crate::error::CTraderError;

/// The [`rmcp::ClientHandler`] identity this crate presents to every cTrader MCP server
/// during the `initialize` handshake. It declines every server-initiated capability
/// (sampling, roots, elicitation) because this crate is a pure tool-calling client, not
/// an interactive agent host — cTrader's servers do not currently call back into the
/// client for any of those capabilities.
#[derive(Debug, Clone, Copy, Default)]
struct ClientIdentity;

impl ClientHandler for ClientIdentity {
    fn get_info(&self) -> ClientConfig {
        // `Implementation` is `#[non_exhaustive]`, so it cannot be built with a struct
        // literal from outside `rmcp` — start from its `Default` impl and overwrite the
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
/// desktop instance or a Remote `rest-proxy` deployment — this type is server-family
/// agnostic; [`crate::local::LocalClient`] and [`crate::remote::RemoteClient`] are the
/// server-family-aware layers built on top of it).
pub struct McpSession {
    running: RunningService<RoleClient, ClientIdentity>,
}

impl McpSession {
    /// Opens a streamable-HTTP + SSE MCP session against `config.uri`, performing the
    /// MCP `initialize` handshake before returning.
    ///
    /// # Errors
    ///
    /// Returns [`CTraderError::Connect`] if the transport cannot be established or the
    /// `initialize` handshake fails (unreachable host, TLS failure, auth rejection,
    /// protocol-version mismatch).
    pub async fn connect(config: &ConnectionConfig) -> Result<Self, CTraderError> {
        let mut transport_config =
            StreamableHttpClientTransportConfig::with_uri(config.uri.clone())
                .control_request_timeout(config.control_request_timeout)
                .session_recovery_timeout(config.session_recovery_timeout);
        if let Some(header) = &config.auth_header {
            transport_config = transport_config.auth_header(header.clone());
        }

        let transport = StreamableHttpClientTransport::from_config(transport_config);
        let running =
            ClientIdentity
                .serve(transport)
                .await
                .map_err(|source| CTraderError::Connect {
                    uri: config.uri.clone(),
                    message: source.to_string(),
                })?;

        Ok(Self { running })
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
    /// to parsing the first text content block as JSON — cTrader's servers have been
    /// observed to use either shape depending on build and tool.
    ///
    /// # Errors
    ///
    /// See [`CTraderError`] — transport failures, schema mismatches, server rejections,
    /// upstream broker failures, and decode failures are all distinguished.
    pub async fn call<P, R>(&self, tool: &'static str, params: P) -> Result<R, CTraderError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let value = serde_json::to_value(&params).map_err(|source| CTraderError::Encode {
            tool: tool.into(),
            source,
        })?;
        let arguments = match value {
            Value::Object(map) => Some(map),
            Value::Null => None,
            other => {
                // A DTO that serializes to a JSON scalar/array instead of an object is a
                // programming error in this crate, not a runtime condition callers can
                // recover from — surface it the same way a serialization failure would
                // be surfaced, rather than silently dropping the arguments.
                return Err(CTraderError::Encode {
                    tool: tool.into(),
                    source: serde::de::Error::custom(format!(
                        "request DTO for `{tool}` serialized to a non-object JSON value: {other}"
                    )),
                });
            }
        };
        self.call_with_arguments(tool, arguments).await
    }

    /// Calls `tool` with no arguments (e.g. `ping`, `get_server_time`,
    /// `get_accounts_list`), and decodes the response into `R`.
    pub async fn call_no_args<R: DeserializeOwned>(
        &self,
        tool: &'static str,
    ) -> Result<R, CTraderError> {
        self.call_with_arguments(tool, None).await
    }

    /// Escape hatch for tools this crate does not (yet) model with a typed DTO — see the
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

    /// Lists every tool the connected server currently advertises. Used by workflow W0
    /// (session bootstrap) to fingerprint the server family per `SKILL.md`'s routing
    /// table (Local: `ping` + `get_accounts_list`; Remote: `get_version`) and to detect
    /// whether the bound connection is `data`-only or also exposes the `trading`
    /// profile's mutating tools (`references/remote-http-server.md` "Profile
    /// distinction").
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
