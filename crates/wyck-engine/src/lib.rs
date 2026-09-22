//! # wyck-engine
//!
//! The headless trading core of wyck: it owns the broker connection, keeps a live picture
//! of the account, turns "buy EURUSD, 30 pip stop, risk 1%" into an exact order, sends it
//! safely, and warns about trading risk. It contains **no user interface code**; a GUI, a terminal tool or a headless
//! service all drive the same [`EngineHandle`].
//!
//! ```text
//!   front end (GUI, CLI, service)
//!        |   async methods            ^  state snapshots, events
//!        v                            |
//!   +--------------------- Engine ---------------------+
//!   |  session   risk planning   order pipeline        |
//!   |  guardrails              state + events        |
//!   +--------+---------------------------------------+
//!            |
//!        Broker trait
//!     (Remote, Local, mock)
//! ```
//!
//! # Quick start
//!
//! ```
//! use std::sync::Arc;
//! use wyck_engine::broker::{ConnectRequest, MockBroker, ServiceKind};
//! use wyck_engine::domain::Side;
//! use wyck_engine::{
//!     Engine, EngineConfig, EngineOptions, EntryIntent, OrderOutcome, RiskSpec,
//!     SizeSpec, StopSpec,
//! };
//!
//! # struct One(Arc<MockBroker>);
//! # #[async_trait::async_trait]
//! # impl wyck_engine::broker::Connector for One {
//! #     async fn connect(&self, _: &ConnectRequest) -> wyck_engine::Result<Arc<dyn wyck_engine::broker::Broker>> {
//! #         Ok(self.0.clone())
//! #     }
//! # }
//! # let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
//! # rt.block_on(async {
//! // In real use, leave `connector` as `None` to talk to cTrader.
//! let engine = Engine::start_with(
//!     EngineConfig::default(),
//!     EngineOptions {
//!         connector: Some(Arc::new(One(Arc::new(MockBroker::new())))),
//!     },
//! )
//! .unwrap();
//! let handle = engine.handle();
//!
//! handle
//!     .connect(ConnectRequest::new(
//!         ServiceKind::CtraderRemote,
//!         "https://mcp.ctrader.com/trading/mcp",
//!         None,
//!     ))
//!     .await
//!     .unwrap();
//!
//! let plan = handle
//!     .plan_entry(EntryIntent {
//!         symbol: "EURUSD".into(),
//!         side: Side::Buy,
//!         size: SizeSpec::Risk(RiskSpec::PercentOfBalance(1.0)),
//!         stop_loss: Some(StopSpec::Pips(30.0)),
//!         take_profit: None,
//!     })
//!     .await
//!     .unwrap();
//! assert!(plan.risk_amount.unwrap() <= 100.0); // 1% of a 10,000 account
//!
//! // The engine starts disarmed, so this is a dry run: nothing is sent.
//! let outcome = handle.submit(plan.id).await.unwrap();
//! assert!(matches!(outcome, OrderOutcome::DryRun { .. }));
//! # engine.shutdown().await;
//! # });
//! ```
//!
//! # Reading state: snapshots and events
//!
//! [`EngineState`] is one immutable snapshot of everything a screen needs: session,
//! account, positions, quotes and warnings. Read it with [`EngineHandle::state`], or
//! await [`EngineHandle::watch_state`] to be woken on every change. [`Event`]s say *what
//! changed and why* (`OrderResult`, `PositionsChanged`, `Reconciled`) and arrive on a bounded
//! broadcast channel from [`EngineHandle::subscribe`].
//!
//! The two are deliberately redundant. An event subscriber that falls behind gets a `Lagged`
//! error; it recovers by reading the state, which is always complete. Nothing correct ever
//! depends on having seen every event.
//!
//! # Safety model
//!
//! The engine is built so that a bug in a front end, a dropped connection or a fat-fingered
//! hotkey costs as little as possible.
//!
//! | Rule | What it means |
//! |---|---|
//! | **Dry-run by default** | The engine starts disarmed. While disarmed, [`EngineHandle::submit`] prepares everything and returns [`OrderOutcome::DryRun`], sending nothing. |
//! | **Explicit arming** | [`EngineHandle::arm`] needs a ready session, a trading-capable connection, the right account, and an acknowledgement of the account kind (demo, live or unknown). Leaving `Ready` disarms. |
//! | **Plans are single use and expire** | Only a plan the engine created can be submitted, once, within `plan_ttl` (15 s). A plan is priced from a live quote; an old one must be re-planned. |
//! | **One order in flight per symbol** | Plus a minimum interval between orders, so a held or double-tapped key cannot double a position. |
//! | **Never replay a mutating call** | A lost reply is resolved by reading positions back and matching the order (by label where the server echoes it, else by symbol, side and volume), never by sending it again. |
//! | **Uncertainty is a first-class outcome** | If the fate of an order cannot be established, it is [`OrderOutcome::Unknown`], a warning stays up, and later refreshes reconcile it. |
//! | **Two-step flatten** | [`EngineHandle::preview_flatten`] shows what would happen and returns a single-use token that [`EngineHandle::flatten`] must present. |
//! | **Warn, never block** | Guardrails ([`guardrails`]) only add sentences. Only structural problems refuse an order. |
//!
//! # Sizing, units and what the servers do not tell us
//!
//! [`risk::build_plan`] is a pure function. Volume is an exact integer ([`domain::Volume`],
//! hundredths of a base-asset unit, so 0.01 of a coin is representable); prices and money are
//! `f64` display values.
//!
//! ```text
//! volume = floor(target risk / (stop distance * quote->account rate)), rounded DOWN to the step
//! ```
//!
//! Volume is always rounded down and the stop distance up, so the loss at the stop never
//! exceeds the target (checked by a property test over random inputs).
//!
//! Two servers, two dialects, one model (checked against live servers, 2026-09):
//!
//! | | Remote | Local |
//! |---|---|---|
//! | Quote prices | integer pipettes, always in units of 1e-5 | display floats |
//! | Position and order prices | display floats | display floats |
//! | Volume | hundredths of a unit (`volume = units * 100`) | base-asset units, sent with `volumeType: units` |
//! | Symbols | numeric `symbolId` | ticker names |
//! | Volume rules (lot, min, step) | **not published**: configured per symbol in [`config::AssumedSpecs`], else assumed | from `get_symbol_details`, in units |
//! | Price digits, pip size | **not published**: digits inferred from quotes | from `get_symbol_details` |
//! | Stop distances | integer points (1e-5) | whole pips |
//! | Server clock | none | `get_server_time` (`unixMs`) |
//! | Account kind | from the token's `environment` claim | unknown unless the account is in `get_accounts_list` |
//! | Order label echoed on positions | **no** | unknown |
//!
//! Where the engine assumed something, [`domain::Instrument::specs_source`] says
//! [`domain::SpecsSource::Assumed`] and every plan for that symbol carries a warning; rules
//! entered by the user are [`domain::SpecsSource::Configured`]. Because Remote does not echo
//! the order label, an order whose outcome is unknown is recognized later by its symbol, side
//! and volume among the positions that appeared after it was sent.
//!
//! # Threads and executors
//!
//! The engine owns a Tokio runtime on its own threads ([`Engine::start`]) or runs on one you
//! provide ([`Engine::start_on`]). Every [`EngineHandle`] method spawns its work there and
//! returns a future that only waits on a channel, so it can be awaited from **any** executor:
//! a UI framework that is not Tokio-based (GPUI, for example), another Tokio runtime, or a
//! test. The state and event channels are plain `tokio::sync` primitives, which do not need a
//! Tokio reactor to be polled.
//!
//! # Errors
//!
//! Every fallible call returns [`EngineError`]. [`EngineError::kind`] gives a stable
//! [`ErrorKind`] for choosing how to show it, and [`EngineError::is_retryable`] says whether
//! repeating a *read* can help. A broker refusal or an uncertain order is not an error: it is
//! an [`OrderOutcome`].
//!
//! # Testing code built on the engine
//!
//! With the `testing` feature, `broker::MockBroker` is a scriptable in-memory broker that
//! can fail on cue, lose replies and add latency, and [`EngineOptions::connector`] lets you
//! hand it to the engine. Pair it with `tokio::time::pause` for deterministic timing. The
//! crate's own scenario tests (`tests/engine_scenarios.rs`) are the best set of examples.
//!
//! # Known limitations
//!
//! - Order placement has not been verified against a live server yet (see `TODO.md`); use a
//!   demo account first.
//! - Only market orders are placed by the engine. Pending orders can be listed and cancelled.
//! - One session at a time. Every command and event already carries an
//!   [`AccountId`], so several accounts can be added without changing the API.
//! - Margin and leverage are not modeled (neither server provides the tier data), so plans do
//!   not check margin; the account snapshot reports what the server says.
//! - Risk conversion uses a direct or inverse spot quote (`EURUSD` or `USDEUR`). A quote
//!   currency with no such pair to the account currency makes risk sizing fail with a clear
//!   message; a fixed volume still works.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod broker;
pub mod config;
mod core;
pub mod domain;
mod engine;
mod error;
pub mod event;
pub mod guardrails;
mod ids;
pub mod openapi_auth;
pub mod risk;
pub mod state;
pub mod trading;

pub use config::EngineConfig;
pub use engine::{Engine, EngineHandle, EngineOptions};
pub use error::{BrokerErrorKind, EngineError, ErrorKind, Result};
pub use event::{Event, EventKind};
pub use ids::{AccountId, CommandId, OrderId, PositionId, Revision};
pub use risk::{EntryIntent, OrderPlan, PlanId, RiskSpec, SizeSpec, StopSpec, TakeProfitSpec};
pub use state::{EngineState, SessionState, TradingMode};
pub use trading::{
    ArmRequest, CloseSize, FlattenPreview, FlattenReport, FlattenScope, OrderOutcome, PlanSummary,
};
