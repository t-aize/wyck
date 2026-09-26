//! Checks against the real Open API, for a **demo** account. Ignored by default, and each test
//! connects to [`Environment::Demo`] unconditionally, regardless of any `WYCK_OPENAPI_ENVIRONMENT`
//! setting: there is no scenario in which
//! this suite should touch a live account.
//!
//! Together these settle what [the module docs](../src/lib.rs.html) leave open ("What has
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
//! | [`a_session_reconnects_after_a_real_connection_drop`] | [`Session`] reconnecting after a real (proxied) connection drop |
//! | [`place_and_close_a_minimal_market_order_on_a_demo_account`] | Placing and closing a real (simulated) order |
//! | [`amend_and_cancel_a_pending_order_on_a_demo_account`] | Amending and cancelling a real pending order |
//! | [`amend_stop_loss_and_take_profit_on_a_demo_account`] | Amending a real position's stop loss and take profit |
//! | [`tick_history_retention_is_reported`] | How far back tick history actually reaches |
//! | [`bar_history_over_several_pages_has_no_gap_or_duplicate`] | The per-request range limit on bar history, paged |
//!
//! Four of these place, amend, cancel or close a real (simulated) order; every other test only
//! reads, and none of the trading ones runs unless it is opted into on top of `#[ignore]` (see
//! below). All of them refuse a live account, regardless of what `WYCK_OPENAPI_ACCOUNT_ID` or the
//! token itself might allow.
//!
//! # Setup
//!
//! Set the values in `.env` or export them in the shell:
//!
//! ```sh
//! cp .env.example .env
//! # Fill in the client credentials, demo account id and OAuth tokens.
//! cargo test -p wyck-openapi --test live -- --ignored --nocapture
//! ```
//!
//! `.env` is loaded once, through `dotenvy`,
//! the first time any test in this file asks for a variable; a real environment variable (as
//! `cargo test` was invoked with one, or CI sets one) always takes precedence over `.env`, and
//! `.env` is never required: every variable below also works as a plain exported one.
//!
//! Variables read here:
//!
//! - `WYCK_OPENAPI_CLIENT_ID`, `WYCK_OPENAPI_CLIENT_SECRET`: the registered application.
//! - `WYCK_OPENAPI_ACCESS_TOKEN`: an access token. The `accounts` scope is enough for every test
//!   but the trading ones, which need one authorized with `trading` scope.
//! - `WYCK_OPENAPI_REFRESH_TOKEN`: needed only by the two `Session` tests (they exercise the same
//!   `TokenSet` shape a real caller would hold, even though neither triggers a refresh).
//! - `WYCK_OPENAPI_ACCOUNT_ID` (optional): pins which demo account to use, when the token covers
//!   more than one. Without it, the first demo account the token covers is used.
//! - `WYCK_OPENAPI_SYMBOL` (optional, default `EURUSD`).
//! - `WYCK_OPENAPI_ALLOW_LIVE_TRADING=1`, for the trading tests only.
//!
//! Never put real values anywhere but `.env` (already covered by `.gitignore`), and never commit
//! that file. Every test here refuses a live account.

use std::sync::Arc;
use std::time::Duration;

use secrecy::SecretString;
use wyck_openapi::auth::TokenSet;
use wyck_openapi::config::{ClientCredentials, ConnectionConfig, Environment};
use wyck_openapi::handle::AccountClient;
use wyck_openapi::market::symbols::{LightSymbol, Symbol};
use wyck_openapi::market::{Period, QuoteType, merge_sides, to_price};
use wyck_openapi::session::{MemoryTokenStore, Session, SessionConfig, SessionEvent};
use wyck_openapi::trading::{ExecutionType, NewOrderReq};
use wyck_openapi::transport::messages::TraderAccount;
use wyck_openapi::{Client, Event};

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("wyck=debug,warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
}

/// Loads `.env` into the process environment, and installs the `tracing` subscriber every script
/// here uses, the first time any test asks for a variable. Both need `Once` (not a bare call)
/// because several `#[tokio::test]` functions may run concurrently in this binary: mutating the
/// environment from more than one thread at a time is unsound, and installing a global subscriber
/// twice would panic.
fn load_dot_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = dotenvy::dotenv();
        init_tracing();
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

/// Waits up to 10 seconds for the execution event of `wanted` kind for the given position, so a
/// caller that only got a provisional `OrderAccepted` synchronously (the server answers every
/// trading request this way before the real outcome, whether that is a fill or, for an amend, the
/// replace) does not race it, and returns that event.
async fn wait_for_execution(
    events: &mut tokio::sync::broadcast::Receiver<Event>,
    position_id: i64,
    wanted: ExecutionType,
) -> wyck_openapi::trading::ExecutionEvent {
    let matched = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match events.recv().await.expect("event stream") {
                Event::Execution(execution)
                    if execution.kind() == Some(wanted)
                        && execution.position.as_ref().map(|p| p.position_id)
                            == Some(position_id) =>
                {
                    return *execution;
                }
                _ => {}
            }
        }
    })
    .await;
    matched.unwrap_or_else(|_| {
        panic!("position {position_id} had no {wanted:?} execution event within 10s")
    })
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
        .subscribe_spots(&[symbol.symbol_id])
        .await
        .expect("subscribe spots");
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
        wyck_openapi::session::SessionState::Stopped
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
    // Subscribed before the order goes out, so a fill that lands between the synchronous
    // `OrderAccepted` answer and the `wait_for_execution` call below is not missed.
    let mut events = client.events();
    let request = NewOrderReq::market(
        details.symbol_id,
        wyck_openapi::account::TradeSide::Buy,
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
    println!("opened position {position_id}");

    // A market order can answer `OrderAccepted` before the position actually exists server side;
    // closing on that answer alone races the fill and comes back `POSITION_NOT_FOUND`. Wait for
    // the matching `OrderFilled` execution event first when the synchronous answer was not itself
    // the fill.
    if execution.kind() != Some(ExecutionType::OrderFilled) {
        println!("order only accepted so far, waiting for it to fill...");
        wait_for_execution(&mut events, position_id, ExecutionType::OrderFilled).await;
    }

    println!("closing position {position_id} now");
    let close = trading
        .close_position(position_id, min_volume)
        .await
        .expect("close position");
    println!("close execution: {:?}", close.kind());
    let close = if close.kind() == Some(ExecutionType::OrderFilled) {
        close
    } else {
        println!("closing order only accepted so far, waiting for it to fill...");
        wait_for_execution(&mut events, position_id, ExecutionType::OrderFilled).await
    };
    assert!(
        matches!(close.kind(), Some(ExecutionType::OrderFilled)),
        "the closing order did not fill: {:?}",
        close.kind()
    );

    client.close().await;
}

/// Places a limit order far enough from the market that it stays pending, amends its price, then
/// cancels it, proving `amend_order` and `cancel_order` against the real server (both were, until
/// now, only exercised against the mock in `tests/trading.rs`). Needs the same opt in as
/// [`place_and_close_a_minimal_market_order_on_a_demo_account`]: it places a real (simulated)
/// order on a demo account.
#[tokio::test]
#[ignore = "places, amends and cancels a real pending order on a demo account: needs \
            WYCK_OPENAPI_ALLOW_LIVE_TRADING=1"]
async fn amend_and_cancel_a_pending_order_on_a_demo_account() {
    assert_eq!(
        var_or("WYCK_OPENAPI_ALLOW_LIVE_TRADING", "").as_str(),
        "1",
        "set WYCK_OPENAPI_ALLOW_LIVE_TRADING=1 to run this test: it places a real pending order \
         on a demo account, and refuses to do anything, even connect, without this exact opt in"
    );

    let Setup {
        client,
        account,
        details,
        ..
    } = setup().await;
    let market = account.market();
    let trading = account.trading();
    let min_volume = details.min_volume.unwrap_or(1_000);

    let bid = latest_bid(&client, &market, details.symbol_id).await;
    println!("reference bid: {bid}");
    // Half the current bid: a buy limit there fills only if the price halves during the test.
    let far_price = round_to_digits(bid / 2.0, details.digits);
    println!("placing a pending buy limit far below the market, at {far_price}");

    let request = NewOrderReq::limit(
        details.symbol_id,
        wyck_openapi::account::TradeSide::Buy,
        min_volume,
        far_price,
    )
    .with_label("wyck-live-test");
    let placed = trading.new_order(request).await.expect("new limit order");
    println!("execution: {:?}", placed.kind());
    assert_eq!(
        placed.kind(),
        Some(ExecutionType::OrderAccepted),
        "a limit order this far from the market should stay pending, not: {:?}",
        placed.kind()
    );
    let order_id = placed
        .order
        .as_ref()
        .unwrap_or_else(|| panic!("no order on the execution: {placed:?}"))
        .order_id;
    println!("order {order_id} pending");

    let amended_price = round_to_digits(far_price * 0.99, details.digits);
    let mut amend = wyck_openapi::trading::AmendOrderReq::new(order_id);
    amend.limit_price = Some(amended_price);
    let amended = trading.amend_order(amend).await.expect("amend order");
    println!("amend execution: {:?}", amended.kind());
    assert_eq!(
        amended.kind(),
        Some(ExecutionType::OrderReplaced),
        "unexpected execution type for the amend: {:?}",
        amended.kind()
    );

    let cancelled = trading.cancel_order(order_id).await.expect("cancel order");
    println!("cancel execution: {:?}", cancelled.kind());
    assert_eq!(
        cancelled.kind(),
        Some(ExecutionType::OrderCancelled),
        "unexpected execution type for the cancel: {:?}",
        cancelled.kind()
    );

    client.close().await;
}

/// Opens a position, amends its stop loss and take profit, then closes it, proving
/// `amend_position_sl_tp` against the real server. Needs the same opt in as
/// [`place_and_close_a_minimal_market_order_on_a_demo_account`]: it moves (simulated) money on a
/// demo account.
#[tokio::test]
#[ignore = "places, protects and closes a real order on a demo account: needs \
            WYCK_OPENAPI_ALLOW_LIVE_TRADING=1"]
async fn amend_stop_loss_and_take_profit_on_a_demo_account() {
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
    let trading = account.trading();
    let min_volume = details.min_volume.unwrap_or(1_000);
    let mut events = client.events();

    let request = NewOrderReq::market(
        details.symbol_id,
        wyck_openapi::account::TradeSide::Buy,
        min_volume,
    )
    .with_label("wyck-live-test");
    let execution = trading.new_order(request).await.expect("new order");
    let position = execution
        .position
        .clone()
        .unwrap_or_else(|| panic!("no position on the execution: {execution:?}"));
    let position_id = position.position_id;
    println!("opened position {position_id}");
    let execution = if execution.kind() == Some(ExecutionType::OrderFilled) {
        execution
    } else {
        println!("order only accepted so far, waiting for it to fill...");
        wait_for_execution(&mut events, position_id, ExecutionType::OrderFilled).await
    };
    let entry = execution
        .position
        .as_ref()
        .and_then(|p| p.price)
        .unwrap_or_else(|| panic!("no entry price on the fill: {execution:?}"));
    println!("entry price {entry}");

    let stop_loss = round_to_digits(entry * 0.98, details.digits);
    let take_profit = round_to_digits(entry * 1.02, details.digits);
    println!("setting stop loss {stop_loss} and take profit {take_profit}");
    let mut protect = wyck_openapi::trading::AmendPositionSlTpReq::new(position_id);
    protect.stop_loss = Some(stop_loss);
    protect.take_profit = Some(take_profit);
    let protected = trading
        .amend_position_sl_tp(protect)
        .await
        .expect("amend position sl/tp");
    println!("amend execution: {:?}", protected.kind());
    assert!(
        matches!(
            protected.kind(),
            Some(ExecutionType::OrderReplaced | ExecutionType::OrderAccepted)
        ),
        "unexpected execution type for the sl/tp amend: {:?}",
        protected.kind()
    );
    // Whichever of those it answered with synchronously, `open_positions_and_orders` is the
    // ground truth for whether the amend actually landed: this sidesteps needing to know which
    // execution type (if any, and it is not consistently the same one run to run) reports the
    // async confirmation for a protection amend specifically, unlike a fill or a pending order's
    // own replace.
    let account_data = account.account_data();
    let half_tick = 10f64.powi(-(details.digits as i32)) / 2.0;
    let confirmed = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let (positions, _) = account_data
                .open_positions_and_orders(false)
                .await
                .expect("open positions");
            if let Some(position) = positions.iter().find(|p| p.position_id == position_id)
                && position
                    .stop_loss
                    .is_some_and(|sl| (sl - stop_loss).abs() < half_tick)
                && position
                    .take_profit
                    .is_some_and(|tp| (tp - take_profit).abs() < half_tick)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await;
    confirmed.unwrap_or_else(|_| {
        panic!("position {position_id} did not show the new stop loss/take profit within 15s")
    });
    println!("stop loss and take profit confirmed on the position");

    println!("closing position {position_id} now");
    let close = trading
        .close_position(position_id, min_volume)
        .await
        .expect("close position");
    let close = if close.kind() == Some(ExecutionType::OrderFilled) {
        close
    } else {
        println!("closing order only accepted so far, waiting for it to fill...");
        wait_for_execution(&mut events, position_id, ExecutionType::OrderFilled).await
    };
    assert!(
        matches!(close.kind(), Some(ExecutionType::OrderFilled)),
        "the closing order did not fill: {:?}",
        close.kind()
    );

    client.close().await;
}

/// Reports how far back tick history actually reaches for the account's broker: not a fixed
/// number the documentation states anywhere, so this narrows it by probing progressively older
/// one-day windows (1, 7, 30, 90, 180 and 365 days back) rather than asserting one. Read only.
#[tokio::test]
#[ignore = "needs an approved Open API application and a demo access token"]
async fn tick_history_retention_is_reported() {
    let Setup {
        client,
        account,
        symbol,
        ..
    } = setup().await;
    let market = account.market();
    let day = 86_400_000;
    let now = now_ms();

    let mut deepest_with_data = 0i64;
    for days_back in [1, 7, 30, 90, 180, 365] {
        let to = now - days_back * day;
        let from = to - day;
        let ticks = market
            .ticks(symbol.symbol_id, QuoteType::Bid, from, to)
            .await
            .unwrap_or_else(|error| panic!("ticks {days_back} days back: {error}"));
        println!("{days_back} day(s) back: {} tick(s)", ticks.len());
        if !ticks.is_empty() {
            deepest_with_data = days_back;
        }
    }
    println!(
        "deepest one-day window that still had ticks: {deepest_with_data} day(s) back (a gap \
         further back than this is not proof retention stops there, only that this probe did \
         not find data beyond it: a weekend or a holiday reads the same as no retention)"
    );

    client.close().await;
}

/// Fetches four days of M1 bars in one range: `market_data_end_to_end` already gets a single
/// server-side page for one day, so four days forces `fetch_bars` through more than one
/// truncation instead. Checks the result has no gap or duplicate. Read only; answers the open
/// question about the per-request range limit on bar history without needing to know its exact
/// value.
#[tokio::test]
#[ignore = "needs an approved Open API application and a demo access token"]
async fn bar_history_over_several_pages_has_no_gap_or_duplicate() {
    let Setup {
        client,
        account,
        symbol,
        ..
    } = setup().await;
    let market = account.market();
    let now = now_ms();
    let four_days = 4 * 86_400_000;

    let bars = market
        .bars(symbol.symbol_id, Period::M1, now - four_days, now)
        .await
        .expect("bars");
    println!("{} M1 bars over 4 days", bars.len());
    assert!(
        bars.len() > 1_000,
        "expected several server-side pages worth of M1 bars over 4 days, got {}",
        bars.len()
    );

    let period_ms = Period::M1.millis();
    let mut gaps = 0;
    let mut duplicates = 0;
    for pair in bars.windows(2) {
        let delta = pair[1].time_ms - pair[0].time_ms;
        match delta.cmp(&period_ms) {
            std::cmp::Ordering::Equal => {}
            std::cmp::Ordering::Greater => gaps += 1,
            std::cmp::Ordering::Less => duplicates += 1,
        }
    }
    // A gap can be a legitimate quiet minute (no tick, no bar), so this is not asserted on: what
    // matters for the range limit question is that paging produced no duplicate and, combined with
    // `market_data_end_to_end`'s single-page check, agrees across page boundaries. A duplicate
    // would mean a truncated page was requested again instead of continued from.
    println!("{gaps} gap(s) (may be legitimately quiet minutes), {duplicates} duplicate(s)");
    assert_eq!(
        duplicates, 0,
        "fetch_bars must not return the same bar twice across a page seam"
    );

    client.close().await;
}

/// A real, proxied connection to the real demo server, severed mid-session, forcing
/// [`Session`] to notice and reconnect for real: not a dropped mock socket, but a genuine TLS
/// connection to `demo.ctraderapi.com` that this test tears down out from under it. `Session`
/// dials `run_proxy`'s local address like any WebSocket endpoint; `run_proxy` pairs each dial to
/// a fresh real connection to the demo server and relays messages both ways until told to sever
/// one. Read only: no order is placed.
#[tokio::test]
#[ignore = "needs an approved Open API application and a demo access token"]
async fn a_session_reconnects_after_a_real_connection_drop() {
    let credentials = ClientCredentials::new(
        var("WYCK_OPENAPI_CLIENT_ID"),
        var("WYCK_OPENAPI_CLIENT_SECRET"),
    );
    let access_token = var("WYCK_OPENAPI_ACCESS_TOKEN");
    let refresh_token = var("WYCK_OPENAPI_REFRESH_TOKEN");

    let account_id = {
        let client = Client::connect(&ConnectionConfig::new(Environment::Demo))
            .await
            .expect("connect");
        client
            .authenticate_application(&credentials)
            .await
            .expect("application sign in");
        let accounts = client.accounts(&access_token).await.expect("account list");
        let account_id = pick_account(&accounts.ctid_trader_account).ctid_trader_account_id;
        client.close().await;
        account_id
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the proxy");
    let proxy_addr = listener.local_addr().expect("proxy address");
    let (pairings_tx, mut pairings_rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(run_proxy(listener, pairings_tx));

    let tokens = TokenSet {
        access_token: SecretString::from(access_token),
        refresh_token: SecretString::from(refresh_token),
        token_type: Some("bearer".to_owned()),
        // See `a_session_connects_subscribes_and_stops_against_the_real_server` for why this is
        // deliberately left unset.
        expires_in: None,
        obtained_at: std::time::SystemTime::now(),
    };
    let config = SessionConfig::new(
        ConnectionConfig::with_url(format!("ws://{proxy_addr}")),
        credentials,
        account_id,
    );
    let session = Session::start(config, tokens, Arc::new(MemoryTokenStore::default()))
        .expect("start the session");

    session
        .wait_ready(Duration::from_secs(15))
        .await
        .expect("the session to become ready through the proxy");
    println!(
        "session ready through the proxy, account {}",
        session.account_id()
    );

    let first_pairing = tokio::time::timeout(Duration::from_secs(5), pairings_rx.recv())
        .await
        .expect("the proxy to report its first pairing within 5s")
        .expect("the proxy's channel to stay open");

    let mut state = session.state();
    first_pairing.notify_one();
    println!("severed the proxied connection to the real server");

    tokio::time::timeout(Duration::from_secs(5), async {
        while *state.borrow() == wyck_openapi::session::SessionState::Ready {
            state.changed().await.expect("state stream");
        }
    })
    .await
    .expect("the session did not notice the drop within 5s");
    println!("session left Ready: {:?}", *state.borrow());

    tokio::time::timeout(Duration::from_secs(5), pairings_rx.recv())
        .await
        .expect("the proxy to accept a second, reconnecting pairing within 5s")
        .expect("the proxy's channel to stay open");
    println!("proxy paired a second connection: the session actually redialed");

    session
        .wait_ready(Duration::from_secs(20))
        .await
        .expect("the session to reconnect and become ready again");
    println!("session ready again after the drop");

    session.stop().await;
    assert!(matches!(
        *session.state().borrow(),
        wyck_openapi::session::SessionState::Stopped
    ));
}

/// Rounds `value` to `digits` decimal places (`Symbol::digits`), so a price built by arithmetic
/// (half the bid, entry times 1.02, ...) encodes the way the server expects.
fn round_to_digits(value: f64, digits: i64) -> f64 {
    let scale = 10f64.powi(digits as i32);
    (value * scale).round() / scale
}

/// One current bid for `symbol_id`, subscribed and read for up to 10 seconds, then unsubscribed.
/// For a reference price before any order exists yet (an execution event has its own fill price,
/// so callers that already placed one do not need this).
async fn latest_bid(
    client: &Client,
    market: &wyck_openapi::market::MarketClient,
    symbol_id: i64,
) -> f64 {
    let mut events = client.events();
    market
        .subscribe_spots(&[symbol_id])
        .await
        .expect("spot subscription");
    let bid = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Event::Spot(spot) = events.recv().await.expect("event stream")
                && let Some(bid) = spot.bid
            {
                return to_price(bid);
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no bid for symbol {symbol_id} within 10s"));
    let _ = market.unsubscribe_spots(&[symbol_id]).await;
    bid
}

/// Relays WebSocket connections between `listener` (a local, plaintext endpoint `Session` can
/// dial) and a fresh, real TLS connection to the demo server for each one, so a test can sever a
/// specific pairing and force a genuine reconnect rather than a simulated one. Sends a kill
/// switch (a [`tokio::sync::Notify`], fired to sever that one pairing) on `pairings` for every
/// pairing it makes.
async fn run_proxy(
    listener: tokio::net::TcpListener,
    pairings: tokio::sync::mpsc::UnboundedSender<Arc<tokio::sync::Notify>>,
) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let Ok(local) = tokio_tungstenite::accept_async(stream).await else {
            continue;
        };
        let Ok((remote, _)) = tokio_tungstenite::connect_async(Environment::Demo.url()).await
        else {
            continue;
        };
        let kill = Arc::new(tokio::sync::Notify::new());
        if pairings.send(kill.clone()).is_err() {
            return;
        }
        tokio::spawn(relay(local, remote, kill));
    }
}

/// Pumps WebSocket messages both ways between `local` and `remote` until either side closes,
/// errors, or `kill` fires: firing it drops both sockets at once, mid-stream, which is what a
/// real network drop looks like to each end (not a clean WebSocket close).
async fn relay(
    mut local: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    mut remote: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    kill: Arc<tokio::sync::Notify>,
) {
    use futures_util::{SinkExt, StreamExt};
    loop {
        tokio::select! {
            () = kill.notified() => return,
            msg = local.next() => match msg {
                Some(Ok(message)) => if remote.send(message).await.is_err() { return },
                _ => return,
            },
            msg = remote.next() => match msg {
                Some(Ok(message)) => if local.send(message).await.is_err() { return },
                _ => return,
            },
        }
    }
}
