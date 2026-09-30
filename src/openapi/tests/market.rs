//! `MarketClient` against a scripted local server: symbols, subscriptions and price events.

use std::time::Duration;

use super::support::{MockServer, answers, connect};
use crate::openapi::market::MarketClient;
use crate::openapi::transport::wire::payload;
use crate::openapi::{Client, Event};
use serde_json::json;

async fn market(server: &MockServer) -> MarketClient {
    connect(server).await.account(1).market()
}

#[tokio::test]
async fn the_symbol_list_and_details_are_decoded() {
    let server = MockServer::start(answers(vec![
        (
            payload::SYMBOLS_LIST_REQ,
            payload::SYMBOLS_LIST_RES,
            json!({"symbol": [
                {"symbolId": 1, "symbolName": "EURUSD", "enabled": true, "description": "Euro vs US Dollar"},
                {"symbolId": "2", "symbolName": "XAUUSD"}
            ]}),
        ),
        (
            payload::SYMBOL_BY_ID_REQ,
            payload::SYMBOL_BY_ID_RES,
            json!({"symbol": [{"symbolId": 1, "digits": 5, "pipPosition": 4, "lotSize": 10000000, "minVolume": 100000}]}),
        ),
    ]))
    .await;
    let market = market(&server).await;
    let symbols = market.symbols().await.unwrap();
    assert_eq!(symbols[1].symbol_id, 2);
    assert_eq!(symbols[0].symbol_name.as_deref(), Some("EURUSD"));
    let details = market.symbol_details(&[1]).await.unwrap();
    assert_eq!((details[0].digits, details[0].pip_position), (5, 4));
    // Archived symbols are only asked for when wanted.
    let list_request = &server.received_of(payload::SYMBOLS_LIST_REQ)[0];
    assert!(list_request.payload.get("includeArchivedSymbols").is_none());
}

#[tokio::test]
async fn archived_symbols_are_asked_for_only_through_the_dedicated_call() {
    let server = MockServer::start(answers(vec![(
        payload::SYMBOLS_LIST_REQ,
        payload::SYMBOLS_LIST_RES,
        json!({"symbol": []}),
    )]))
    .await;
    let market = market(&server).await;
    market.symbols_including_archived().await.unwrap();
    assert_eq!(
        server.received_of(payload::SYMBOLS_LIST_REQ)[0].payload["includeArchivedSymbols"],
        true
    );
}

#[tokio::test]
async fn a_conversion_chain_is_asked_for_and_a_symbol_change_arrives_as_an_event() {
    let server = MockServer::start(answers(vec![(
        payload::SYMBOLS_FOR_CONVERSION_REQ,
        payload::SYMBOLS_FOR_CONVERSION_RES,
        json!({"symbol": [
            {"symbolId": 1, "symbolName": "EURUSD"},
            {"symbolId": 2, "symbolName": "USDJPY"}
        ]}),
    )]))
    .await;
    let client = connect(&server).await;
    let market = client.account(1).market();
    let chain = market.symbols_for_conversion(3, 4).await.unwrap();
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[1].symbol_name.as_deref(), Some("USDJPY"));
    assert_eq!(
        server.received_of(payload::SYMBOLS_FOR_CONVERSION_REQ)[0].payload,
        json!({"ctidTraderAccountId": 1, "firstAssetId": 3, "lastAssetId": 4})
    );

    let mut events = client.events();
    server.push(payload::SYMBOL_CHANGED_EVENT, json!({"symbolId": [1, 2]}));
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, Event::SymbolChanged(e) if e.symbol_id == vec![1, 2]));
}

#[tokio::test]
async fn the_catalogs_are_read() {
    let server = MockServer::start(answers(vec![
        (
            payload::ASSET_LIST_REQ,
            payload::ASSET_LIST_RES,
            json!({"asset": [{"assetId": 1, "name": "EUR", "digits": 2}]}),
        ),
        (
            payload::ASSET_CLASS_LIST_REQ,
            payload::ASSET_CLASS_LIST_RES,
            json!({"assetClass": [{"id": 1, "name": "Forex"}]}),
        ),
        (
            payload::SYMBOL_CATEGORY_REQ,
            payload::SYMBOL_CATEGORY_RES,
            json!({"symbolCategory": [{"id": 5, "assetClassId": 1, "name": "Majors"}]}),
        ),
    ]))
    .await;
    let market = market(&server).await;
    assert_eq!(market.assets().await.unwrap()[0].name, "EUR");
    assert_eq!(
        market.asset_classes().await.unwrap()[0].name.as_deref(),
        Some("Forex")
    );
    assert_eq!(market.symbol_categories().await.unwrap()[0].name, "Majors");
}

#[tokio::test]
async fn subscribing_sends_the_symbols_and_spots_arrive_as_events() {
    let server = MockServer::start(answers(vec![(
        payload::SUBSCRIBE_SPOTS_REQ,
        payload::SUBSCRIBE_SPOTS_RES,
        json!({"ctidTraderAccountId": 1}),
    )]))
    .await;
    let client = connect(&server).await;
    let market = client.account(1).market();
    let mut events = client.events();
    market.subscribe_spots(&[1, 2]).await.unwrap();
    assert_eq!(
        server.received_of(payload::SUBSCRIBE_SPOTS_REQ)[0].payload,
        json!({"ctidTraderAccountId": 1, "symbolId": [1, 2], "subscribeToSpotTimestamp": true})
    );

    server.push(
        payload::SPOT_EVENT,
        json!({"ctidTraderAccountId": 1, "symbolId": 1, "bid": "108499", "timestamp": 1000}),
    );
    server.push(
        payload::SPOT_EVENT,
        json!({"ctidTraderAccountId": 1, "symbolId": 1, "ask": 108501}),
    );
    let first = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    let second = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    match (first, second) {
        (Event::Spot(a), Event::Spot(b)) => {
            assert_eq!(
                (a.bid, a.ask, a.timestamp),
                (Some(108_499), None, Some(1000))
            );
            assert_eq!((b.bid, b.ask), (None, Some(108_501)));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_bad_symbol_subscription_is_a_rejection() {
    let server = MockServer::start(answers(vec![(
        payload::SUBSCRIBE_SPOTS_REQ,
        payload::ERROR_RES,
        json!({"errorCode": "SYMBOL_NOT_FOUND", "description": "unknown"}),
    )]))
    .await;
    let market = market(&server).await;
    let refused = market.subscribe_spots(&[999]).await.unwrap_err();
    assert_eq!(refused.kind(), crate::openapi::ErrorKind::Rejected);
}
