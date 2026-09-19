//! Shared helpers for the live tests (`live_remote.rs`, `live_local.rs`).
//!
//! The live tests talk to real cTrader servers, so they are all `#[ignore]`d and CI never
//! runs them. Read this before running one.
//!
//! - **They can place real orders.** Every test that mutates state refuses to run unless
//!   `WYCK_LIVE_CONFIRM_DEMO=1` is set, and the account must be a **demo** one: Remote checks
//!   the token's `environment` claim, Local needs the trader id spelled out in
//!   `WYCK_LIVE_LOCAL_TRADER_ID` (Local cannot tell demo from live by itself).
//! - **They refuse a busy account.** A mutating test panics if the account already has an
//!   open position or a working order, so it can never flatten someone's real trades.
//! - **They clean up.** Whatever happens, the test flattens the positions it opened.
//!
//! Variables: `WYCK_LIVE_REMOTE_TOKEN`, `WYCK_LIVE_REMOTE_ENDPOINT` (optional),
//! `WYCK_LIVE_LOCAL_ENDPOINT` (optional), `WYCK_LIVE_LOCAL_TRADER_ID`,
//! `WYCK_LIVE_CONFIRM_DEMO`, `WYCK_LIVE_SYMBOL` (default `BTCUSD`, the one symbol that
//! trades on a weekend).
#![allow(dead_code)]

use std::time::{Duration, Instant};

use wyck_engine::config::SymbolVolumeRules;
use wyck_engine::domain::Volume;
use wyck_engine::{
    Engine, EngineConfig, EngineHandle, EngineState, FlattenScope, OrderOutcome, SessionState,
};

pub fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

pub fn symbol() -> String {
    env("WYCK_LIVE_SYMBOL").unwrap_or_else(|| "BTCUSD".to_owned())
}

/// Panics unless the caller confirmed that mutating a demo account is fine.
pub fn require_demo_confirmation() {
    assert_eq!(
        env("WYCK_LIVE_CONFIRM_DEMO").as_deref(),
        Some("1"),
        "this test places real orders: set WYCK_LIVE_CONFIRM_DEMO=1, on a DEMO account only"
    );
}

/// The volume rules the Spotware demo servers publish through Local's
/// `get_symbol_details` (checked 2026-09-19), configured for Remote, which publishes none.
pub fn spotware_rules() -> Vec<(&'static str, SymbolVolumeRules)> {
    let rules = |lot_size, min_volume, volume_step, max_volume| SymbolVolumeRules {
        lot_size,
        min_volume,
        volume_step,
        max_volume: Some(max_volume),
    };
    vec![
        ("EURUSD", rules(100_000.0, 1_000.0, 1_000.0, 10_000_000.0)),
        ("USDJPY", rules(100_000.0, 1_000.0, 1_000.0, 10_000_000.0)),
        ("XAUUSD", rules(100.0, 1.0, 1.0, 10_000.0)),
        ("BTCUSD", rules(1.0, 0.01, 0.01, 50.0)),
    ]
}

pub fn live_config() -> EngineConfig {
    let mut config = EngineConfig::default();
    for (name, rules) in spotware_rules() {
        config.assumed_specs.symbols.insert(name.to_owned(), rules);
    }
    // No spending a calendar request per test run.
    config.calendar_enabled = false;
    config
}

pub fn start(config: EngineConfig) -> (Engine, EngineHandle) {
    let engine = Engine::start(config).expect("engine starts");
    let handle = engine.handle();
    (engine, handle)
}

/// Polls the state until `condition` holds, or panics after `timeout` with `what`.
pub async fn eventually(
    handle: &EngineHandle,
    timeout: Duration,
    what: &str,
    condition: impl Fn(&EngineState) -> bool,
) {
    let deadline = Instant::now() + timeout;
    loop {
        let state = handle.state();
        if condition(&state) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out after {timeout:?} waiting for: {what}\nlast state: {state:#?}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub async fn wait_ready(handle: &EngineHandle) {
    eventually(
        handle,
        Duration::from_secs(30),
        "the session to be Ready",
        |s| matches!(s.session, SessionState::Ready),
    )
    .await;
}

/// Panics when the account has anything open: the tests only ever run on an empty account.
pub fn assert_account_is_empty(handle: &EngineHandle) {
    let state = handle.state();
    assert!(
        state.positions.is_empty() && state.pending_orders.is_empty(),
        "refusing to run: the account already has {} position(s) and {} working order(s). \
         Close them yourself first, these tests flatten everything when they finish.",
        state.positions.len(),
        state.pending_orders.len()
    );
}

/// Flattens everything and waits until the account is empty again.
pub async fn flatten_everything(handle: &EngineHandle) {
    let preview = handle
        .preview_flatten(FlattenScope::All)
        .await
        .expect("flatten preview");
    if preview.positions.is_empty() && preview.orders.is_empty() {
        return;
    }
    let report = handle.flatten(preview.token).await.expect("flatten");
    assert!(report.fully_flattened(), "flatten left errors: {report:?}");
    eventually(
        handle,
        Duration::from_secs(20),
        "the account to be empty after a flatten",
        |s| s.positions.is_empty() && s.pending_orders.is_empty(),
    )
    .await;
}

/// The position a filled outcome carries, or a panic that says what came back instead.
pub fn filled(outcome: OrderOutcome) -> wyck_engine::domain::Position {
    match outcome {
        OrderOutcome::Filled { position, .. } => position,
        other => panic!("expected a filled order, got {other:#?}"),
    }
}

pub fn volume(units: f64) -> Volume {
    Volume::from_units_f64(units)
}

/// Runs `body`, then always flattens, then re-raises a failure of the body.
pub async fn with_cleanup<F>(handle: &EngineHandle, body: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let result = tokio::spawn(body).await;
    let _ = handle.disarm().await;
    // Trading has to be armed to close, so arm again just for the cleanup when needed.
    if let Some(account) = handle.state().account.as_ref() {
        let _ = handle
            .arm(wyck_engine::ArmRequest {
                account: account.account_id.clone(),
                acknowledged_kind: account.kind,
            })
            .await;
    }
    flatten_everything(handle).await;
    let _ = handle.disarm().await;
    if let Err(error) = result {
        std::panic::resume_unwind(error.into_panic());
    }
}

/// One order size and stop distance that suit a symbol: the smallest volume the servers
/// accept and a stop wide enough not to be hit while a test runs.
#[derive(Debug, Clone, Copy)]
pub struct Sizing {
    /// The minimum volume, in units. Tests trade one or two of these.
    pub min_units: f64,
    /// The stop distance, as a price distance.
    pub stop: f64,
}

pub fn sizing(symbol: &str) -> Sizing {
    match symbol.to_ascii_uppercase().as_str() {
        "EURUSD" => Sizing {
            min_units: 1_000.0,
            stop: 0.0050,
        },
        "USDJPY" => Sizing {
            min_units: 1_000.0,
            stop: 0.50,
        },
        "XAUUSD" => Sizing {
            min_units: 1.0,
            stop: 5.0,
        },
        "BTCUSD" => Sizing {
            min_units: 0.01,
            stop: 500.0,
        },
        other => panic!("no live-test sizing for {other}: add it to `sizing` in live_support"),
    }
}

/// Rounds `price` to `digits` decimals.
pub fn round_to(price: f64, digits: i32) -> f64 {
    let factor = 10f64.powi(digits);
    (price * factor).round() / factor
}
