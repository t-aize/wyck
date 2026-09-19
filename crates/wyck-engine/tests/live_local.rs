//! Live tests against the **Local** MCP server inside cTrader Desktop. All `#[ignore]`d.
//! See `live_support/mod.rs` for the safety rules and variables.
//!
//! Local cannot tell a demo account from a live one, so the tests that place orders need
//! `WYCK_LIVE_LOCAL_TRADER_ID` set to the `traderId` that `get_balance` reports for the
//! account currently active in cTrader Desktop, as a deliberate acknowledgement that it is a
//! demo account. Read the id with the read-only test, which prints it.
//!
//! ```sh
//! # Read only, safe:
//! cargo test -p wyck-engine --test live_local -- --ignored read_only --nocapture
//! # Places and closes real orders on the active account. DEMO ONLY:
//! WYCK_LIVE_CONFIRM_DEMO=1 WYCK_LIVE_LOCAL_TRADER_ID=<id> \
//!     cargo test -p wyck-engine --test live_local -- --ignored --test-threads=1 lifecycle
//! ```
//!
//! The traded symbol (`WYCK_LIVE_SYMBOL`, `BTCUSD` by default) must be in the platform's
//! Market Watch: Local has no quote for a symbol that is not.

mod live_support;

use std::time::Duration;

use live_support::{
    assert_account_is_empty, env, eventually, filled, live_config, require_demo_confirmation,
    round_to, sizing, start, symbol, volume, wait_ready, with_cleanup,
};
use wyck_engine::broker::{Broker, ConnectRequest, LocalBroker, ServiceKind};
use wyck_engine::domain::{AccountKind, Side, SpecsSource};
use wyck_engine::{
    ArmRequest, CloseSize, EngineHandle, EntryIntent, FlattenScope, SizeSpec, StopSpec,
    TakeProfitSpec,
};

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:9876/mcp/";

fn endpoint() -> String {
    env("WYCK_LIVE_LOCAL_ENDPOINT").unwrap_or_else(|| DEFAULT_ENDPOINT.to_owned())
}

async fn connect(config: wyck_engine::EngineConfig) -> (wyck_engine::Engine, EngineHandle) {
    let (engine, handle) = start(config);
    handle
        .connect(ConnectRequest::new(
            ServiceKind::CtraderLocal,
            endpoint(),
            None,
        ))
        .await
        .expect("connect to Local: is cTrader Desktop running with the MCP server enabled?");
    wait_ready(&handle).await;
    (engine, handle)
}

/// Refuses unless the active account is the one the caller named as a demo account.
fn require_named_demo_account(handle: &EngineHandle) {
    require_demo_confirmation();
    let named = env("WYCK_LIVE_LOCAL_TRADER_ID")
        .expect("set WYCK_LIVE_LOCAL_TRADER_ID to the demo account's traderId");
    let state = handle.state();
    let active = state
        .account_id
        .as_ref()
        .expect("an account id")
        .to_string();
    assert_eq!(
        active, named,
        "the account active in cTrader Desktop is {active}, not the demo account {named} you \
         named: switch account in the platform, or fix the variable"
    );
}

fn buy(symbol: &str, units: f64, stop_distance: f64) -> EntryIntent {
    EntryIntent {
        symbol: symbol.to_owned(),
        side: Side::Buy,
        size: SizeSpec::Fixed(volume(units)),
        stop_loss: Some(StopSpec::Distance(stop_distance)),
        take_profit: Some(TakeProfitSpec::RiskReward(2.0)),
    }
}

#[tokio::test]
#[ignore = "talks to cTrader Desktop on this machine"]
async fn read_only_snapshot_matches_what_the_server_says() {
    let (engine, handle) = connect(live_config()).await;
    let state = handle.state();
    let account = state.account.as_ref().expect("account");
    eprintln!(
        "active account {} kind {:?} currency {:?} balance {:?}",
        account.account_id, account.kind, account.currency, account.balance
    );
    assert!(
        account.currency.is_some(),
        "the currency comes from `depositAsset`"
    );
    assert!(account.balance.is_some());
    // Local does not say demo or live; it is only known when the account is in the list.
    assert!(matches!(
        account.kind,
        AccountKind::Demo | AccountKind::Live | AccountKind::Unknown
    ));

    // Instrument rules as `get_symbol_details` publishes them, in units (checked 2026-09-19).
    let request = ConnectRequest::new(ServiceKind::CtraderLocal, endpoint(), None);
    let broker = LocalBroker::connect(&request, &live_config().assumed_specs)
        .await
        .expect("connect the adapter directly");
    for (name, lot, min, step, digits, pip) in [
        ("EURUSD", 100_000.0, 1_000.0, 1_000.0, 5, 0.0001),
        ("USDJPY", 100_000.0, 1_000.0, 1_000.0, 3, 0.01),
        ("XAUUSD", 100.0, 1.0, 1.0, 2, 0.01),
        ("BTCUSD", 1.0, 0.01, 0.01, 3, 0.1),
    ] {
        let i = broker.instrument(name).await.expect(name);
        assert_eq!(i.specs_source, SpecsSource::Broker, "{name}");
        assert!((i.volume.lot_size - lot).abs() < 1e-9, "{name} lot size");
        assert!((i.volume.min.as_units() - min).abs() < 1e-9, "{name} min");
        assert!(
            (i.volume.step.as_units() - step).abs() < 1e-9,
            "{name} step"
        );
        assert_eq!(i.price_digits, digits, "{name}");
        assert!((i.pip_size - pip).abs() < 1e-12, "{name} pip size");
    }

    let time = broker
        .server_time()
        .await
        .expect("Local has a server clock");
    let skew = (time - wyck_engine::domain::now_millis()).abs();
    assert!(skew < 60_000, "the server clock is {skew} ms from this one");

    for position in broker.positions().await.expect("positions") {
        eprintln!("open position as decoded: {position:?}");
    }
    engine.shutdown().await;
}

#[tokio::test]
#[ignore = "places real orders on the ACTIVE cTrader Desktop account: demo only"]
async fn lifecycle_market_order_protection_partial_close_and_flatten() {
    let (engine, handle) = connect(live_config()).await;
    require_named_demo_account(&handle);
    assert_account_is_empty(&handle);
    let symbol = symbol();
    let sz = sizing(&symbol);
    handle.watch_symbols([symbol.clone()]);
    eventually(
        &handle,
        Duration::from_secs(20),
        "a quote (the symbol must be in the Market Watch)",
        |s| s.quotes.contains_key(&symbol),
    )
    .await;

    // Local's kind is `Unknown` unless the account is in its list: acknowledge exactly that.
    let account = handle.state().account.clone().expect("account");
    handle
        .arm(ArmRequest {
            account: account.account_id.clone(),
            acknowledged_kind: account.kind,
        })
        .await
        .expect("arm");

    let h = handle.clone();
    with_cleanup(&handle, async move {
        let plan = h
            .plan_entry(buy(&symbol, sz.min_units * 2.0, sz.stop))
            .await
            .expect("plan");
        eprintln!("plan: {plan:#?}");
        let position = filled(h.submit(plan.id).await.expect("submit"));
        eprintln!("position as decoded: {position:#?}");
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
            "stop {stop} vs entry {entry}"
        );
        assert!(
            ((target - entry) - sz.stop * 2.0).abs() < sz.stop * 0.02,
            "target {target} vs entry {entry}"
        );

        // A stop-only change keeps the target.
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
        assert!(
            h.state().positions[0]
                .take_profit
                .is_some_and(|v| (v - target).abs() < sz.stop * 0.001),
            "the take profit must survive"
        );

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

        tokio::time::sleep(Duration::from_secs(1)).await;
        for _ in 0..2 {
            let plan = h
                .plan_entry(buy(&symbol, sz.min_units, sz.stop))
                .await
                .expect("plan");
            filled(h.submit(plan.id).await.expect("submit"));
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let preview = h.preview_flatten(FlattenScope::All).await.expect("preview");
        assert_eq!(preview.positions.len(), 2);
        let report = h.flatten(preview.token).await.expect("flatten");
        assert!(report.fully_flattened(), "{report:?}");
        assert_eq!(report.closed.len(), 2);
    })
    .await;
    engine.shutdown().await;
}
