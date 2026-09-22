//! Margin calls against the scripted server: expected margin, margin call thresholds, dynamic
//! leverage.

mod support;

use std::time::Duration;

use serde_json::json;
use support::{MockServer, answers, connect};
use wyck::openapi::margin::{MarginCall, MarginCallType};
use wyck::openapi::transport::wire::payload;
use wyck::openapi::{ErrorKind, Event};

#[tokio::test]
async fn expected_margin_is_read_for_every_volume_asked() {
    let server = MockServer::start(answers(vec![(
        payload::EXPECTED_MARGIN_REQ,
        payload::EXPECTED_MARGIN_RES,
        json!({"margin": [
            {"volume": 100000, "buyMargin": 2000, "sellMargin": 2000},
            {"volume": 200000, "buyMargin": 4000, "sellMargin": 4000}
        ], "moneyDigits": 2}),
    )]))
    .await;
    let client = connect(&server).await;
    let margins = client
        .account(1)
        .margin()
        .expected_margin(1, &[100_000, 200_000])
        .await
        .unwrap();
    assert_eq!(margins.len(), 2);
    assert_eq!(margins[1].buy_margin, 4000);
    assert_eq!(
        server.received_of(payload::EXPECTED_MARGIN_REQ)[0].payload,
        json!({"ctidTraderAccountId": 1, "symbolId": 1, "volume": [100000, 200000]})
    );
}

#[tokio::test]
async fn margin_calls_are_listed_and_one_threshold_can_be_updated() {
    let server = MockServer::start(answers(vec![
        (
            payload::MARGIN_CALL_LIST_REQ,
            payload::MARGIN_CALL_LIST_RES,
            json!({"marginCall": [
                {"marginCallType": 61, "marginLevelThreshold": 100.0},
                {"marginCallType": 62, "marginLevelThreshold": 80.0},
                {"marginCallType": 63, "marginLevelThreshold": 50.0}
            ]}),
        ),
        (
            payload::MARGIN_CALL_UPDATE_REQ,
            payload::MARGIN_CALL_UPDATE_RES,
            json!({}),
        ),
    ]))
    .await;
    let client = connect(&server).await;
    let margin = client.account(1).margin();
    let calls = margin.margin_calls().await.unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].kind(), Some(MarginCallType::First));

    margin
        .update_margin_call(MarginCall {
            margin_call_type: 61,
            margin_level_threshold: 120.0,
            utc_last_update_timestamp: None,
        })
        .await
        .unwrap();
    let sent = &server.received_of(payload::MARGIN_CALL_UPDATE_REQ)[0].payload;
    assert_eq!(sent["marginCall"]["marginLevelThreshold"], 120.0);
}

#[tokio::test]
async fn a_dynamic_leverage_schedule_is_read_with_its_tiers() {
    let server = MockServer::start(answers(vec![(
        payload::GET_DYNAMIC_LEVERAGE_REQ,
        payload::GET_DYNAMIC_LEVERAGE_RES,
        json!({"leverage": {"leverageId": 9, "tiers": [
            {"volume": 100000000, "leverage": 100},
            {"volume": 500000000, "leverage": 50}
        ]}}),
    )]))
    .await;
    let client = connect(&server).await;
    let leverage = client
        .account(1)
        .margin()
        .dynamic_leverage(9)
        .await
        .unwrap();
    assert_eq!(leverage.leverage_id, 9);
    assert_eq!(leverage.tiers.len(), 2);
    assert_eq!(leverage.tiers[0].leverage, 100);
}

#[tokio::test]
async fn margin_change_and_call_trigger_events_arrive_as_events() {
    let server = MockServer::start(answers(vec![])).await;
    let client = connect(&server).await;
    let mut events = client.events();

    server.push(
        payload::MARGIN_CHANGED_EVENT,
        json!({"positionId": 1, "usedMargin": 1500, "moneyDigits": 2}),
    );
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, Event::MarginChanged(e) if e.used_margin == 1500));

    server.push(
        payload::MARGIN_CALL_TRIGGER_EVENT,
        json!({"marginCall": {"marginCallType": 63, "marginLevelThreshold": 50.0}}),
    );
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        event,
        Event::MarginCallTriggered(e) if e.margin_call.kind() == Some(MarginCallType::Third)
    ));
}

#[tokio::test]
async fn an_unknown_leverage_id_is_a_rejection() {
    let server = MockServer::start(answers(vec![(
        payload::GET_DYNAMIC_LEVERAGE_REQ,
        payload::ERROR_RES,
        json!({"errorCode": "UNKNOWN_SYMBOL"}),
    )]))
    .await;
    let client = connect(&server).await;
    let error = client
        .account(1)
        .margin()
        .dynamic_leverage(404)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Rejected);
}
