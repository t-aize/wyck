mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use ctrader_mcp::common::TradeSide;
use ctrader_mcp::config::ConnectionConfig;
use ctrader_mcp::error::CTraderError;
use ctrader_mcp::quirks::market_two_step_open;
use ctrader_mcp::remote::RemoteClient;
use rmcp::model::CallToolResult;
use serde_json::json;
use support::MockMcpServer;

async fn open(position_id_present: bool) -> (Result<(), CTraderError>, u32) {
    let amendments = Arc::new(AtomicU32::new(0));
    let amendments_in_handler = amendments.clone();
    let server = MockMcpServer::builder()
        .with_tool("create_order", move |arguments| {
            let args = arguments.unwrap();
            assert_eq!(args["orderType"], json!("MARKET"));
            assert!(args.get("stopLoss").is_none());
            assert!(args.get("takeProfit").is_none());
            if position_id_present {
                CallToolResult::structured(json!({ "position": { "positionId": 7 } }))
            } else {
                CallToolResult::structured(json!({ "position": {} }))
            }
        })
        .with_tool("get_position_details", |_arguments| {
            CallToolResult::structured(json!({ "position": { "positionId": 7 } }))
        })
        .with_tool("amend_position", move |arguments| {
            amendments_in_handler.fetch_add(1, Ordering::SeqCst);
            let args = arguments.unwrap();
            assert_eq!(args["stopLoss"], json!(1.08));
            assert_eq!(args["takeProfit"], json!(1.12));
            CallToolResult::structured(json!({
                "position": { "positionId": 7, "stopLoss": 1.08, "takeProfit": 1.12 }
            }))
        })
        .build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;
    let client = RemoteClient::connect(&ConnectionConfig::new(url))
        .await
        .unwrap();
    let result = market_two_step_open(&client, 42, TradeSide::Buy, 100_000, 1.08, 1.12, None)
        .await
        .map(|_| ());
    let count = amendments.load(Ordering::SeqCst);
    handle.abort();
    (result, count)
}

#[tokio::test]
async fn market_open_amends_both_legs_after_fill() {
    let (result, amendments) = open(true).await;
    result.expect("both steps should succeed");
    assert_eq!(amendments, 1);
}

#[tokio::test]
async fn missing_position_id_stops_before_amend() {
    let (result, amendments) = open(false).await;
    assert!(matches!(result, Err(CTraderError::Invariant(_))));
    assert_eq!(amendments, 0);
}
