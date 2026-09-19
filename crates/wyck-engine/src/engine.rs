//! Starting and stopping the engine, and the public handle.

use std::future::Future;
use std::sync::Arc;

use tokio::runtime::{Builder, Handle, Runtime};
use tokio::sync::{broadcast, watch};
use wyck_calendar::{CalendarHandle, CalendarService};

use crate::broker::{ConnectRequest, Connector, CtraderConnector};
use crate::config::EngineConfig;
use crate::core::Inner;
use crate::error::{EngineError, Result};
use crate::event::Event;
use crate::ids::{CommandId, OrderId, PositionId};
use crate::news;
use crate::risk::{EntryIntent, OrderPlan, PlanId};
use crate::state::EngineState;
use crate::trading::{
    ArmRequest, CloseSize, FlattenPreview, FlattenReport, FlattenScope, OrderOutcome,
};

/// Where the engine gets its economic calendar.
#[derive(Clone, Default)]
#[non_exhaustive]
pub enum CalendarSource {
    /// Start a `wyck_calendar` service against the real feed. The default, and the choice
    /// that follows [`EngineConfig::calendar_enabled`].
    #[default]
    Default,
    /// No calendar: the news view stays `Disabled` and no news warnings are raised.
    Disabled,
    /// Use an already running calendar service (tests, or an app sharing one).
    Custom(CalendarHandle),
}

/// Optional pieces of an [`Engine`], for tests and embedding.
#[derive(Clone, Default)]
pub struct EngineOptions {
    /// How brokers are reached. `None` uses real cTrader connections.
    pub connector: Option<Arc<dyn Connector>>,
    /// Where news comes from.
    pub calendar: CalendarSource,
}

/// The running engine. Owns its Tokio runtime (unless started on an existing one) and stops
/// everything when [`shutdown`](Engine::shutdown) is called or the value is dropped.
///
/// Use [`Engine::handle`] to get the cheap, cloneable [`EngineHandle`] the rest of the
/// application talks to.
pub struct Engine {
    inner: Arc<Inner>,
    runtime: Option<Runtime>,
    handle: Handle,
}

impl Engine {
    /// Starts an engine with its own runtime, real cTrader connections and the real
    /// calendar feed. Callable from any thread, no async context required.
    ///
    /// # Errors
    ///
    /// [`EngineError::Config`] for an invalid configuration, [`EngineError::Internal`] if the
    /// runtime cannot be created.
    pub fn start(config: EngineConfig) -> Result<Self> {
        Self::start_with(config, EngineOptions::default())
    }

    /// Like [`Engine::start`], with a custom connector and calendar source.
    ///
    /// # Errors
    ///
    /// As [`Engine::start`].
    pub fn start_with(config: EngineConfig, options: EngineOptions) -> Result<Self> {
        config.validate()?;
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("wyck-engine")
            .enable_all()
            .build()
            .map_err(|e| {
                EngineError::Internal(format!("could not start the engine runtime: {e}"))
            })?;
        let handle = runtime.handle().clone();
        let engine = Self::assemble(config, options, handle, Some(runtime))?;
        Ok(engine)
    }

    /// Starts an engine on an existing Tokio runtime (the caller keeps ownership of it).
    ///
    /// # Errors
    ///
    /// [`EngineError::Config`] for an invalid configuration.
    pub fn start_on(handle: Handle, config: EngineConfig, options: EngineOptions) -> Result<Self> {
        config.validate()?;
        Self::assemble(config, options, handle, None)
    }

    fn assemble(
        config: EngineConfig,
        options: EngineOptions,
        handle: Handle,
        runtime: Option<Runtime>,
    ) -> Result<Self> {
        let connector = options
            .connector
            .unwrap_or_else(|| Arc::new(CtraderConnector::new(config.assumed_specs.clone())));
        let calendar_enabled = config.calendar_enabled;
        let inner = Inner::new(config, connector);

        // Everything that spawns must run inside the runtime.
        let _guard = handle.enter();
        let calendar = match options.calendar {
            CalendarSource::Disabled => None,
            CalendarSource::Custom(c) => Some(c),
            CalendarSource::Default if calendar_enabled => match CalendarService::spawn_default() {
                Ok(c) => Some(c),
                Err(error) => {
                    tracing::warn!(%error, "could not start the economic calendar");
                    None
                }
            },
            CalendarSource::Default => None,
        };
        if let Some(calendar) = calendar {
            *crate::core::lock(&inner.calendar) = Some(calendar.clone());
            news::spawn(&inner, calendar);
        }
        drop(_guard);

        Ok(Self {
            inner,
            runtime,
            handle,
        })
    }

    /// A handle to talk to the engine. Cheap to clone and `Send + Sync`.
    #[must_use]
    pub fn handle(&self) -> EngineHandle {
        EngineHandle {
            inner: Arc::clone(&self.inner),
            runtime: self.handle.clone(),
        }
    }

    /// Stops the engine: disarms, closes the broker session, stops every background task and
    /// waits for them. Safe to call from any async context.
    pub async fn shutdown(mut self) {
        let inner = Arc::clone(&self.inner);
        // Run the teardown on the engine's runtime, where its tasks and I/O live.
        let done = self.handle.spawn(async move { inner.shutdown().await });
        let _ = done.await;
        if let Some(runtime) = self.runtime.take() {
            // Never drop a runtime from async code: it would block the calling thread.
            runtime.shutdown_background();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.inner.shutdown.cancel();
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

/// The application's view of the engine. Cheap to clone; every method is `async` and safe
/// to await from **any** executor (a UI framework's, another Tokio runtime, a test): the
/// work itself runs on the engine's runtime.
#[derive(Clone)]
pub struct EngineHandle {
    inner: Arc<Inner>,
    runtime: Handle,
}

impl EngineHandle {
    /// Runs `work` on the engine's runtime and awaits its result.
    async fn run<T, F>(&self, work: F) -> Result<T>
    where
        T: Send + 'static,
        F: Future<Output = Result<T>> + Send + 'static,
    {
        if self.inner.shutdown.is_cancelled() {
            return Err(EngineError::ShuttingDown);
        }
        match self.runtime.spawn(work).await {
            Ok(result) => result,
            // The runtime went away while the work was queued or running.
            Err(_) => Err(EngineError::ShuttingDown),
        }
    }

    // ---- observing ----

    /// The current state. A cheap snapshot.
    #[must_use]
    pub fn state(&self) -> Arc<EngineState> {
        self.inner.snapshot()
    }

    /// A receiver that wakes when the state changes. `await` its `changed()` from any
    /// executor, then read [`state`](Self::state).
    #[must_use]
    pub fn watch_state(&self) -> watch::Receiver<Arc<EngineState>> {
        self.inner.watch_state()
    }

    /// Subscribes to events. A receiver that lags gets `RecvError::Lagged` and should
    /// resynchronize from [`state`](Self::state).
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.subscribe()
    }

    /// The most recent events, oldest first (bounded by `activity_log_len`).
    #[must_use]
    pub fn recent_events(&self) -> Vec<Event> {
        self.inner.recent_events()
    }

    // ---- session ----

    /// Connects (replacing any existing session) and returns when the session is `Ready`
    /// or has failed.
    ///
    /// # Errors
    ///
    /// The connection or bootstrap error; the state is `Failed` in that case.
    pub async fn connect(&self, request: ConnectRequest) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        let command = CommandId::next();
        self.run(async move { inner.connect(request, command).await })
            .await
    }

    /// Ends the session and disarms.
    ///
    /// # Errors
    ///
    /// [`EngineError::ShuttingDown`] only.
    pub async fn disconnect(&self) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        let command = CommandId::next();
        self.run(async move {
            inner
                .teardown("disconnected by request", Some(command))
                .await;
            Ok(())
        })
        .await
    }

    /// Sets the symbols the engine keeps quotes for (symbols with open positions are always
    /// included).
    pub fn watch_symbols(&self, symbols: impl IntoIterator<Item = String>) {
        self.inner.set_watched(symbols);
    }

    // ---- trading ----

    /// Enables real order sending. See [`ArmRequest`] for what must be acknowledged.
    ///
    /// # Errors
    ///
    /// [`EngineError::ArmRefused`] with the reason.
    pub async fn arm(&self, request: ArmRequest) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        let command = CommandId::next();
        self.run(async move { inner.arm(&request, command) }).await
    }

    /// Returns to dry-run mode.
    ///
    /// # Errors
    ///
    /// [`EngineError::ShuttingDown`] only.
    pub async fn disarm(&self) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        let command = CommandId::next();
        self.run(async move {
            inner.disarm(command);
            Ok(())
        })
        .await
    }

    /// Resolves an intent into a validated, sized plan. Reads the market; sends nothing.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] if the intent cannot be turned into a valid order, or a
    /// broker error from the reads it needs.
    pub async fn plan_entry(&self, intent: EntryIntent) -> Result<OrderPlan> {
        let inner = Arc::clone(&self.inner);
        let command = CommandId::next();
        self.run(async move { inner.plan_entry(intent, command).await })
            .await
    }

    /// Sends a plan (or, while disarmed, only reports what would be sent).
    ///
    /// # Errors
    ///
    /// State and validation errors. A broker refusal or an uncertain result is **not** an
    /// error: it is an [`OrderOutcome`].
    pub async fn submit(&self, plan: PlanId) -> Result<OrderOutcome> {
        let inner = Arc::clone(&self.inner);
        let command = CommandId::next();
        self.run(async move { inner.submit(plan, command).await })
            .await
    }

    /// Changes a position's stop loss and/or take profit (absolute prices). A leg left as
    /// `None` is kept as it is.
    ///
    /// # Errors
    ///
    /// [`EngineError::NotArmed`], validation and broker errors.
    pub async fn set_protection(
        &self,
        position: PositionId,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        let command = CommandId::next();
        self.run(async move {
            inner
                .set_protection(position, stop_loss, take_profit, command)
                .await
        })
        .await
    }

    /// Closes all or part of a position.
    ///
    /// # Errors
    ///
    /// [`EngineError::NotArmed`], validation and broker errors.
    pub async fn close_position(&self, position: PositionId, size: CloseSize) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        let command = CommandId::next();
        self.run(async move { inner.close_position(position, size, command).await })
            .await
    }

    /// Cancels a working order.
    ///
    /// # Errors
    ///
    /// [`EngineError::NotArmed`], validation and broker errors.
    pub async fn cancel_order(&self, order: OrderId) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        let command = CommandId::next();
        self.run(async move { inner.cancel_order(order, command).await })
            .await
    }

    /// Shows what a flatten would close and cancel, with a single-use confirmation token.
    ///
    /// # Errors
    ///
    /// State errors and broker read errors.
    pub async fn preview_flatten(&self, scope: FlattenScope) -> Result<FlattenPreview> {
        let inner = Arc::clone(&self.inner);
        self.run(async move { inner.preview_flatten(scope).await })
            .await
    }

    /// Executes a previewed flatten.
    ///
    /// # Errors
    ///
    /// [`EngineError::ConfirmationRejected`] for an unknown, used or expired token,
    /// [`EngineError::NotArmed`], and state errors. Per-item failures are in the report.
    pub async fn flatten(&self, token: String) -> Result<FlattenReport> {
        let inner = Arc::clone(&self.inner);
        let command = CommandId::next();
        self.run(async move { inner.flatten(&token, command).await })
            .await
    }

    /// Dismisses a warning, for example an unknown-order notice after checking the platform.
    /// Returns whether it existed.
    pub fn dismiss_warning(&self, id: &str) -> bool {
        self.inner.dismiss_warning(id)
    }
}
