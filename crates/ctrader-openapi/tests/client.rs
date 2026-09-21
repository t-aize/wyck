//! The client against a scripted local server: requests, answers, errors, events, the end of the
//! connection, and the history helpers.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ctrader_openapi::config::{ClientCredentials, ConnectionConfig};
use ctrader_openapi::history::{fetch_bars, fetch_ticks};
use ctrader_openapi::types::{Period, QuoteType};
use ctrader_openapi::wire::payload;
use ctrader_openapi::{Client, ConnectionState, DisconnectReason, ErrorKind, Event, OpenApiError};
use serde_json::{Value, json};
use support::{MockServer, Reply, answers};

fn config(server: &MockServer) -> ConnectionConfig {
    let mut config = ConnectionConfig::with_url(server.url.clone());
    config.request_timeout = Duration::from_secs(5);
    config.heartbeat_interval = Duration::from_millis(200);
    config
}

async fn connect(server: &MockServer) -> Client {
    Client::connect(&config(server)).await.unwrap()
}

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

    let authorized = client.authorize_account(40_212, "AT").await.unwrap();
    assert_eq!(authorized, 40_212);
    assert_eq!(
        server.received_of(payload::ACCOUNT_AUTH_REQ)[0].payload,
        json!({"ctidTraderAccountId": 40212, "accessToken": "AT"})
    );
}

#[tokio::test]
async fn the_version_and_the_symbol_list_are_decoded() {
    let server = MockServer::start(answers(vec![
        (payload::VERSION_REQ, payload::VERSION_RES, json!({"version": "84.2"})),
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
    let client = connect(&server).await;
    assert_eq!(client.version().await.unwrap(), "84.2");
    let symbols = client.symbols(1, false).await.unwrap();
    assert_eq!(symbols[1].symbol_id, 2);
    assert_eq!(symbols[0].symbol_name.as_deref(), Some("EURUSD"));
    let details = client.symbol_details(1, &[1]).await.unwrap();
    assert_eq!((details[0].digits, details[0].pip_position), (5, 4));
    // Archived symbols are only asked for when wanted.
    let list_request = &server.received_of(payload::SYMBOLS_LIST_REQ)[0];
    assert!(list_request.payload.get("includeArchivedSymbols").is_none());
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
            payload::SUBSCRIBE_SPOTS_REQ,
            payload::ERROR_RES,
            json!({"errorCode": "SYMBOL_NOT_FOUND", "description": "unknown"}),
        ),
    ]))
    .await;
    let client = connect(&server).await;
    let token = client.accounts("bad").await.unwrap_err();
    assert_eq!(token.kind(), ErrorKind::TokenInvalid);
    assert!(!token.is_retryable());
    let refused = client.subscribe_spots(1, &[999], false).await.unwrap_err();
    assert_eq!(refused.kind(), ErrorKind::Rejected);
}

#[tokio::test]
async fn an_answer_of_the_wrong_type_is_a_protocol_error() {
    let server = MockServer::start(answers(vec![(
        payload::VERSION_REQ,
        payload::SYMBOLS_LIST_RES,
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
        if request.payload_type != payload::SYMBOL_BY_ID_REQ {
            return vec![];
        }
        let symbol = request.payload["symbolId"][0].as_i64().unwrap();
        // The first request is answered last.
        let delay = if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            300
        } else {
            10
        };
        vec![Reply::AnswerAfter(
            Duration::from_millis(delay),
            payload::SYMBOL_BY_ID_RES,
            json!({"symbol": [{"symbolId": symbol, "digits": symbol, "pipPosition": 4}]}),
        )]
    }))
    .await;
    let client = connect(&server).await;
    let (a, b, c) = tokio::join!(
        client.symbol_details(1, &[10]),
        client.symbol_details(1, &[20]),
        client.symbol_details(1, &[30]),
    );
    assert_eq!(a.unwrap()[0].digits, 10);
    assert_eq!(b.unwrap()[0].digits, 20);
    assert_eq!(c.unwrap()[0].digits, 30);
}

// ---- events ----

#[tokio::test]
async fn subscribing_sends_the_symbols_and_spots_arrive_as_events() {
    let server = MockServer::start(answers(vec![(
        payload::SUBSCRIBE_SPOTS_REQ,
        payload::SUBSCRIBE_SPOTS_RES,
        json!({"ctidTraderAccountId": 1}),
    )]))
    .await;
    let client = connect(&server).await;
    let mut events = client.events();
    client.subscribe_spots(1, &[1, 2], true).await.unwrap();
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
    let chain = client.symbols_for_conversion(1, 3, 4).await.unwrap();
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

// ---- limits ----

#[tokio::test]
async fn history_requests_are_spread_to_the_configured_rate() {
    let server = MockServer::start(answers(vec![(
        payload::GET_TICK_DATA_REQ,
        payload::GET_TICK_DATA_RES,
        json!({"tickData": [], "hasMore": false}),
    )]))
    .await;
    let mut config = config(&server);
    config.historical_rate = 2;
    let client = Client::connect(&config).await.unwrap();
    let started = Instant::now();
    for _ in 0..4 {
        client
            .tick_page(1, 1, QuoteType::Bid, 0, 1000)
            .await
            .unwrap();
    }
    // Four requests at two per second cannot all go in under about a second.
    assert!(
        started.elapsed() >= Duration::from_millis(900),
        "{:?}",
        started.elapsed()
    );
}

// ---- history ----

#[tokio::test]
async fn ticks_are_decoded_to_absolute_times_oldest_first() {
    let server = MockServer::start(answers(vec![(
        payload::GET_TICK_DATA_REQ,
        payload::GET_TICK_DATA_RES,
        json!({"tickData": [
            {"timestamp": 1_000_000, "tick": 108501},
            {"timestamp": -400, "tick": -2},
            {"timestamp": -100, "tick": 1}
        ], "hasMore": false}),
    )]))
    .await;
    let client = connect(&server).await;
    let (ticks, more) = client
        .tick_page(1, 1, QuoteType::Ask, 0, 2_000_000)
        .await
        .unwrap();
    assert!(!more);
    let times: Vec<i64> = ticks.iter().map(|t| t.time_ms).collect();
    assert_eq!(times, vec![999_500, 999_600, 1_000_000]);
    // Prices are differences too: the newest is absolute, the others are steps from the one before.
    let prices: Vec<i64> = ticks.iter().map(|t| t.price).collect();
    assert_eq!(prices, vec![108_500, 108_499, 108_501]);
    let request = &server.received_of(payload::GET_TICK_DATA_REQ)[0].payload;
    assert_eq!(request["type"], 2, "ask ticks are type 2");
}

#[tokio::test]
async fn a_long_tick_range_is_fetched_backwards_page_by_page() {
    // The first request gets the newest ticks and hasMore; the second gets the older ones.
    let served = Arc::new(AtomicUsize::new(0));
    let server = MockServer::start(Arc::new(move |request| {
        if request.payload_type != payload::GET_TICK_DATA_REQ {
            return vec![];
        }
        let page = served.fetch_add(1, Ordering::SeqCst);
        let body = if page == 0 {
            json!({"tickData": [{"timestamp": 5000, "tick": 3}, {"timestamp": -1000, "tick": 2}], "hasMore": true})
        } else {
            json!({"tickData": [{"timestamp": 3999, "tick": 1}, {"timestamp": -500, "tick": 0}], "hasMore": false})
        };
        vec![Reply::Answer(payload::GET_TICK_DATA_RES, body)]
    }))
    .await;
    let client = connect(&server).await;
    let ticks = fetch_ticks(&client, 1, 1, QuoteType::Bid, 1000, 6000)
        .await
        .unwrap();
    let times: Vec<i64> = ticks.iter().map(|t| t.time_ms).collect();
    assert_eq!(times, vec![3499, 3999, 4000, 5000]);
    let requests = server.received_of(payload::GET_TICK_DATA_REQ);
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].payload["toTimestamp"], 6000);
    assert_eq!(
        requests[1].payload["toTimestamp"], 3999,
        "the part before the oldest tick received"
    );
}

#[tokio::test]
async fn bars_are_rebuilt_from_their_low_and_offsets() {
    let server = MockServer::start(answers(vec![(
        payload::GET_TRENDBARS_REQ,
        payload::GET_TRENDBARS_RES,
        json!({"period": 1, "symbolId": 1, "hasMore": false, "trendbar": [
            {"volume": 12, "period": 1, "low": 108400, "deltaOpen": 50, "deltaClose": 80, "deltaHigh": 100, "utcTimestampInMinutes": 29000000},
            {"volume": 5, "period": 1, "low": 108450, "deltaOpen": 0, "deltaClose": 10, "deltaHigh": 40, "utcTimestampInMinutes": 29000001}
        ]}),
    )]))
    .await;
    let client = connect(&server).await;
    let (bars, more) = client
        .bars_page(1, 1, Period::M1, 0, i64::MAX / 2)
        .await
        .unwrap();
    assert!(!more);
    assert_eq!(bars.len(), 2);
    assert_eq!(
        (bars[0].open, bars[0].high, bars[0].low, bars[0].close),
        (108_450, 108_500, 108_400, 108_480)
    );
    assert_eq!(bars[0].time_ms, 29_000_000 * 60_000);
    let request = &server.received_of(payload::GET_TRENDBARS_REQ)[0].payload;
    assert_eq!(request["period"], 1);
}

#[tokio::test]
async fn a_truncated_bar_answer_is_continued_from_the_missing_side() {
    let m = Period::M1.millis();
    let minute = move |i: i64| (i * m) / 60_000;
    // Range 0..=99 minutes. The server returns the newest 50 first, then the rest.
    let served = Arc::new(AtomicUsize::new(0));
    let server = MockServer::start(Arc::new(move |request| {
        if request.payload_type != payload::GET_TRENDBARS_REQ {
            return vec![];
        }
        let page = served.fetch_add(1, Ordering::SeqCst);
        let range: Vec<i64> = if page == 0 { (50..100).collect() } else { (0..50).collect() };
        let bars: Vec<Value> = range
            .iter()
            .map(|i| json!({"volume": 1, "low": 100, "deltaOpen": 1, "deltaClose": 1, "deltaHigh": 2, "utcTimestampInMinutes": minute(*i)}))
            .collect();
        vec![Reply::Answer(
            payload::GET_TRENDBARS_RES,
            json!({"trendbar": bars, "hasMore": page == 0}),
        )]
    }))
    .await;
    let client = connect(&server).await;
    let bars = fetch_bars(&client, 1, 1, Period::M1, 0, 99 * m)
        .await
        .unwrap();
    assert_eq!(bars.len(), 100);
    assert!(bars.windows(2).all(|w| w[0].time_ms < w[1].time_ms));
    let second = &server.received_of(payload::GET_TRENDBARS_REQ)[1].payload;
    assert_eq!(second["toTimestamp"], 50 * m - 1);
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
