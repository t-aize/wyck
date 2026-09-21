//! Proves the retry/rate-limit split works end to end: [`McpSession::call_no_args_idempotent`]
//! recovers from a transient failure, while the plain (non-idempotent)
//! [`McpSession::call_no_args`] path (the one every mutating method on
//! [`ctrader_mcp::remote::RemoteClient`]/[`ctrader_mcp::local::LocalClient`] uses) never
//! retries, even for the exact same error. See `crates/ctrader-mcp/src/retry.rs`'s module
//! doc comment for why that split exists (double-submitting a real order).

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use ctrader_mcp::config::ConnectionConfig;
use ctrader_mcp::error::CTraderError;
use ctrader_mcp::local::LocalClient;
use ctrader_mcp::transport::McpSession;
use rmcp::model::CallToolResult;
use serde_json::json;
use support::MockMcpServer;

/// A `502 uProxy` envelope: classified as [`CTraderError::UpstreamBrokerError`],
/// documented as "retry at most once".
fn upstream_broker_error() -> CallToolResult {
    CallToolResult::structured_error(json!({
        "error": { "code": "502 BAD_GATEWAY", "message": "uProxy error: broker unavailable" }
    }))
}

#[tokio::test]
async fn call_no_args_idempotent_recovers_from_one_transient_failure() {
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_in_handler = attempts.clone();

    let server = MockMcpServer::builder()
        .with_tool("get_positions", move |_arguments| {
            let attempt = attempts_in_handler.fetch_add(1, Ordering::SeqCst) + 1;
            if attempt < 2 {
                upstream_broker_error()
            } else {
                CallToolResult::structured(json!({ "positions": [] }))
            }
        })
        .build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;

    let session = McpSession::connect(&ConnectionConfig::new(url))
        .await
        .expect("connect should succeed");

    let result: serde_json::Value = session
        .call_no_args_idempotent("get_positions")
        .await
        .expect("should recover after exactly one transient failure");
    assert_eq!(result["positions"], json!([]));
    assert_eq!(attempts.load(Ordering::SeqCst), 2);

    handle.abort();
}

#[tokio::test]
async fn plain_call_never_retries_even_a_transient_failure() {
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_in_handler = attempts.clone();

    let server = MockMcpServer::builder()
        .with_tool("create_order", move |_arguments| {
            attempts_in_handler.fetch_add(1, Ordering::SeqCst);
            upstream_broker_error()
        })
        .build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;

    let session = McpSession::connect(&ConnectionConfig::new(url))
        .await
        .expect("connect should succeed");

    let result: Result<serde_json::Value, _> = session.call_no_args("create_order").await;

    assert!(matches!(
        result,
        Err(CTraderError::UpstreamBrokerError { .. })
    ));
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "a mutating-style call must fail on the first attempt, never retry"
    );

    handle.abort();
}

#[tokio::test]
async fn local_raw_getter_recovers_from_one_transient_failure() {
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_in_handler = attempts.clone();
    let server = MockMcpServer::builder()
        .with_tool("get_watchlists", move |_arguments| {
            if attempts_in_handler.fetch_add(1, Ordering::SeqCst) == 0 {
                upstream_broker_error()
            } else {
                CallToolResult::structured(json!({ "watchlists": [] }))
            }
        })
        .build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;
    let client = LocalClient::connect(&ConnectionConfig::new(url))
        .await
        .expect("connect should succeed");

    let result = client.get_watchlists().await.expect("getter should retry");
    assert_eq!(result["watchlists"], json!([]));
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    handle.abort();
}
