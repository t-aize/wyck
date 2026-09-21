//! The session against the scripted server: it must come up, stay up across dropped connections,
//! restore what the program subscribed to, renew the tokens, and stop cleanly.

mod support;

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use ctrader_openapi::auth::{TokenSet, parse_token_response};
use ctrader_openapi::config::{ClientCredentials, ConnectionConfig};
use ctrader_openapi::session::{
    Backoff, MemoryTokenStore, Session, SessionConfig, SessionEvent, SessionState, TokenStore,
};
use ctrader_openapi::transport::wire::payload;
use ctrader_openapi::{ErrorKind, Event, OpenApiError};
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use support::http::{TokenServer, token_server_sequence, tokens_body};
use support::{Handler, MockServer, Reply, answers};
use tokio::sync::broadcast::Receiver;

const ACCOUNT: i64 = 48_332_955;

fn tokens(access: &str, refresh: &str, expires_in_secs: i64) -> TokenSet {
    let body = tokens_body(access, refresh, expires_in_secs);
    parse_token_response(body.as_bytes(), SystemTime::now()).unwrap()
}

/// The answers a healthy server gives to everything a session sends.
fn healthy() -> Vec<(u32, u32, Value)> {
    vec![
        (
            payload::APPLICATION_AUTH_REQ,
            payload::APPLICATION_AUTH_RES,
            json!({}),
        ),
        (
            payload::ACCOUNT_AUTH_REQ,
            payload::ACCOUNT_AUTH_RES,
            json!({"ctidTraderAccountId": ACCOUNT}),
        ),
        (
            payload::SUBSCRIBE_SPOTS_REQ,
            payload::SUBSCRIBE_SPOTS_RES,
            json!({}),
        ),
        (
            payload::UNSUBSCRIBE_SPOTS_REQ,
            payload::UNSUBSCRIBE_SPOTS_RES,
            json!({}),
        ),
        (
            payload::SUBSCRIBE_LIVE_TRENDBAR_REQ,
            payload::SUBSCRIBE_LIVE_TRENDBAR_RES,
            json!({}),
        ),
        (
            payload::SUBSCRIBE_DEPTH_QUOTES_REQ,
            payload::SUBSCRIBE_DEPTH_QUOTES_RES,
            json!({}),
        ),
    ]
}

fn config(url: &str) -> SessionConfig {
    let mut connection = ConnectionConfig::with_url(url);
    connection.heartbeat_interval = Duration::from_millis(200);
    connection.request_timeout = Duration::from_secs(3);
    let mut config = SessionConfig::new(
        connection,
        ClientCredentials::new("the-id", "the-secret"),
        ACCOUNT,
    );
    config.backoff = Backoff {
        initial: Duration::from_millis(20),
        max: Duration::from_millis(100),
        factor: 2,
    };
    config
}

fn start(config: SessionConfig, tokens: TokenSet) -> (Session, Arc<MemoryTokenStore>) {
    let store = Arc::new(MemoryTokenStore::default());
    let session = Session::start(config, tokens, store.clone()).unwrap();
    (session, store)
}

/// Waits for the first event the predicate accepts, skipping the others.
async fn next_event<F>(events: &mut Receiver<SessionEvent>, mut accept: F) -> SessionEvent
where
    F: FnMut(&SessionEvent) -> bool,
{
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.expect("the session ended");
            if accept(&event) {
                return event;
            }
        }
    })
    .await
    .expect("the expected session event did not come")
}

fn is_ready(event: &SessionEvent) -> bool {
    matches!(event, SessionEvent::Ready)
}

/// Waits for the session to notice a drop and come back: the `Ready` of the first connection is
/// still in the channel, so wait for the drop before looking for the next `Ready`.
async fn reconnected(events: &mut Receiver<SessionEvent>) {
    next_event(events, |e| matches!(e, SessionEvent::Reconnecting { .. })).await;
    next_event(events, is_ready).await;
}

fn spot_ids(server: &MockServer) -> Vec<Vec<i64>> {
    server
        .received_of(payload::SUBSCRIBE_SPOTS_REQ)
        .iter()
        .map(|r| {
            r.payload["symbolId"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap())
                .collect()
        })
        .collect()
}

// ---- coming up ----

#[tokio::test]
async fn a_session_signs_the_application_and_the_account_in_and_becomes_ready() {
    let server = MockServer::start(answers(healthy())).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();

    let client = session.wait_ready(Duration::from_secs(5)).await.unwrap();
    assert!(!client.is_closed());
    assert_eq!(*session.state().borrow(), SessionState::Ready);

    let app = server.received_of(payload::APPLICATION_AUTH_REQ);
    assert_eq!(
        app[0].payload,
        json!({"clientId": "the-id", "clientSecret": "the-secret"})
    );
    let account = server.received_of(payload::ACCOUNT_AUTH_REQ);
    assert_eq!(
        account[0].payload,
        json!({"ctidTraderAccountId": ACCOUNT, "accessToken": "AT-1"})
    );
    // The Ready event was sent even if this reader started after it: a later one proves the type.
    session.stop().await;
    next_event(&mut events, |e| matches!(e, SessionEvent::Stopped)).await;
}

#[tokio::test]
async fn subscriptions_made_before_the_connection_is_up_are_applied_when_it_is() {
    let server = MockServer::start(answers(healthy())).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    // Recorded at once; the connection is not up yet.
    session.subscribe_spots(&[1, 2]).await.unwrap();
    session
        .subscribe_live_bars(1, ctrader_openapi::market::Period::M5)
        .await
        .unwrap();
    session.subscribe_depth(&[3]).await.unwrap();
    session.wait_ready(Duration::from_secs(5)).await.unwrap();

    assert!(spot_ids(&server).contains(&vec![1, 2]));
    let bars = server.received_of(payload::SUBSCRIBE_LIVE_TRENDBAR_REQ);
    assert_eq!(bars[0].payload["period"], 5);
    assert_eq!(bars[0].payload["symbolId"], 1);
    assert_eq!(
        server.received_of(payload::SUBSCRIBE_DEPTH_QUOTES_REQ)[0].payload["symbolId"],
        json!([3])
    );
    session.stop().await;
}

#[tokio::test]
async fn the_accessors_report_the_account_and_the_current_tokens() {
    let server = MockServer::start(answers(healthy())).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    assert_eq!(session.account_id(), ACCOUNT);
    assert_eq!(session.tokens().access_token.expose_secret(), "AT-1");
    assert!(format!("{session:?}").contains("Session"));
    session.stop().await;
}

// ---- staying up ----

#[tokio::test]
async fn a_dropped_connection_is_replaced_and_the_subscriptions_come_back() {
    let server = MockServer::start(answers(healthy())).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    session.wait_ready(Duration::from_secs(5)).await.unwrap();
    session.subscribe_spots(&[7]).await.unwrap();
    assert_eq!(server.connections(), 1);

    server.close();
    let down = next_event(&mut events, |e| {
        matches!(e, SessionEvent::Reconnecting { .. })
    })
    .await;
    match down {
        SessionEvent::Reconnecting {
            attempt, retry_in, ..
        } => {
            assert_eq!(attempt, 1);
            assert!(retry_in >= Duration::from_millis(20));
        }
        other => panic!("{other:?}"),
    }
    next_event(&mut events, is_ready).await;

    assert_eq!(server.connections(), 2);
    assert_eq!(
        server.received_of(payload::APPLICATION_AUTH_REQ).len(),
        2,
        "signed in again"
    );
    assert_eq!(server.received_of(payload::ACCOUNT_AUTH_REQ).len(), 2);
    assert_eq!(
        spot_ids(&server),
        vec![vec![7], vec![7]],
        "restored after the reconnect"
    );
    session.stop().await;
}

#[tokio::test]
async fn the_session_survives_several_drops_in_a_row() {
    let server = MockServer::start(answers(healthy())).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    session.wait_ready(Duration::from_secs(5)).await.unwrap();
    for round in 2..=4 {
        server.close();
        reconnected(&mut events).await;
        assert_eq!(server.connections(), round);
    }
    session.stop().await;
}

#[tokio::test]
async fn events_are_forwarded_before_and_after_a_reconnect() {
    let server = MockServer::start(answers(healthy())).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    session.wait_ready(Duration::from_secs(5)).await.unwrap();

    server.push(
        payload::SPOT_EVENT,
        json!({"symbolId": 1, "bid": 100, "ask": 102}),
    );
    let first = next_event(&mut events, |e| matches!(e, SessionEvent::Data(_))).await;
    assert!(matches!(first, SessionEvent::Data(Event::Spot(s)) if s.bid == Some(100)));

    server.close();
    next_event(&mut events, is_ready).await;
    server.push(payload::SPOT_EVENT, json!({"symbolId": 1, "bid": 101}));
    let second = next_event(&mut events, |e| matches!(e, SessionEvent::Data(_))).await;
    assert!(matches!(second, SessionEvent::Data(Event::Spot(s)) if s.bid == Some(101)));
    session.stop().await;
}

#[tokio::test]
async fn the_client_is_none_while_down_and_a_new_one_after() {
    let server = MockServer::start(answers(healthy())).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    let first = session.wait_ready(Duration::from_secs(5)).await.unwrap();
    server.close();
    next_event(&mut events, |e| {
        matches!(e, SessionEvent::Reconnecting { .. })
    })
    .await;
    assert!(session.client().is_none() || first.is_closed());
    next_event(&mut events, is_ready).await;
    let second = session.client().unwrap();
    assert!(!second.is_closed());
    assert!(first.is_closed(), "the old connection is closed");
    session.stop().await;
}

// ---- subscriptions ----

#[tokio::test]
async fn unsubscribing_forgets_a_symbol_so_a_reconnect_does_not_bring_it_back() {
    let server = MockServer::start(answers(healthy())).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    session.wait_ready(Duration::from_secs(5)).await.unwrap();
    session.subscribe_spots(&[1, 2]).await.unwrap();
    session.unsubscribe_spots(&[1]).await.unwrap();
    assert_eq!(
        server.received_of(payload::UNSUBSCRIBE_SPOTS_REQ)[0].payload["symbolId"],
        json!([1])
    );

    server.close();
    reconnected(&mut events).await;
    let restored = spot_ids(&server);
    assert_eq!(
        restored.last().unwrap(),
        &vec![2],
        "only the symbol still wanted"
    );
    session.stop().await;
}

#[tokio::test]
async fn subscribing_to_what_is_already_followed_sends_nothing_more() {
    let server = MockServer::start(answers(healthy())).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    session.wait_ready(Duration::from_secs(5)).await.unwrap();
    session.subscribe_spots(&[1]).await.unwrap();
    session.subscribe_spots(&[1]).await.unwrap();
    assert_eq!(spot_ids(&server), vec![vec![1]]);
    session.stop().await;
}

#[tokio::test]
async fn a_subscription_the_server_refuses_is_reported_and_not_kept() {
    let handler: Handler = Arc::new(|request| match request.payload_type {
        payload::SUBSCRIBE_SPOTS_REQ => vec![Reply::Answer(
            payload::ERROR_RES,
            json!({"errorCode": "SYMBOL_NOT_FOUND"}),
        )],
        payload::APPLICATION_AUTH_REQ => {
            vec![Reply::Answer(payload::APPLICATION_AUTH_RES, json!({}))]
        }
        payload::ACCOUNT_AUTH_REQ => vec![Reply::Answer(
            payload::ACCOUNT_AUTH_RES,
            json!({"ctidTraderAccountId": ACCOUNT}),
        )],
        _ => vec![],
    });
    let server = MockServer::start(handler).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    session.wait_ready(Duration::from_secs(5)).await.unwrap();

    let error = session.subscribe_spots(&[999]).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Rejected);
    server.close();
    reconnected(&mut events).await;
    assert_eq!(
        spot_ids(&server).len(),
        1,
        "the refused symbol was not asked for again"
    );
    session.stop().await;
}

#[tokio::test]
async fn a_refusal_while_restoring_is_reported_and_the_session_stays_up() {
    let handler: Handler = Arc::new(|request| match request.payload_type {
        payload::SUBSCRIBE_SPOTS_REQ => vec![Reply::Answer(
            payload::ERROR_RES,
            json!({"errorCode": "SYMBOL_NOT_FOUND", "description": "gone"}),
        )],
        payload::APPLICATION_AUTH_REQ => {
            vec![Reply::Answer(payload::APPLICATION_AUTH_RES, json!({}))]
        }
        payload::ACCOUNT_AUTH_REQ => vec![Reply::Answer(
            payload::ACCOUNT_AUTH_RES,
            json!({"ctidTraderAccountId": ACCOUNT}),
        )],
        _ => vec![],
    });
    let server = MockServer::start(handler).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    session.subscribe_spots(&[5]).await.unwrap(); // recorded, the connection is not up yet
    let failed = next_event(&mut events, |e| {
        matches!(e, SessionEvent::SubscriptionFailed { .. })
    })
    .await;
    match failed {
        SessionEvent::SubscriptionFailed { what, error } => {
            assert!(what.contains("prices"), "{what}");
            assert_eq!(error.code(), Some("SYMBOL_NOT_FOUND"));
        }
        other => panic!("{other:?}"),
    }
    assert!(session.wait_ready(Duration::from_secs(5)).await.is_ok());
    session.stop().await;
}

#[tokio::test]
async fn already_subscribed_counts_as_success() {
    let handler: Handler = Arc::new(|request| match request.payload_type {
        payload::SUBSCRIBE_SPOTS_REQ => vec![Reply::Answer(
            payload::ERROR_RES,
            json!({"errorCode": "ALREADY_SUBSCRIBED"}),
        )],
        payload::APPLICATION_AUTH_REQ => {
            vec![Reply::Answer(payload::APPLICATION_AUTH_RES, json!({}))]
        }
        payload::ACCOUNT_AUTH_REQ => vec![Reply::Answer(
            payload::ACCOUNT_AUTH_RES,
            json!({"ctidTraderAccountId": ACCOUNT}),
        )],
        _ => vec![],
    });
    let server = MockServer::start(handler).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    session.wait_ready(Duration::from_secs(5)).await.unwrap();
    session
        .subscribe_spots(&[4])
        .await
        .expect("already following is fine");
    // ... and it stays wanted: a reconnect asks for it again.
    server.close();
    reconnected(&mut events).await;
    assert_eq!(spot_ids(&server).len(), 2);
    session.stop().await;
}

// ---- tokens ----

#[tokio::test]
async fn tokens_close_to_expiry_are_refreshed_and_saved_before_connecting() {
    let server = MockServer::start(answers(healthy())).await;
    let token_server: TokenServer =
        token_server_sequence(vec![("200 OK", tokens_body("AT-2", "RT-2", 2_592_000))]).await;
    let mut config = config(&server.url);
    config.token_url = Some(token_server.url.clone());
    // 100 seconds left, and a day of margin: refresh first.
    let (session, store) = start(config, tokens("AT-1", "RT-1", 100));
    let mut events = session.events();
    session.wait_ready(Duration::from_secs(5)).await.unwrap();

    let request = token_server.requests()[0].clone();
    assert!(request.contains("grant_type=refresh_token") && request.contains("refresh_token=RT-1"));
    assert_eq!(
        store
            .load()
            .await
            .unwrap()
            .unwrap()
            .access_token
            .expose_secret(),
        "AT-2",
        "saved"
    );
    assert_eq!(
        server.received_of(payload::ACCOUNT_AUTH_REQ)[0].payload["accessToken"],
        "AT-2",
        "the new token is the one used"
    );
    assert_eq!(session.tokens().refresh_token.expose_secret(), "RT-2");
    // The refresh event came before the session was ready.
    let _ = &mut events;
    session.stop().await;
}

#[tokio::test]
async fn a_tokens_invalidated_notice_forces_a_refresh_and_a_reconnect() {
    let server = MockServer::start(answers(healthy())).await;
    let token_server =
        token_server_sequence(vec![("200 OK", tokens_body("AT-2", "RT-2", 2_592_000))]).await;
    let mut config = config(&server.url);
    config.token_url = Some(token_server.url.clone());
    let (session, _) = start(config, tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    session.wait_ready(Duration::from_secs(5)).await.unwrap();
    assert!(
        token_server.requests().is_empty(),
        "no refresh was needed yet"
    );

    server.push(
        payload::ACCOUNTS_TOKEN_INVALIDATED_EVENT,
        json!({"ctidTraderAccountIds": [ACCOUNT], "reason": "revoked"}),
    );
    next_event(&mut events, |e| matches!(e, SessionEvent::TokensRefreshed)).await;
    next_event(&mut events, is_ready).await;

    assert_eq!(token_server.requests().len(), 1);
    let signed_in = server.received_of(payload::ACCOUNT_AUTH_REQ);
    assert_eq!(signed_in.len(), 2);
    assert_eq!(signed_in[1].payload["accessToken"], "AT-2");
    session.stop().await;
}

#[tokio::test]
async fn a_notice_about_other_accounts_is_forwarded_but_changes_nothing() {
    let server = MockServer::start(answers(healthy())).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    session.wait_ready(Duration::from_secs(5)).await.unwrap();
    server.push(
        payload::ACCOUNTS_TOKEN_INVALIDATED_EVENT,
        json!({"ctidTraderAccountIds": [1234], "reason": "not ours"}),
    );
    let notice = next_event(&mut events, |e| matches!(e, SessionEvent::Data(_))).await;
    assert!(matches!(
        notice,
        SessionEvent::Data(Event::TokensInvalidated(_))
    ));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(server.connections(), 1, "no reconnect");
    session.stop().await;
}

#[tokio::test]
async fn a_refused_refresh_ends_the_session_without_ever_connecting() {
    let server = MockServer::start(answers(healthy())).await;
    let token_server = token_server_sequence(vec![(
        "200 OK",
        r#"{"errorCode":"ACCESS_DENIED","description":"refresh token revoked"}"#.to_owned(),
    )])
    .await;
    let mut config = config(&server.url);
    config.token_url = Some(token_server.url.clone());
    let (session, _) = start(config, tokens("AT-1", "RT-1", 100));
    let mut events = session.events();

    let failed = next_event(&mut events, |e| matches!(e, SessionEvent::Failed(_))).await;
    match failed {
        SessionEvent::Failed(OpenApiError::Auth(text)) => assert!(text.contains("ACCESS_DENIED")),
        other => panic!("{other:?}"),
    }
    assert!(matches!(*session.state().borrow(), SessionState::Failed(_)));
    assert_eq!(server.connections(), 0);
    assert!(matches!(
        session.wait_ready(Duration::from_secs(1)).await,
        Err(OpenApiError::Closed)
    ));
}

#[tokio::test]
async fn an_unreadable_refusal_from_the_token_endpoint_ends_the_session() {
    let server = MockServer::start(answers(healthy())).await;
    // A gateway error with a page instead of a token answer: the endpoint answered, and refused.
    let token_server = token_server_sequence(vec![
        ("502 Bad Gateway", "<html>down</html>".to_owned()),
        ("200 OK", tokens_body("AT-2", "RT-2", 2_592_000)),
    ])
    .await;
    let mut config = config(&server.url);
    config.token_url = Some(token_server.url.clone());
    let (session, _) = start(config, tokens("AT-1", "RT-1", 100));
    let mut events = session.events();
    // The endpoint was reached, so this is not a failure to reach it: the session ends (only a
    // failure to reach the endpoint is retried).
    let end = next_event(&mut events, |e| {
        matches!(e, SessionEvent::Failed(_) | SessionEvent::Ready)
    })
    .await;
    assert!(matches!(end, SessionEvent::Failed(_)), "{end:?}");
}

// ---- failing ----

#[tokio::test]
async fn an_unreachable_server_is_retried_with_a_growing_wait_then_given_up() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    drop(listener);
    let mut config = config(&url);
    config.connection.connect_timeout = Duration::from_millis(300);
    config.max_reconnect_attempts = Some(2);
    let (session, _) = start(config, tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();

    let mut waits = Vec::new();
    let end = loop {
        match next_event(&mut events, |_| true).await {
            SessionEvent::Reconnecting {
                attempt, retry_in, ..
            } => waits.push((attempt, retry_in)),
            other => break other,
        }
    };
    assert_eq!(waits.iter().map(|w| w.0).collect::<Vec<_>>(), vec![1, 2]);
    assert!(
        waits[1].1 >= waits[0].1,
        "the wait does not shrink: {waits:?}"
    );
    match end {
        SessionEvent::Failed(OpenApiError::Transport(text)) => assert!(text.contains("gave up")),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn an_account_that_is_not_authorized_ends_the_session_at_once() {
    let handler: Handler = Arc::new(|request| match request.payload_type {
        payload::APPLICATION_AUTH_REQ => {
            vec![Reply::Answer(payload::APPLICATION_AUTH_RES, json!({}))]
        }
        payload::ACCOUNT_AUTH_REQ => vec![Reply::Answer(
            payload::ERROR_RES,
            json!({"errorCode": "ACCOUNT_NOT_AUTHORIZED", "description": "no"}),
        )],
        _ => vec![],
    });
    let server = MockServer::start(handler).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    let failed = next_event(&mut events, |e| matches!(e, SessionEvent::Failed(_))).await;
    assert!(matches!(failed, SessionEvent::Failed(e) if e.kind() == ErrorKind::NotAuthorized));
    assert_eq!(server.connections(), 1, "it did not retry");
}

#[tokio::test]
async fn an_invalid_token_is_refreshed_once_and_a_second_failure_ends_the_session() {
    // The server keeps calling the token bad, even the fresh one: refreshing cannot help.
    let handler: Handler = Arc::new(|request| match request.payload_type {
        payload::APPLICATION_AUTH_REQ => {
            vec![Reply::Answer(payload::APPLICATION_AUTH_RES, json!({}))]
        }
        payload::ACCOUNT_AUTH_REQ => vec![Reply::Answer(
            payload::ERROR_RES,
            json!({"errorCode": "CH_ACCESS_TOKEN_INVALID"}),
        )],
        _ => vec![],
    });
    let server = MockServer::start(handler).await;
    let token_server =
        token_server_sequence(vec![("200 OK", tokens_body("AT-2", "RT-2", 2_592_000))]).await;
    let mut config = config(&server.url);
    config.token_url = Some(token_server.url.clone());
    let (session, _) = start(config, tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    let failed = next_event(&mut events, |e| matches!(e, SessionEvent::Failed(_))).await;
    assert!(matches!(failed, SessionEvent::Failed(e) if e.kind() == ErrorKind::TokenInvalid));
    assert_eq!(token_server.requests().len(), 1, "one refresh, not a loop");
    assert_eq!(
        server.connections(),
        2,
        "the original try and the one with fresh tokens"
    );
}

// ---- stopping ----

#[tokio::test]
async fn stopping_closes_the_connection_and_announces_it_once() {
    let server = MockServer::start(answers(healthy())).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    let client = session.wait_ready(Duration::from_secs(5)).await.unwrap();

    session.stop().await;
    next_event(&mut events, |e| matches!(e, SessionEvent::Stopped)).await;
    assert_eq!(*session.state().borrow(), SessionState::Stopped);
    assert!(session.client().is_none());
    assert!(client.is_closed());
    session.stop().await; // a second stop does nothing
    assert!(matches!(
        session.wait_ready(Duration::from_millis(200)).await,
        Err(OpenApiError::Closed)
    ));
}

#[tokio::test]
async fn dropping_every_clone_without_stopping_still_ends_the_session() {
    let server = MockServer::start(answers(healthy())).await;
    let (session, _) = start(config(&server.url), tokens("AT-1", "RT-1", 2_592_000));
    let mut state = session.state();
    let client = session.wait_ready(Duration::from_secs(5)).await.unwrap();
    let other = session.clone();
    drop(other);
    drop(session);
    tokio::time::timeout(
        Duration::from_secs(2),
        state.wait_for(|s| matches!(s, SessionState::Stopped)),
    )
    .await
    .expect("the session did not stop on its own once every clone was gone")
    .unwrap();
    assert!(client.is_closed());
}

#[tokio::test]
async fn stopping_while_waiting_to_reconnect_is_prompt() {
    let server = MockServer::start(answers(healthy())).await;
    let mut config = config(&server.url);
    config.backoff = Backoff {
        initial: Duration::from_secs(30),
        max: Duration::from_secs(60),
        factor: 2,
    };
    let (session, _) = start(config, tokens("AT-1", "RT-1", 2_592_000));
    let mut events = session.events();
    session.wait_ready(Duration::from_secs(5)).await.unwrap();
    server.shutdown();
    next_event(&mut events, |e| {
        matches!(e, SessionEvent::Reconnecting { .. })
    })
    .await;

    let started = std::time::Instant::now();
    session.stop().await;
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "not held up by the 30 second wait"
    );
}

#[tokio::test]
async fn waiting_for_a_session_that_cannot_come_up_times_out() {
    // The server never answers the application sign in.
    let server = MockServer::start(Arc::new(|_| vec![Reply::Silence])).await;
    let mut config = config(&server.url);
    config.connection.request_timeout = Duration::from_secs(30);
    let (session, _) = start(config, tokens("AT-1", "RT-1", 2_592_000));
    let error = session
        .wait_ready(Duration::from_millis(300))
        .await
        .unwrap_err();
    assert!(matches!(error, OpenApiError::Timeout { .. }));
    session.stop().await;
}

#[tokio::test]
async fn unusable_settings_are_refused_when_starting() {
    let mut bad = config("http://not-a-websocket");
    bad.connection.url = "http://not-a-websocket".to_owned();
    let store: Arc<dyn TokenStore> = Arc::new(MemoryTokenStore::default());
    let result = Session::start(bad, tokens("a", "r", 10), store);
    assert!(matches!(result, Err(OpenApiError::Config(_))));
}

#[tokio::test]
async fn a_token_endpoint_that_cannot_be_reached_is_retried_and_not_fatal() {
    let server = MockServer::start(answers(healthy())).await;
    let dead = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_url = format!("http://{}/apps/token", dead.local_addr().unwrap());
    drop(dead);
    let mut config = config(&server.url);
    config.token_url = Some(dead_url);
    let (session, _) = start(config, tokens("AT-1", "RT-1", 100));
    let mut events = session.events();

    let waiting = next_event(&mut events, |e| {
        matches!(
            e,
            SessionEvent::Reconnecting { .. } | SessionEvent::Failed(_)
        )
    })
    .await;
    match waiting {
        SessionEvent::Reconnecting {
            attempt, reason, ..
        } => {
            assert_eq!(attempt, 1);
            assert!(reason.contains("token endpoint"), "{reason}");
        }
        other => panic!("an unreachable endpoint must not end the session: {other:?}"),
    }
    assert!(matches!(
        *session.state().borrow(),
        SessionState::Waiting { .. } | SessionState::Connecting { .. }
    ));
    assert_eq!(
        server.connections(),
        0,
        "no connection without fresh tokens"
    );
    session.stop().await;
}
