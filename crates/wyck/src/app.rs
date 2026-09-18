//! The top-level application state machine: owns the current [`Screen`], the channel
//! to the [`crate::engine::Engine`], and the render/input loop.

use color_eyre::eyre::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use ratatui::{DefaultTerminal, Frame};
use secrecy::SecretString;
use tokio::sync::mpsc;
use wyck_config::WyckConfig;

use crate::engine::{EngineCommand, EngineEvent};
use crate::ui::{ConnectionStatus, DashboardScreen, FirstRunOutcome, Screen};

/// A ready-to-send connect request for the profile that was already active when the
/// app started — resolved once in `main`, before the terminal or the event loop exist
/// (see that module's `resolve_initial_connect`).
pub struct InitialConnect {
    pub display_name: String,
    pub endpoint: String,
    pub token: SecretString,
}

/// The application: everything [`main`](crate::main) hands off control to once the
/// terminal is ready.
pub struct App {
    config: WyckConfig,
    screen: Screen,
    engine_commands: mpsc::Sender<EngineCommand>,
    initial_connect: Option<InitialConnect>,
    should_quit: bool,
}

impl App {
    /// Builds the app. Starts on the [`Screen::Dashboard`] (in its `Connecting` state)
    /// if `initial_connect` is `Some`, otherwise on [`Screen::FirstRun`].
    pub fn new(
        config: WyckConfig,
        engine_commands: mpsc::Sender<EngineCommand>,
        initial_connect: Option<InitialConnect>,
    ) -> Self {
        let screen = match &initial_connect {
            Some(initial) => {
                Screen::Dashboard(DashboardScreen::connecting(initial.display_name.clone()))
            }
            None => Screen::FirstRun(Box::default()),
        };

        Self {
            config,
            screen,
            engine_commands,
            initial_connect,
            should_quit: false,
        }
    }

    /// Runs the render/input loop until the user quits. Every iteration draws exactly
    /// once, then waits for whichever happens first: a terminal input event, or an
    /// [`EngineEvent`] from the connection engine — so the UI never busy-polls and
    /// never blocks on one source while the other has something ready.
    ///
    /// # Errors
    ///
    /// Propagates a terminal I/O failure (draw or event-read) or a
    /// [`wyck_config::ConfigError`] from persisting a newly created profile.
    pub async fn run(
        mut self,
        terminal: &mut DefaultTerminal,
        mut engine_events: mpsc::Receiver<EngineEvent>,
    ) -> Result<()> {
        if let Some(initial) = self.initial_connect.take() {
            let _ = self
                .engine_commands
                .send(EngineCommand::Connect {
                    endpoint: initial.endpoint,
                    token: initial.token,
                })
                .await;
        }

        let mut terminal_events = EventStream::new();

        while !self.should_quit {
            terminal.draw(|frame| self.draw(frame))?;

            tokio::select! {
                Some(event) = terminal_events.next() => {
                    self.handle_terminal_event(event?).await?;
                }
                Some(event) = engine_events.recv() => {
                    self.handle_engine_event(event);
                }
            }
        }

        Ok(())
    }

    async fn handle_terminal_event(&mut self, event: Event) -> Result<()> {
        // Ctrl+C always quits, regardless of screen or field focus — the one keybind
        // that must never be swallowed by a text input.
        if is_ctrl_c(&event) {
            self.should_quit = true;
            return Ok(());
        }

        match &mut self.screen {
            Screen::FirstRun(form) => match form.handle_event(&event) {
                FirstRunOutcome::Continue => {}
                FirstRunOutcome::Cancelled => self.should_quit = true,
                FirstRunOutcome::Submit {
                    display_name,
                    service,
                    endpoint,
                    token,
                } => {
                    self.submit_first_run(display_name, service, endpoint, token)
                        .await?;
                }
            },
            Screen::Dashboard(_) => {
                if is_quit_key(&event) {
                    self.should_quit = true;
                } else if is_refresh_key(&event) {
                    let _ = self
                        .engine_commands
                        .send(EngineCommand::RefreshAccount)
                        .await;
                }
            }
        }

        Ok(())
    }

    /// Persists the profile the first-run form just collected, makes it the active
    /// profile, switches to the dashboard, and asks the engine to connect.
    async fn submit_first_run(
        &mut self,
        display_name: String,
        service: String,
        endpoint: String,
        token: SecretString,
    ) -> Result<()> {
        let id = self.config.add_profile(
            display_name.clone(),
            service,
            Some(endpoint.clone()),
            token.clone(),
        )?;
        self.config.set_active_profile(Some(id))?;

        self.screen = Screen::Dashboard(DashboardScreen::connecting(display_name));
        let _ = self
            .engine_commands
            .send(EngineCommand::Connect { endpoint, token })
            .await;

        Ok(())
    }

    fn handle_engine_event(&mut self, event: EngineEvent) {
        let Screen::Dashboard(dashboard) = &mut self.screen else {
            // Only reachable if a connection attempt from a since-replaced profile
            // resolves after the user has already moved back to the first-run form;
            // there is nothing to update in that case.
            return;
        };

        match event {
            EngineEvent::Connecting => dashboard.set_status(ConnectionStatus::Connecting),
            EngineEvent::Connected(snapshot) | EngineEvent::AccountRefreshed(snapshot) => {
                dashboard.set_status(ConnectionStatus::Connected(snapshot));
            }
            EngineEvent::ConnectionFailed(message) => {
                dashboard.set_status(ConnectionStatus::Failed(message))
            }
            EngineEvent::RefreshFailed(message) => {
                tracing::warn!(%message, "account refresh failed; keeping the last known snapshot on screen");
            }
        }
    }

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        match &self.screen {
            Screen::FirstRun(form) => form.draw(frame, area),
            Screen::Dashboard(dashboard) => dashboard.draw(frame, area),
        }
    }
}

fn is_ctrl_c(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(key)
            if key.kind == KeyEventKind::Press
                && key.code == KeyCode::Char('c')
                && key.modifiers.contains(KeyModifiers::CONTROL)
    )
}

fn is_quit_key(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(key)
            if key.kind == KeyEventKind::Press && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
    )
}

fn is_refresh_key(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(key) if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('r')
    )
}
