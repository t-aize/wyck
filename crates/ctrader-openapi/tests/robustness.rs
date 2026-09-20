//! What the client does when the network or the server misbehaves.
//!
//! A connection to a real server lives for days. Frames arrive that make no sense, bursts arrive
//! faster than a reader can take them, the peer vanishes without a goodbye, and hundreds of
//! requests fly at once. None of that may hang the client, panic it, or mix up answers.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ctrader_openapi::config::ConnectionConfig;
use ctrader_openapi::wire::payload;
use ctrader_openapi::{Client, DisconnectReason, ErrorKind, Event, OpenApiError};
use futures_util::StreamExt;
use serde_json::json;
use support::{MockServer, Reply, answers};
use tokio::sync::broadcast::error::RecvError;

fn config(server: &MockServer) -> ConnectionConfig {
    let mut config = ConnectionConfig::with_url(server.url.clone());
    config.request_timeout = Duration::from_secs(5);
    config.heartbeat_interval = Duration::from_millis(200);
    config
}

async fn connect(server: &MockServer) -> Client {
    Client::connect(&config(server)).await.unwrap()
}

fn version_answers() -> Vec<(u32, u32, serde_json::Value)> {
    vec![(
        payload::VERSION_REQ,
        payload::VERSION_RES,
        json!({"version": "ok"}),
    )]
}

// ---- nonsense from the server ----

#[tokio::test]
async fn frames_that_make_no_sense_are_ignored_and_the_connection_carries_on() {
    let server = MockServer::start(answers(version_answers())).await;
    let client = connect(&server).await;

    server.push_raw_text("this is not json");
    server.push_raw_text("[1, 2, 3]");
    server.push_raw_text(r#"{"payload": {}}"#);
    server.push_binary(vec![0xff, 0xfe, 0xfd]);
    server.push_ping();
    server.push_raw_text(r#"{"payloadType": 51}"#);

    assert_eq!(client.version().await.unwrap(), "ok");
    assert!(!client.is_closed());
}

#[tokio::test]
async fn a_binary_frame_that_is_valid_text_is_read_like_a_text_frame() {
    let server = MockServer::start(answers(vec![])).await;
    let client = connect(&server).await;
    let mut events = client.events();
    let text = json!({"payloadType": payload::SPOT_EVENT, "payload": {"symbolId": 9, "bid": 5}});
    server.push_binary(text.to_string().into_bytes());
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, Event::Spot(s) if s.symbol_id == 9));
}

#[tokio::test]
async fn a_second_answer_to_the_same_request_does_not_disturb_anything() {
    let server = MockServer::start(Arc::new(|request| match request.payload_type {
        payload::VERSION_REQ => vec![
            Reply::Answer(payload::VERSION_RES, json!({"version": "first"})),
            Reply::Answer(payload::VERSION_RES, json!({"version": "second"})),
        ],
        _ => vec![],
    }))
    .await;
    let client = connect(&server).await;
    assert_eq!(client.version().await.unwrap(), "first");
    assert_eq!(
        client.version().await.unwrap(),
        "first",
        "still works afterwards"
    );
    assert!(!client.is_closed());
}

#[tokio::test]
async fn an_answer_for_a_request_nobody_made_is_only_an_event() {
    let server = MockServer::start(answers(vec![])).await;
    let client = connect(&server).await;
    let mut events = client.events();
    // An answer carrying an id that no request has.
    server.push_raw_text(
        &json!({"clientMsgId": "nobody-asked", "payloadType": payload::VERSION_RES, "payload": {"version": "x"}})
            .to_string(),
    );
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(event, Event::Other { payload_type, .. } if payload_type == payload::VERSION_RES)
    );
}

#[tokio::test]
async fn a_very_large_answer_is_read_whole() {
    let symbols: Vec<serde_json::Value> = (0..20_000)
        .map(|i| json!({"symbolId": i, "symbolName": format!("SYMBOL{i}"), "description": "x".repeat(60)}))
        .collect();
    let server = MockServer::start(answers(vec![(
        payload::SYMBOLS_LIST_REQ,
        payload::SYMBOLS_LIST_RES,
        json!({"symbol": symbols}),
    )]))
    .await;
    let client = connect(&server).await;
    let list = client.symbols(1, false).await.unwrap();
    assert_eq!(list.len(), 20_000);
    assert_eq!(list[19_999].symbol_name.as_deref(), Some("SYMBOL19999"));
}

// ---- load ----

#[tokio::test]
async fn a_burst_of_events_faster_than_the_reader_is_survived_and_the_loss_is_reported() {
    let server = MockServer::start(answers(vec![])).await;
    let mut config = config(&server);
    config.event_capacity = 8;
    let client = Client::connect(&config).await.unwrap();
    let mut events = client.events();

    for i in 0..200 {
        server.push(payload::SPOT_EVENT, json!({"symbolId": 1, "bid": i}));
    }
    // Let the whole burst arrive before reading any of it.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut lagged = 0u64;
    let mut received = 0;
    loop {
        match tokio::time::timeout(Duration::from_millis(300), events.recv()).await {
            Ok(Ok(_)) => received += 1,
            Ok(Err(RecvError::Lagged(missed))) => lagged += missed,
            Ok(Err(RecvError::Closed)) | Err(_) => break,
        }
    }
    assert!(lagged > 0, "the reader was told it lost events");
    assert!(received >= 1);
    assert_eq!(
        lagged + received,
        200,
        "every event is either read or counted as missed"
    );
    assert!(!client.is_closed(), "the connection did not care");
}

#[tokio::test]
async fn three_hundred_requests_at_once_each_get_their_own_answer() {
    let server = MockServer::start(Arc::new(|request| {
        if request.payload_type != payload::SYMBOL_BY_ID_REQ {
            return vec![];
        }
        let id = request.payload["symbolId"][0].as_i64().unwrap();
        // A varying delay makes the answers come back in a shuffled order.
        vec![Reply::AnswerAfter(
            Duration::from_millis((id * 7) as u64 % 40),
            payload::SYMBOL_BY_ID_RES,
            json!({"symbol": [{"symbolId": id, "digits": id, "pipPosition": 4}]}),
        )]
    }))
    .await;
    let mut config = config(&server);
    config.standard_rate = 1000; // this test is about matching, not about the limiter
    let client = Client::connect(&config).await.unwrap();

    let tasks: Vec<_> = (0..300)
        .map(|id| {
            let client = client.clone();
            tokio::spawn(async move { (id, client.symbol_details(1, &[id]).await) })
        })
        .collect();
    for task in tasks {
        let (id, result) = task.await.unwrap();
        let details = result.unwrap();
        assert_eq!(
            details[0].symbol_id, id,
            "an answer went to the wrong request"
        );
        assert_eq!(details[0].digits, id);
    }
}

#[tokio::test]
async fn requests_racing_a_close_all_finish_instead_of_hanging() {
    let server = MockServer::start(Arc::new(|request| match request.payload_type {
        payload::VERSION_REQ => vec![Reply::AnswerAfter(
            Duration::from_millis(100),
            payload::VERSION_RES,
            json!({"version": "ok"}),
        )],
        _ => vec![],
    }))
    .await;
    let mut config = config(&server);
    config.standard_rate = 1000;
    let client = Client::connect(&config).await.unwrap();
    let tasks: Vec<_> = (0..50)
        .map(|_| {
            let client = client.clone();
            tokio::spawn(async move { client.version().await })
        })
        .collect();
    tokio::time::sleep(Duration::from_millis(30)).await;
    client.close().await;

    let started = Instant::now();
    for task in tasks {
        match task.await.unwrap() {
            Ok(version) => assert_eq!(version, "ok"),
            Err(error) => assert!(matches!(error, OpenApiError::Closed), "{error:?}"),
        }
    }
    assert!(started.elapsed() < Duration::from_secs(3), "nothing hung");
}

#[tokio::test]
async fn every_clone_shares_one_connection() {
    let server = MockServer::start(answers(version_answers())).await;
    let client = connect(&server).await;
    let other = client.clone();
    assert_eq!(other.version().await.unwrap(), "ok");
    other.close().await;
    let mut state = client.state();
    state
        .wait_for(|s| matches!(s, ctrader_openapi::ConnectionState::Closed(_)))
        .await
        .unwrap();
    assert!(
        client.is_closed(),
        "closing one clone closed the connection for all"
    );
    assert_eq!(server.connections(), 1);
}

// ---- the network ----

#[tokio::test]
async fn a_peer_that_never_finishes_the_handshake_is_a_timeout() {
    // A TCP server that accepts and then says nothing at all.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let _hold = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            held.push(stream);
        }
    });
    let mut config = ConnectionConfig::with_url(url);
    config.connect_timeout = Duration::from_millis(300);
    let started = Instant::now();
    let error = Client::connect(&config).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Timeout);
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn a_peer_that_vanishes_without_a_goodbye_ends_the_connection_cleanly() {
    // A server that completes the handshake and then drops the socket with no close frame.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let accepted = Arc::new(AtomicUsize::new(0));
    let count = accepted.clone();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        count.fetch_add(1, Ordering::SeqCst);
        // Read one message so the request is really in flight, then vanish.
        let (_sink, mut source) = socket.split();
        let _ = source.next().await;
    });

    let mut config = ConnectionConfig::with_url(url);
    config.request_timeout = Duration::from_secs(5);
    let client = Client::connect(&config).await.unwrap();
    let mut events = client.events();

    let started = Instant::now();
    let error = client.version().await.unwrap_err();
    assert!(matches!(error, OpenApiError::Closed), "{error:?}");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "did not wait for the request timeout"
    );

    let reason = loop {
        match tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            Event::Disconnected(reason) => break reason,
            _ => {}
        }
    };
    assert!(
        matches!(
            reason,
            DisconnectReason::Failed(_) | DisconnectReason::ClosedByServer
        ),
        "{reason:?}"
    );
    assert!(client.is_closed());
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
}
