//! Checks against the real Open API, for a **demo** account. Ignored by default, and each test
//! connects to [`Environment::Demo`] unconditionally, regardless of any `WYCK_OPENAPI_ENVIRONMENT`
//! setting (that variable is for `scripts/`, not for this file): there is no scenario in which
//! this suite should touch a live account.
//!
//! Together these settle what [the module docs](../src/openapi/mod.rs.html) leave open ("What has
//! and has not been verified against a live server") across every domain the crate covers:
//!
//! | Test | Covers |
//! |---|---|
//! | [`market_data_end_to_end`] | Symbols, live prices, tick and bar history, paging, the seam between pages |
//! | [`account_data_end_to_end`] | Balance, positions, working orders, deals, orders, cash flow, unrealized PnL |
//! | [`margin_data_end_to_end`] | Expected margin, the margin call thresholds |
//! | [`reference_catalogs_are_read_end_to_end`] | Assets, asset classes, symbol categories, a conversion chain |
//! | [`live_bars_and_depth_subscriptions_receive_events`] | Live trendbars and the order book, as events |
//! | [`a_session_connects_subscribes_and_stops_against_the_real_server`] | [`Session`] end to end: connect, sign in, subscribe, stop |
//! | [`place_and_close_a_minimal_market_order_on_a_demo_account`] | Placing and closing a real (simulated) order |
//!
//! Only the last one writes anything; every other test only reads, and none of them place or
//! touch an order unless that one specific test is opted into (see below). All of them refuse a
//! live account, regardless of what `WYCK_OPENAPI_ACCOUNT_ID` or the token itself might allow.
//!
//! # Setup
//!
//! The fast path is `scripts/`, not filling in variables by hand:
//!
//! ```sh
//! cp .env.example .env                        # then fill in the two WYCK_OPENAPI_CLIENT_* values
//! cargo run --bin sign-in -- --write-env       # signs in through the browser, fills in the rest
//! cargo run --bin check-connection             # a few seconds: confirms .env actually works
//! cargo test --test live -- --ignored --nocapture
//! ```
//!
//! See `scripts/README.md` for what each script does. `.env` is loaded once, through `dotenvy`,
//! the first time any test in this file asks for a variable; a real environment variable (as
//! `cargo test` was invoked with one, or CI sets one) always takes precedence over `.env`, and
//! `.env` is never required: every variable below also works as a plain exported one.
//!
//! Variables read here:
//!
//! - `WYCK_OPENAPI_CLIENT_ID`, `WYCK_OPENAPI_CLIENT_SECRET`: the registered application.
//! - `WYCK_OPENAPI_ACCESS_TOKEN`: an access token. The `accounts` scope is enough for every test
//!   but the trading one, which needs one authorized with `trading` scope.
//! - `WYCK_OPENAPI_REFRESH_TOKEN`: needed only by the `Session` test (it exercises the same
//!   `TokenSet` shape a real caller would hold, even though this test never triggers a refresh).
//! - `WYCK_OPENAPI_ACCOUNT_ID` (optional): pins which demo account to use, when the token covers
//!   more than one. Without it, the first demo account the token covers is used.
//! - `WYCK_OPENAPI_SYMBOL` (optional, default `EURUSD`).
//! - `WYCK_OPENAPI_ALLOW_LIVE_TRADING=1`, for the trading test only.
//!
//! Never put real values anywhere but `.env` (already covered by `.gitignore`), and never commit
//! that file. Every test here refuses a live account.

use std::sync::Arc;
use std::time::Duration;

use secrecy::SecretString;
use wyck::openapi::auth::TokenSet;
use wyck::openapi::config::{ClientCredentials, ConnectionConfig, Environment};
use wyck::openapi::handle::AccountClient;
use wyck::openapi::market::symbols::{LightSymbol, Symbol};
use wyck::openapi::market::{Period, QuoteType, merge_sides, to_price};
use wyck::openapi::session::{MemoryTokenStore, Session, SessionConfig, SessionEvent};
use wyck::openapi::trading::{ExecutionType, NewOrderReq};
use wyck::openapi::transport::messages::TraderAccount;
use wyck::openapi::{Client, Event};

/// Loads `.env` into the process environment the first time any test asks for a variable.
/// `Once` (not a bare call) because several `#[tokio::test]` functions may run concurrently in
/// this binary, and mutating the environment from more than one thread at a time is unsound.
fn load_dot_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = dotenvy::dotenv();
    });
}

fn var(name: &str) -> String {
    load_dot_env();
    std::env::var(name)
        .unwrap_or_else(|_| panic!("set {name} (in the environment or in .env) to run this test"))
}

fn var_or(name: &str, default: &str) -> String {
    load_dot_env();
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// `WYCK_OPENAPI_ACCOUNT_ID`, if set and covered by the token, else the first demo account.
/// Explicit beats implicit, so a token covering several demo accounts stays deterministic.
fn pick_account(accounts: &[TraderAccount]) -> &TraderAccount {
    if let Ok(text) = std::env::var("WYCK_OPENAPI_ACCOUNT_ID")
        && !text.trim().is_empty()
    {
        let id: i64 = text
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("WYCK_OPENAPI_ACCOUNT_ID={text:?} is not a number"));
        return accounts
            .iter()
            .find(|a| a.ctid_trader_account_id == id)
            .unwrap_or_else(|| {
                panic!("account {id} (from WYCK_OPENAPI_ACCOUNT_ID) is not covered by this token")
            });
    }
    accounts
        .iter()
        .find(|a| a.is_live == Some(false))
        .expect("the token covers no demo account")
}

/// What every test below but the `Session` one builds on: a connected, signed-in client, bound
/// to the demo account, with `WYCK_OPENAPI_SYMBOL` (default `EURUSD`) looked up on it.
struct Setup {
    client: Client,
    account: AccountClient,
    symbol: LightSymbol,
    details: Symbol,
}

async fn setup() -> Setup {
    let credentials = ClientCredentials::new(
        var("WYCK_OPENAPI_CLIENT_ID"),
        var("WYCK_OPENAPI_CLIENT_SECRET"),
    );
    let token = var("WYCK_OPENAPI_ACCESS_TOKEN");
    let wanted = var_or("WYCK_OPENAPI_SYMBOL", "EURUSD");

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
    let account_info = pick_account(&accounts.ctid_trader_account);
    let account_id = account_info.ctid_trader_account_id;
    println!(
        "using demo account {account_id} ({:?})",
        account_info.broker_title_short
    );
    let account = client.account(account_id);
    account.authorize(&token).await.expect("account sign in");

    let symbols = account.market().symbols().await.expect("symbols");
    println!("{} symbols", symbols.len());
    let symbol = symbols
        .iter()
        .find(|s| s.symbol_name.as_deref() == Some(wanted.as_str()))
        .unwrap_or_else(|| panic!("{wanted} not offered"))
        .clone();
    let details = account
        .market()
        .symbol_details(&[symbol.symbol_id])
        .await
        .expect("details")
        .into_iter()
        .next()
        .expect("the server answered with no details for a symbol it just listed");
    println!("details: {details:?}");

    Setup {
        client,
        account,
        symbol,
        details,
    }
}

#[tokio::test]
#[ignore = "needs an approved Open API application and a demo access token"]
async fn market_data_end_to_end() {
    let Setup {
        client,
        account,
        symbol,
        ..
    } = setup().await;
    let market = account.market();

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
    let clock = now_ms();
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
            "first bar {:?}\nlast bar  {:?}",
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

#[tokio::test]
#[ignore = "needs an approved Open API application and a demo access token"]
async fn account_data_end_to_end() {
    let Setup {
        client, account, ..
    } = setup().await;
    let data = account.account_data();

    let trader = data.trader().await.expect("trader");
    println!(
        "balance {:.2}, leverage {:?}, rights {:?}, kind {:?}",
        trader.balance_amount(),
        trader.leverage(),
        trader.rights(),
        trader.kind()
    );

    let (positions, orders) = data
        .open_positions_and_orders(true)
        .await
        .expect("open positions and orders");
    println!(
        "{} open position(s), {} working order(s)",
        positions.len(),
        orders.len()
    );

    let now = now_ms();
    let day = 86_400_000;
    let (deals, more_deals) = data
        .deals(now - 30 * day, now, Some(50))
        .await
        .expect("deals");
    println!(
        "{} deal(s) in the last 30 days (more: {more_deals})",
        deals.len()
    );
    let (order_history, more_orders) = data.orders(now - 30 * day, now).await.expect("orders");
    println!(
        "{} order(s) in the last 30 days (more: {more_orders})",
        order_history.len()
    );

    let cash_flow = data
        .cash_flow_history(now - 7 * day, now)
        .await
        .expect("cash flow history");
    println!(
        "{} cash flow entry/entries in the last week",
        cash_flow.len()
    );

    let pnl = data
        .position_unrealized_pnl()
        .await
        .expect("unrealized pnl");
    println!("unrealized pnl on {} open position(s): {pnl:?}", pnl.len());

    // The by-position and by-order calls need an existing position/order/deal to be meaningful.
    // A brand new demo account has none, so these are exercised opportunistically instead of
    // required, and this test stays meaningful either way (run the trading test first, or place
    // one order by hand, to also cover this part).
    if let Some(position) = positions.first() {
        let (position_deals, _) = data
            .deals_by_position(position.position_id, None, None)
            .await
            .expect("deals by position");
        let (position_orders, _) = data
            .orders_by_position(position.position_id, None, None)
            .await
            .expect("orders by position");
        println!(
            "position {}: {} deal(s), {} order(s)",
            position.position_id,
            position_deals.len(),
            position_orders.len()
        );
    } else {
        println!("no open position: deals_by_position/orders_by_position not exercised");
    }
    if let Some(deal) = deals.first() {
        let (offset_by, offsetting) = data.deal_offsets(deal.deal_id).await.expect("deal offsets");
        println!(
            "deal {}: offset by {} deal(s), offsetting {} deal(s)",
            deal.deal_id,
            offset_by.len(),
            offsetting.len()
        );
    } else {
        println!("no deal in the last 30 days: deal_offsets not exercised");
    }
    if let Some(order) = order_history.first() {
        let (order, order_deals) = data
            .order_details(order.order_id)
            .await
            .expect("order details");
        println!(
            "order {}: {} deal(s) filled it",
            order.order_id,
            order_deals.len()
        );
    } else {
        println!("no order in the last 30 days: order_details not exercised");
    }

    client.close().await;
}

#[tokio::test]
#[ignore = "needs an approved Open API application and a demo access token"]
async fn margin_data_end_to_end() {
    let Setup {
        client,
        account,
        details,
        ..
    } = setup().await;
    let margin = account.margin();

    let volume = details.min_volume.unwrap_or(1_000);
    let expected = margin
        .expected_margin(details.symbol_id, &[volume, volume * 2])
        .await
        .expect("expected margin");
    println!(
        "expected margin for {volume} and {}: {expected:?}",
        volume * 2
    );
    assert_eq!(expected.len(), 2, "one answer per volume asked");

    let thresholds = margin.margin_calls().await.expect("margin calls");
    println!(
        "{} margin call threshold(s): {thresholds:?}",
        thresholds.len()
    );

    // `MarginClient::dynamic_leverage` needs a `leverage_id`, which this crate's trimmed
    // `Symbol` does not carry (see its docs): not exercised here for lack of one to ask with.

    client.close().await;
}

#[tokio::test]
#[ignore = "needs an approved Open API application and a demo access token"]
async fn reference_catalogs_are_read_end_to_end() {
    let Setup {
        client,
        account,
        symbol,
        ..
    } = setup().await;
    let market = account.market();

    let assets = market.assets().await.expect("assets");
    println!("{} asset(s)", assets.len());
    assert!(
        !assets.is_empty(),
        "a broker always lists at least one asset"
    );

    let asset_classes = market.asset_classes().await.expect("asset classes");
    println!("{} asset class(es)", asset_classes.len());
    assert!(!asset_classes.is_empty());

    let categories = market.symbol_categories().await.expect("symbol categories");
    println!("{} symbol categor(y/ies)", categories.len());
    assert!(!categories.is_empty());

    let symbols_incl_archived = market
        .symbols_including_archived()
        .await
        .expect("symbols including archived");
    let symbols = market.symbols().await.expect("symbols");
    println!(
        "{} symbols enabled for the account, {} including archived ones",
        symbols.len(),
        symbols_incl_archived.len()
    );
    assert!(symbols_incl_archived.len() >= symbols.len());

    let trader = account.account_data().trader().await.expect("trader");
    match (symbol.quote_asset_id, trader.deposit_asset_id) {
        (Some(quote), Some(deposit)) if quote != deposit => {
            match market.symbols_for_conversion(quote, deposit).await {
                Ok(chain) => println!(
                    "conversion chain from asset {quote} to {deposit}: {} symbol(s)",
                    chain.len()
                ),
                Err(error) => println!(
                    "symbols_for_conversion({quote}, {deposit}) answered with a server error, \
                     not a panic: {error}"
                ),
            }
        }
        _ => println!(
            "the traded symbol is already quoted in the account's deposit asset: nothing to \
             convert, symbols_for_conversion not exercised"
        ),
    }

    client.close().await;
}

#[tokio::test]
#[ignore = "needs an approved Open API application and a demo access token"]
async fn live_bars_and_depth_subscriptions_receive_events() {
    let Setup {
        client,
        account,
        symbol,
        ..
    } = setup().await;
    let market = account.market();
    let mut events = client.events();

    market
        .subscribe_live_bars(symbol.symbol_id, Period::M1)
        .await
        .expect("subscribe live bars");
    market
        .subscribe_depth(&[symbol.symbol_id])
        .await
        .expect("subscribe depth");

    let (mut live_bar_seen, mut depth_seen) = (false, false);
    let _ = tokio::time::timeout(Duration::from_secs(15), async {
        while let Ok(event) = events.recv().await {
            match event {
                Event::Spot(spot) if !spot.trendbar.is_empty() => {
                    live_bar_seen = true;
                    println!("live bar update: {:?}", spot.trendbar);
                }
                Event::Depth(depth) => {
                    depth_seen = true;
                    println!(
                        "depth update: {} new/changed level(s), {} removed",
                        depth.new_quotes.len(),
                        depth.deleted_quotes.len()
                    );
                }
                _ => {}
            }
            if live_bar_seen && depth_seen {
                break;
            }
        }
    })
    .await;

    // Neither is guaranteed inside any fixed window (a quiet minute has no bar update yet; not
    // every symbol offers a book at all), so this reports on each separately rather than failing
    // outright on either one alone; `market_data_end_to_end` already proves the base spot stream
    // itself works. Seeing neither in 15 seconds on EURUSD during market hours is still a real
    // signal, so that combination does fail the test.
    println!("live bar event seen: {live_bar_seen}, depth event seen: {depth_seen}");
    assert!(
        live_bar_seen || depth_seen,
        "neither a live bar nor a depth event arrived in 15 seconds"
    );
    client.close().await;
}

#[tokio::test]
#[ignore = "needs an approved Open API application and a demo access token"]
async fn a_session_connects_subscribes_and_stops_against_the_real_server() {
    let credentials = ClientCredentials::new(
        var("WYCK_OPENAPI_CLIENT_ID"),
        var("WYCK_OPENAPI_CLIENT_SECRET"),
    );
    let access_token = var("WYCK_OPENAPI_ACCESS_TOKEN");
    let refresh_token = var("WYCK_OPENAPI_REFRESH_TOKEN");
    let wanted = var_or("WYCK_OPENAPI_SYMBOL", "EURUSD");

    // Picking the account and the symbol needs its own short-lived connection: `Session` takes
    // ownership of the account id and the tokens, not of a `Client` to reuse.
    let (account_id, symbol_id) = {
        let client = Client::connect(&ConnectionConfig::new(Environment::Demo))
            .await
            .expect("connect");
        client
            .authenticate_application(&credentials)
            .await
            .expect("application sign in");
        let accounts = client.accounts(&access_token).await.expect("account list");
        let account_id = pick_account(&accounts.ctid_trader_account).ctid_trader_account_id;
        let account = client.account(account_id);
        account
            .authorize(&access_token)
            .await
            .expect("account sign in");
        let symbols = account.market().symbols().await.expect("symbols");
        let symbol_id = symbols
            .iter()
            .find(|s| s.symbol_name.as_deref() == Some(wanted.as_str()))
            .unwrap_or_else(|| panic!("{wanted} not offered"))
            .symbol_id;
        client.close().await;
        (account_id, symbol_id)
    };

    let tokens = TokenSet {
        access_token: SecretString::from(access_token),
        refresh_token: SecretString::from(refresh_token),
        token_type: Some("bearer".to_owned()),
        // Deliberately not set: `Session::expires_within` then never reports the token expiring,
        // so this short test never triggers `OAuthClient::refresh` (which would rotate, and so
        // invalidate, the refresh token this run was given).
        expires_in: None,
        obtained_at: std::time::SystemTime::now(),
    };
    let config = SessionConfig::new(
        ConnectionConfig::new(Environment::Demo),
        credentials,
        account_id,
    );
    let session = Session::start(config, tokens, Arc::new(MemoryTokenStore::default()))
        .expect("start the session");
    let mut events = session.events();

    session
        .wait_ready(Duration::from_secs(15))
        .await
        .expect("the session to become ready");
    println!("session ready, account {}", session.account_id());

    session
        .subscribe_spots(&[symbol_id])
        .await
        .expect("subscribe");

    let mut seen = 0;
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while let Ok(event) = events.recv().await {
            if let SessionEvent::Data(Event::Spot(_)) = event {
                seen += 1;
                break;
            }
        }
    })
    .await;
    // Not asserted on its own: a closed market gives no spot event, same reasoning as
    // `market_data_end_to_end`. What this test is really proving is the connect/sign-in/ready
    // path and a clean stop, both of which already ran above and below regardless.
    println!("{seen} spot event(s) received through the session in 10 seconds");

    session.stop().await;
    assert!(matches!(
        *session.state().borrow(),
        wyck::openapi::session::SessionState::Stopped
    ));
}

/// Places the smallest market order the symbol allows on a **demo** account and closes it at
/// once, proving the trading path works end to end. This moves (simulated) money, so it needs a
/// second, explicit opt in on top of `#[ignore]`: set `WYCK_OPENAPI_ALLOW_LIVE_TRADING=1` as well
/// as the variables [`setup`] needs, and `WYCK_OPENAPI_ACCESS_TOKEN` must be a token authorized
/// with `auth::Scope::Trading` (an `accounts` token is refused by the server). A bare
/// `cargo test`, or a run of this file without that variable, never places an order: the check at
/// the top of this function runs before anything else, including the connection.
#[tokio::test]
#[ignore = "places and closes a real order on a demo account: needs WYCK_OPENAPI_ALLOW_LIVE_TRADING=1"]
async fn place_and_close_a_minimal_market_order_on_a_demo_account() {
    assert_eq!(
        var_or("WYCK_OPENAPI_ALLOW_LIVE_TRADING", "").as_str(),
        "1",
        "set WYCK_OPENAPI_ALLOW_LIVE_TRADING=1 to run this test: it places and closes a real \
         order on a demo account, and refuses to do anything, even connect, without this exact \
         opt in"
    );

    let Setup {
        client,
        account,
        details,
        ..
    } = setup().await;
    let min_volume = details.min_volume.unwrap_or(1_000);
    println!(
        "placing a {min_volume} (hundredths of a unit) market buy on symbol {}",
        details.symbol_id
    );

    let trading = account.trading();
    let request = NewOrderReq::market(
        details.symbol_id,
        wyck::openapi::account::TradeSide::Buy,
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
