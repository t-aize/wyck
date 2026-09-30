use std::collections::BTreeSet;
use std::fmt::Display;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table, TableState, Tabs};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use super::Msg;
use super::form::{Field, Form, FormAction, centered};
use crate::config::DocumentStore;
use crate::openapi::account::book::{AccountBook, Tone, describe, is_buy};
use crate::openapi::account::{Order, Position, PositionUnrealizedPnL, TradeSide, Trader};
use crate::openapi::market::{LightSymbol, SpotTracker, Symbol, to_price};
use crate::openapi::session::{Session, SessionEvent};
use crate::openapi::trading::NewOrderReq;
use crate::openapi::trading::contract::{Contract, format_lots};
use crate::openapi::{Environment, Event};
use crate::trading::guard::{self, OrderFacts, RiskPrefs, Standing, Verdict};
use crate::trading::math::format_money;

const WATCHLIST: &str = "watchlist";
const RISK: &str = "risk";
const PNL_EVERY_TICKS: u64 = 20;

#[derive(Default, Serialize, Deserialize)]
struct Watchlist {
    #[serde(default)]
    symbols: Vec<String>,
}

pub struct Loaded {
    symbols: Vec<LightSymbol>,
    trader: Trader,
    positions: Vec<Position>,
    orders: Vec<Order>,
    details: Vec<Symbol>,
    watch: Vec<i64>,
    currency: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Watchlist,
    Positions,
    Orders,
}

const TABS: [Tab; 3] = [Tab::Watchlist, Tab::Positions, Tab::Orders];

#[derive(Clone, Copy)]
enum Confirm {
    Close(i64, i64),
    Cancel(i64),
    SignOut,
}

struct Ticket {
    form: Form,
    symbol_id: i64,
    side: TradeSide,
    warned: bool,
}

enum Popup {
    None,
    Add(Form),
    Ticket(Ticket),
    Confirm(Confirm),
}

pub enum Outcome {
    None,
    SignOut,
}

pub struct Dashboard {
    label: String,
    environment: Environment,
    session: Session,
    docs: DocumentStore,
    risk: RiskPrefs,
    status: String,
    symbols: Vec<LightSymbol>,
    book: AccountBook,
    quotes: SpotTracker,
    watch: Vec<i64>,
    wanted: Vec<String>,
    tab: Tab,
    selected: [usize; 3],
    popup: Popup,
    notice: Option<(Tone, String)>,
    tx: UnboundedSender<Msg>,
}

fn err(error: impl Display) -> String {
    error.to_string()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
}

impl Dashboard {
    pub fn new(
        label: String,
        environment: Environment,
        session: Session,
        docs: DocumentStore,
        tx: UnboundedSender<Msg>,
    ) -> Self {
        let wanted = docs.load_or_default::<Watchlist>(WATCHLIST).symbols;
        let risk = docs.load_or_default::<RiskPrefs>(RISK).normalized();
        Self {
            label,
            environment,
            session,
            docs,
            risk,
            status: "Connecting...".to_owned(),
            symbols: Vec::new(),
            book: AccountBook::default(),
            quotes: SpotTracker::new(),
            watch: Vec::new(),
            wanted,
            tab: Tab::Watchlist,
            selected: [0; 3],
            popup: Popup::None,
            notice: None,
            tx,
        }
    }

    pub async fn stop(&self) {
        self.session.stop().await;
    }

    pub fn session(&self) -> Session {
        self.session.clone()
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.popup, Popup::None)
    }

    fn account(&self) -> Option<crate::openapi::AccountClient> {
        Some(self.session.client()?.account(self.session.account_id()))
    }

    fn notify(&mut self, tone: Tone, text: impl Into<String>) {
        self.notice = Some((tone, text.into()));
    }

    fn quote_fn(&self) -> impl Fn(i64) -> (Option<f64>, Option<f64>) + '_ {
        |id| {
            let spot = self.quotes.get(id);
            (
                spot.and_then(|s| s.bid).map(to_price),
                spot.and_then(|s| s.ask).map(to_price),
            )
        }
    }

    fn money(&self, amount: f64) -> String {
        format_money(amount, &self.book.currency)
    }

    // ---- loading ----

    fn spawn_load(&self) {
        let session = self.session.clone();
        let wanted = self.wanted.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(Msg::Loaded(Box::new(load(session, wanted).await)));
        });
    }

    pub fn on_loaded(&mut self, loaded: Result<Loaded, String>) {
        let loaded = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                self.notify(Tone::Error, format!("Could not load the account: {error}"));
                return;
            }
        };
        self.book.names = loaded
            .symbols
            .iter()
            .map(|s| (s.symbol_id, s.symbol_name.clone().unwrap_or_default()))
            .collect();
        for symbol in &loaded.details {
            self.book
                .contracts
                .insert(symbol.symbol_id, Contract::from_symbol(symbol));
        }
        self.book.currency = loaded.currency;
        self.book.trader = Some(loaded.trader);
        self.book.reconcile(loaded.positions, loaded.orders);
        self.symbols = loaded.symbols;
        self.watch = loaded.watch;
        self.status = "Connected".to_owned();
        self.clamp();
        self.refresh_pnl();
    }

    fn refresh_pnl(&self) {
        if self.book.positions.is_empty() {
            return;
        }
        let Some(account) = self.account() else {
            return;
        };
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Ok(answer) = account.account_data().position_unrealized_pnl().await {
                let _ = tx.send(Msg::Pnl(answer));
            }
        });
    }

    fn refresh_trader(&self) {
        let Some(account) = self.account() else {
            return;
        };
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Ok(trader) = account.account_data().trader().await {
                let _ = tx.send(Msg::Trader(trader));
            }
        });
    }

    pub fn on_pnl(&mut self, answer: Vec<PositionUnrealizedPnL>) {
        let digits = self.book.money_digits();
        let quotes = |id: i64| {
            let spot = self.quotes.get(id);
            (
                spot.and_then(|s| s.bid).map(to_price),
                spot.and_then(|s| s.ask).map(to_price),
            )
        };
        let mut book = std::mem::take(&mut self.book);
        book.set_pnl(&answer, digits, &quotes);
        self.book = book;
    }

    pub fn on_trader(&mut self, trader: Trader) {
        self.book.trader = Some(trader);
    }

    pub fn on_done(&mut self, result: Result<String, String>) {
        match result {
            Ok(text) => self.notify(Tone::Success, text),
            Err(text) => self.notify(Tone::Error, text),
        }
    }

    pub fn on_added(&mut self, result: Result<(Symbol, String), String>) {
        match result {
            Ok((symbol, name)) => {
                self.book
                    .contracts
                    .insert(symbol.symbol_id, Contract::from_symbol(&symbol));
                if !self.watch.contains(&symbol.symbol_id) {
                    self.watch.push(symbol.symbol_id);
                }
                self.save_watchlist();
                self.notify(Tone::Success, format!("{name} added to the watchlist"));
            }
            Err(error) => self.notify(Tone::Error, error),
        }
    }

    fn save_watchlist(&mut self) {
        let symbols: Vec<String> = self.watch.iter().map(|id| self.book.name(*id)).collect();
        self.wanted.clone_from(&symbols);
        if let Err(error) = self.docs.save(WATCHLIST, &Watchlist { symbols }) {
            tracing::warn!(%error, "could not save the watchlist");
        }
    }

    // ---- session events ----

    pub fn on_session(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Ready => {
                self.status = "Loading...".to_owned();
                self.spawn_load();
            }
            SessionEvent::Reconnecting {
                attempt, retry_in, ..
            } => {
                self.status = format!(
                    "Reconnecting (attempt {attempt}, in {}s)",
                    retry_in.as_secs()
                );
            }
            SessionEvent::SubscriptionFailed { what, error } => {
                self.notify(Tone::Warning, format!("Could not follow {what}: {error}"));
            }
            SessionEvent::Failed(error) => {
                self.status = "Disconnected".to_owned();
                self.notify(
                    Tone::Error,
                    format!("Session ended: {error}. Sign out (o) and sign in again."),
                );
            }
            SessionEvent::Stopped => self.status = "Stopped".to_owned(),
            SessionEvent::Data(event) => self.on_event(event),
            _ => {}
        }
    }

    fn on_event(&mut self, event: Event) {
        match event {
            Event::Spot(spot) => {
                self.quotes.apply(&spot);
            }
            Event::Execution(execution) => {
                let applied = self.book.apply(&execution);
                if let Some(notice) = applied.notice {
                    self.notify(notice.tone, format!("{}: {}", notice.title, notice.message));
                }
                if applied.balance_changed {
                    self.refresh_trader();
                }
                self.refresh_pnl();
                self.clamp();
            }
            Event::TraderUpdated(_) => self.refresh_trader(),
            Event::OrderError(error) => {
                self.notify(
                    Tone::Error,
                    format!(
                        "Order failed: {}",
                        error.description.unwrap_or(error.error_code)
                    ),
                );
            }
            _ => {}
        }
    }

    pub fn on_tick(&mut self, tick: u64) {
        if tick.is_multiple_of(PNL_EVERY_TICKS) {
            self.refresh_pnl();
        }
    }

    // ---- keys ----

    pub fn on_key(&mut self, key: KeyEvent) -> Outcome {
        let popup = std::mem::replace(&mut self.popup, Popup::None);
        match popup {
            Popup::Add(form) => self.key_add(form, key),
            Popup::Ticket(ticket) => self.key_ticket(ticket, key),
            Popup::Confirm(confirm) => return self.key_confirm(confirm, key),
            Popup::None => return self.key_main(key),
        }
        Outcome::None
    }

    fn key_main(&mut self, key: KeyEvent) -> Outcome {
        self.notice = None;
        match key.code {
            KeyCode::Char('1') => self.tab = Tab::Watchlist,
            KeyCode::Char('2') => self.tab = Tab::Positions,
            KeyCode::Char('3') => self.tab = Tab::Orders,
            KeyCode::Tab => self.shift_tab(1),
            KeyCode::BackTab => self.shift_tab(TABS.len() - 1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Char('o') => self.popup = Popup::Confirm(Confirm::SignOut),
            KeyCode::Char('a') if self.tab == Tab::Watchlist => {
                self.popup = Popup::Add(Form::new(
                    "Add a symbol",
                    "Enter: add   Esc: cancel",
                    vec![Field::new("Symbol", "", false)],
                ));
            }
            KeyCode::Char('d') if self.tab == Tab::Watchlist => self.remove_selected(),
            KeyCode::Char('b') if self.tab == Tab::Watchlist => self.open_ticket(TradeSide::Buy),
            KeyCode::Char('s') if self.tab == Tab::Watchlist => self.open_ticket(TradeSide::Sell),
            KeyCode::Char('x') if self.tab == Tab::Positions => {
                if let Some(position) = self.book.positions.values().nth(self.selected[1]) {
                    let confirm = Confirm::Close(position.position_id, position.trade_data.volume);
                    self.popup = Popup::Confirm(confirm);
                }
            }
            KeyCode::Char('c') if self.tab == Tab::Orders => {
                if let Some(order) = self.book.orders.values().nth(self.selected[2]) {
                    self.popup = Popup::Confirm(Confirm::Cancel(order.order_id));
                }
            }
            _ => {}
        }
        Outcome::None
    }

    fn shift_tab(&mut self, by: usize) {
        let current = TABS.iter().position(|t| *t == self.tab).unwrap_or(0);
        self.tab = TABS[(current + by) % TABS.len()];
    }

    fn tab_index(&self) -> usize {
        TABS.iter().position(|t| *t == self.tab).unwrap_or(0)
    }

    fn row_count(&self) -> usize {
        match self.tab {
            Tab::Watchlist => self.watch.len(),
            Tab::Positions => self.book.positions.len(),
            Tab::Orders => self.book.orders.len(),
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let count = self.row_count();
        let index = self.tab_index();
        if count == 0 {
            self.selected[index] = 0;
            return;
        }
        let next = self.selected[index].saturating_add_signed(delta);
        self.selected[index] = next.min(count - 1);
    }

    fn clamp(&mut self) {
        for (index, tab) in TABS.iter().enumerate() {
            let count = match tab {
                Tab::Watchlist => self.watch.len(),
                Tab::Positions => self.book.positions.len(),
                Tab::Orders => self.book.orders.len(),
            };
            self.selected[index] = self.selected[index].min(count.saturating_sub(1));
        }
    }

    fn remove_selected(&mut self) {
        let index = self.selected[0];
        if index >= self.watch.len() {
            return;
        }
        let id = self.watch.remove(index);
        self.save_watchlist();
        self.clamp();
        let session = self.session.clone();
        tokio::spawn(async move {
            let _ = session.unsubscribe_spots(&[id]).await;
        });
    }

    fn key_add(&mut self, mut form: Form, key: KeyEvent) {
        match form.key(key) {
            FormAction::Cancel => {}
            FormAction::None => self.popup = Popup::Add(form),
            FormAction::Submit => {
                let wanted = form.value(0).to_owned();
                let found = self.symbols.iter().find(|s| {
                    s.symbol_name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(&wanted))
                });
                let Some(symbol) = found else {
                    form.error = Some(format!("{wanted} is not offered by this broker"));
                    self.popup = Popup::Add(form);
                    return;
                };
                let id = symbol.symbol_id;
                let name = symbol.symbol_name.clone().unwrap_or_default();
                let session = self.session.clone();
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(Msg::Added(add_symbol(session, id, name).await));
                });
            }
        }
    }

    fn open_ticket(&mut self, side: TradeSide) {
        let Some(id) = self.watch.get(self.selected[0]).copied() else {
            return;
        };
        let name = self.book.name(id);
        let contract = self.book.contract(id);
        let verb = if side == TradeSide::Buy {
            "Buy"
        } else {
            "Sell"
        };
        let lots = format_lots(contract.lots_of_volume(contract.min_volume));
        let mut form = Form::new(
            format!("{verb} {name} at market"),
            "Enter: send   Tab: next field   Esc: cancel",
            vec![
                Field::new("Lots", &lots, false),
                Field::new("Stop loss", "", false),
                Field::new("Take profit", "", false),
            ],
        );
        form.focus = 0;
        self.popup = Popup::Ticket(Ticket {
            form,
            symbol_id: id,
            side,
            warned: false,
        });
    }

    fn key_ticket(&mut self, mut ticket: Ticket, key: KeyEvent) {
        match ticket.form.key(key) {
            FormAction::Cancel => {}
            FormAction::None => {
                if ticket.form.warning.is_none() {
                    ticket.warned = false;
                }
                self.popup = Popup::Ticket(ticket);
            }
            FormAction::Submit => {
                ticket.form.error = None;
                match self.submit_ticket(&mut ticket) {
                    Ok(true) => {}
                    Ok(false) => self.popup = Popup::Ticket(ticket),
                    Err(error) => {
                        ticket.form.error = Some(error);
                        self.popup = Popup::Ticket(ticket);
                    }
                }
            }
        }
    }

    fn submit_ticket(&mut self, ticket: &mut Ticket) -> Result<bool, String> {
        let lots: f64 = ticket
            .form
            .value(0)
            .parse()
            .map_err(|_| "Lots must be a number".to_owned())?;
        if !lots.is_finite() || lots <= 0.0 {
            return Err("Lots must be above zero".to_owned());
        }
        let price = |text: &str, what: &str| -> Result<Option<f64>, String> {
            if text.is_empty() {
                return Ok(None);
            }
            text.parse::<f64>()
                .map(Some)
                .map_err(|_| format!("{what} must be a price"))
        };
        let stop_loss = price(ticket.form.value(1), "Stop loss")?;
        let take_profit = price(ticket.form.value(2), "Take profit")?;

        let id = ticket.symbol_id;
        let contract = self.book.contract(id);
        let stepped = contract.volume_near(lots);
        let volume = stepped.volume;
        let held: f64 = self
            .book
            .positions
            .values()
            .filter(|p| p.trade_data.symbol_id == id)
            .map(|p| contract.lots_of_volume(p.trade_data.volume))
            .sum();
        let spread = self
            .quotes
            .get(id)
            .and_then(|s| Some((s.ask? - s.bid?, to_price(1))));
        let facts = OrderFacts {
            lots: contract.lots_of_volume(volume),
            symbol_lots: held,
            has_stop: stop_loss.is_some(),
            spread_pips: spread.map(|(gap, unit)| gap as f64 * unit / contract.pip()),
            ..OrderFacts::default()
        };
        let standing = Standing {
            open_positions: self.book.positions.len(),
            day_start_balance: self.book.balance(),
            now: now_ms(),
            ..Standing::default()
        };
        let money = |amount: f64| self.money(amount);
        match guard::check(&self.risk, &standing, &facts, &money) {
            Verdict::Ok => {}
            Verdict::Block(reason) => return Err(reason),
            Verdict::Warn(reasons) if !ticket.warned => {
                ticket.warned = true;
                ticket.form.warning = Some(format!("{}. Enter again to send.", reasons.join("; ")));
                return Ok(false);
            }
            Verdict::Warn(_) => {}
        }

        let Some(account) = self.account() else {
            return Err("Not connected".to_owned());
        };
        let request = NewOrderReq::market(id, ticket.side, volume)
            .with_protection(stop_loss, take_profit)
            .with_label("wyck");
        let tx = self.tx.clone();
        let name = self.book.name(id);
        tokio::spawn(async move {
            let result = account
                .trading()
                .new_order(request)
                .await
                .map(|_| format!("Order sent for {name}"))
                .map_err(|e| describe(&e).message);
            let _ = tx.send(Msg::Done(result));
        });
        Ok(true)
    }

    fn key_confirm(&mut self, confirm: Confirm, key: KeyEvent) -> Outcome {
        if !matches!(key.code, KeyCode::Char('y' | 'Y') | KeyCode::Enter) {
            return Outcome::None;
        }
        let account = self.account();
        let tx = self.tx.clone();
        match confirm {
            Confirm::SignOut => return Outcome::SignOut,
            Confirm::Close(id, volume) => {
                if let Some(account) = account {
                    tokio::spawn(async move {
                        let result = account
                            .trading()
                            .close_position(id, volume)
                            .await
                            .map(|_| format!("Closing position {id}"))
                            .map_err(|e| describe(&e).message);
                        let _ = tx.send(Msg::Done(result));
                    });
                }
            }
            Confirm::Cancel(id) => {
                if let Some(account) = account {
                    tokio::spawn(async move {
                        let result = account
                            .trading()
                            .cancel_order(id)
                            .await
                            .map(|_| format!("Cancelling order {id}"))
                            .map_err(|e| describe(&e).message);
                        let _ = tx.send(Msg::Done(result));
                    });
                }
            }
        }
        Outcome::None
    }

    // ---- drawing ----

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        let [header, tabs, body, notice, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        frame.render_widget(Paragraph::new(self.header_line()), header);
        let titles = ["1 Watchlist", "2 Positions", "3 Orders"];
        frame.render_widget(
            Tabs::new(titles)
                .select(self.tab_index())
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
            tabs,
        );
        match self.tab {
            Tab::Watchlist => self.draw_watchlist(frame, body),
            Tab::Positions => self.draw_positions(frame, body),
            Tab::Orders => self.draw_orders(frame, body),
        }
        if let Some((tone, text)) = &self.notice {
            let color = match tone {
                Tone::Success => Color::Green,
                Tone::Warning => Color::Yellow,
                Tone::Error => Color::Red,
                _ => Color::Cyan,
            };
            frame.render_widget(
                Paragraph::new(text.as_str()).style(Style::default().fg(color)),
                notice,
            );
        }
        let hints = match self.tab {
            Tab::Watchlist => "a add  d remove  b buy  s sell  o sign out  q quit",
            Tab::Positions => "x close  o sign out  q quit",
            Tab::Orders => "c cancel  o sign out  q quit",
        };
        frame.render_widget(
            Paragraph::new(format!("{hints}   Tab/1-3 switch  j/k move"))
                .style(Style::default().fg(Color::DarkGray)),
            footer,
        );
        self.draw_popup(frame, area);
    }

    fn header_line(&self) -> Line<'static> {
        let (env, color) = match self.environment {
            Environment::Live => ("LIVE", Color::Red),
            Environment::Demo => ("DEMO", Color::Green),
        };
        let mut spans = vec![
            Span::styled(
                format!(" {env} "),
                Style::default().fg(Color::Black).bg(color),
            ),
            Span::raw(format!(" {}  ", self.label)),
            Span::styled(self.status.clone(), Style::default().fg(Color::Gray)),
        ];
        if self.book.trader.is_some() {
            let summary = self.book.summary(&self.quote_fn());
            spans.push(Span::raw(format!(
                "   Balance {}   Equity {}   Margin {}",
                self.money(summary.balance),
                self.money(summary.equity),
                self.money(summary.margin),
            )));
        }
        Line::from(spans)
    }

    fn table<'a>(
        &self,
        rows: Vec<Row<'a>>,
        widths: &[u16],
        header: Vec<&'a str>,
        title: &'a str,
        selected: usize,
    ) -> (Table<'a>, TableState) {
        let table = Table::new(rows, widths.iter().map(|w| Constraint::Length(*w)))
            .header(
                Row::new(header)
                    .style(Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            )
            .block(Block::default().borders(Borders::ALL).title(title))
            .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        (table, TableState::default().with_selected(Some(selected)))
    }

    fn draw_watchlist(&self, frame: &mut Frame, area: Rect) {
        if self.watch.is_empty() {
            frame.render_widget(
                Paragraph::new("No symbol yet. Press a to add one (for example EURUSD).")
                    .block(Block::default().borders(Borders::ALL).title(" Watchlist ")),
                area,
            );
            return;
        }
        let rows: Vec<Row> = self
            .watch
            .iter()
            .map(|id| {
                let contract = self.book.contract(*id);
                let spot = self.quotes.get(*id);
                let price = |raw: Option<i64>| {
                    raw.map_or_else(|| "-".to_owned(), |v| contract.format_price(to_price(v)))
                };
                let spread = spot
                    .and_then(|s| Some((s.ask? - s.bid?) as f64 * to_price(1) / contract.pip()))
                    .map_or_else(|| "-".to_owned(), |p| format!("{p:.1}"));
                Row::new(vec![
                    self.book.name(*id),
                    price(spot.and_then(|s| s.bid)),
                    price(spot.and_then(|s| s.ask)),
                    spread,
                ])
            })
            .collect();
        let (table, mut state) = self.table(
            rows,
            &[14, 14, 14, 10],
            vec!["Symbol", "Bid", "Ask", "Spread"],
            " Watchlist ",
            self.selected[0],
        );
        frame.render_stateful_widget(table, area, &mut state);
    }

    fn draw_positions(&self, frame: &mut Frame, area: Rect) {
        let quotes = self.quote_fn();
        let rows: Vec<Row> = self
            .book
            .positions
            .values()
            .map(|p| {
                let contract = self.book.contract(p.trade_data.symbol_id);
                let buy = is_buy(p.trade_data.trade_side);
                let price =
                    |v: Option<f64>| v.map_or_else(|| "-".to_owned(), |v| contract.format_price(v));
                let profit = self.book.net_profit(p.position_id, &quotes);
                let profit_text = profit.map_or_else(|| "-".to_owned(), |v| self.money(v));
                let color = match profit {
                    Some(v) if v > 0.0 => Color::Green,
                    Some(v) if v < 0.0 => Color::Red,
                    _ => Color::Reset,
                };
                Row::new(vec![
                    p.position_id.to_string(),
                    self.book.name(p.trade_data.symbol_id),
                    if buy { "Buy" } else { "Sell" }.to_owned(),
                    format_lots(contract.lots_of_volume(p.trade_data.volume)),
                    price(p.price),
                    price(p.stop_loss),
                    price(p.take_profit),
                    profit_text,
                ])
                .style(Style::default().fg(color))
            })
            .collect();
        let header = vec!["Id", "Symbol", "Side", "Lots", "Entry", "SL", "TP", "P/L"];
        let (table, mut state) = self.table(
            rows,
            &[12, 12, 6, 8, 12, 12, 12, 16],
            header,
            " Positions ",
            self.selected[1],
        );
        frame.render_stateful_widget(table, area, &mut state);
    }

    fn draw_orders(&self, frame: &mut Frame, area: Rect) {
        let rows: Vec<Row> = self
            .book
            .orders
            .values()
            .map(|o| {
                let contract = self.book.contract(o.trade_data.symbol_id);
                let price = o.limit_price.or(o.stop_price);
                Row::new(vec![
                    o.order_id.to_string(),
                    self.book.name(o.trade_data.symbol_id),
                    if is_buy(o.trade_data.trade_side) {
                        "Buy"
                    } else {
                        "Sell"
                    }
                    .to_owned(),
                    o.kind().map_or("order", |k| k.label()).to_owned(),
                    format_lots(contract.lots_of_volume(o.trade_data.volume)),
                    price.map_or_else(|| "-".to_owned(), |p| contract.format_price(p)),
                ])
            })
            .collect();
        let header = vec!["Id", "Symbol", "Side", "Type", "Lots", "Price"];
        let (table, mut state) = self.table(
            rows,
            &[12, 12, 6, 14, 8, 12],
            header,
            " Orders ",
            self.selected[2],
        );
        frame.render_stateful_widget(table, area, &mut state);
    }

    fn draw_popup(&self, frame: &mut Frame, area: Rect) {
        match &self.popup {
            Popup::None => {}
            Popup::Add(form) => form.render(frame, area, 50),
            Popup::Ticket(ticket) => ticket.form.render(frame, area, 60),
            Popup::Confirm(confirm) => {
                let text = match confirm {
                    Confirm::Close(id, _) => format!("Close position {id}? (y/n)"),
                    Confirm::Cancel(id) => format!("Cancel order {id}? (y/n)"),
                    Confirm::SignOut => {
                        "Sign out and forget the saved connection? (y/n)".to_owned()
                    }
                };
                let rect = centered(area, 56, 3);
                frame.render_widget(Clear, rect);
                frame.render_widget(
                    Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
                    rect,
                );
            }
        }
    }
}

async fn load(session: Session, wanted: Vec<String>) -> Result<Loaded, String> {
    let client = session
        .wait_ready(Duration::from_secs(30))
        .await
        .map_err(err)?;
    let account = client.account(session.account_id());
    let market = account.market();
    let data = account.account_data();

    let symbols = market.symbols().await.map_err(err)?;
    let trader = data.trader().await.map_err(err)?;
    let (positions, orders) = data.open_positions_and_orders(false).await.map_err(err)?;

    let mut ids = BTreeSet::new();
    ids.extend(positions.iter().map(|p| p.trade_data.symbol_id));
    ids.extend(orders.iter().map(|o| o.trade_data.symbol_id));
    let mut watch = Vec::new();
    for name in &wanted {
        let found = symbols.iter().find(|s| {
            s.symbol_name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        });
        if let Some(symbol) = found {
            ids.insert(symbol.symbol_id);
            watch.push(symbol.symbol_id);
        }
    }
    let ids: Vec<i64> = ids.into_iter().collect();
    let mut details = Vec::new();
    if !ids.is_empty() {
        details = market.symbol_details(&ids).await.map_err(err)?;
        session.subscribe_spots(&ids).await.map_err(err)?;
    }
    let currency = match trader.deposit_asset_id {
        Some(asset_id) => market
            .assets()
            .await
            .ok()
            .and_then(|assets| assets.into_iter().find(|a| a.asset_id == asset_id))
            .map(|a| a.name)
            .unwrap_or_default(),
        None => String::new(),
    };
    Ok(Loaded {
        symbols,
        trader,
        positions,
        orders,
        details,
        watch,
        currency,
    })
}

async fn add_symbol(session: Session, id: i64, name: String) -> Result<(Symbol, String), String> {
    let client = session.client().ok_or_else(|| "Not connected".to_owned())?;
    let market = client.account(session.account_id()).market();
    let symbol = market
        .symbol_details(&[id])
        .await
        .map_err(err)?
        .into_iter()
        .next()
        .ok_or_else(|| format!("The broker gave no details for {name}"))?;
    session.subscribe_spots(&[id]).await.map_err(err)?;
    Ok((symbol, name))
}
