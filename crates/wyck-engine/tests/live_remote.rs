//! Live tests against a real cTrader **Remote** MCP server. All `#[ignore]`d: run them by
//! hand, on a demo account. See `live_support/mod.rs` for the safety rules and variables.
//!
//! ```sh
//! # Read only, safe:
//! WYCK_LIVE_REMOTE_TOKEN=... cargo test -p wyck-engine --test live_remote -- --ignored read_only
//! # Places and closes real orders on the demo account:
//! WYCK_LIVE_REMOTE_TOKEN=... WYCK_LIVE_CONFIRM_DEMO=1 \
//!     cargo test -p wyck-engine --test live_remote -- --ignored --test-threads=1
//! ```

mod live_support;

use std::time::Duration;

use live_support::{
    assert_account_is_empty, env, eventually, filled, flatten_everything, live_config,
    require_demo_confirmation, round_to, sizing, start, symbol, volume, wait_ready, with_cleanup,
};
use secrecy::SecretString;
use wyck_engine::broker::{ConnectRequest, ServiceKind};
use wyck_engine::domain::{AccountKind, Side};
use wyck_engine::{
    ArmRequest, CloseSize, EngineHandle, EntryIntent, FlattenScope, OrderOutcome, SizeSpec,
    StopSpec, TakeProfitSpec,
};

const DEFAULT_ENDPOINT: &str = "https://mcp.ctrader.com/trading/mcp";

fn token() -> SecretString {
    SecretString::from(
        env("WYCK_LIVE_REMOTE_TOKEN").expect("set WYCK_LIVE_REMOTE_TOKEN to a demo Remote token"),
    )
}

/// Connects and refuses anything but a demo token, whatever else is set.
async fn connect(config: wyck_engine::EngineConfig) -> (wyck_engine::Engine, EngineHandle) {
    let token = token();
    assert_eq!(
        AccountKind::from_token(&token),
        AccountKind::Demo,
        "the live tests only run with a token whose environment claim is `demo`"
    );
    let (engine, handle) = start(config);
    let endpoint = env("WYCK_LIVE_REMOTE_ENDPOINT").unwrap_or_else(|| DEFAULT_ENDPOINT.to_owned());
    handle
        .connect(ConnectRequest::new(
            ServiceKind::CtraderRemote,
            endpoint,
            Some(token),
        ))
        .await
        .expect("connect to Remote");
    wait_ready(&handle).await;
    (engine, handle)
}

async fn arm_demo(handle: &EngineHandle) {
    let account = handle.state().account.clone().expect("account is loaded");
    assert_eq!(account.kind, AccountKind::Demo);
    handle
        .arm(ArmRequest {
            account: account.account_id,
            acknowledged_kind: AccountKind::Demo,
        })
        .await
        .expect("arm on the demo account");
}

/// A buy of `volume` units with a fixed price-distance stop and a 2R target.
fn buy(symbol: &str, units: f64, stop_distance: f64) -> EntryIntent {
    EntryIntent {
        symbol: symbol.to_owned(),
        side: Side::Buy,
        size: SizeSpec::Fixed(volume(units)),
        stop_loss: Some(StopSpec::Distance(stop_distance)),
        take_profit: Some(TakeProfitSpec::RiskReward(2.0)),
    }
}

async fn open(handle: &EngineHandle, intent: EntryIntent) -> wyck_engine::domain::Position {
    let plan = handle.plan_entry(intent).await.expect("plan");
    eprintln!("plan: {plan:#?}");
    let outcome = handle.submit(plan.id).await.expect("submit");
    eprintln!("outcome: {outcome:#?}");
    filled(outcome)
}

#[tokio::test]
#[ignore = "talks to a real server; needs WYCK_LIVE_REMOTE_TOKEN"]
async fn read_only_snapshot_matches_what_the_server_says() {
    let (engine, handle) = connect(live_config()).await;
    handle.watch_symbols(
        ["EURUSD", "USDJPY", "XAUUSD", "BTCUSD"]
            .into_iter()
            .map(str::to_owned),
    );
    eventually(&handle, Duration::from_secs(20), "four quotes", |s| {
        s.quotes.len() == 4
    })
    .await;

    let state = handle.state();
    let account = state.account.as_ref().expect("account");
    assert_eq!(account.kind, AccountKind::Demo, "from the token's claim");
    assert!(account.currency.is_some(), "the account currency is known");
    assert!(account.balance.is_some_and(|b| b > 0.0));
    assert!(account.server_version.is_some());

    for (name, quote) in &state.quotes {
        assert!(
            quote.bid > 0.0 && quote.ask >= quote.bid,
            "{name}: {quote:?}"
        );
        // Timestamps are milliseconds since the epoch: 2020 to 2100.
        let ts = quote.timestamp.expect("a quote timestamp");
        assert!(
            (1_577_836_800_000..4_102_444_800_000).contains(&ts),
            "{name}: timestamp {ts} is not in milliseconds"
        );
    }

    // Precision is inferred from quotes: check the symbol classes against known digits.
    use wyck_engine::broker::{Broker, RemoteBroker};
    let request = ConnectRequest::new(
        ServiceKind::CtraderRemote,
        env("WYCK_LIVE_REMOTE_ENDPOINT").unwrap_or_else(|| DEFAULT_ENDPOINT.to_owned()),
        Some(token()),
    );
    let broker = RemoteBroker::connect(&request, &live_config().assumed_specs)
        .await
        .expect("connect the adapter directly");
    for (name, digits, pip) in [
        ("EURUSD", 5, 0.0001),
        ("USDJPY", 3, 0.01),
        ("XAUUSD", 2, 0.01),
    ] {
        let instrument = broker.instrument(name).await.expect(name);
        assert_eq!(instrument.price_digits, digits, "{name}");
        assert!((instrument.pip_size - pip).abs() < 1e-12, "{name}");
    }
    // BTCUSD quotes 3 decimals but the samples only show 2: coarser, which is the safe side.
    let btc = broker.instrument("BTCUSD").await.expect("BTCUSD");
    assert!(btc.price_digits <= 3);
    assert_eq!(
        btc.specs_source,
        wyck_engine::domain::SpecsSource::Configured
    );
    engine.shutdown().await;
}

#[tokio::test]
#[ignore = "places real orders on a demo account"]
async fn market_order_lifecycle_with_protection_partial_close_and_flatten() {
    require_demo_confirmation();
    let (engine, handle) = connect(live_config()).await;
    assert_account_is_empty(&handle);
    let symbol = symbol();
    let sz = sizing(&symbol);
    handle.watch_symbols([symbol.clone()]);
    arm_demo(&handle).await;

    let h = handle.clone();
    with_cleanup(&handle, async move {
        // Market order with SL and TP. Two hundredths so a partial close is possible.
        let position = open(&h, buy(&symbol, sz.min_units * 2.0, sz.stop)).await;
        assert_eq!(position.symbol, symbol);
        assert_eq!(
            position.volume,
            volume(sz.min_units * 2.0),
            "the volume round-trips"
        );
        let entry = position.entry_price.expect("entry price");
        let stop = position.stop_loss.expect("a stop loss is set");
        let target = position.take_profit.expect("a take profit is set");
        assert!(
            ((entry - stop) - sz.stop).abs() < sz.stop * 0.01,
            "stop {stop} should be about 500 below the entry {entry}"
        );
        assert!(
            ((target - entry) - sz.stop * 2.0).abs() < sz.stop * 0.02,
            "target {target} should be about 1000 above the entry {entry}"
        );
        eprintln!("label echoed by the server: {:?}", position.label);

        // Changing one leg keeps the other (Q-R10).
        let new_stop = round_to(entry - sz.stop * 0.8, 5);
        h.set_protection(position.id, Some(new_stop), None)
            .await
            .expect("move the stop");
        eventually(&h, Duration::from_secs(15), "the new stop to show", |s| {
            s.positions.iter().any(|p| {
                p.stop_loss
                    .is_some_and(|v| (v - new_stop).abs() < sz.stop * 0.001)
            })
        })
        .await;
        let after = h.state().positions[0].clone();
        assert!(
            after
                .take_profit
                .is_some_and(|v| (v - target).abs() < sz.stop * 0.001),
            "the take profit must survive a stop-only amend: {after:?}"
        );

        // Partial close, then the rest.
        h.close_position(position.id, CloseSize::Volume(volume(sz.min_units)))
            .await
            .expect("partial close");
        eventually(&h, Duration::from_secs(15), "half of the position", |s| {
            s.positions.len() == 1 && s.positions[0].volume == volume(sz.min_units)
        })
        .await;
        h.close_position(position.id, CloseSize::Full)
            .await
            .expect("full close");
        eventually(&h, Duration::from_secs(15), "no position", |s| {
            s.positions.is_empty()
        })
        .await;

        // Flatten with two open positions: preview, token, execute.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let _ = open(&h, buy(&symbol, sz.min_units, sz.stop)).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let _ = open(&h, buy(&symbol, sz.min_units, sz.stop)).await;
        eventually(&h, Duration::from_secs(15), "two positions", |s| {
            s.positions.len() == 2
        })
        .await;
        let preview = h.preview_flatten(FlattenScope::All).await.expect("preview");
        assert_eq!(preview.positions.len(), 2);
        let report = h.flatten(preview.token).await.expect("flatten");
        assert_eq!(report.closed.len(), 2, "{report:?}");
        assert!(report.fully_flattened(), "{report:?}");
    })
    .await;
    engine.shutdown().await;
}

#[tokio::test]
#[ignore = "places a real order on a demo account"]
async fn a_rejected_order_reports_a_reason_and_leaves_no_position() {
    require_demo_confirmation();
    use wyck_engine::broker::{Broker, MarketOrder, RemoteBroker};
    let token = token();
    assert_eq!(AccountKind::from_token(&token), AccountKind::Demo);
    let request = ConnectRequest::new(
        ServiceKind::CtraderRemote,
        env("WYCK_LIVE_REMOTE_ENDPOINT").unwrap_or_else(|| DEFAULT_ENDPOINT.to_owned()),
        Some(token),
    );
    let broker = RemoteBroker::connect(&request, &live_config().assumed_specs)
        .await
        .expect("connect");
    assert!(broker.positions().await.expect("positions").is_empty());

    // Straight to the broker, past the planner: a zero volume can never be valid.
    let error = broker
        .place_market(&MarketOrder {
            symbol: symbol(),
            side: Side::Buy,
            volume: wyck_engine::domain::Volume::ZERO,
            stop_loss_distance: None,
            take_profit_distance: None,
            label: "wyck-live-reject".to_owned(),
            slippage_points: None,
        })
        .await
        .expect_err("the server must refuse a zero volume");
    eprintln!("rejection: {error:?}");
    assert!(
        !error.to_string().trim().is_empty(),
        "the reason must be readable"
    );
    assert!(broker.positions().await.expect("positions").is_empty());
}

#[tokio::test]
#[ignore = "places real orders on a demo account"]
async fn a_lost_reply_never_produces_a_second_order_and_is_reconciled() {
    require_demo_confirmation();
    let mut config = live_config();
    // Around the server's latency: the client gives up on the reply, sometimes before the
    // request left (no order at all), sometimes after (an order nobody heard back about).
    config.trading.order_timeout = Duration::from_millis(
        env("WYCK_LIVE_ORDER_TIMEOUT_MS")
            .and_then(|v| v.parse().ok())
            .unwrap_or(400),
    );
    let (engine, handle) = connect(config).await;
    assert_account_is_empty(&handle);
    let symbol = symbol();
    let sz = sizing(&symbol);
    handle.watch_symbols([symbol.clone()]);
    arm_demo(&handle).await;

    let h = handle.clone();
    let body = tokio::spawn(async move {
        let plan = h
            .plan_entry(buy(&symbol, sz.min_units, sz.stop))
            .await
            .expect("plan");
        let outcome = h.submit(plan.id).await.expect("submit");
        eprintln!("outcome under a short timeout: {outcome:#?}");
        assert!(
            matches!(
                outcome,
                OrderOutcome::Unknown { .. } | OrderOutcome::Filled { .. }
            ),
            "an ambiguous send is Unknown or confirmed Filled, never a guess: {outcome:#?}"
        );
        // Let a late order land and the engine reconcile.
        tokio::time::sleep(Duration::from_secs(8)).await;
        let state = h.state();
        assert!(
            state.positions.len() <= 1,
            "at most one order was sent, whatever the outcome said: {:?}",
            state.positions
        );
        if let Some(position) = state.positions.first() {
            eprintln!("the order did land: {position:?}");
            // The server does not echo the label, so this goes through the fallback match.
            assert!(
                state
                    .warnings
                    .iter()
                    .all(|w| !w.id.starts_with("unknown-order")),
                "a position that matches the uncertain order must clear its warning: {:?}",
                state.warnings
            );
        } else {
            eprintln!("the order never left: nothing to reconcile");
        }
    });
    let result = body.await;
    engine.shutdown().await;
    // The short timeout would also cut the cleanup short, so a fresh engine does it.
    let (engine, cleaner) = connect(live_config()).await;
    arm_demo(&cleaner).await;
    flatten_everything(&cleaner).await;
    engine.shutdown().await;
    if let Err(error) = result {
        std::panic::resume_unwind(error.into_panic());
    }
}

#[tokio::test]
#[ignore = "places an order while the forex market is closed (weekend), or a normal one otherwise"]
async fn an_order_while_the_market_is_closed_is_rejected_readably() {
    require_demo_confirmation();
    let (engine, handle) = connect(live_config()).await;
    assert_account_is_empty(&handle);
    handle.watch_symbols(["EURUSD".to_owned()]);
    arm_demo(&handle).await;

    let h = handle.clone();
    with_cleanup(&handle, async move {
        let plan = h
            .plan_entry(buy("EURUSD", 1_000.0, 0.0050))
            .await
            .expect("plan");
        let outcome = h.submit(plan.id).await.expect("submit");
        eprintln!("EURUSD outcome: {outcome:#?}");
        match outcome {
            OrderOutcome::Rejected { reason, .. } => {
                assert!(!reason.trim().is_empty(), "a readable reason");
                assert!(h.state().positions.is_empty());
            }
            OrderOutcome::Filled { .. } => eprintln!("the market was open: filled, closing"),
            other => panic!("unexpected outcome: {other:#?}"),
        }
    })
    .await;
    engine.shutdown().await;
}
