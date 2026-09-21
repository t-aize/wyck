//! Trading (order placement, amend, cancel, position close) against the scripted server: what is
//! sent, the execution events that come back, and the error paths a trading call can hit.

mod support;

use std::time::Duration;

use ctrader_openapi::account::TradeSide;
use ctrader_openapi::config::ConnectionConfig;
use ctrader_openapi::trading::{AmendOrderReq, AmendPositionSlTpReq, ExecutionType, NewOrderReq};
use ctrader_openapi::transport::wire::payload;
use ctrader_openapi::{Client, ErrorKind};
use serde_json::json;
use support::{MockServer, answers};

async fn connect(server: &MockServer) -> Client {
    let mut config = ConnectionConfig::with_url(server.url.clone());
    config.heartbeat_interval = Duration::from_millis(200);
    Client::connect(&config).await.unwrap()
}

#[tokio::test]
async fn a_market_order_is_sent_and_its_execution_is_read() {
    let server = MockServer::start(answers(vec![(
        payload::NEW_ORDER_REQ,
        payload::EXECUTION_EVENT,
        json!({"ctidTraderAccountId": 1, "executionType": 3, "deal": {
            "dealId": 1, "orderId": 2, "positionId": 3, "volume": 10000, "filledVolume": 10000,
            "symbolId": 1, "createTimestamp": 1, "executionTimestamp": 2,
            "tradeSide": 1, "dealStatus": 2, "executionPrice": 1.1
        }}),
    )]))
    .await;
    let trading = connect(&server).await.account(1).trading();
    let request = NewOrderReq::market(1, TradeSide::Buy, 10_000).with_label("wyck-test");
    let execution = trading.new_order(request).await.unwrap();
    assert_eq!(execution.kind(), Some(ExecutionType::OrderFilled));
    assert_eq!(execution.deal.as_ref().unwrap().deal_id, 1);

    let sent = &server.received_of(payload::NEW_ORDER_REQ)[0].payload;
    assert_eq!(sent["ctidTraderAccountId"], 1);
    assert_eq!(sent["orderType"], 1);
    assert_eq!(sent["tradeSide"], 1);
    assert_eq!(sent["volume"], 10000);
    assert_eq!(sent["label"], "wyck-test");
}

#[tokio::test]
async fn a_limit_order_carries_its_price_and_protection() {
    let server = MockServer::start(answers(vec![(
        payload::NEW_ORDER_REQ,
        payload::EXECUTION_EVENT,
        json!({"ctidTraderAccountId": 1, "executionType": 2}),
    )]))
    .await;
    let trading = connect(&server).await.account(1).trading();
    let request = NewOrderReq::limit(1, TradeSide::Sell, 5_000, 1.2345)
        .with_protection(Some(1.30), Some(1.10));
    let execution = trading.new_order(request).await.unwrap();
    assert_eq!(execution.kind(), Some(ExecutionType::OrderAccepted));

    let sent = &server.received_of(payload::NEW_ORDER_REQ)[0].payload;
    assert_eq!(sent["orderType"], 2);
    assert_eq!(sent["limitPrice"], 1.2345);
    assert_eq!(sent["stopLoss"], 1.30);
    assert_eq!(sent["takeProfit"], 1.10);
}

#[tokio::test]
async fn cancel_amend_and_close_send_the_right_ids() {
    let server = MockServer::start(answers(vec![
        (
            payload::CANCEL_ORDER_REQ,
            payload::EXECUTION_EVENT,
            json!({"executionType": 5}),
        ),
        (
            payload::AMEND_ORDER_REQ,
            payload::EXECUTION_EVENT,
            json!({"executionType": 4}),
        ),
        (
            payload::CLOSE_POSITION_REQ,
            payload::EXECUTION_EVENT,
            json!({"executionType": 3}),
        ),
        (
            payload::AMEND_POSITION_SLTP_REQ,
            payload::EXECUTION_EVENT,
            json!({"executionType": 4}),
        ),
    ]))
    .await;
    let trading = connect(&server).await.account(1).trading();

    let cancelled = trading.cancel_order(9).await.unwrap();
    assert_eq!(cancelled.kind(), Some(ExecutionType::OrderCancelled));
    assert_eq!(
        server.received_of(payload::CANCEL_ORDER_REQ)[0].payload,
        json!({"ctidTraderAccountId": 1, "orderId": 9})
    );

    let mut amend = AmendOrderReq::new(9);
    amend.stop_loss = Some(1.15);
    let amended = trading.amend_order(amend).await.unwrap();
    assert_eq!(amended.kind(), Some(ExecutionType::OrderReplaced));
    assert_eq!(
        server.received_of(payload::AMEND_ORDER_REQ)[0].payload,
        json!({"ctidTraderAccountId": 1, "orderId": 9, "stopLoss": 1.15})
    );

    let closed = trading.close_position(77, 5_000).await.unwrap();
    assert_eq!(closed.kind(), Some(ExecutionType::OrderFilled));
    assert_eq!(
        server.received_of(payload::CLOSE_POSITION_REQ)[0].payload,
        json!({"ctidTraderAccountId": 1, "positionId": 77, "volume": 5000})
    );

    let mut sltp = AmendPositionSlTpReq::new(77);
    sltp.take_profit = Some(1.5);
    let amended_sltp = trading.amend_position_sl_tp(sltp).await.unwrap();
    assert_eq!(amended_sltp.kind(), Some(ExecutionType::OrderReplaced));
    assert_eq!(
        server.received_of(payload::AMEND_POSITION_SLTP_REQ)[0].payload,
        json!({"ctidTraderAccountId": 1, "positionId": 77, "takeProfit": 1.5})
    );
}

#[tokio::test]
async fn a_bad_volume_is_told_apart_from_insufficient_margin() {
    let server = MockServer::start(answers(vec![(
        payload::NEW_ORDER_REQ,
        payload::ERROR_RES,
        json!({"errorCode": "TRADING_BAD_VOLUME", "description": "volume must be a multiple of the step"}),
    )]))
    .await;
    let trading = connect(&server).await.account(1).trading();
    let request = NewOrderReq::market(1, TradeSide::Buy, 1);
    let error = trading.new_order(request).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Rejected);
    assert_eq!(error.code(), Some("TRADING_BAD_VOLUME"));
    assert!(!error.is_retryable());
}

#[tokio::test]
async fn not_enough_money_is_a_rejection_not_a_protocol_error() {
    let server = MockServer::start(answers(vec![(
        payload::NEW_ORDER_REQ,
        payload::ERROR_RES,
        json!({"errorCode": "NOT_ENOUGH_MONEY"}),
    )]))
    .await;
    let trading = connect(&server).await.account(1).trading();
    let request = NewOrderReq::market(1, TradeSide::Buy, 100_000_000);
    let error = trading.new_order(request).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Rejected);
    assert_eq!(error.code(), Some("NOT_ENOUGH_MONEY"));
}

#[tokio::test]
async fn cancelling_an_unknown_order_is_reported() {
    let server = MockServer::start(answers(vec![(
        payload::CANCEL_ORDER_REQ,
        payload::ERROR_RES,
        json!({"errorCode": "ORDER_NOT_FOUND"}),
    )]))
    .await;
    let trading = connect(&server).await.account(1).trading();
    let error = trading.cancel_order(404).await.unwrap_err();
    assert_eq!(error.code(), Some("ORDER_NOT_FOUND"));
}

#[tokio::test]
async fn an_unsolicited_execution_event_arrives_on_the_event_stream() {
    let server = MockServer::start(answers(vec![])).await;
    let client = connect(&server).await;
    let mut events = client.events();
    server.push(
        payload::EXECUTION_EVENT,
        json!({"ctidTraderAccountId": 1, "executionType": 9, "isServerEvent": true}),
    );
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    match event {
        ctrader_openapi::Event::Execution(e) => {
            assert_eq!(e.kind(), Some(ExecutionType::Swap));
            assert_eq!(e.is_server_event, Some(true));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn an_order_error_event_and_a_trailing_stop_change_arrive_as_events() {
    let server = MockServer::start(answers(vec![])).await;
    let client = connect(&server).await;
    let mut events = client.events();

    server.push(
        payload::ORDER_ERROR_EVENT,
        json!({"errorCode": "POSITION_NOT_FOUND", "positionId": 5}),
    );
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        event,
        ctrader_openapi::Event::OrderError(e) if e.error_code == "POSITION_NOT_FOUND" && e.position_id == Some(5)
    ));

    server.push(
        payload::TRAILING_SL_CHANGED_EVENT,
        json!({"positionId": 5, "orderId": 6, "stopPrice": 1.09, "utcLastUpdateTimestamp": 123}),
    );
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        event,
        ctrader_openapi::Event::TrailingSlChanged(e) if e.stop_price == 1.09
    ));
}
