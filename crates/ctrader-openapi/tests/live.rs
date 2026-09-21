//! Checks against the real Open API, for a **demo** account. Ignored by default.
//!
//! `read_a_demo_account_end_to_end` exists to settle what the documentation leaves open (see the
//! crate docs, "What has not been verified against a live server"). It only reads: no order is
//! placed.
//!
//! `place_and_close_a_minimal_market_order_on_a_demo_account` does place (and immediately close)
//! one order, so it needs a second, explicit opt in beyond `#[ignore]`; see its own doc comment and
//! [Trading safety in the README](../README.md#trading-safety) before running it.
//!
//! Set these variables, then run
//! `cargo test -p ctrader-openapi --test live -- --ignored --nocapture`:
//!
//! - `WYCK_OPENAPI_CLIENT_ID` and `WYCK_OPENAPI_CLIENT_SECRET`: the registered application.
//! - `WYCK_OPENAPI_ACCESS_TOKEN`: an access token. `accounts` scope is enough for the read only
//!   test; the trading test needs one authorized with `trading` scope.
//! - `WYCK_OPENAPI_SYMBOL` (optional, default `EURUSD`).
//! - `WYCK_OPENAPI_ALLOW_LIVE_TRADING=1`, for the trading test only.
//!
//! Never put the values in a file. Both tests refuse a live account.

use std::time::Duration;

use ctrader_openapi::config::{ClientCredentials, ConnectionConfig, Environment};
use ctrader_openapi::market::{Period, QuoteType, merge_sides, to_price};
use ctrader_openapi::trading::{ExecutionType, NewOrderReq};
use ctrader_openapi::{Client, Event};

fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run this test"))
}

#[tokio::test]
#[ignore = "needs an approved Open API application and a demo access token"]
async fn read_a_demo_account_end_to_end() {
    let credentials = ClientCredentials::new(
        var("WYCK_OPENAPI_CLIENT_ID"),
        var("WYCK_OPENAPI_CLIENT_SECRET"),
    );
    let token = var("WYCK_OPENAPI_ACCESS_TOKEN");
    let wanted = std::env::var("WYCK_OPENAPI_SYMBOL").unwrap_or_else(|_| "EURUSD".into());

    let client = Client::connect(&ConnectionConfig::new(Environment::Demo))
        .await
        .expect("connect");
    println!("proxy version: {:?}", client.version().await);
    client
        .authenticate_application(&credentials)
        .await
        .expect("application sign in");

    let accounts = client.accounts(&token).await.expect("account list");
    println!("permission scope: {:?}", accounts.permission_scope);
    let account_info = accounts
        .ctid_trader_account
        .iter()
        .find(|a| a.is_live == Some(false))
        .expect("the token covers no demo account");
    let account_id = account_info.ctid_trader_account_id;
    println!(
        "using demo account {account_id} ({:?})",
        account_info.broker_title_short
    );
    let account = client.account(account_id);
    account.authorize(&token).await.expect("account sign in");
    let market = account.market();

    let symbols = market.symbols().await.expect("symbols");
    println!("{} symbols", symbols.len());
    let symbol = symbols
        .iter()
        .find(|s| s.symbol_name.as_deref() == Some(wanted.as_str()))
        .unwrap_or_else(|| panic!("{wanted} not offered"));
    let details = market
        .symbol_details(&[symbol.symbol_id])
        .await
        .expect("details");
    println!("details: {details:?}");

    // Live prices for a few seconds.
    let mut events = client.events();
    market
        .subscribe_spots(&[symbol.symbol_id])
        .await
        .expect("spot subscription");
    let mut seen = 0;
    let mut last_spot_time: Option<i64> = None;
    let _ = tokio::time::timeout(Duration::from_secs(8), async {
        while let Ok(event) = events.recv().await {
            if let Event::Spot(spot) = event {
                seen += 1;
                last_spot_time = spot.timestamp.or(last_spot_time);
                println!(
                    "spot: bid {:?} ask {:?} at {:?}",
                    spot.bid.map(to_price),
                    spot.ask.map(to_price),
                    spot.timestamp
                );
            }
        }
    })
    .await;
    println!("{seen} spot events in 8 seconds");

    // History: ticks on each side, and M1 bars. The range ends at the time of the last price seen,
    // not at the clock: with the market closed (a weekend) the last hours hold nothing, and an
    // empty answer would say nothing about the client.
    let clock = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let now = last_spot_time.unwrap_or(clock);
    println!(
        "history anchored at {now} ({} minutes before the clock)",
        (clock - now) / 60_000
    );
    let hour = 3_600_000;
    for side in [QuoteType::Bid, QuoteType::Ask] {
        let ticks = market
            .ticks(symbol.symbol_id, side, now - 6 * hour, now)
            .await
            .expect("tick history");
        println!(
            "{side:?}: {} ticks, first {:?}, last {:?}",
            ticks.len(),
            ticks.first(),
            ticks.last()
        );
        // The prices must be plausible end to end: a tick history whose prices are misread (a
        // difference taken for a price) shows a range far wider than a few percent.
        if let (Some(low), Some(high), Some(last)) = (
            ticks.iter().map(|t| t.price).min(),
            ticks.iter().map(|t| t.price).max(),
            ticks.last(),
        ) {
            println!(
                "{side:?} price range {} to {} (last {})",
                to_price(low),
                to_price(high),
                to_price(last.price)
            );
            assert!(
                (high - low) * 20 < last.price.abs().max(1),
                "the tick prices are spread too wide to be prices"
            );
        }
    }
    let bids = market
        .ticks(symbol.symbol_id, QuoteType::Bid, now - hour, now)
        .await
        .unwrap();
    let asks = market
        .ticks(symbol.symbol_id, QuoteType::Ask, now - hour, now)
        .await
        .unwrap();
    println!(
        "{} quotes in the last hour",
        merge_sides(&bids, &asks).len()
    );

    let bars = market
        .bars(symbol.symbol_id, Period::M1, now - 24 * hour, now)
        .await
        .expect("bar history");
    println!("{} M1 bars in a day", bars.len());
    if let (Some(first), Some(last)) = (bars.first(), bars.last()) {
        println!(
            "first bar {:?}
last bar  {:?}",
            (
                first.time_ms,
                to_price(first.open),
                to_price(first.high),
                to_price(first.low),
                to_price(first.close),
                first.volume
            ),
            (
                last.time_ms,
                to_price(last.open),
                to_price(last.high),
                to_price(last.low),
                to_price(last.close),
                last.volume
            ),
        );
    }
    // Cross check: the bars and the ticks are two views of the same prices, so the high and the
    // low of a bar should be the highest and lowest bid tick of its minute (bars are built on the
    // bid). A few differences at the edges are normal; a systematic one means a decoding mistake.
    let ticks = market
        .ticks(symbol.symbol_id, QuoteType::Bid, now - 6 * hour, now)
        .await
        .unwrap();
    let (mut compared, mut matching) = (0, 0);
    let (mut volume_sum, mut tick_sum) = (0i64, 0usize);
    for bar in bars
        .iter()
        .filter(|b| b.time_ms >= now - 5 * hour && b.time_ms + 60_000 <= now)
    {
        let inside: Vec<i64> = ticks
            .iter()
            .filter(|t| t.time_ms >= bar.time_ms && t.time_ms < bar.time_ms + 60_000)
            .map(|t| t.price)
            .collect();
        if let (Some(low), Some(high)) = (inside.iter().min(), inside.iter().max()) {
            compared += 1;
            volume_sum += bar.volume;
            tick_sum += inside.len();
            if *low == bar.low && *high == bar.high {
                matching += 1;
            } else {
                println!(
                    "  differs at {}: bar low {} high {}, ticks low {} high {} ({} ticks)",
                    bar.time_ms,
                    bar.low,
                    bar.high,
                    low,
                    high,
                    inside.len()
                );
            }
        }
    }
    println!("bars against ticks: {matching} of {compared} minutes have the same high and low");
    println!(
        "bar volume against tick count over those minutes: {volume_sum} against {tick_sum} (a bar counts ticks; if the bars count more, the tick history is thinner than the feed the bars come from)"
    );

    // Seam check: six hours fetched in one call and in twelve pieces of half an hour must hold the
    // same ticks. The whole range needs several pages and each piece one, so a difference means
    // ticks are lost where two pages meet.
    let whole = market
        .ticks(symbol.symbol_id, QuoteType::Bid, now - 6 * hour, now)
        .await
        .unwrap();
    let mut pieces = Vec::new();
    for i in 0..12 {
        let start = now - 6 * hour + i * 1_800_000;
        let end = if i == 11 { now } else { start + 1_799_999 };
        pieces.extend(
            market
                .ticks(symbol.symbol_id, QuoteType::Bid, start, end)
                .await
                .unwrap(),
        );
    }
    println!(
        "seam check: {} ticks in one call, {} in twelve pieces{}",
        whole.len(),
        pieces.len(),
        if whole == pieces {
            " (identical)"
        } else {
            " (DIFFERENT)"
        }
    );
    client.close().await;
}

/// Places the smallest market order the symbol allows on a **demo** account and closes it at
/// once, proving the trading path works end to end. This moves (simulated) money, so it needs a
/// second, explicit opt in on top of `#[ignore]`: set `WYCK_OPENAPI_ALLOW_LIVE_TRADING=1` as well
/// as the variables `read_a_demo_account_end_to_end` needs, and `WYCK_OPENAPI_ACCESS_TOKEN` must be
/// a token authorized with `auth::Scope::Trading` (an `accounts` token is refused by the server). A
/// bare `cargo test`, or a run of this file without that variable, never places an order: the check
/// at the top of this function runs before anything else, including the connection.
#[tokio::test]
#[ignore = "places and closes a real order on a demo account: needs WYCK_OPENAPI_ALLOW_LIVE_TRADING=1"]
async fn place_and_close_a_minimal_market_order_on_a_demo_account() {
    assert_eq!(
        std::env::var("WYCK_OPENAPI_ALLOW_LIVE_TRADING").as_deref(),
        Ok("1"),
        "set WYCK_OPENAPI_ALLOW_LIVE_TRADING=1 to run this test: it places and closes a real \
         order on a demo account, and refuses to do anything, even connect, without this exact \
         opt in"
    );

    let credentials = ClientCredentials::new(
        var("WYCK_OPENAPI_CLIENT_ID"),
        var("WYCK_OPENAPI_CLIENT_SECRET"),
    );
    let token = var("WYCK_OPENAPI_ACCESS_TOKEN");
    let wanted = std::env::var("WYCK_OPENAPI_SYMBOL").unwrap_or_else(|_| "EURUSD".into());

    let client = Client::connect(&ConnectionConfig::new(Environment::Demo))
        .await
        .expect("connect");
    client
        .authenticate_application(&credentials)
        .await
        .expect("application sign in");
    let accounts = client.accounts(&token).await.expect("account list");
    let account_info = accounts
        .ctid_trader_account
        .iter()
        .find(|a| a.is_live == Some(false))
        .expect("the token covers no demo account");
    let account_id = account_info.ctid_trader_account_id;
    let account = client.account(account_id);
    account.authorize(&token).await.expect("account sign in");
    let market = account.market();

    let symbols = market.symbols().await.expect("symbols");
    let symbol = symbols
        .iter()
        .find(|s| s.symbol_name.as_deref() == Some(wanted.as_str()))
        .unwrap_or_else(|| panic!("{wanted} not offered"));
    let details = market
        .symbol_details(&[symbol.symbol_id])
        .await
        .expect("details");
    let min_volume = details[0].min_volume.unwrap_or(1000);
    println!("placing a {min_volume} (hundredths of a unit) market buy on {wanted}");

    let trading = account.trading();
    let request = NewOrderReq::market(
        symbol.symbol_id,
        ctrader_openapi::account::TradeSide::Buy,
        min_volume,
    )
    .with_label("wyck-live-test");
    let execution = trading.new_order(request).await.expect("new order");
    println!("execution: {:?}", execution.kind());
    assert!(
        matches!(
            execution.kind(),
            Some(ExecutionType::OrderFilled | ExecutionType::OrderAccepted)
        ),
        "unexpected execution type: {:?}",
        execution.kind()
    );
    let position_id = execution
        .position
        .as_ref()
        .unwrap_or_else(|| panic!("no position on the execution: {execution:?}"))
        .position_id;
    println!("opened position {position_id}, closing it now");

    let close = trading
        .close_position(position_id, min_volume)
        .await
        .expect("close position");
    println!("close execution: {:?}", close.kind());
    assert!(
        matches!(close.kind(), Some(ExecutionType::OrderFilled)),
        "the closing order did not fill: {:?}",
        close.kind()
    );

    client.close().await;
}
