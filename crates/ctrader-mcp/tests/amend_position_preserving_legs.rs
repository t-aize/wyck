mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use ctrader_mcp::config::ConnectionConfig;
use ctrader_mcp::error::CTraderError;
use ctrader_mcp::quirks::amend_position_preserving_legs;
use ctrader_mcp::remote::RemoteClient;
use rmcp::model::CallToolResult;
use serde_json::json;
use support::MockMcpServer;

#[tokio::test]
async fn amend_preserves_the_unchanged_leg() {
    let amendments = Arc::new(AtomicU32::new(0));
    let amendments_in_handler = amendments.clone();
    let server = MockMcpServer::builder()
        .with_tool("get_position_details", |arguments| {
            assert_eq!(arguments.unwrap()["positionId"], json!(7));
            CallToolResult::structured(json!({
                "position": { "positionId": 7, "stopLoss": 1.08, "takeProfit": 1.12 }
            }))
        })
        .with_tool("amend_position", move |arguments| {
            amendments_in_handler.fetch_add(1, Ordering::SeqCst);
            let args = arguments.unwrap();
            assert_eq!(args["positionId"], json!(7));
            assert_eq!(args["stopLoss"], json!(1.09));
            assert_eq!(args["takeProfit"], json!(1.12));
            CallToolResult::structured(json!({
                "position": { "positionId": 7, "stopLoss": 1.09, "takeProfit": 1.12 }
            }))
        })
        .build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;
    let client = RemoteClient::connect(&ConnectionConfig::new(url))
        .await
        .unwrap();

    let result = amend_position_preserving_legs(&client, 7, Some(1.09), None)
        .await
        .expect("both legs should survive");
    assert_eq!(result.position.unwrap().take_profit, Some(1.12));
    assert_eq!(amendments.load(Ordering::SeqCst), 1);
    handle.abort();
}

#[tokio::test]
async fn missing_unchanged_leg_fails_before_amend() {
    let amendments = Arc::new(AtomicU32::new(0));
    let amendments_in_handler = amendments.clone();
    let server = MockMcpServer::builder()
        .with_tool("get_position_details", |_arguments| {
            CallToolResult::structured(json!({
                "position": { "positionId": 7, "stopLoss": 1.08 }
            }))
        })
        .with_tool("amend_position", move |_arguments| {
            amendments_in_handler.fetch_add(1, Ordering::SeqCst);
            CallToolResult::structured(json!({}))
        })
        .build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;
    let client = RemoteClient::connect(&ConnectionConfig::new(url))
        .await
        .unwrap();

    let result = amend_position_preserving_legs(&client, 7, Some(1.09), None).await;
    assert!(matches!(result, Err(CTraderError::Invariant(_))));
    assert_eq!(amendments.load(Ordering::SeqCst), 0);
    handle.abort();
}

#[tokio::test]
async fn missing_leg_in_amend_response_is_an_invariant_error() {
    let server = MockMcpServer::builder()
        .with_tool("get_position_details", |_arguments| {
            CallToolResult::structured(json!({
                "position": { "positionId": 7, "stopLoss": 1.08, "takeProfit": 1.12 }
            }))
        })
        .with_tool("amend_position", |arguments| {
            let args = arguments.unwrap();
            assert_eq!(args["stopLoss"], json!(1.09));
            assert_eq!(args["takeProfit"], json!(1.12));
            CallToolResult::structured(json!({
                "position": { "positionId": 7, "stopLoss": 1.09 }
            }))
        })
        .build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;
    let client = RemoteClient::connect(&ConnectionConfig::new(url))
        .await
        .unwrap();

    let result = amend_position_preserving_legs(&client, 7, Some(1.09), None).await;
    assert!(matches!(result, Err(CTraderError::Invariant(_))));
    handle.abort();
}
