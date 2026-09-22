//! Scenario tests for the whole engine, driven through its public API against a scriptable
//! mock broker, with Tokio time paused so timeouts, backoff and refresh intervals are
//! deterministic and instant.

mod support;

use std::sync::Arc;
use std::time::Duration;

use support::{MockConnector, request};
use tokio::runtime::Handle;
use wyck_engine::broker::{BrokerCall, MockBroker};
use wyck_engine::domain::{AccountKind, Side, Volume};
use wyck_engine::{
    ArmRequest, CloseSize, Engine, EngineConfig, EngineError, EngineHandle, EngineOptions,
    EntryIntent, ErrorKind, EventKind, FlattenScope, OrderOutcome, RiskSpec, SessionState,
    SizeSpec, StopSpec, TakeProfitSpec, TradingMode,
};

struct Fixture {
    engine: Engine,
    handle: EngineHandle,
    broker: Arc<MockBroker>,
    connector: Arc<MockConnector>,
}

fn intent(pct: f64) -> EntryIntent {
    EntryIntent {
        symbol: "EURUSD".into(),
        side: Side::Buy,
        size: SizeSpec::Risk(RiskSpec::PercentOfBalance(pct)),
        stop_loss: Some(StopSpec::Pips(30.0)),
        take_profit: Some(TakeProfitSpec::RiskReward(2.0)),
    }
}

async fn fixture_with(broker: MockBroker, config: EngineConfig) -> Fixture {
    let broker = Arc::new(broker);
    let connector = MockConnector::new(Arc::clone(&broker));
    let engine = Engine::start_on(
        Handle::current(),
        config,
        EngineOptions {
            connector: Some(connector.clone()),
        },
    )
    .unwrap();
    let handle = engine.handle();
    Fixture {
        engine,
        handle,
        broker,
        connector,
    }
}

async fn fixture() -> Fixture {
    fixture_with(MockBroker::new(), EngineConfig::default()).await
}

async fn connected() -> Fixture {
    let f = fixture().await;
    f.handle.connect(request()).await.unwrap();
    f
}

async fn armed() -> Fixture {
    let f = connected().await;
    arm(&f.handle).await;
    f
}

async fn arm(handle: &EngineHandle) {
    let state = handle.state();
    handle
        .arm(ArmRequest {
            account: state.account_id.clone().unwrap(),
            acknowledged_kind: AccountKind::Demo,
        })
        .await
        .unwrap();
}

async fn sleep(secs: u64) {
    tokio::time::sleep(Duration::from_secs(secs)).await;
}

fn count(events: &[wyck_engine::Event], f: impl Fn(&EventKind) -> bool) -> usize {
    events.iter().filter(|e| f(&e.kind)).count()
}

// ---- session and state ----

#[tokio::test(start_paused = true)]
async fn connecting_publishes_a_ready_session_with_account_data() {
    let f = fixture().await;
    assert_eq!(f.handle.state().session, SessionState::Disconnected);
    let before = f.handle.state().revision;

    f.handle.connect(request()).await.unwrap();

    let s = f.handle.state();
    assert_eq!(s.session, SessionState::Ready);
    assert_eq!(s.account.as_ref().unwrap().balance, Some(10_000.0));
    assert_eq!(s.mode, TradingMode::DryRun, "the engine starts disarmed");
    assert!(s.revision > before);
    let events = f.handle.recent_events();
    assert!(
        count(&events, |k| matches!(
            k,
            EventKind::SessionChanged(SessionState::Ready)
        )) == 1
    );
}

#[tokio::test(start_paused = true)]
async fn a_failed_connection_is_reported_and_leaves_a_failed_state() {
    let f = fixture().await;
    f.connector.then(Err(EngineError::Broker {
        kind: wyck_engine::BrokerErrorKind::Connection,
        retryable: true,
        message: "refused".into(),
    }));
    f.handle.connect(request()).await.unwrap(); // the first queued entry succeeds
    let err = f.handle.connect(request()).await.unwrap_err(); // the next one fails
    assert_eq!(err.kind(), ErrorKind::Broker);
    assert!(matches!(
        f.handle.state().session,
        SessionState::Failed { .. }
    ));
    assert_eq!(f.handle.state().mode, TradingMode::DryRun);
}

#[tokio::test(start_paused = true)]
async fn requests_that_need_a_session_fail_cleanly_when_there_is_none() {
    let f = fixture().await;
    let err = f.handle.plan_entry(intent(1.0)).await.unwrap_err();
    assert!(matches!(err, EngineError::NotConnected));
    assert!(matches!(
        f.handle.close_position(1.into(), CloseSize::Full).await,
        Err(EngineError::NotConnected)
    ));
}

#[tokio::test(start_paused = true)]
async fn refreshes_keep_state_current_and_a_single_failure_keeps_the_old_data() {
    let f = connected().await;
    f.broker
        .fail_next(BrokerCall::Account, EngineError::Timeout { operation: "x" });
    let mut events = f.handle.subscribe();
    let revision = f.handle.state().revision;

    sleep(6).await; // one refresh tick: fails
    assert_eq!(
        f.handle.state().session,
        SessionState::Ready,
        "one failure is not a disconnect"
    );
    assert!(f.handle.state().account.is_some(), "stale data is kept");
    assert!(f.handle.state().last_error.is_some());
    let mut saw_failure = false;
    while let Ok(e) = events.try_recv() {
        saw_failure |= matches!(e.kind, EventKind::RefreshFailed { .. });
    }
    assert!(saw_failure);

    sleep(6).await; // the next tick succeeds
    assert!(f.handle.state().last_error.is_none());
    assert!(f.handle.state().revision > revision);
}

#[tokio::test(start_paused = true)]
async fn repeated_failures_trigger_a_reconnect_that_returns_to_dry_run() {
    let f = armed().await;
    assert_eq!(f.handle.state().mode, TradingMode::Armed);
    // A healthy replacement broker for the reconnect.
    f.connector
        .then(Ok(Arc::new(MockBroker::new().with_balance(12_345.0))));
    for _ in 0..3 {
        f.broker
            .fail_next(BrokerCall::Account, EngineError::Timeout { operation: "x" });
    }

    sleep(30).await;

    let s = f.handle.state();
    assert_eq!(s.session, SessionState::Ready);
    assert_eq!(
        s.account.as_ref().unwrap().balance,
        Some(12_345.0),
        "now on the new session"
    );
    assert_eq!(
        s.mode,
        TradingMode::DryRun,
        "losing the session disarms trading"
    );
    assert!(*f.connector.connects.lock().unwrap() >= 2);
    assert!(f.broker.is_closed(), "the broken session was closed");
    let events = f.handle.recent_events();
    assert!(
        count(&events, |k| matches!(
            k,
            EventKind::SessionChanged(SessionState::Reconnecting { .. })
        )) >= 1
    );
}

#[tokio::test(start_paused = true)]
async fn when_every_reconnect_fails_the_session_ends_failed() {
    let mut config = EngineConfig::default();
    config.session.reconnect_attempts = 2;
    let f = fixture_with(MockBroker::new(), config).await;
    f.handle.connect(request()).await.unwrap();
    f.connector.then(Err(EngineError::Internal("down".into())));
    for _ in 0..3 {
        f.broker
            .fail_next(BrokerCall::Account, EngineError::Timeout { operation: "x" });
    }
    sleep(60).await;
    assert!(
        matches!(f.handle.state().session, SessionState::Failed { .. }),
        "{:?}",
        f.handle.state().session
    );
}

#[tokio::test(start_paused = true)]
async fn disconnecting_clears_the_session_and_disarms() {
    let f = armed().await;
    f.handle.disconnect().await.unwrap();
    let s = f.handle.state();
    assert_eq!(s.session, SessionState::Disconnected);
    assert_eq!(s.mode, TradingMode::DryRun);
    assert!(s.account.is_none() && s.positions.is_empty());
    assert!(f.broker.is_closed());
}

// ---- planning and dry-run ----

#[tokio::test(start_paused = true)]
async fn planning_sizes_the_trade_and_never_touches_the_broker_mutably() {
    let f = connected().await;
    let plan = f.handle.plan_entry(intent(1.0)).await.unwrap();
    assert_eq!(plan.volume, Volume::from_units(33_000));
    assert!(plan.risk_amount.unwrap() <= 100.0);
    assert_eq!(f.broker.call_count(BrokerCall::PlaceMarket), 0);
    assert!(
        f.handle
            .recent_events()
            .iter()
            .any(|e| matches!(e.kind, EventKind::OrderPlanned(_)))
    );
}

#[tokio::test(start_paused = true)]
async fn while_disarmed_submit_is_a_dry_run_that_sends_nothing() {
    let f = connected().await;
    let plan = f.handle.plan_entry(intent(1.0)).await.unwrap();
    let outcome = f.handle.submit(plan.id).await.unwrap();
    assert!(
        matches!(outcome, OrderOutcome::DryRun { ref would_send } if would_send.volume == plan.volume),
        "{outcome:?}"
    );
    assert_eq!(f.broker.call_count(BrokerCall::PlaceMarket), 0);
    assert!(f.broker.peek_positions().is_empty());
}

#[tokio::test(start_paused = true)]
async fn plans_are_single_use_and_expire() {
    let f = connected().await;
    let plan = f.handle.plan_entry(intent(1.0)).await.unwrap();
    f.handle.submit(plan.id).await.unwrap();
    let again = f.handle.submit(plan.id).await.unwrap_err();
    assert!(
        matches!(again, EngineError::ConfirmationRejected(_)),
        "{again:?}"
    );

    let stale = f.handle.plan_entry(intent(1.0)).await.unwrap();
    sleep(20).await; // plan_ttl is 15 s
    let expired = f.handle.submit(stale.id).await.unwrap_err();
    assert!(matches!(expired, EngineError::ConfirmationRejected(m) if m.contains("expired")));
}

#[tokio::test(start_paused = true)]
async fn an_unknown_plan_id_is_refused() {
    let f = connected().await;
    let plan = f.handle.plan_entry(intent(1.0)).await.unwrap();
    // A plan id the engine never issued cannot be submitted.
    let forged: wyck_engine::PlanId =
        serde_json::from_value(serde_json::json!(plan.id.get() + 1_000_000)).unwrap();
    assert!(matches!(
        f.handle.submit(forged).await,
        Err(EngineError::ConfirmationRejected(_))
    ));
}

// ---- arming ----

#[tokio::test(start_paused = true)]
async fn arming_requires_the_right_account_and_the_acknowledged_kind() {
    let f = connected().await;
    let account = f.handle.state().account_id.clone().unwrap();

    let wrong_kind = f
        .handle
        .arm(ArmRequest {
            account: account.clone(),
            acknowledged_kind: AccountKind::Live,
        })
        .await
        .unwrap_err();
    assert!(matches!(wrong_kind, EngineError::ArmRefused(m) if m.contains("acknowledged")));

    let wrong_account = f
        .handle
        .arm(ArmRequest {
            account: wyck_engine::AccountId::new("someone-else"),
            acknowledged_kind: AccountKind::Demo,
        })
        .await
        .unwrap_err();
    assert!(matches!(wrong_account, EngineError::ArmRefused(_)));
    assert_eq!(f.handle.state().mode, TradingMode::DryRun);

    arm(&f.handle).await;
    assert_eq!(f.handle.state().mode, TradingMode::Armed);
    f.handle.disarm().await.unwrap();
    assert_eq!(f.handle.state().mode, TradingMode::DryRun);
}

#[tokio::test(start_paused = true)]
async fn a_read_only_connection_cannot_be_armed() {
    let f = fixture_with(MockBroker::new().read_only(), EngineConfig::default()).await;
    f.handle.connect(request()).await.unwrap();
    let state = f.handle.state();
    let err = f
        .handle
        .arm(ArmRequest {
            account: state.account_id.clone().unwrap(),
            acknowledged_kind: AccountKind::Demo,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, EngineError::ArmRefused(m) if m.contains("read-only")));
}

#[tokio::test(start_paused = true)]
async fn mutations_other_than_submit_need_the_engine_to_be_armed() {
    let f = connected().await;
    assert!(matches!(
        f.handle.set_protection(1.into(), Some(1.0), None).await,
        Err(EngineError::NotArmed)
    ));
    assert!(matches!(
        f.handle.close_position(1.into(), CloseSize::Full).await,
        Err(EngineError::NotArmed)
    ));
    assert!(matches!(
        f.handle.cancel_order(1.into()).await,
        Err(EngineError::NotArmed)
    ));
    assert!(matches!(
        f.handle.flatten("x".into()).await,
        Err(EngineError::NotArmed)
    ));
}

// ---- real orders ----

#[tokio::test(start_paused = true)]
async fn an_armed_order_fills_and_is_confirmed_by_reading_positions_back() {
    let f = armed().await;
    let plan = f.handle.plan_entry(intent(1.0)).await.unwrap();

    let outcome = f.handle.submit(plan.id).await.unwrap();

    let OrderOutcome::Filled { position, .. } = outcome else {
        panic!("expected a fill, got {outcome:?}");
    };
    assert_eq!(position.volume, plan.volume);
    assert!(position.label.as_deref().unwrap().contains("mock-plan-"));
    assert_eq!(f.broker.call_count(BrokerCall::PlaceMarket), 1);
    let s = f.handle.state();
    assert_eq!(s.positions.len(), 1, "state already shows the new position");
    assert!(
        s.orders_in_flight.is_empty(),
        "the in-flight marker is released"
    );
    let events = f.handle.recent_events();
    assert_eq!(
        count(&events, |k| matches!(k, EventKind::OrderSubmitted { .. })),
        1
    );
    assert_eq!(
        count(&events, |k| matches!(
            k,
            EventKind::OrderResult(OrderOutcome::Filled { .. })
        )),
        1
    );
    assert_eq!(
        count(&events, |k| matches!(k, EventKind::PositionsChanged { .. })),
        1
    );
}

#[tokio::test(start_paused = true)]
async fn a_double_tap_cannot_double_the_position() {
    let f = armed().await;
    let first = f.handle.plan_entry(intent(1.0)).await.unwrap();
    let second = f.handle.plan_entry(intent(1.0)).await.unwrap();
    f.handle.submit(first.id).await.unwrap();

    let err = f.handle.submit(second.id).await.unwrap_err();
    assert!(matches!(err, EngineError::Busy(_)), "{err:?}");
    assert_eq!(f.broker.call_count(BrokerCall::PlaceMarket), 1);

    // After the minimum interval a fresh plan goes through.
    sleep(2).await;
    let third = f.handle.plan_entry(intent(1.0)).await.unwrap();
    f.handle.submit(third.id).await.unwrap();
    assert_eq!(f.broker.call_count(BrokerCall::PlaceMarket), 2);
}

#[tokio::test(start_paused = true)]
async fn a_broker_refusal_is_a_rejected_outcome_after_exactly_one_attempt() {
    let f = armed().await;
    f.broker.fail_next(
        BrokerCall::PlaceMarket,
        EngineError::Broker {
            kind: wyck_engine::BrokerErrorKind::Rejected,
            retryable: false,
            message: "not enough margin".into(),
        },
    );
    let plan = f.handle.plan_entry(intent(1.0)).await.unwrap();
    let outcome = f.handle.submit(plan.id).await.unwrap();
    assert!(
        matches!(outcome, OrderOutcome::Rejected { ref reason, .. } if reason.contains("margin")),
        "{outcome:?}"
    );
    assert_eq!(f.broker.call_count(BrokerCall::PlaceMarket), 1);
    assert!(f.broker.peek_positions().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_lost_reply_is_resolved_by_reading_positions_never_by_resending() {
    let f = armed().await;
    // The order reaches the broker and fills, but the answer never arrives.
    f.broker.drop_reply_next(BrokerCall::PlaceMarket, 1);
    let plan = f.handle.plan_entry(intent(1.0)).await.unwrap();

    let outcome = f.handle.submit(plan.id).await.unwrap();

    assert!(
        matches!(outcome, OrderOutcome::Filled { .. }),
        "{outcome:?}"
    );
    assert_eq!(
        f.broker.call_count(BrokerCall::PlaceMarket),
        1,
        "an order is never replayed"
    );
    assert_eq!(f.broker.peek_positions().len(), 1, "and never doubled");
}

#[tokio::test(start_paused = true)]
async fn an_order_whose_fate_cannot_be_determined_is_tracked_then_reconciled() {
    let f = armed().await;
    // The connection dies: the order did NOT execute, but the engine cannot know that.
    f.broker.fail_next(
        BrokerCall::PlaceMarket,
        EngineError::Broker {
            kind: wyck_engine::BrokerErrorKind::Connection,
            retryable: true,
            message: "connection reset".into(),
        },
    );
    let plan = f.handle.plan_entry(intent(1.0)).await.unwrap();

    let outcome = f.handle.submit(plan.id).await.unwrap();

    let OrderOutcome::Unknown { label, .. } = outcome else {
        panic!("expected an unknown outcome, got {outcome:?}");
    };
    assert!(
        f.handle
            .state()
            .warnings
            .iter()
            .any(|w| w.id == format!("unknown-order:{label}"))
    );
    assert_eq!(f.broker.call_count(BrokerCall::PlaceMarket), 1);

    // Later it turns out the order did reach the broker after all.
    f.broker.push_position(wyck_engine::domain::Position {
        id: 500.into(),
        symbol: "EURUSD".into(),
        side: Side::Buy,
        volume: plan.volume,
        entry_price: Some(1.08501),
        stop_loss: None,
        take_profit: None,
        swap: None,
        commission: None,
        unrealized_pnl: None,
        label: Some(label.clone()),
    });
    sleep(6).await;

    assert!(
        f.handle
            .state()
            .warnings
            .iter()
            .all(|w| w.kind != wyck_engine::state::WarningKind::UnknownOrder)
    );
    assert!(
        f.handle
            .recent_events()
            .iter()
            .any(|e| matches!(&e.kind, EventKind::Reconciled { label: l, .. } if *l == label))
    );
}

fn lookalike(id: i64, volume: wyck_engine::domain::Volume) -> wyck_engine::domain::Position {
    wyck_engine::domain::Position {
        id: id.into(),
        symbol: "EURUSD".into(),
        side: Side::Buy,
        volume,
        entry_price: Some(1.08501),
        stop_loss: None,
        take_profit: None,
        swap: None,
        commission: None,
        unrealized_pnl: None,
        label: None,
    }
}

fn connection_reset() -> EngineError {
    EngineError::Broker {
        kind: wyck_engine::BrokerErrorKind::Connection,
        retryable: true,
        message: "connection reset".into(),
    }
}

#[tokio::test(start_paused = true)]
async fn a_late_order_is_reconciled_by_shape_when_the_server_never_echoes_the_label() {
    let f = armed().await;
    f.broker
        .fail_next(BrokerCall::PlaceMarket, connection_reset());
    let plan = f.handle.plan_entry(intent(1.0)).await.unwrap();
    let OrderOutcome::Unknown { label, .. } = f.handle.submit(plan.id).await.unwrap() else {
        panic!("expected an unknown outcome");
    };

    // Remote's positions carry no label: the order is recognized by symbol, side, volume.
    f.broker.push_position(lookalike(500, plan.volume));
    sleep(6).await;

    assert!(
        f.handle
            .state()
            .warnings
            .iter()
            .all(|w| w.kind != wyck_engine::state::WarningKind::UnknownOrder)
    );
    assert!(
        f.handle
            .recent_events()
            .iter()
            .any(|e| matches!(&e.kind, EventKind::Reconciled { label: l, .. } if *l == label))
    );
}

#[tokio::test(start_paused = true)]
async fn a_position_that_predates_the_order_is_not_mistaken_for_it() {
    let f = armed().await;
    let plan = f.handle.plan_entry(intent(1.0)).await.unwrap();
    // Same symbol, side and volume, but already open when the order goes out.
    f.broker.push_position(lookalike(400, plan.volume));
    sleep(6).await;
    f.broker
        .fail_next(BrokerCall::PlaceMarket, connection_reset());
    let OrderOutcome::Unknown { .. } = f.handle.submit(plan.id).await.unwrap() else {
        panic!("expected an unknown outcome");
    };
    sleep(6).await;

    assert!(
        f.handle
            .state()
            .warnings
            .iter()
            .any(|w| w.kind == wyck_engine::state::WarningKind::UnknownOrder),
        "nothing new appeared, so the order is still unaccounted for"
    );
}

#[tokio::test(start_paused = true)]
async fn an_unknown_order_warning_can_be_dismissed_by_the_user() {
    let f = armed().await;
    f.broker.fail_next(
        BrokerCall::PlaceMarket,
        EngineError::Timeout { operation: "x" },
    );
    let plan = f.handle.plan_entry(intent(1.0)).await.unwrap();
    let OrderOutcome::Unknown { label, .. } = f.handle.submit(plan.id).await.unwrap() else {
        panic!("expected unknown");
    };
    assert!(f.handle.dismiss_warning(&format!("unknown-order:{label}")));
    assert!(f.handle.state().warnings.is_empty());
    assert!(!f.handle.dismiss_warning("unknown-order:nope"));
}

// ---- modifying and closing ----

async fn with_open_position() -> (Fixture, wyck_engine::PositionId) {
    let f = armed().await;
    let plan = f.handle.plan_entry(intent(1.0)).await.unwrap();
    let OrderOutcome::Filled { position, .. } = f.handle.submit(plan.id).await.unwrap() else {
        panic!("expected a fill");
    };
    (f, position.id)
}

#[tokio::test(start_paused = true)]
async fn protection_changes_close_and_partial_close_flow_through() {
    let (f, id) = with_open_position().await;
    let entry = f.handle.state().position(id).unwrap().entry_price.unwrap();

    f.handle
        .set_protection(id, Some(entry - 0.0020), None)
        .await
        .unwrap();
    let p = f.handle.state().position(id).unwrap().clone();
    assert!((p.stop_loss.unwrap() - (entry - 0.0020)).abs() < 1e-9);
    assert!(p.take_profit.is_some(), "the untouched leg is kept");

    let bad = f
        .handle
        .set_protection(id, Some(entry + 0.5), Some(entry - 0.5))
        .await
        .unwrap_err();
    assert!(
        matches!(bad, EngineError::Invalid(_)),
        "a buy's stop above its target is refused"
    );

    f.handle
        .close_position(id, CloseSize::Volume(Volume::from_units(3_000)))
        .await
        .unwrap();
    assert_eq!(
        f.handle.state().position(id).unwrap().volume,
        Volume::from_units(30_000)
    );
    let too_big = f
        .handle
        .close_position(id, CloseSize::Volume(Volume::from_units(30_000)))
        .await
        .unwrap_err();
    assert!(matches!(too_big, EngineError::Invalid(m) if m.contains("full close")));

    f.handle.close_position(id, CloseSize::Full).await.unwrap();
    assert!(f.handle.state().positions.is_empty());
}

#[tokio::test(start_paused = true)]
async fn flatten_is_two_steps_with_a_single_use_token() {
    let (f, _) = with_open_position().await;
    sleep(2).await;
    let plan = f.handle.plan_entry(intent(0.5)).await.unwrap();
    f.handle.submit(plan.id).await.unwrap();
    assert_eq!(f.handle.state().positions.len(), 2);

    let preview = f.handle.preview_flatten(FlattenScope::All).await.unwrap();
    assert_eq!(preview.positions.len(), 2);

    let wrong = f.handle.flatten("not-the-token".into()).await.unwrap_err();
    assert!(matches!(wrong, EngineError::ConfirmationRejected(_)));

    let report = f.handle.flatten(preview.token.clone()).await.unwrap();
    assert_eq!(report.closed.len(), 2);
    assert!(report.fully_flattened());
    assert!(f.handle.state().positions.is_empty());

    let reuse = f.handle.flatten(preview.token).await.unwrap_err();
    assert!(
        matches!(reuse, EngineError::ConfirmationRejected(_)),
        "the token is single use"
    );
}

#[tokio::test(start_paused = true)]
async fn a_flatten_preview_expires() {
    let (f, _) = with_open_position().await;
    let preview = f
        .handle
        .preview_flatten(FlattenScope::Symbol("EURUSD".into()))
        .await
        .unwrap();
    sleep(31).await;
    let err = f.handle.flatten(preview.token).await.unwrap_err();
    assert!(matches!(err, EngineError::ConfirmationRejected(m) if m.contains("expired")));
    assert_eq!(f.handle.state().positions.len(), 1, "nothing was closed");
}

// ---- lifecycle ----

#[tokio::test(start_paused = true)]
async fn shutdown_is_clean_and_later_calls_report_it() {
    let f = armed().await;
    let handle = f.handle.clone();
    let broker = Arc::clone(&f.broker);
    f.engine.shutdown().await;
    assert!(broker.is_closed());
    assert!(matches!(
        handle.plan_entry(intent(1.0)).await,
        Err(EngineError::ShuttingDown)
    ));
    assert!(matches!(
        handle.connect(request()).await,
        Err(EngineError::ShuttingDown)
    ));
}

#[tokio::test(start_paused = true)]
async fn a_slow_event_subscriber_lags_and_can_resynchronize_from_state() {
    let config = EngineConfig {
        event_buffer: 4,
        ..EngineConfig::default()
    };
    let f = fixture_with(MockBroker::new(), config).await;
    let mut events = f.handle.subscribe();
    f.handle.connect(request()).await.unwrap();
    sleep(60).await; // many refresh events pile up unread

    match events.recv().await {
        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => assert!(n > 0),
        other => panic!("expected Lagged, got {other:?}"),
    }
    // State is still complete: nothing depended on having read every event.
    assert_eq!(f.handle.state().session, SessionState::Ready);
    assert!(f.handle.state().account.is_some());
}

#[test]
fn the_handle_works_from_an_executor_that_is_not_the_engines() {
    // The engine owns a runtime on its own threads. A caller with a completely separate
    // executor (a UI framework's, here another Tokio runtime) can await every method.
    let broker = Arc::new(MockBroker::new());
    let connector = MockConnector::new(Arc::clone(&broker));
    let engine = Engine::start_with(
        EngineConfig::default(),
        EngineOptions {
            connector: Some(connector),
        },
    )
    .unwrap();
    let handle = engine.handle();

    let other = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    other.block_on(async {
        handle.connect(request()).await.unwrap();
        assert_eq!(handle.state().session, SessionState::Ready);
        let plan = handle.plan_entry(intent(1.0)).await.unwrap();
        let outcome = handle.submit(plan.id).await.unwrap();
        assert!(matches!(outcome, OrderOutcome::DryRun { .. }));
        let mut watch = handle.watch_state();
        handle.disconnect().await.unwrap();
        watch.changed().await.unwrap();
        assert_eq!(handle.state().session, SessionState::Disconnected);
    });
    other.block_on(engine.shutdown());
}

#[tokio::test(start_paused = true)]
async fn a_dry_run_does_not_consume_the_interval_between_real_orders() {
    let f = connected().await;
    let dry = f.handle.plan_entry(intent(1.0)).await.unwrap();
    assert!(matches!(
        f.handle.submit(dry.id).await.unwrap(),
        OrderOutcome::DryRun { .. }
    ));

    arm(&f.handle).await;
    let real = f.handle.plan_entry(intent(1.0)).await.unwrap();
    assert!(matches!(
        f.handle.submit(real.id).await.unwrap(),
        OrderOutcome::Filled { .. }
    ));
}

// ---- the symbol catalog, details and one-off quotes ----

#[tokio::test(start_paused = true)]
async fn the_symbol_catalog_is_read_once_per_connection() {
    let f = connected().await;
    let first = f.handle.symbol_catalog().await.unwrap();
    assert!(!first.is_empty());
    assert!(first.iter().any(|s| s.symbol == "EURUSD"));
    let second = f.handle.symbol_catalog().await.unwrap();
    assert!(
        Arc::ptr_eq(&first, &second),
        "the second answer comes from memory"
    );
    assert_eq!(f.broker.call_count(BrokerCall::Symbols), 1);

    // A new connection may be another account: the list is read again.
    f.handle.connect(request()).await.unwrap();
    f.handle.symbol_catalog().await.unwrap();
    assert_eq!(f.broker.call_count(BrokerCall::Symbols), 2);
}

#[tokio::test(start_paused = true)]
async fn without_a_session_the_symbol_catalog_is_an_error_not_a_hang() {
    let f = fixture().await;
    assert!(f.handle.symbol_catalog().await.is_err());
    assert!(f.handle.instrument("EURUSD").await.is_err());
    assert!(f.handle.quote("EURUSD").await.is_err());
}

#[tokio::test(start_paused = true)]
async fn the_details_of_a_symbol_are_loaded_on_demand_and_cached() {
    let f = connected().await;
    let details = f.handle.instrument("EURUSD").await.unwrap();
    assert_eq!(details.symbol, "EURUSD");
    f.handle.instrument("eurusd").await.unwrap();
    assert!(
        f.broker.call_count(BrokerCall::Instrument) <= 1,
        "the second read is served from the cache"
    );
    let unknown = f.handle.instrument("NOPE").await.unwrap_err();
    assert!(matches!(unknown, EngineError::Invalid(_)), "{unknown:?}");
}

#[tokio::test(start_paused = true)]
async fn a_one_off_quote_does_not_change_the_watch_list() {
    let f = connected().await;
    let before = f.handle.state().watched.clone();
    let quote = f.handle.quote("EURUSD").await.unwrap();
    assert!(quote.is_some());
    assert_eq!(f.handle.state().watched, before);
}
