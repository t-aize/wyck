mod commands;
mod console;
mod dashboard;
mod form;
mod header;
mod input;
mod login;
mod sessions;
mod theme;
mod ticket;

use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, Event as TermEvent, EventStream, KeyCode,
    KeyEvent, KeyEventKind, KeyModifiers,
};
use futures_util::StreamExt;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use secrecy::SecretString;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;

use self::dashboard::{Dashboard, Loaded, Outcome, Sender};
use self::form::{Field, Form, FormAction, centered};
use self::login::{Saved, SignedIn};
use crate::config::{AppPaths, KeyringSecretStore, SecretKey, SecretStore, WyckConfig};
use crate::openapi::ClientCredentials;
use crate::openapi::account::{PositionUnrealizedPnL, Trader};
use crate::openapi::config::{ConnectionConfig, Environment};
use crate::openapi::market::Symbol;
use crate::openapi::session::{Session, SessionConfig, SessionEvent, TokenStore};
use crate::session_tokens::ConfigTokenStore;

const TICK: Duration = Duration::from_millis(250);

pub enum Msg {
    LoginProgress(&'static str),
    Url(String),
    SignedIn(Result<SignedIn, String>),
    Authorized(Result<(), String>),
    Session(SessionEvent),
    Loaded(u64, Box<Result<Loaded, String>>),
    Trader(Trader),
    Pnl(Vec<PositionUnrealizedPnL>),
    Done(Result<String, String>),
    Added(Result<(Symbol, String), String>),
    Scoped(u64, Box<Msg>),
}

enum Screen {
    Passphrase,
    Login,
    Waiting(String),
    Accounts(usize),
    Starting(&'static str),
    Dashboard(Box<Dashboard>),
}

struct App {
    paths: AppPaths,
    config: Option<WyckConfig>,
    screen: Screen,
    passphrase: Form,
    login: Form,
    sign_in: Option<JoinHandle<()>>,
    pending: Option<SignedIn>,
    reset: bool,
    encrypted: bool,
    tick: u64,
    generation: u64,
    quit: bool,
    tx: UnboundedSender<Msg>,
}

pub async fn run(paths: AppPaths, reset: bool) -> anyhow::Result<()> {
    run_app(paths, reset, false).await
}

pub async fn preview(paths: AppPaths) -> anyhow::Result<()> {
    run_app(paths, false, true).await
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::execute!(std::io::stdout(), DisableBracketedPaste);
        ratatui::restore();
    }
}

async fn run_app(paths: AppPaths, reset: bool, preview: bool) -> anyhow::Result<()> {
    let (tx, mut rx) = unbounded_channel();
    let mut app = App::new(paths, reset, tx);
    let mut terminal = ratatui::init();
    let guard = TerminalGuard;
    crossterm::execute!(std::io::stdout(), EnableBracketedPaste)?;
    if preview {
        app.screen = Screen::Dashboard(Box::new(Dashboard::preview(app.tx.clone())));
    } else {
        app.start();
    }
    let result = event_loop(&mut terminal, &mut app, &mut rx).await;
    drop(guard);
    app.shutdown().await;
    result
}

async fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    rx: &mut UnboundedReceiver<Msg>,
) -> anyhow::Result<()> {
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(TICK);
    while !app.quit {
        terminal.draw(|frame| app.draw(frame))?;
        tokio::select! {
            event = events.next() => match event {
                Some(Ok(TermEvent::Key(key))) if key.kind == KeyEventKind::Press => app.on_key(key),
                Some(Ok(TermEvent::Paste(text))) => app.on_paste(&text),
                Some(Err(error)) => return Err(error.into()),
                None => break,
                _ => {}
            },
            Some(msg) = rx.recv() => app.on_msg(msg),
            _ = ticker.tick() => app.on_tick(),
        }
    }
    Ok(())
}

fn keyring_works() -> bool {
    let store = KeyringSecretStore::new("wyck");
    let key = SecretKey::new("probe", "keyring");
    let secret = SecretString::from("probe".to_owned());
    store.store(&key, &secret).is_ok()
        && store.retrieve(&key).is_ok_and(|found| found.is_some())
        && store.delete(&key).is_ok()
}

impl App {
    fn new(paths: AppPaths, reset: bool, tx: UnboundedSender<Msg>) -> Self {
        let mut login = Form::new(
            "Sign in to cTrader",
            "Enter: sign in   Tab: next field   Esc: quit",
            vec![
                Field::new("Client ID", "", false),
                Field::new("Client secret", "", true),
            ],
        );
        login.live = Some(false);
        let passphrase = Form::new(
            "Passphrase",
            "Enter: unlock   Esc: quit",
            vec![Field::new("Passphrase", "", true)],
        );
        Self {
            paths,
            config: None,
            screen: Screen::Starting("Starting..."),
            passphrase,
            login,
            sign_in: None,
            pending: None,
            reset,
            encrypted: false,
            tick: 0,
            generation: 0,
            quit: false,
            tx,
        }
    }

    fn start(&mut self) {
        if keyring_works() {
            self.open_config(None);
        } else {
            self.encrypted = true;
            self.screen = Screen::Passphrase;
        }
    }

    fn open_config(&mut self, passphrase: Option<SecretString>) {
        let mut config = match WyckConfig::open(self.paths.clone(), passphrase) {
            Ok(config) => config,
            Err(error) => return self.fail_config(error.to_string()),
        };
        if self.reset
            && let Err(error) = login::forget_connections(&mut config)
        {
            return self.fail_config(error.to_string());
        }
        self.reset = false;
        match login::saved_connection(&config) {
            Ok(saved) => {
                self.config = Some(config);
                match saved {
                    Some(saved) => self.start_dashboard(saved),
                    None => self.screen = Screen::Login,
                }
            }
            Err(error) => self.fail_config(format!("could not read the saved connection: {error}")),
        }
    }

    fn fail_config(&mut self, message: String) {
        if self.encrypted {
            self.passphrase.busy = false;
            self.passphrase.error = Some(format!("{message} (wrong passphrase?)"));
            self.screen = Screen::Passphrase;
        } else {
            self.login.error = Some(message);
            self.screen = Screen::Login;
        }
    }

    fn start_dashboard(&mut self, saved: Saved) {
        let Some(config) = self.config.as_ref() else {
            return;
        };
        let store: Arc<dyn TokenStore> = Arc::new(ConfigTokenStore::new(
            config.openapi_token_storage(&saved.profile_id),
        ));
        let session_config = SessionConfig::new(
            ConnectionConfig::new(saved.environment),
            saved.credentials,
            saved.account_id,
        );
        let session = match Session::start(session_config, saved.tokens, store) {
            Ok(session) => session,
            Err(error) => {
                self.login.busy = false;
                self.login.error = Some(error.to_string());
                self.screen = Screen::Login;
                return;
            }
        };
        let mut events = session.events();
        self.generation = self.generation.wrapping_add(1);
        let tx = Sender::new(self.tx.clone(), self.generation);
        let dashboard_tx = tx.clone();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        let ended =
                            matches!(event, SessionEvent::Stopped | SessionEvent::Failed(_));
                        if tx.send(Msg::Session(event)).is_err() || ended {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        let docs = config.scope(&login::document_scope(saved.environment, saved.account_id));
        self.screen = Screen::Dashboard(Box::new(Dashboard::new(
            saved.label,
            saved.environment,
            session,
            docs,
            dashboard_tx,
        )));
    }

    async fn shutdown(&mut self) {
        if let Some(task) = self.sign_in.take() {
            task.abort();
        }
        if let Screen::Dashboard(dashboard) = &self.screen {
            let _ = tokio::time::timeout(Duration::from_secs(3), dashboard.stop()).await;
        }
    }

    // ---- messages ----

    fn on_tick(&mut self) {
        self.tick += 1;
        if let Screen::Dashboard(dashboard) = &mut self.screen {
            dashboard.on_tick(self.tick);
        }
    }

    fn on_msg(&mut self, msg: Msg) {
        match msg {
            Msg::LoginProgress(label)
                if matches!(self.screen, Screen::Login) && self.login.busy =>
            {
                self.login.busy_label = label;
            }
            Msg::Url(url) => {
                if matches!(self.screen, Screen::Login) {
                    self.screen = Screen::Waiting(url);
                }
            }
            Msg::SignedIn(Ok(signed)) => {
                self.sign_in = None;
                self.login.busy = false;
                let single = signed.accounts.len() == 1;
                self.pending = Some(signed);
                if single {
                    self.choose_account(0);
                } else {
                    self.screen = Screen::Accounts(0);
                }
            }
            Msg::SignedIn(Err(error)) => {
                self.sign_in = None;
                self.back_to_login(error);
            }
            Msg::Authorized(Ok(())) => self.finish_sign_in(),
            Msg::Authorized(Err(error)) => self.back_to_login(error),
            Msg::Scoped(generation, msg) if generation == self.generation => {
                if let Screen::Dashboard(dashboard) = &mut self.screen {
                    match *msg {
                        Msg::Session(event) => dashboard.on_session(event),
                        Msg::Loaded(id, loaded) => dashboard.on_loaded(id, *loaded),
                        Msg::Trader(trader) => dashboard.on_trader(trader),
                        Msg::Pnl(answer) => dashboard.on_pnl(answer),
                        Msg::Done(result) => dashboard.on_done(result),
                        Msg::Added(result) => dashboard.on_added(result),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn back_to_login(&mut self, error: String) {
        if let Some(task) = self.sign_in.take() {
            task.abort();
        }
        self.pending = None;
        self.login.busy = false;
        self.login.error = Some(error);
        self.screen = Screen::Login;
    }

    fn choose_account(&mut self, index: usize) {
        let Some(pending) = &self.pending else {
            return;
        };
        let Some(account) = pending.accounts.get(index) else {
            return;
        };
        let account_id = account.ctid_trader_account_id;
        let client = pending.client.clone();
        let tokens = pending.tokens.clone();
        let tx = self.tx.clone();
        self.screen = Screen::Starting("Authorizing the account...");
        if let Some(pending) = self.pending.as_mut() {
            pending.accounts.swap(0, index);
        }
        self.sign_in = Some(tokio::spawn(async move {
            let _ = tx.send(Msg::Authorized(
                login::authorize(client, account_id, tokens).await,
            ));
        }));
    }

    fn finish_sign_in(&mut self) {
        self.sign_in = None;
        let (Some(pending), Some(config)) = (self.pending.take(), self.config.as_mut()) else {
            return;
        };
        let account = pending.accounts[0].clone();
        match login::save_connection(
            config,
            &pending.credentials,
            pending.environment,
            &account,
            &pending.tokens,
        ) {
            Ok(saved) => self.start_dashboard(saved),
            Err(error) => {
                self.back_to_login(format!(
                    "connected, but could not save the profile: {error}"
                ));
            }
        }
    }

    // ---- keys ----

    fn on_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if let Screen::Dashboard(dashboard) = &mut self.screen
                && dashboard.cancel_input()
            {
                return;
            }
            self.quit = true;
            return;
        }
        match &mut self.screen {
            Screen::Passphrase => match self.passphrase.key(key) {
                FormAction::Cancel => self.quit = true,
                FormAction::Submit => self.submit_passphrase(),
                FormAction::None => {}
            },
            Screen::Login => match self.login.key(key) {
                FormAction::Cancel => self.quit = true,
                FormAction::Submit => self.submit_login(),
                FormAction::None => {}
            },
            Screen::Waiting(_) => {
                if key.code == KeyCode::Esc {
                    if let Some(task) = self.sign_in.take() {
                        task.abort();
                    }
                    self.login.busy = false;
                    self.screen = Screen::Login;
                }
            }
            Screen::Accounts(selected) => {
                let count = self.pending.as_ref().map_or(0, |p| p.accounts.len());
                match key.code {
                    KeyCode::Down | KeyCode::Char('j') => {
                        *selected = (*selected + 1).min(count.saturating_sub(1));
                    }
                    KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
                    KeyCode::Enter => {
                        let index = *selected;
                        self.choose_account(index);
                    }
                    KeyCode::Esc => self.back_to_login(String::new()),
                    _ => {}
                }
            }
            Screen::Starting(_) => {
                if key.code == KeyCode::Esc {
                    self.quit = true;
                }
            }
            Screen::Dashboard(dashboard) => {
                if key.code == KeyCode::Char('q') && dashboard.is_idle() {
                    self.quit = true;
                } else {
                    match dashboard.on_key(key) {
                        Outcome::SignOut => self.sign_out(),
                        Outcome::Quit => self.quit = true,
                        Outcome::None => {}
                    }
                }
            }
        }
    }

    fn submit_passphrase(&mut self) {
        let text = self.passphrase.fields[0].input.text().to_owned();
        if text.is_empty() {
            self.passphrase.error = Some("A passphrase is required".to_owned());
            return;
        }
        self.passphrase.error = None;
        self.open_config(Some(SecretString::from(text)));
    }

    fn submit_login(&mut self) {
        let client_id = self.login.value(0).to_owned();
        let secret = self.login.value(1).to_owned();
        if client_id.is_empty() || secret.is_empty() {
            self.login.error = Some("Client ID and client secret are both required".to_owned());
            return;
        }
        self.login.error = None;
        self.login.busy = true;
        self.login.busy_label = "Connecting to cTrader...";
        let environment = if self.login.live == Some(true) {
            Environment::Live
        } else {
            Environment::Demo
        };
        let credentials = ClientCredentials::new(client_id, secret);
        let tx = self.tx.clone();
        self.sign_in = Some(tokio::spawn(async move {
            let result = login::sign_in(credentials, environment, tx.clone()).await;
            let _ = tx.send(Msg::SignedIn(result));
        }));
    }

    fn sign_out(&mut self) {
        if let Screen::Dashboard(dashboard) = &self.screen
            && let Some(session) = dashboard.session()
        {
            tokio::spawn(async move { session.stop().await });
        }
        if let Some(config) = self.config.as_mut()
            && let Err(error) = login::forget_connections(config)
        {
            tracing::warn!(%error, "could not forget the saved connection");
        }
        self.login.fields[1].input.clear();
        self.login.busy = false;
        self.screen = Screen::Login;
    }

    // ---- drawing ----

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Block::default().style(theme::base()), area);
        if let Screen::Dashboard(dashboard) = &self.screen {
            dashboard.draw(frame);
            return;
        }
        let [banner, area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        let status = match self.screen {
            Screen::Passphrase => "Locked",
            Screen::Login if self.login.busy => self.login.busy_label,
            Screen::Login => "Sign in",
            Screen::Waiting(_) => "Awaiting approval",
            Screen::Accounts(_) => "Select account",
            Screen::Starting(text) => text,
            Screen::Dashboard(_) => unreachable!(),
        };
        header::Header {
            context: "cTrader Open API",
            environment: self.login.live.map(|live| {
                if live {
                    Environment::Live
                } else {
                    Environment::Demo
                }
            }),
            status,
            market: None,
            balance: None,
        }
        .render(frame, banner);
        match &self.screen {
            Screen::Passphrase => {
                intro(
                    frame,
                    area,
                    "No OS keyring was found. Your credentials are kept in encrypted files, \
                     locked by a passphrase you type at every start.",
                );
                self.passphrase.render(frame, area, 64);
            }
            Screen::Login => {
                intro(
                    frame,
                    area,
                    &format!(
                        "Use the client ID and secret of your cTrader Open API application, \
                         with http://localhost:{} as its redirect address.",
                        login::CALLBACK_PORT
                    ),
                );
                self.login.render(frame, area, 64);
            }
            Screen::Waiting(url) => {
                let text = format!(
                    "Your browser should open on the cTrader consent page. If it did not, open \
                     this address:\n\n{url}\n\nWaiting for you to approve (5 minutes). Esc to cancel."
                );
                let rect = centered(area, area.width.min(90), 12);
                frame.render_widget(
                    Paragraph::new(text)
                        .wrap(Wrap { trim: false })
                        .block(theme::popup(" Sign in ".to_owned())),
                    rect,
                );
            }
            Screen::Accounts(selected) => {
                let items: Vec<ListItem> = self
                    .pending
                    .iter()
                    .flat_map(|p| p.accounts.iter())
                    .map(|a| ListItem::new(login::account_label(a)))
                    .collect();
                let height = u16::try_from(items.len() + 2).unwrap_or(10);
                let rect = centered(area, 60, height.max(4));
                let list = List::new(items)
                    .block(theme::popup(" Pick an account ".to_owned()))
                    .highlight_style(Style::default().bg(theme::SELECTED))
                    .highlight_symbol("> ");
                let mut state = ListState::default().with_selected(Some(*selected));
                frame.render_stateful_widget(list, rect, &mut state);
            }
            Screen::Starting(text) => {
                let rect = centered(area, 50, 3);
                frame.render_widget(
                    Paragraph::new(*text).block(theme::popup(" wyck ".to_owned())),
                    rect,
                );
            }
            Screen::Dashboard(dashboard) => dashboard.draw(frame),
        }
    }
}

impl App {
    fn on_paste(&mut self, text: &str) {
        match &mut self.screen {
            Screen::Passphrase => self.passphrase.paste(text),
            Screen::Login => self.login.paste(text),
            Screen::Dashboard(dashboard) => dashboard.paste(text),
            _ => {}
        }
    }
}

fn intro(frame: &mut Frame, area: Rect, text: &str) {
    let [top, _] = Layout::vertical([Constraint::Length(4), Constraint::Min(0)]).areas(area);
    frame.render_widget(
        Paragraph::new(format!("\n {text}"))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(theme::MUTED)),
        top,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn header(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        (0..100)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
            .collect()
    }

    #[test]
    fn messages_from_a_previous_session_are_ignored() {
        let (tx, _) = unbounded_channel();
        let mut app = App::new(
            AppPaths::at(std::env::temp_dir().join("wyck-tui-unused")),
            false,
            tx.clone(),
        );
        app.screen = Screen::Dashboard(Box::new(Dashboard::preview(tx)));
        app.generation = 2;
        app.on_msg(Msg::Scoped(
            1,
            Box::new(Msg::Session(SessionEvent::Stopped)),
        ));
        assert!(header(&app).contains("PREVIEW"));
        app.on_msg(Msg::Scoped(
            2,
            Box::new(Msg::Session(SessionEvent::Stopped)),
        ));
        assert!(header(&app).contains("OFFLINE"));
    }
}
