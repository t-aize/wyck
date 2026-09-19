//! An end-to-end happy path through the real HTTP + MCP + decode pipeline: connect a
//! [`RemoteClient`] to an in-process mock server, list its advertised tools, and call two
//! typed methods, proving the whole stack works together (not just the error classifier
//! in isolation: see `crates/ctrader-mcp/src/error.rs`'s own unit tests for that).

mod support;

use ctrader_mcp::config::ConnectionConfig;
use ctrader_mcp::remote::RemoteClient;
use rmcp::model::CallToolResult;
use serde_json::json;
use support::MockMcpServer;

#[tokio::test]
async fn remote_client_connects_lists_tools_and_decodes_typed_responses() {
    let server = MockMcpServer::builder()
        .with_tool("get_version", |_arguments| {
            CallToolResult::structured(json!({
                "version": "1.0.18",
                "build_time": "2026-01-01T00:00:00Z",
                "service": "rest-proxy"
            }))
        })
        .with_tool("get_balance", |_arguments| {
            CallToolResult::structured(json!({
                "trader_id": 42,
                "balance": 1_000_000,
                "equity": 1_000_000,
                "free_margin": 1_000_000,
                "money_digits": 2,
                "deposit_asset_id": 1
            }))
        })
        .build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;

    let client = RemoteClient::connect(&ConnectionConfig::new(url))
        .await
        .expect("connect should succeed against the mock server");

    let tools = client
        .session()
        .list_tool_names()
        .await
        .expect("tools/list should succeed");
    assert!(tools.contains(&"get_version".to_owned()));
    assert!(tools.contains(&"get_balance".to_owned()));

    let version = client
        .get_version()
        .await
        .expect("get_version should decode successfully");
    assert_eq!(version.version.as_deref(), Some("1.0.18"));
    assert_eq!(version.service.as_deref(), Some("rest-proxy"));

    let balance = client
        .get_balance()
        .await
        .expect("get_balance should decode successfully");
    assert_eq!(balance.trader_id, Some(42));
    assert_eq!(balance.display_balance(), Some(10_000.0));

    handle.abort();
}
