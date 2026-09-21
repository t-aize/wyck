//! `AccountClient`: routes to the four sub-clients, each carrying its own bound account.

mod support;

use std::time::Duration;

use ctrader_openapi::Client;
use ctrader_openapi::config::ConnectionConfig;
use ctrader_openapi::market::{Period, QuoteType};
use ctrader_openapi::transport::wire::payload;
use serde_json::json;
use support::{MockServer, answers};

async fn connect(server: &MockServer) -> Client {
    let mut config = ConnectionConfig::with_url(server.url.clone());
    config.heartbeat_interval = Duration::from_millis(200);
    Client::connect(&config).await.unwrap()
}

#[tokio::test]
async fn every_sub_client_carries_the_bound_account() {
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
        (
            payload::EXPECTED_MARGIN_REQ,
            payload::EXPECTED_MARGIN_RES,
            json!({"margin": []}),
        ),
        (
            payload::ACCOUNT_LOGOUT_REQ,
            payload::ACCOUNT_LOGOUT_RES,
            json!({}),
        ),
    ]))
    .await;
    let account = connect(&server).await.account(42);
    assert_eq!(account.id(), 42);

    account.authorize("tok").await.unwrap();
    let market = account.market();
    let symbols = market.symbols().await.unwrap();
    market
        .subscribe_spots(&[symbols[0].symbol_id])
        .await
        .unwrap();
    market.ticks(1, QuoteType::Bid, 0, 1_000).await.unwrap();
    market.bars(1, Period::M1, 0, 1_000).await.unwrap();
    assert_eq!(account.account_data().trader().await.unwrap().balance, 100);
    assert!(
        account
            .margin()
            .expected_margin(1, &[1])
            .await
            .unwrap()
            .is_empty()
    );
    account.logout().await.unwrap();

    for kind in [
        payload::ACCOUNT_AUTH_REQ,
        payload::SYMBOLS_LIST_REQ,
        payload::SUBSCRIBE_SPOTS_REQ,
        payload::GET_TICK_DATA_REQ,
        payload::GET_TRENDBARS_REQ,
        payload::TRADER_REQ,
        payload::EXPECTED_MARGIN_REQ,
        payload::ACCOUNT_LOGOUT_REQ,
    ] {
        let sent = server.received_of(kind);
        assert_eq!(sent.len(), 1, "type {kind}");
        assert_eq!(sent[0].payload["ctidTraderAccountId"], 42, "type {kind}");
    }
    assert_eq!(
        server.received_of(payload::SUBSCRIBE_SPOTS_REQ)[0].payload["subscribeToSpotTimestamp"],
        true,
        "the market client asks for server timestamps"
    );
}

#[tokio::test]
async fn the_trading_sub_client_overwrites_the_account_on_its_requests() {
    let server = MockServer::start(answers(vec![(
        payload::NEW_ORDER_REQ,
        payload::EXECUTION_EVENT,
        json!({"executionType": 3}),
    )]))
    .await;
    let account = connect(&server).await.account(7);
    let request = ctrader_openapi::trading::NewOrderReq::market(
        1,
        ctrader_openapi::account::TradeSide::Buy,
        10_000,
    );
    account.trading().new_order(request).await.unwrap();
    assert_eq!(
        server.received_of(payload::NEW_ORDER_REQ)[0].payload["ctidTraderAccountId"],
        7,
        "the account id is always the bound one, never the request's own field"
    );
}
