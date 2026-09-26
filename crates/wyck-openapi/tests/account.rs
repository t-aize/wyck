//! `AccountDataClient` against the scripted server: what is sent, what comes back, and the events
//! an account produces. The reference catalogs (assets, asset classes, symbol categories) are
//! `MarketClient` calls and live in `tests/market.rs`.

mod support;

use std::time::Duration;

use serde_json::json;
use support::{MockServer, answers, connect};
use wyck_openapi::account::{
    AccessRights, AccountType, DealStatus, OrderStatus, OrderType, PositionStatus, TradeSide,
};
use wyck_openapi::transport::wire::payload;
use wyck_openapi::{ErrorKind, Event};

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
    let trader = client
        .account(48_332_955)
        .account_data()
        .trader()
        .await
        .unwrap();
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
    let data = client.account(1).account_data();
    let (positions, orders) = data.open_positions_and_orders(true).await.unwrap();
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
    let data = client.account(1).account_data();
    let (deals, more) = data.deals(100, 200, Some(50)).await.unwrap();
    assert!(more);
    assert_eq!(deals[0].status(), Some(DealStatus::Filled));
    let request = &server.received_of(payload::DEAL_LIST_REQ)[0].payload;
    assert_eq!(request["fromTimestamp"], 100);
    assert_eq!(request["toTimestamp"], 200);
    assert_eq!(request["maxRows"], 50);

    let (orders, more) = data.orders(100, 200).await.unwrap();
    assert!(orders.is_empty() && !more);
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
    let error = client.account(1).account_data().trader().await.unwrap_err();
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
    client.account(1).logout().await.unwrap();
    assert_eq!(
        server.received_of(payload::ACCOUNT_LOGOUT_REQ)[0].payload,
        json!({"ctidTraderAccountId": 1})
    );
}

#[tokio::test]
async fn the_account_gaps_are_read() {
    let server = MockServer::start(answers(vec![
        (
            payload::CASH_FLOW_HISTORY_LIST_REQ,
            payload::CASH_FLOW_HISTORY_LIST_RES,
            json!({"depositWithdraw": [{
                "operationType": 0, "balanceHistoryId": 1, "balance": 110_000, "delta": 10_000,
                "changeBalanceTimestamp": 5, "moneyDigits": 2
            }]}),
        ),
        (
            payload::DEAL_LIST_BY_POSITION_ID_REQ,
            payload::DEAL_LIST_BY_POSITION_ID_RES,
            json!({"deal": [{
                "dealId": 4, "orderId": 9, "positionId": 77, "volume": 100, "filledVolume": 100,
                "symbolId": 1, "createTimestamp": 10, "executionTimestamp": 11,
                "tradeSide": 1, "dealStatus": 2
            }], "hasMore": false}),
        ),
        (
            payload::ORDER_LIST_BY_POSITION_ID_REQ,
            payload::ORDER_LIST_BY_POSITION_ID_RES,
            json!({"order": [], "hasMore": false}),
        ),
        (
            payload::ORDER_DETAILS_REQ,
            payload::ORDER_DETAILS_RES,
            json!({"order": {
                "orderId": 9, "tradeData": {"symbolId": 1, "volume": 100, "tradeSide": 1},
                "orderType": 1, "orderStatus": 2
            }, "deal": []}),
        ),
        (
            payload::DEAL_OFFSET_LIST_REQ,
            payload::DEAL_OFFSET_LIST_RES,
            json!({"offsetBy": [{"dealId": 4, "volume": 100}], "offsetting": []}),
        ),
        (
            payload::GET_POSITION_UNREALIZED_PNL_REQ,
            payload::GET_POSITION_UNREALIZED_PNL_RES,
            json!({"positionUnrealizedPnL": [
                {"positionId": 77, "grossUnrealizedPnL": 500, "netUnrealizedPnL": 480}
            ], "moneyDigits": 2}),
        ),
    ]))
    .await;
    let client = connect(&server).await;
    let data = client.account(1).account_data();

    let cash_flow = data.cash_flow_history(0, 1000).await.unwrap();
    assert_eq!(cash_flow[0].amount(), 100.0);

    let (deals, more) = data.deals_by_position(77, None, None).await.unwrap();
    assert_eq!(deals.len(), 1);
    assert!(!more);

    let (orders, more) = data.orders_by_position(77, None, None).await.unwrap();
    assert!(orders.is_empty() && !more);

    let (order, deals) = data.order_details(9).await.unwrap();
    assert_eq!(order.order_id, 9);
    assert!(deals.is_empty());

    let (offset_by, offsetting) = data.deal_offsets(4).await.unwrap();
    assert_eq!(offset_by[0].deal_id, 4);
    assert!(offsetting.is_empty());

    let pnl = data.position_unrealized_pnl().await.unwrap();
    assert_eq!(pnl[0].net_unrealized_pnl, 480);
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
