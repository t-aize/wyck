//! A check against the real Open API, for a **demo** account. Ignored by default.
//!
//! It exists to settle what the documentation leaves open (see the crate docs, "What has not been
//! verified against a live server"). It only reads: no order is placed.
//!
//! Set these variables, then run
//! `cargo test -p ctrader-openapi --test live -- --ignored --nocapture`:
//!
//! - `WYCK_OPENAPI_CLIENT_ID` and `WYCK_OPENAPI_CLIENT_SECRET`: the registered application.
//! - `WYCK_OPENAPI_ACCESS_TOKEN`: an access token with at least the `accounts` scope (get one with
//!   the `sign_in` example: `cargo run -p ctrader-openapi --example sign_in`).
//! - `WYCK_OPENAPI_SYMBOL` (optional, default `EURUSD`).
//!
//! Never put the values in a file. The test refuses a live account.

use std::time::Duration;

use ctrader_openapi::config::{ClientCredentials, ConnectionConfig, Environment};
use ctrader_openapi::history::{fetch_bars, fetch_ticks};
use ctrader_openapi::types::{Period, QuoteType, merge_sides, to_price};
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
    let account = accounts
        .ctid_trader_account
        .iter()
        .find(|a| a.is_live == Some(false))
        .expect("the token covers no demo account");
    let account_id = account.ctid_trader_account_id;
    println!(
        "using demo account {account_id} ({:?})",
        account.broker_title_short
    );
    client
        .authorize_account(account_id, &token)
        .await
        .expect("account sign in");

    let symbols = client.symbols(account_id, false).await.expect("symbols");
    println!("{} symbols", symbols.len());
    let symbol = symbols
        .iter()
        .find(|s| s.symbol_name.as_deref() == Some(wanted.as_str()))
        .unwrap_or_else(|| panic!("{wanted} not offered"));
    let details = client
        .symbol_details(account_id, &[symbol.symbol_id])
        .await
        .expect("details");
    println!("details: {details:?}");

    // Live prices for a few seconds.
    let mut events = client.events();
    client
        .subscribe_spots(account_id, &[symbol.symbol_id], true)
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
        let ticks = fetch_ticks(
            &client,
            account_id,
            symbol.symbol_id,
            side,
            now - 6 * hour,
            now,
        )
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
    let bids = fetch_ticks(
        &client,
        account_id,
        symbol.symbol_id,
        QuoteType::Bid,
        now - hour,
        now,
    )
    .await
    .unwrap();
    let asks = fetch_ticks(
        &client,
        account_id,
        symbol.symbol_id,
        QuoteType::Ask,
        now - hour,
        now,
    )
    .await
    .unwrap();
    println!(
        "{} quotes in the last hour",
        merge_sides(&bids, &asks).len()
    );

    let bars = fetch_bars(
        &client,
        account_id,
        symbol.symbol_id,
        Period::M1,
        now - 24 * hour,
        now,
    )
    .await
    .expect("bar history");
    println!("{} M1 bars in a day", bars.len());
    client.close().await;
}
