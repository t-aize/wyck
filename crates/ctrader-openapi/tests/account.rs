//! The account calls against the scripted server: what is sent, what comes back, and the events an
//! account produces.

mod support;

use std::time::Duration;

use ctrader_openapi::account::{
    AccessRights, AccountType, DealStatus, OrderStatus, OrderType, PositionStatus, TradeSide,
};
use ctrader_openapi::config::ConnectionConfig;
use ctrader_openapi::wire::payload;
use ctrader_openapi::{Client, ErrorKind, Event};
use serde_json::json;
use support::{MockServer, answers};

async fn connect(server: &MockServer) -> Client {
    let mut config = ConnectionConfig::with_url(server.url.clone());
    config.heartbeat_interval = Duration::from_millis(200);
    Client::connect(&config).await.unwrap()
}

#[tokio::test]
async fn the_account_details_are_read_and_converted() {
    let server = MockServer::start(answers(vec![(
        payload::TRADER_REQ,
        payload::TRADER_RES,
        json!({"ctidTraderAccountId": 48332955, "trader": {
            "ctidTraderAccountId": 48332955, "balance": 1_000_000, "depositAssetId": 15,
            "accessRights": 0, "leverageInCents": 10_000, "accountType": 0,
            "brokerName": "Spotware", "moneyDigits": 2
        }}),
    )]))
    .await;
    let client = connect(&server).await;
    let trader = client.trader(48_332_955).await.unwrap();
    assert_eq!(trader.balance_amount(), 10_000.0);
    assert_eq!(trader.leverage(), Some(100.0));
    assert_eq!(trader.rights(), Some(AccessRights::FullAccess));
    assert_eq!(trader.kind(), Some(AccountType::Hedged));
    assert_eq!(
        server.received_of(payload::TRADER_REQ)[0].payload,
        json!({"ctidTraderAccountId": 48332955})
    );
}

#[tokio::test]
async fn open_positions_and_working_orders_come_together() {
    let server = MockServer::start(answers(vec![(
        payload::RECONCILE_REQ,
        payload::RECONCILE_RES,
        json!({"position": [{
            "positionId": 1, "positionStatus": 1, "swap": 0, "price": 1.1,
            "tradeData": {"symbolId": 1, "volume": 100000, "tradeSide": 1}
        }], "order": [{
            "orderId": 9, "orderType": 2, "orderStatus": 1, "limitPrice": 1.05,
            "tradeData": {"symbolId": 1, "volume": 50000, "tradeSide": 2}
        }]}),
    )]))
    .await;
    let client = connect(&server).await;
    let (positions, orders) = client.open_positions_and_orders(1, true).await.unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].status(), Some(PositionStatus::Open));
    assert_eq!(positions[0].trade_data.side(), Some(TradeSide::Buy));
    assert_eq!(orders[0].kind(), Some(OrderType::Limit));
    assert_eq!(orders[0].status(), Some(OrderStatus::Accepted));
    assert_eq!(
        server.received_of(payload::RECONCILE_REQ)[0].payload["returnProtectionOrders"],
        true
    );
}

#[tokio::test]
async fn deals_and_orders_carry_their_range_and_tell_when_there_is_more() {
    let server = MockServer::start(answers(vec![
        (
            payload::DEAL_LIST_REQ,
            payload::DEAL_LIST_RES,
            json!({"deal": [{
                "dealId": 4, "orderId": 9, "positionId": 1, "volume": 100, "filledVolume": 100,
                "symbolId": 1, "createTimestamp": 10, "executionTimestamp": 11,
                "tradeSide": 1, "dealStatus": 2, "executionPrice": 1.1
            }], "hasMore": true}),
        ),
        (
            payload::ORDER_LIST_REQ,
            payload::ORDER_LIST_RES,
            json!({"order": [], "hasMore": false}),
        ),
    ]))
    .await;
    let client = connect(&server).await;
    let (deals, more) = client.deals(1, 100, 200, Some(50)).await.unwrap();
    assert!(more);
    assert_eq!(deals[0].status(), Some(DealStatus::Filled));
    let request = &server.received_of(payload::DEAL_LIST_REQ)[0].payload;
    assert_eq!(request["fromTimestamp"], 100);
    assert_eq!(request["toTimestamp"], 200);
    assert_eq!(request["maxRows"], 50);

    let (orders, more) = client.orders(1, 100, 200).await.unwrap();
    assert!(orders.is_empty() && !more);
}

#[tokio::test]
async fn the_catalogs_and_the_profile_are_read() {
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
        (
            payload::GET_CTID_PROFILE_BY_TOKEN_REQ,
            payload::GET_CTID_PROFILE_BY_TOKEN_RES,
            json!({"profile": {"userId": 12345}}),
        ),
    ]))
    .await;
    let client = connect(&server).await;
    assert_eq!(client.assets(1).await.unwrap()[0].name, "EUR");
    assert_eq!(
        client.asset_classes(1).await.unwrap()[0].name.as_deref(),
        Some("Forex")
    );
    assert_eq!(client.symbol_categories(1).await.unwrap()[0].name, "Majors");
    assert_eq!(client.ctid_profile("tok").await.unwrap().user_id, 12_345);
    assert_eq!(
        server.received_of(payload::GET_CTID_PROFILE_BY_TOKEN_REQ)[0].payload,
        json!({"accessToken": "tok"})
    );
}

#[tokio::test]
async fn an_account_that_is_not_authorized_is_told_apart_from_a_bad_symbol() {
    let server = MockServer::start(answers(vec![(
        payload::TRADER_REQ,
        payload::ERROR_RES,
        json!({"errorCode": "ACCOUNT_NOT_AUTHORIZED"}),
    )]))
    .await;
    let client = connect(&server).await;
    let error = client.trader(1).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::NotAuthorized);
    assert!(!error.is_retryable());
}

#[tokio::test]
async fn logging_an_account_out_sends_the_account_and_succeeds() {
    let server = MockServer::start(answers(vec![(
        payload::ACCOUNT_LOGOUT_REQ,
        payload::ACCOUNT_LOGOUT_RES,
        json!({"ctidTraderAccountId": 1}),
    )]))
    .await;
    let client = connect(&server).await;
    client.logout_account(1).await.unwrap();
    assert_eq!(
        server.received_of(payload::ACCOUNT_LOGOUT_REQ)[0].payload,
        json!({"ctidTraderAccountId": 1})
    );
}

#[tokio::test]
async fn an_account_update_arrives_as_an_event() {
    let server = MockServer::start(answers(vec![])).await;
    let client = connect(&server).await;
    let mut events = client.events();
    server.push(
        payload::TRADER_UPDATE_EVENT,
        json!({"ctidTraderAccountId": 1, "trader": {
            "ctidTraderAccountId": 1, "balance": 999_900, "depositAssetId": 15, "moneyDigits": 2
        }}),
    );
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    match event {
        Event::TraderUpdated(update) => assert_eq!(update.trader.balance_amount(), 9_999.0),
        other => panic!("{other:?}"),
    }
}
