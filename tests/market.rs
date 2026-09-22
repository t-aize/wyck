//! `MarketClient` against a scripted local server: symbols, subscriptions, price events, and
//! history (one page and a whole range paged).

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::{MockServer, answers, connect};
use wyck::openapi::market::{MarketClient, Period, QuoteType};
use wyck::openapi::transport::wire::payload;
use wyck::openapi::{Client, Event};

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
    assert_eq!(refused.kind(), wyck::openapi::ErrorKind::Rejected);
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
    let market = market(&server).await;
    let (ticks, more) = market
        .tick_page(1, QuoteType::Ask, 0, 2_000_000)
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
        vec![support::Reply::Answer(payload::GET_TICK_DATA_RES, body)]
    }))
    .await;
    let market = market(&server).await;
    let ticks = market.ticks(1, QuoteType::Bid, 1000, 6000).await.unwrap();
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
async fn a_tick_page_claiming_more_without_progress_is_an_error() {
    let server = MockServer::start(answers(vec![(
        payload::GET_TICK_DATA_REQ,
        payload::GET_TICK_DATA_RES,
        json!({"tickData": [], "hasMore": true}),
    )]))
    .await;
    let market = market(&server).await;
    let error = market
        .ticks(1, QuoteType::Bid, 1000, 6000)
        .await
        .unwrap_err();
    assert!(matches!(error, wyck::openapi::OpenApiError::Protocol(_)));
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
    let market = market(&server).await;
    let (bars, more) = market
        .bars_page(1, Period::M1, 0, i64::MAX / 2)
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
        vec![support::Reply::Answer(
            payload::GET_TRENDBARS_RES,
            json!({"trendbar": bars, "hasMore": page == 0}),
        )]
    }))
    .await;
    let market = market(&server).await;
    let bars = market.bars(1, Period::M1, 0, 99 * m).await.unwrap();
    assert_eq!(bars.len(), 100);
    assert!(bars.windows(2).all(|w| w[0].time_ms < w[1].time_ms));
    let second = &server.received_of(payload::GET_TRENDBARS_REQ)[1].payload;
    assert_eq!(second["toTimestamp"], 50 * m - 1);
}

#[tokio::test]
async fn an_empty_bar_page_claiming_more_is_an_error() {
    let server = MockServer::start(answers(vec![(
        payload::GET_TRENDBARS_REQ,
        payload::GET_TRENDBARS_RES,
        json!({"trendbar": [], "hasMore": true}),
    )]))
    .await;
    let market = market(&server).await;
    let error = market.bars(1, Period::M1, 0, 60_000).await.unwrap_err();
    assert!(matches!(error, wyck::openapi::OpenApiError::Protocol(_)));
}

#[tokio::test]
async fn history_requests_are_spread_to_the_configured_rate() {
    let server = MockServer::start(answers(vec![(
        payload::GET_TICK_DATA_REQ,
        payload::GET_TICK_DATA_RES,
        json!({"tickData": [], "hasMore": false}),
    )]))
    .await;
    let mut config = support::config(&server);
    config.historical_rate = 2;
    let client = Client::connect(&config).await.unwrap();
    let market = client.account(1).market();
    let started = Instant::now();
    for _ in 0..4 {
        market.tick_page(1, QuoteType::Bid, 0, 1000).await.unwrap();
    }
    // Four requests at two per second cannot all go in under about a second.
    assert!(
        started.elapsed() >= Duration::from_millis(900),
        "{:?}",
        started.elapsed()
    );
}
