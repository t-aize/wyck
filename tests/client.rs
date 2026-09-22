//! The connection against a scripted local server: sign in, requests, answers, errors, events, the
//! end of the connection, and rate limit refusals. Market data calls are in `tests/market.rs`.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::json;
use support::{MockServer, Reply, answers, config, connect};
use wyck::openapi::config::{ClientCredentials, ConnectionConfig};
use wyck::openapi::transport::wire::payload;
use wyck::openapi::{Client, ConnectionState, DisconnectReason, ErrorKind, Event, OpenApiError};

// ---- sign in ----

#[tokio::test]
async fn the_application_sign_in_sends_the_credentials_in_the_documented_shape() {
    let server = MockServer::start(answers(vec![(
        payload::APPLICATION_AUTH_REQ,
        payload::APPLICATION_AUTH_RES,
        json!({}),
    )]))
    .await;
    let client = connect(&server).await;
    client
        .authenticate_application(&ClientCredentials::new("the-id", "the-secret"))
        .await
        .unwrap();

    let sent = server.received_of(payload::APPLICATION_AUTH_REQ);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].payload,
        json!({"clientId": "the-id", "clientSecret": "the-secret"})
    );
    assert!(sent[0].client_msg_id.is_some(), "every request is tagged");
}

#[tokio::test]
async fn the_account_list_reads_ids_and_the_account_is_authorized_with_its_token() {
    let server = MockServer::start(answers(vec![
        (
            payload::GET_ACCOUNTS_BY_ACCESS_TOKEN_REQ,
            payload::GET_ACCOUNTS_BY_ACCESS_TOKEN_RES,
            json!({"accessToken": "AT", "permissionScope": 0, "ctidTraderAccount": [
                {"ctidTraderAccountId": "40212", "isLive": false, "traderLogin": 900001},
                {"ctidTraderAccountId": 40213, "isLive": true}
            ]}),
        ),
        (
            payload::ACCOUNT_AUTH_REQ,
            payload::ACCOUNT_AUTH_RES,
            json!({"ctidTraderAccountId": 40212}),
        ),
    ]))
    .await;
    let client = connect(&server).await;
    let accounts = client.accounts("AT").await.unwrap();
    assert_eq!(accounts.ctid_trader_account.len(), 2);
    assert_eq!(
        accounts.ctid_trader_account[0].ctid_trader_account_id,
        40_212
    );
    assert_eq!(accounts.ctid_trader_account[1].is_live, Some(true));

    client.account(40_212).authorize("AT").await.unwrap();
    assert_eq!(
        server.received_of(payload::ACCOUNT_AUTH_REQ)[0].payload,
        json!({"ctidTraderAccountId": 40212, "accessToken": "AT"})
    );
}

#[tokio::test]
async fn the_version_is_decoded_and_the_ctid_profile_needs_only_a_token() {
    let server = MockServer::start(answers(vec![
        (
            payload::VERSION_REQ,
            payload::VERSION_RES,
            json!({"version": "84.2"}),
        ),
        (
            payload::GET_CTID_PROFILE_BY_TOKEN_REQ,
            payload::GET_CTID_PROFILE_BY_TOKEN_RES,
            json!({"profile": {"userId": 12345}}),
        ),
    ]))
    .await;
    let client = connect(&server).await;
    assert_eq!(client.version().await.unwrap(), "84.2");
    assert_eq!(client.ctid_profile("tok").await.unwrap().user_id, 12_345);
    assert_eq!(
        server.received_of(payload::GET_CTID_PROFILE_BY_TOKEN_REQ)[0].payload,
        json!({"accessToken": "tok"})
    );
}

// ---- errors ----

#[tokio::test]
async fn a_server_error_becomes_a_typed_error_with_its_advice() {
    let server = MockServer::start(answers(vec![(
        payload::VERSION_REQ,
        payload::ERROR_RES,
        json!({"errorCode": "REQUEST_FREQUENCY_EXCEEDED", "description": "slow down", "retryAfter": 3}),
    )]))
    .await;
    let mut config = config(&server);
    config.rate_limit_retries = 0; // look at the error itself
    let client = Client::connect(&config).await.unwrap();
    let error = client.version().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::RateLimited);
    assert!(error.is_retryable());
    assert_eq!(
        error.retry_after(),
        Some(Duration::from_secs(3)),
        "the server sends seconds"
    );
    assert_eq!(error.code(), Some("REQUEST_FREQUENCY_EXCEEDED"));
}

#[tokio::test]
async fn a_token_error_is_told_apart_from_a_refusal() {
    let server = MockServer::start(answers(vec![
        (
            payload::GET_ACCOUNTS_BY_ACCESS_TOKEN_REQ,
            payload::ERROR_RES,
            json!({"errorCode": "CH_ACCESS_TOKEN_INVALID"}),
        ),
        (
            payload::ACCOUNT_AUTH_REQ,
            payload::ERROR_RES,
            json!({"errorCode": "ACCOUNT_NOT_AUTHORIZED", "description": "unknown"}),
        ),
    ]))
    .await;
    let client = connect(&server).await;
    let token = client.accounts("bad").await.unwrap_err();
    assert_eq!(token.kind(), ErrorKind::TokenInvalid);
    assert!(!token.is_retryable());
    let refused = client.account(1).authorize("bad").await.unwrap_err();
    assert_eq!(refused.kind(), ErrorKind::NotAuthorized);
}

#[tokio::test]
async fn an_answer_of_the_wrong_type_is_a_protocol_error() {
    let server = MockServer::start(answers(vec![(
        payload::VERSION_REQ,
        payload::GET_ACCOUNTS_BY_ACCESS_TOKEN_RES,
        json!({}),
    )]))
    .await;
    let client = connect(&server).await;
    let error = client.version().await.unwrap_err();
    assert!(matches!(error, OpenApiError::Protocol(_)), "{error:?}");
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_is_a_protocol_error() {
    let server = MockServer::start(answers(vec![(
        payload::VERSION_REQ,
        payload::VERSION_RES,
        json!({"version": {"not": "text"}}),
    )]))
    .await;
    let client = connect(&server).await;
    assert!(matches!(
        client.version().await,
        Err(OpenApiError::Protocol(_))
    ));
}

#[tokio::test]
async fn silence_ends_in_a_timeout_and_a_late_answer_does_no_harm() {
    let server = MockServer::start(Arc::new(|request| match request.payload_type {
        payload::VERSION_REQ => vec![Reply::AnswerAfter(
            Duration::from_millis(400),
            payload::VERSION_RES,
            json!({"version": "late"}),
        )],
        _ => vec![],
    }))
    .await;
    let mut config = config(&server);
    config.request_timeout = Duration::from_millis(150);
    let client = Client::connect(&config).await.unwrap();

    let error = client.version().await.unwrap_err();
    assert!(matches!(error, OpenApiError::Timeout { .. }));
    assert!(error.is_retryable());
    // The late answer arrives while nothing waits for it, and the client carries on.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!client.is_closed());
}

// ---- many requests at once ----

#[tokio::test]
async fn answers_are_matched_to_their_requests_even_out_of_order() {
    let counter = Arc::new(AtomicUsize::new(0));
    let server = MockServer::start(Arc::new(move |request| {
        if request.payload_type != payload::GET_CTID_PROFILE_BY_TOKEN_REQ {
            return vec![];
        }
        // The first request is answered last.
        let delay = if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            300
        } else {
            10
        };
        let token = request.payload["accessToken"].as_str().unwrap().to_owned();
        let user_id: i64 = token.parse().unwrap();
        vec![Reply::AnswerAfter(
            Duration::from_millis(delay),
            payload::GET_CTID_PROFILE_BY_TOKEN_RES,
            json!({"profile": {"userId": user_id}}),
        )]
    }))
    .await;
    let client = connect(&server).await;
    let (a, b, c) = tokio::join!(
        client.ctid_profile("10"),
        client.ctid_profile("20"),
        client.ctid_profile("30"),
    );
    assert_eq!(a.unwrap().user_id, 10);
    assert_eq!(b.unwrap().user_id, 20);
    assert_eq!(c.unwrap().user_id, 30);
}

// ---- events ----

#[tokio::test]
async fn every_reader_of_the_events_sees_them() {
    let server = MockServer::start(answers(vec![])).await;
    let client = connect(&server).await;
    let (mut one, mut two) = (client.events(), client.events());
    server.push(
        payload::DEPTH_EVENT,
        json!({"symbolId": 3, "newQuotes": [{"id": 1, "size": 500, "bid": 100}]}),
    );
    for reader in [&mut one, &mut two] {
        let event = tokio::time::timeout(Duration::from_secs(2), reader.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(event, Event::Depth(d) if d.symbol_id == 3));
    }
}

#[tokio::test]
async fn a_token_notice_and_an_unknown_message_are_events_too() {
    let server = MockServer::start(answers(vec![])).await;
    let client = connect(&server).await;
    let mut events = client.events();
    server.push(
        payload::ACCOUNTS_TOKEN_INVALIDATED_EVENT,
        json!({"ctidTraderAccountIds": [7], "reason": "revoked"}),
    );
    server.push(4242, json!({"hello": true}));
    let first = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(first, Event::TokensInvalidated(e) if e.ctid_trader_account_ids == [7]));
    let second = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        second,
        Event::Other {
            payload_type: 4242,
            ..
        }
    ));
}

// ---- the life of the connection ----

#[tokio::test]
async fn heartbeats_keep_flowing() {
    let server = MockServer::start(answers(vec![])).await;
    let _client = connect(&server).await;
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert!(
        server.heartbeats() >= 3,
        "only {} heartbeats",
        server.heartbeats()
    );
}

#[tokio::test]
async fn when_the_server_closes_waiting_requests_fail_and_the_end_is_announced() {
    let server = MockServer::start(Arc::new(|request| match request.payload_type {
        payload::VERSION_REQ => vec![Reply::CloseSocket],
        _ => vec![],
    }))
    .await;
    let client = connect(&server).await;
    let mut events = client.events();
    let mut state = client.state();

    let error = client.version().await.unwrap_err();
    assert!(matches!(error, OpenApiError::Closed), "{error:?}");

    let last = loop {
        let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap();
        if let Event::Disconnected(reason) = event {
            break reason;
        }
    };
    assert_eq!(last, DisconnectReason::ClosedByServer);
    state
        .wait_for(|s| matches!(s, ConnectionState::Closed(_)))
        .await
        .unwrap();
    assert!(client.is_closed());
    // A closed client fails fast instead of waiting for a timeout.
    let started = Instant::now();
    assert!(matches!(client.version().await, Err(OpenApiError::Closed)));
    assert!(started.elapsed() < Duration::from_millis(500));
}

#[tokio::test]
async fn closing_the_client_ends_the_connection_once() {
    let server = MockServer::start(answers(vec![])).await;
    let client = connect(&server).await;
    let mut events = client.events();
    client.close().await;
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event, Event::Disconnected(DisconnectReason::ClosedByClient));
    assert_eq!(
        *client.state().borrow(),
        ConnectionState::Closed(DisconnectReason::ClosedByClient)
    );
    client.close().await; // does nothing
    assert!(matches!(client.version().await, Err(OpenApiError::Closed)));
}

#[tokio::test]
async fn a_server_that_announces_its_end_gives_its_reason() {
    let server = MockServer::start(answers(vec![])).await;
    let client = connect(&server).await;
    let mut events = client.events();
    server.push(
        payload::CLIENT_DISCONNECT_EVENT,
        json!({"reason": "maintenance"}),
    );
    let mut saw_notice = false;
    let reason = loop {
        match tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            Event::ServerDisconnecting(e) => {
                saw_notice = e.reason.as_deref() == Some("maintenance");
            }
            Event::Disconnected(reason) => break reason,
            _ => {}
        }
    };
    assert!(saw_notice);
    assert_eq!(
        reason,
        DisconnectReason::ServerAnnounced(Some("maintenance".into()))
    );
}

#[tokio::test]
async fn connecting_to_nothing_is_a_transport_error() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    drop(listener);
    let error = Client::connect(&ConnectionConfig::with_url(url))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
}

#[tokio::test]
async fn unusable_settings_are_refused_before_connecting() {
    let error = Client::connect(&ConnectionConfig::with_url("http://nope"))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Config);
}

// ---- rate limit refusals ----

fn blocked_then_ok(blocked_times: usize) -> (Arc<AtomicUsize>, support::Handler) {
    let seen = Arc::new(AtomicUsize::new(0));
    let counter = seen.clone();
    let handler: support::Handler = Arc::new(move |request| {
        if request.payload_type != payload::VERSION_REQ {
            return vec![];
        }
        if counter.fetch_add(1, Ordering::SeqCst) < blocked_times {
            vec![Reply::Answer(
                payload::ERROR_RES,
                json!({"errorCode": "BLOCKED_PAYLOAD_TYPE", "description": "You are being rate limited", "retryAfter": 0}),
            )]
        } else {
            vec![Reply::Answer(
                payload::VERSION_RES,
                json!({"version": "ok"}),
            )]
        }
    });
    (seen, handler)
}

#[tokio::test]
async fn a_request_refused_for_its_rate_is_sent_again_after_the_wait() {
    let (seen, handler) = blocked_then_ok(1);
    let server = MockServer::start(handler).await;
    let client = connect(&server).await;
    let started = Instant::now();
    assert_eq!(client.version().await.unwrap(), "ok");
    assert_eq!(seen.load(Ordering::SeqCst), 2, "one refusal, one success");
    assert!(
        started.elapsed() >= Duration::from_millis(300),
        "it waited: {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn retries_stop_after_the_configured_number() {
    let (seen, handler) = blocked_then_ok(usize::MAX);
    let server = MockServer::start(handler).await;
    let mut config = config(&server);
    config.rate_limit_retries = 2;
    let client = Client::connect(&config).await.unwrap();
    let error = client.version().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::RateLimited);
    assert_eq!(
        seen.load(Ordering::SeqCst),
        3,
        "the first try and two retries"
    );
}

#[tokio::test]
async fn with_retries_off_the_refusal_is_returned_at_once() {
    let (seen, handler) = blocked_then_ok(usize::MAX);
    let server = MockServer::start(handler).await;
    let mut config = config(&server);
    config.rate_limit_retries = 0;
    let client = Client::connect(&config).await.unwrap();
    let started = Instant::now();
    assert!(client.version().await.is_err());
    assert_eq!(seen.load(Ordering::SeqCst), 1);
    assert!(started.elapsed() < Duration::from_millis(250));
}

#[tokio::test]
async fn other_errors_are_never_retried() {
    let seen = Arc::new(AtomicUsize::new(0));
    let counter = seen.clone();
    let server = MockServer::start(Arc::new(move |request| {
        if request.payload_type == payload::VERSION_REQ {
            counter.fetch_add(1, Ordering::SeqCst);
            vec![Reply::Answer(
                payload::ERROR_RES,
                json!({"errorCode": "SYMBOL_NOT_FOUND"}),
            )]
        } else {
            vec![]
        }
    }))
    .await;
    let client = connect(&server).await;
    assert!(client.version().await.is_err());
    assert_eq!(seen.load(Ordering::SeqCst), 1);
}
