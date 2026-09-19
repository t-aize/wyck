//! Error types and the self-healing error-classification matrix.
//!
//! cTrader's two MCP servers do not return errors in a single uniform shape. A failed
//! `tools/call` can surface as:
//!
//! - an MCP protocol-level error (`-32602 Input validation error`, a Zod schema
//!   mismatch on the *caller's* request — never worth retrying as-is);
//! - a structured JSON envelope (`{"error":{"code":"INVALID_REQUEST", ...}}` on
//!   Remote `rest-proxy` builds up to 1.0.14, or `{"error":{"code":"502 BAD_GATEWAY",
//!   "message":"uProxy error: ..."}}` for upstream broker failures on any build);
//! - a plain-text string with an embedded actionable hint (Remote `rest-proxy` 1.0.18+
//!   pre-upstream validation errors, e.g. `"create_order: Absolute stopLoss is not
//!   supported for MARKET orders..."`);
//! - a plain-text string with no structure at all (Local, e.g. `"Order error: Not
//!   enough funds to open this Position"`);
//! - a well-formed but semantically "not available" payload (`{"available": false}`,
//!   e.g. Local `get_account_statistics`).
//!
//! [`CTraderError::classify_tool_error`] implements the decision tree from the
//! `self-healing-playbook.md` §3 error-classification matrix so callers get a single,
//! typed error variant to match on regardless of which envelope shape the live server
//! happened to use.

use std::borrow::Cow;

use rmcp::model::CallToolResult;
use serde_json::Value;

/// The crate-wide result alias.
pub type Result<T> = std::result::Result<T, CTraderError>;

/// Errors produced while talking to a cTrader MCP server.
///
/// Every variant carries the offending `tool` name so a caller building user-facing
/// messages (or a retry/backoff policy) never has to re-derive *which* call failed.
#[derive(Debug, thiserror::Error)]
pub enum CTraderError {
    /// The transport (or MCP-session initialization) failed before a request/response
    /// round-trip could complete: DNS failure, TCP reset, TLS error, SSE stream drop,
    /// or session-recovery exhaustion. Safe to retry with backoff.
    #[error("failed to connect to cTrader MCP endpoint `{uri}`: {message}")]
    Connect { uri: String, message: String },

    /// The MCP session closed or the underlying transport reported a hard failure while
    /// issuing `{tool}`. Distinguished from [`CTraderError::Connect`] because it happens
    /// mid-session rather than during the initial handshake.
    #[error("transport error while calling `{tool}`: {message}")]
    Transport {
        tool: Cow<'static, str>,
        message: String,
    },

    /// `serde_json` could not encode this crate's request DTO into a JSON object
    /// (should only happen if a DTO's `Serialize` impl is broken — the pre-flight gate
    /// "schema-fields-only enforcement" from `self-healing-playbook.md` §1.5 relies on
    /// every request type serializing to a plain object).
    #[error("failed to encode request for `{tool}`: {source}")]
    Encode {
        tool: Cow<'static, str>,
        #[source]
        source: serde_json::Error,
    },

    /// The response body for `{tool}` could not be decoded into the expected DTO shape.
    /// Frequently indicates the live server's response shape has drifted from what this
    /// crate documents — check the tool's live JSON-Schema before assuming a crate bug.
    #[error("failed to decode response for `{tool}`: {source}")]
    Decode {
        tool: Cow<'static, str>,
        #[source]
        source: serde_json::Error,
    },

    /// `{tool}` returned a successful envelope with no content block and no
    /// `structured_content` to decode.
    #[error("`{tool}` returned an empty response body")]
    EmptyResponse { tool: Cow<'static, str> },

    /// MCP protocol-level `-32602 Input validation error` (a Zod schema mismatch on the
    /// *caller's* request). Per the error-classification matrix this is **never**
    /// retryable as-is — the caller's request shape must change (e.g. an unsupported
    /// `period` enum value; see `Q-R1`).
    #[error("`{tool}` rejected the request (MCP -32602 schema mismatch): {message}")]
    SchemaMismatch {
        tool: Cow<'static, str>,
        message: String,
    },

    /// The server rejected the request before it reached the broker: either the legacy
    /// structured `{"error":{"code":"INVALID_REQUEST", ...}}` envelope (rest-proxy
    /// <= 1.0.14) or the 1.0.18+ plain-string envelope that begins with the tool name or
    /// a topic label and embeds an actionable hint (e.g. `Q-R4`'s
    /// `"create_order: Absolute stopLoss is not supported for MARKET orders..."`, or
    /// `Q-R7`'s `"Time range exceeds upstream cap of 720h..."`). Never retry as-is; the
    /// request must be corrected. `message` is preserved verbatim so the embedded hint
    /// reaches the end user unmodified.
    #[error("`{tool}` was rejected by the server: {message}")]
    ServerRejection {
        tool: Cow<'static, str>,
        /// Present only for the legacy structured-JSON envelope.
        code: Option<String>,
        /// Present only for the legacy structured-JSON envelope.
        http_status: Option<u16>,
        message: String,
    },

    /// A `502 {"error":{"code":"502 BAD_GATEWAY","message":"uProxy error: ..."}}`
    /// envelope — the request reached the Remote proxy but failed at the upstream
    /// broker gateway (e.g. `Q-R8`'s unknown-symbol `UNKNOWN_SYMBOL` on
    /// `get_trendbars`). Per the classification matrix: retry at most once, and prefer
    /// surfacing a "try again" message over a silent automatic retry loop.
    #[error("`{tool}` failed upstream at the broker gateway ({code}): {message}")]
    UpstreamBrokerError {
        tool: Cow<'static, str>,
        code: String,
        message: String,
    },

    /// A Local plain-text `"Order error: ..."` style fault (`Q-L11`). Local never wraps
    /// errors in structured JSON, so this variant carries the raw message for the
    /// caller to pattern-match (e.g. "Not enough funds", "Position not found").
    #[error("`{tool}` reported a local-server fault: {message}")]
    LocalFault {
        tool: Cow<'static, str>,
        message: String,
    },

    /// The tool call succeeded at the protocol level but the payload is
    /// `{"available": false}` (e.g. `Q-L12`'s `get_account_statistics`). The caller
    /// should fall back to deriving the requested metric from in-session reads rather
    /// than treating this as a hard failure.
    #[error("`{tool}` reports its result is unavailable ({{\"available\": false}})")]
    ResourceUnavailable { tool: Cow<'static, str> },

    /// `is_error: true` was set on the tool result, but the payload did not match any
    /// recognized envelope shape (not the legacy JSON envelope, not a 1.0.18-style
    /// plain-string hint, not a Local `"Order error:"` string). Per the unknown-quirk
    /// decision tree in `self-healing-playbook.md` §4: STOP, capture the evidence
    /// (preserved in `message`), and surface it to the user rather than guessing.
    #[error("`{tool}` returned an unrecognized error envelope: {message}")]
    UnclassifiedToolError {
        tool: Cow<'static, str>,
        message: String,
    },

    /// A pre-flight gate (see [`crate::quirks`] gate helpers, mirroring
    /// `self-healing-playbook.md` §1) rejected the request locally, before any network
    /// call was made. This is a deliberate STOP, not a server error.
    #[error("pre-flight check failed for `{tool}`: {message}")]
    PreFlightRejected {
        tool: Cow<'static, str>,
        message: String,
    },

    /// A workflow-level invariant was violated (e.g. attempting to remove an SL/TP leg
    /// via `amend_position`, which `Q-R10` documents as unsupported; or a conversion
    /// chain that could not be resolved).
    #[error("{0}")]
    Invariant(String),

    /// Catch-all for conditions that do not fit the categories above.
    #[error("{0}")]
    Other(String),
}

impl CTraderError {
    /// Converts an [`rmcp::ServiceError`] surfaced while calling `tool` into a
    /// [`CTraderError`], mapping the MCP `-32602` schema-mismatch code to
    /// [`CTraderError::SchemaMismatch`] and everything else to
    /// [`CTraderError::Transport`].
    pub fn from_service_error(tool: &'static str, source: rmcp::ServiceError) -> Self {
        use rmcp::model::ErrorCode;
        if let rmcp::ServiceError::McpError(data) = &source
            && data.code == ErrorCode::INVALID_PARAMS
        {
            return CTraderError::SchemaMismatch {
                tool: Cow::Borrowed(tool),
                message: data.message.to_string(),
            };
        }
        CTraderError::Transport {
            tool: Cow::Borrowed(tool),
            // `{:?}`, not `{}` — see the matching comment on `McpSession::connect` in
            // `transport.rs` for why Debug surfaces the full error chain here and
            // Display can silently drop it.
            message: format!("{source:?}"),
        }
    }

    /// Classifies a tool result with `is_error == Some(true)` per the
    /// `self-healing-playbook.md` §3 decision tree:
    ///
    /// 1. Try-parse the payload as JSON. If it parses and contains an `error` object
    ///    with a `code`, branch on whether the code/message indicates an upstream
    ///    broker (502 / `uProxy error:`) or a pre-upstream rejection
    ///    (`INVALID_REQUEST`).
    /// 2. Otherwise treat the payload as a plain string. A Local `"Order error: ..."`
    ///    prefix indicates a local-server fault (`Q-L11`); anything else is treated as
    ///    a 1.0.18-style Remote rejection and surfaced **verbatim** (never regex-stripped
    ///    — the embedded hint is the whole point).
    /// 3. If neither branch matches (e.g. the payload is `{"available": false}`, or is
    ///    empty, or is some other shape entirely), fall back to
    ///    [`CTraderError::ResourceUnavailable`] or
    ///    [`CTraderError::UnclassifiedToolError`].
    pub fn classify_tool_error(tool: &'static str, result: &CallToolResult) -> Self {
        let tool = Cow::Borrowed(tool);

        if let Some(structured) = &result.structured_content
            && let Some(available) = structured.get("available").and_then(Value::as_bool)
            && !available
        {
            return CTraderError::ResourceUnavailable { tool };
        }

        let raw_text = result
            .content
            .iter()
            .find_map(|block| block.as_text())
            .map(|text| text.text.as_str());

        let candidate_json: Option<Value> = raw_text
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .or_else(|| result.structured_content.clone());

        if let Some(json) = candidate_json.as_ref()
            && let Some(error_obj) = json.get("error")
        {
            let code = error_obj
                .get("code")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let http_status = error_obj
                .get("httpStatus")
                .and_then(Value::as_u64)
                .map(|n| n as u16);
            let message = error_obj
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("(no message)")
                .to_owned();

            let is_upstream = code.as_deref().is_some_and(|c| c.contains("502"))
                || message.contains("uProxy error");

            return if is_upstream {
                CTraderError::UpstreamBrokerError {
                    tool,
                    code: code.unwrap_or_else(|| "502".to_owned()),
                    message,
                }
            } else {
                CTraderError::ServerRejection {
                    tool,
                    code,
                    http_status,
                    message,
                }
            };
        }

        if let Some(text) = raw_text {
            if text.trim_start().starts_with("Order error:") {
                return CTraderError::LocalFault {
                    tool,
                    message: text.to_owned(),
                };
            }
            // rest-proxy 1.0.18+ plain-string envelope: begins with the offending tool
            // name (e.g. "create_order: ...") or a topic label (e.g. "Time range
            // exceeds..."). Surface verbatim per Q-R4 / Q-R7 guidance.
            return CTraderError::ServerRejection {
                tool,
                code: None,
                http_status: None,
                message: text.to_owned(),
            };
        }

        CTraderError::UnclassifiedToolError {
            tool,
            message: "tool reported is_error=true with no decodable content".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rmcp::ServiceError;
    use rmcp::model::{ContentBlock, ErrorCode, ErrorData};
    use serde_json::json;

    use super::*;

    // --- from_service_error --------------------------------------------------------

    #[test]
    fn from_service_error_maps_invalid_params_to_schema_mismatch() {
        let source = ServiceError::McpError(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "period must be one of the 9 documented values",
            None,
        ));

        let error = CTraderError::from_service_error("get_trendbars", source);

        assert!(matches!(
            error,
            CTraderError::SchemaMismatch { tool, message }
                if tool == "get_trendbars"
                    && message == "period must be one of the 9 documented values"
        ));
    }

    #[test]
    fn from_service_error_maps_everything_else_to_transport() {
        let source = ServiceError::McpError(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            "server panicked",
            None,
        ));

        let error = CTraderError::from_service_error("get_balance", source);

        assert!(matches!(error, CTraderError::Transport { tool, .. } if tool == "get_balance"));
    }

    // --- classify_tool_error: the self-healing-playbook.md §3 matrix, one case each -

    #[test]
    fn classifies_available_false_as_resource_unavailable() {
        let result = CallToolResult::structured(json!({ "available": false }));

        let error = CTraderError::classify_tool_error("get_account_statistics", &result);

        assert!(matches!(
            error,
            CTraderError::ResourceUnavailable { tool } if tool == "get_account_statistics"
        ));
    }

    #[test]
    fn classifies_legacy_invalid_request_envelope_as_server_rejection() {
        let result = CallToolResult::structured_error(json!({
            "error": {
                "code": "INVALID_REQUEST",
                "httpStatus": 400,
                "message": "symbolId is required"
            }
        }));

        let error = CTraderError::classify_tool_error("create_order", &result);

        match error {
            CTraderError::ServerRejection {
                tool,
                code,
                http_status,
                message,
            } => {
                assert_eq!(tool, "create_order");
                assert_eq!(code.as_deref(), Some("INVALID_REQUEST"));
                assert_eq!(http_status, Some(400));
                assert_eq!(message, "symbolId is required");
            }
            other => panic!("expected ServerRejection, got {other:?}"),
        }
    }

    #[test]
    fn classifies_502_code_as_upstream_broker_error() {
        let result = CallToolResult::structured_error(json!({
            "error": {
                "code": "502 BAD_GATEWAY",
                "message": "uProxy error: broker unavailable"
            }
        }));

        let error = CTraderError::classify_tool_error("get_trendbars", &result);

        match error {
            CTraderError::UpstreamBrokerError {
                tool,
                code,
                message,
            } => {
                assert_eq!(tool, "get_trendbars");
                assert_eq!(code, "502 BAD_GATEWAY");
                assert_eq!(message, "uProxy error: broker unavailable");
            }
            other => panic!("expected UpstreamBrokerError, got {other:?}"),
        }
    }

    #[test]
    fn classifies_upstream_by_message_even_without_a_502_code() {
        let result = CallToolResult::structured_error(json!({
            "error": {
                "code": "GATEWAY_ERROR",
                "message": "uProxy error: timed out waiting for the broker"
            }
        }));

        let error = CTraderError::classify_tool_error("get_spot_prices", &result);

        assert!(matches!(error, CTraderError::UpstreamBrokerError { .. }));
    }

    #[test]
    fn classifies_rest_proxy_1_0_18_plain_string_hint_as_server_rejection_verbatim() {
        let result = CallToolResult::error(vec![ContentBlock::text(
            "create_order: Absolute stopLoss is not supported for MARKET orders; use relativeStopLoss instead",
        )]);

        let error = CTraderError::classify_tool_error("create_order", &result);

        match error {
            CTraderError::ServerRejection {
                tool,
                code,
                http_status,
                message,
            } => {
                assert_eq!(tool, "create_order");
                assert_eq!(code, None);
                assert_eq!(http_status, None);
                assert_eq!(
                    message,
                    "create_order: Absolute stopLoss is not supported for MARKET orders; use relativeStopLoss instead"
                );
            }
            other => panic!("expected ServerRejection, got {other:?}"),
        }
    }

    #[test]
    fn classifies_local_order_error_prefix_as_local_fault() {
        let result = CallToolResult::error(vec![ContentBlock::text(
            "Order error: Not enough funds to open this Position",
        )]);

        let error = CTraderError::classify_tool_error("place_market_order", &result);

        match error {
            CTraderError::LocalFault { tool, message } => {
                assert_eq!(tool, "place_market_order");
                assert_eq!(
                    message,
                    "Order error: Not enough funds to open this Position"
                );
            }
            other => panic!("expected LocalFault, got {other:?}"),
        }
    }

    #[test]
    fn classifies_no_decodable_content_as_unclassified() {
        let result = CallToolResult::error(vec![]);

        let error = CTraderError::classify_tool_error("get_deals", &result);

        assert!(matches!(
            error,
            CTraderError::UnclassifiedToolError { tool, .. } if tool == "get_deals"
        ));
    }

    #[test]
    fn structured_content_takes_priority_over_unparsable_text() {
        // Some builds populate both `content` (human-readable) and `structured_content`
        // (machine-readable) on the same error result; the JSON-envelope branch must
        // still fire even when the accompanying text block isn't itself valid JSON.
        let mut result = CallToolResult::error(vec![ContentBlock::text(
            "Order rejected — see structured data",
        )]);
        result.structured_content = Some(json!({
            "error": { "code": "INVALID_REQUEST", "message": "bad request" }
        }));

        let error = CTraderError::classify_tool_error("create_order", &result);

        assert!(matches!(error, CTraderError::ServerRejection { .. }));
    }
}
