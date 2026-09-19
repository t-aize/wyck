//! The application's use cases, without any interface: start the engine, connect, send a hotkey
//! order, dismiss a warning.
//!
//! [`AppController`] is what a front end calls. Every method returns something the UI can show as is
//! (a [`Notice`]) and never panics on an engine error. Because it holds no UI type, all of it is
//! tested against the engine's `MockBroker`.
//!
//! # Hotkey orders are dry runs
//!
//! This version has no arming flow, and a global shortcut that can fire while another window
//! has the focus must never be able to send a real order by accident. So [`AppController::hotkey_order`]
//! goes through the engine's normal planning and submission, and **refuses to run at all if the
//! engine is armed**, whoever armed it. Real orders come with an arming flow and its
//! confirmation (`TODO.md` 6.1).

use wyck_engine::broker::ConnectRequest;
use wyck_engine::domain::Side;
use wyck_engine::{
    Engine, EngineConfig, EngineError, EngineHandle, EngineOptions, EntryIntent, RiskSpec,
    SizeSpec, StopSpec, TakeProfitSpec, TradingMode,
};

use crate::messages::{Level, Notice, describe_error, describe_outcome};
use crate::settings::{AppSettings, OrderDefaults};

/// Owns the engine and turns UI intents into engine calls.
pub struct AppController {
    engine: Engine,
    symbol: String,
    order: OrderDefaults,
}

impl AppController {
    /// Starts the engine with real cTrader connections.
    ///
    /// # Errors
    ///
    /// [`EngineError::Config`] or [`EngineError::Internal`] when the engine cannot start.
    pub fn start(settings: &AppSettings) -> Result<Self, EngineError> {
        Self::start_with(settings, EngineOptions::default())
    }

    /// Starts the engine with custom options (a test connector, a fixed calendar).
    ///
    /// # Errors
    ///
    /// [`EngineError::Config`] or [`EngineError::Internal`] when the engine cannot start.
    pub fn start_with(settings: &AppSettings, options: EngineOptions) -> Result<Self, EngineError> {
        let config = EngineConfig {
            calendar_enabled: settings.news_enabled,
            ..EngineConfig::default()
        };
        Ok(Self {
            engine: Engine::start_with(config, options)?,
            symbol: settings.symbol.clone(),
            order: settings.order,
        })
    }

    /// The engine's handle: cheap to clone, awaitable from any executor.
    #[must_use]
    pub fn handle(&self) -> EngineHandle {
        self.engine.handle()
    }

    /// The symbol hotkey orders trade.
    #[must_use]
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Connects and starts quoting `watched`.
    ///
    /// # Errors
    ///
    /// A [`Notice`] describing why the connection failed.
    pub async fn connect(
        &self,
        request: ConnectRequest,
        watched: Vec<String>,
    ) -> Result<(), Notice> {
        let handle = self.handle();
        handle
            .connect(request)
            .await
            .map_err(|e| describe_error(&e))?;
        handle.watch_symbols(watched);
        Ok(())
    }

    /// Plans an order for the traded symbol with the default risk and stop, and submits it.
    ///
    /// Returns what to tell the user. Refuses, and sends nothing, when the engine is armed
    /// (see the module docs).
    pub async fn hotkey_order(&self, side: Side) -> Notice {
        let handle = self.handle();
        if handle.state().mode == TradingMode::Armed {
            return Notice {
                level: Level::Error,
                title: "Hotkey orders are dry runs only, and the engine is armed".to_owned(),
                detail: Some("Nothing was sent.".to_owned()),
                hint: Some("Disarm the engine to plan orders from the keyboard."),
            };
        }
        let intent = EntryIntent {
            symbol: self.symbol.clone(),
            side,
            size: SizeSpec::Risk(RiskSpec::PercentOfBalance(self.order.risk_percent)),
            stop_loss: Some(StopSpec::Pips(self.order.stop_pips)),
            take_profit: Some(TakeProfitSpec::RiskReward(self.order.reward_risk)),
        };
        let plan = match handle.plan_entry(intent).await {
            Ok(plan) => plan,
            Err(e) => return describe_error(&e),
        };
        match handle.submit(plan.id).await {
            Ok(outcome) => describe_outcome(&outcome),
            Err(e) => describe_error(&e),
        }
    }

    /// Dismisses a warning by id. Returns whether it existed.
    #[must_use]
    pub fn dismiss_warning(&self, id: &str) -> bool {
        self.handle().dismiss_warning(id)
    }

    /// Stops the engine and waits for its tasks. Dropping the controller stops it too, without
    /// waiting.
    pub async fn shutdown(self) {
        self.engine.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use wyck_engine::broker::{Broker, BrokerCall, Connector, MockBroker, ServiceKind};
    use wyck_engine::domain::AccountKind;
    use wyck_engine::{ArmRequest, CalendarSource, SessionState};

    use super::*;

    struct FixedConnector(Arc<MockBroker>);

    #[async_trait]
    impl Connector for FixedConnector {
        async fn connect(&self, _request: &ConnectRequest) -> Result<Arc<dyn Broker>, EngineError> {
            Ok(Arc::clone(&self.0) as Arc<dyn Broker>)
        }
    }

    fn settings() -> AppSettings {
        AppSettings::from_lookup(|name| (name == "WYCK_NEWS").then(|| "off".to_owned())).unwrap()
    }

    fn request() -> ConnectRequest {
        ConnectRequest::new(ServiceKind::CtraderRemote, "mock://server", None)
    }

    fn controller() -> (AppController, Arc<MockBroker>) {
        let broker = Arc::new(MockBroker::new());
        let options = EngineOptions {
            connector: Some(Arc::new(FixedConnector(Arc::clone(&broker)))),
            calendar: CalendarSource::Disabled,
        };
        (
            AppController::start_with(&settings(), options).unwrap(),
            broker,
        )
    }

    #[tokio::test]
    async fn connecting_reaches_ready_and_starts_quoting_the_traded_symbol() {
        let (app, _broker) = controller();
        app.connect(request(), settings().watched_symbols())
            .await
            .unwrap();
        let state = app.handle().state();
        assert_eq!(state.session, SessionState::Ready);
        assert_eq!(state.watched, ["EURUSD"]);
    }

    #[tokio::test]
    async fn a_hotkey_order_is_planned_and_reported_as_a_dry_run_that_sends_nothing() {
        let (app, broker) = controller();
        app.connect(request(), vec!["EURUSD".to_owned()])
            .await
            .unwrap();

        let notice = app.hotkey_order(Side::Buy).await;

        assert_eq!(notice.level, Level::Info, "{notice:?}");
        assert!(
            notice.title.starts_with("Dry run, nothing sent: Buy"),
            "{notice:?}"
        );
        assert!(notice.hint.unwrap().contains("dry-run mode"));
        assert_eq!(
            broker.call_count(BrokerCall::PlaceMarket),
            0,
            "nothing reached the broker"
        );
    }

    #[tokio::test]
    async fn a_hotkey_order_is_refused_when_the_engine_is_armed() {
        let (app, broker) = controller();
        app.connect(request(), vec!["EURUSD".to_owned()])
            .await
            .unwrap();
        let handle = app.handle();
        let account = handle.state().account_id.clone().unwrap();
        handle
            .arm(ArmRequest {
                account,
                acknowledged_kind: AccountKind::Demo,
            })
            .await
            .unwrap();
        assert_eq!(handle.state().mode, TradingMode::Armed);

        let notice = app.hotkey_order(Side::Sell).await;

        assert_eq!(notice.level, Level::Error);
        assert!(notice.title.contains("dry runs only"));
        assert_eq!(
            broker.call_count(BrokerCall::PlaceMarket),
            0,
            "a global key must never send a real order"
        );
    }

    #[tokio::test]
    async fn a_hotkey_order_before_the_connection_is_an_explained_error_not_a_panic() {
        let (app, broker) = controller();
        let notice = app.hotkey_order(Side::Buy).await;
        assert_eq!(notice.level, Level::Error);
        assert!(!notice.title.is_empty());
        assert_eq!(broker.call_count(BrokerCall::PlaceMarket), 0);
    }

    #[tokio::test]
    async fn an_order_that_cannot_be_built_says_why() {
        let (app, _broker) = controller();
        app.connect(request(), vec!["EURUSD".to_owned()])
            .await
            .unwrap();
        // An instrument the broker does not have.
        let mut s = settings();
        s.symbol = "NOPE".to_owned();
        let broker = Arc::new(MockBroker::new());
        let options = EngineOptions {
            connector: Some(Arc::new(FixedConnector(broker))),
            calendar: CalendarSource::Disabled,
        };
        let other = AppController::start_with(&s, options).unwrap();
        other.connect(request(), vec![]).await.unwrap();
        let notice = other.hotkey_order(Side::Buy).await;
        assert_eq!(notice.level, Level::Error);
        assert!(
            notice.detail.unwrap().to_lowercase().contains("nope"),
            "the symbol is named"
        );
    }

    #[tokio::test]
    async fn dismissing_an_unknown_warning_is_harmless() {
        let (app, _broker) = controller();
        assert!(!app.dismiss_warning("does-not-exist"));
    }
}
