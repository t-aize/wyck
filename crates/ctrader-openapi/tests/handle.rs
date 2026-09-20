//! The account bound client: it must send its own account id on every call.

mod support;

use std::time::Duration;

use ctrader_openapi::Client;
use ctrader_openapi::config::ConnectionConfig;
use ctrader_openapi::types::{Period, QuoteType};
use ctrader_openapi::wire::payload;
use serde_json::json;
use support::{MockServer, answers};

async fn connect(server: &MockServer) -> Client {
    let mut config = ConnectionConfig::with_url(server.url.clone());
    config.heartbeat_interval = Duration::from_millis(200);
    Client::connect(&config).await.unwrap()
}

#[tokio::test]
async fn every_call_carries_the_bound_account() {
    let server = MockServer::start(answers(vec![
        (
            payload::ACCOUNT_AUTH_REQ,
            payload::ACCOUNT_AUTH_RES,
            json!({"ctidTraderAccountId": 42}),
        ),
        (
            payload::SYMBOLS_LIST_REQ,
            payload::SYMBOLS_LIST_RES,
            json!({"symbol": [{"symbolId": 1, "symbolName": "EURUSD"}]}),
        ),
        (
            payload::SUBSCRIBE_SPOTS_REQ,
            payload::SUBSCRIBE_SPOTS_RES,
            json!({}),
        ),
        (
            payload::GET_TICK_DATA_REQ,
            payload::GET_TICK_DATA_RES,
            json!({"tickData": [], "hasMore": false}),
        ),
        (
            payload::GET_TRENDBARS_REQ,
            payload::GET_TRENDBARS_RES,
            json!({"trendbar": [], "hasMore": false}),
        ),
        (
            payload::TRADER_REQ,
            payload::TRADER_RES,
            json!({"trader": {"ctidTraderAccountId": 42, "balance": 100}}),
        ),
    ]))
    .await;
    let account = connect(&server).await.account(42);
    assert_eq!(account.id(), 42);

    account.authorize("tok").await.unwrap();
    let symbols = account.symbols().await.unwrap();
    account
        .subscribe_spots(&[symbols[0].symbol_id])
        .await
        .unwrap();
    account.ticks(1, QuoteType::Bid, 0, 1_000).await.unwrap();
    account.bars(1, Period::M1, 0, 1_000).await.unwrap();
    assert_eq!(account.trader().await.unwrap().balance, 100);

    for kind in [
        payload::ACCOUNT_AUTH_REQ,
        payload::SYMBOLS_LIST_REQ,
        payload::SUBSCRIBE_SPOTS_REQ,
        payload::GET_TICK_DATA_REQ,
        payload::GET_TRENDBARS_REQ,
        payload::TRADER_REQ,
    ] {
        let sent = server.received_of(kind);
        assert_eq!(sent.len(), 1, "type {kind}");
        assert_eq!(sent[0].payload["ctidTraderAccountId"], 42, "type {kind}");
    }
    assert_eq!(
        server.received_of(payload::SUBSCRIBE_SPOTS_REQ)[0].payload["subscribeToSpotTimestamp"],
        true,
        "the handle asks for server timestamps"
    );
}
