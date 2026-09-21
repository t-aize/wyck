mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use ctrader_mcp::common::Period;
use ctrader_mcp::config::ConnectionConfig;
use ctrader_mcp::remote::RemoteClient;
use ctrader_mcp::time::local_iso8601_to_epoch_millis;
use ctrader_mcp::workflows::backfill_trendbars;
use rmcp::model::CallToolResult;
use serde_json::json;
use support::MockMcpServer;

#[tokio::test]
async fn backfill_follows_full_pages_even_with_has_more_false() {
    const START: i64 = 1_767_225_600_000;
    const BAR_MS: i64 = 60_000;
    const COUNT: i64 = 230;
    let calls = Arc::new(AtomicU32::new(0));
    let calls_in_handler = calls.clone();
    let server = MockMcpServer::builder()
        .with_tool("get_trendbars", move |arguments| {
            calls_in_handler.fetch_add(1, Ordering::SeqCst);
            let args = arguments.expect("trendbar arguments");
            assert_eq!(args["symbolId"], json!(42));
            let upper =
                local_iso8601_to_epoch_millis(args["toTimestamp"].as_str().expect("ISO timestamp"))
                    .expect("valid ISO timestamp");
            let bars: Vec<_> = (0..COUNT)
                .map(|index| START + index * BAR_MS)
                .filter(|&timestamp| timestamp < upper)
                .rev()
                .take(100)
                .map(|timestamp| json!({ "timestamp": timestamp, "close": 12345 }))
                .collect();
            CallToolResult::structured(json!({ "trendbars": bars, "hasMore": false }))
        })
        .build();
    let (url, handle) = support::spawn_mock_mcp_server(server).await;
    let client = RemoteClient::connect(&ConnectionConfig::new(url))
        .await
        .expect("connect should succeed");

    let bars = tokio::time::timeout(
        Duration::from_secs(10),
        backfill_trendbars(&client, 42, Period::M1, START, START + COUNT * BAR_MS),
    )
    .await
    .expect("backfill must terminate")
    .expect("backfill should succeed");
    let timestamps: Vec<_> = bars.iter().map(|bar| bar.timestamp.unwrap()).collect();
    let expected: Vec<_> = (0..COUNT).map(|index| START + index * BAR_MS).collect();
    assert_eq!(timestamps, expected);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    handle.abort();
}
