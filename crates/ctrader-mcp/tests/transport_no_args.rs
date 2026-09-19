//! Regression coverage for the `get_balance` no-args bug: cTrader's Remote `rest-proxy`
//! rejects a `tools/call` whose `arguments` field is entirely omitted, even for tools
//! with no required parameters, with `invalid_type: expected object, received
//! undefined`. `McpSession::call_no_args`/`call_no_args_idempotent` must always send
//! `"arguments": {}` instead of omitting the field.

mod support;

use ctrader_mcp::config::ConnectionConfig;
use ctrader_mcp::transport::McpSession;
use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::json;
use support::MockMcpServer;

fn strict_no_args_tool()
-> impl Fn(Option<rmcp::model::JsonObject>) -> CallToolResult + Send + Sync + 'static {
    // Mirrors the real rest-proxy Zod schema behavior this regression test guards
    // against: succeeds only when `arguments` is present AND empty; an omitted
    // (`None`) arguments field is exactly the bug this crate hit.
    |arguments| match arguments {
        Some(map) if map.is_empty() => CallToolResult::structured(json!({ "balance": 12345 })),
        Some(_) => CallToolResult::error(vec![ContentBlock::text(
            "expected no properties, received extra fields",
        )]),
        None => CallToolResult::error(vec![ContentBlock::text(
            "Invalid input: expected object, received undefined",
        )]),
    }
}

#[tokio::test]
async fn call_no_args_sends_an_empty_object_not_an_omitted_field() {
    let server = MockMcpServer::builder()
        .with_tool("get_balance", strict_no_args_tool())
        .build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;

    let session = McpSession::connect(&ConnectionConfig::new(url))
        .await
        .expect("connect should succeed");

    let result: serde_json::Value = session
        .call_no_args("get_balance")
        .await
        .expect("call_no_args must send `{}`, not an omitted arguments field");
    assert_eq!(result["balance"], 12345);

    handle.abort();
}

#[tokio::test]
async fn call_no_args_idempotent_also_sends_an_empty_object() {
    let server = MockMcpServer::builder()
        .with_tool("get_version", strict_no_args_tool())
        .build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;

    let session = McpSession::connect(&ConnectionConfig::new(url))
        .await
        .expect("connect should succeed");

    let result: serde_json::Value = session
        .call_no_args_idempotent("get_version")
        .await
        .expect("call_no_args_idempotent must send `{}`, not an omitted arguments field");
    assert_eq!(result["balance"], 12345);

    handle.abort();
}
