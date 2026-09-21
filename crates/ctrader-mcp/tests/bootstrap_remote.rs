mod support;

use ctrader_mcp::config::ConnectionConfig;
use ctrader_mcp::remote::RemoteClient;
use ctrader_mcp::workflows::bootstrap_remote;
use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::json;
use support::MockMcpServer;

async fn bootstrap(version_available: bool) -> ctrader_mcp::workflows::RemoteSessionContext {
    let server = MockMcpServer::builder()
        .with_tool("get_version", move |_arguments| {
            if version_available {
                CallToolResult::structured(json!({
                    "version": "1.0.18", "buildTime": "2026-09-21"
                }))
            } else {
                CallToolResult::error(vec![ContentBlock::text(
                    "MCP -32602: Tool get_version not found",
                )])
            }
        })
        .with_tool("get_balance", |_arguments| {
            CallToolResult::structured(json!({
                "traderId": 7, "depositAssetId": 1, "moneyDigits": 2,
                "balance": 123456, "equity": 120000, "freeMargin": 100000
            }))
        })
        .with_tool("get_assets", |_arguments| {
            CallToolResult::structured(json!({
                "assets": [{ "assetId": 1, "name": "USD" }]
            }))
        })
        .with_tool("get_symbols", |_arguments| {
            CallToolResult::structured(json!({
                "symbols": [{ "symbolId": 42, "symbolName": "EURUSD" }]
            }))
        })
        .with_tool("create_order", |_arguments| {
            CallToolResult::structured(json!({}))
        })
        .build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;
    let client = RemoteClient::connect(&ConnectionConfig::new(url))
        .await
        .unwrap();
    let context = bootstrap_remote(&client)
        .await
        .expect("bootstrap should succeed");
    handle.abort();
    context
}

#[tokio::test]
async fn bootstrap_caches_account_assets_symbols_and_profile() {
    let context = bootstrap(true).await;
    assert_eq!(context.version.as_deref(), Some("1.0.18"));
    assert_eq!(context.trader_id, Some(7));
    assert_eq!(context.account_currency.as_deref(), Some("USD"));
    assert_eq!(context.balance_display, Some(1234.56));
    assert_eq!(context.find_symbol("eurusd").unwrap().symbol_id, 42);
    assert!(context.has_trading_profile);
}

#[tokio::test]
async fn missing_version_does_not_abort_bootstrap() {
    let context = bootstrap(false).await;
    assert_eq!(context.version, None);
    assert_eq!(context.build_time, None);
    assert_eq!(context.trader_id, Some(7));
    assert_eq!(context.find_asset_name(1), Some("USD"));
}
