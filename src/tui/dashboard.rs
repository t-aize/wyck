use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Display;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use super::Msg;
use super::form::{Field, Form, FormAction, centered};
use super::{console::Console, theme};
use crate::config::DocumentStore;
use crate::openapi::account::book::{AccountBook, Tone, describe, is_buy};
use crate::openapi::account::{Order, Position, PositionUnrealizedPnL, TradeSide, Trader};
use crate::openapi::market::{LightSymbol, SpotTracker, Symbol, to_price};
use crate::openapi::session::{Session, SessionEvent};
use crate::openapi::trading::contract::{Contract, format_lots};
use crate::openapi::{Environment, Event};
use crate::trading::guard::{self, OrderFacts, RiskPrefs, Standing, Verdict};
use crate::trading::math::format_money;

const WATCHLIST: &str = "watchlist";
const RISK: &str = "risk";
const PNL_EVERY_TICKS: u64 = 20;

mod command;
mod preview;
mod view;

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
    Console,
}

const TABS: [Tab; 4] = [Tab::Watchlist, Tab::Positions, Tab::Orders, Tab::Console];

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
    Quit,
}

#[derive(Clone)]
pub(super) struct Sender {
    tx: UnboundedSender<Msg>,
    generation: u64,
}

impl Sender {
    pub(super) fn new(tx: UnboundedSender<Msg>, generation: u64) -> Self {
        Self { tx, generation }
    }

    pub(super) fn send(&self, msg: Msg) -> Result<(), ()> {
        self.tx
            .send(Msg::Scoped(self.generation, Box::new(msg)))
            .map_err(|_| ())
    }
}

pub struct Dashboard {
    label: String,
    environment: Environment,
    session: Option<Session>,
    docs: Option<DocumentStore>,
    risk: RiskPrefs,
    status: String,
    symbols: Vec<LightSymbol>,
    book: AccountBook,
    quotes: SpotTracker,
    price_history: BTreeMap<i64, VecDeque<i64>>,
    spread_history: BTreeMap<i64, VecDeque<i64>>,
    watch: Vec<i64>,
    wanted: Vec<String>,
    tab: Tab,
    selected: [usize; 4],
    popup: Popup,
    console: Console,
    loading: bool,
    load_id: u64,
    busy: bool,
    tx: Sender,
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
        tx: Sender,
    ) -> Self {
        let wanted = docs.load_or_default::<Watchlist>(WATCHLIST).symbols;
        let risk = docs.load_or_default::<RiskPrefs>(RISK).normalized();
        Self {
            label,
            environment,
            session: Some(session),
            docs: Some(docs),
            risk,
            status: "Connecting...".to_owned(),
            symbols: Vec::new(),
            book: AccountBook::default(),
            quotes: SpotTracker::new(),
            price_history: BTreeMap::new(),
            spread_history: BTreeMap::new(),
            watch: Vec::new(),
            wanted,
            tab: Tab::Watchlist,
            selected: [0; 4],
            popup: Popup::None,
            console: Console::default(),
            loading: false,
            load_id: 0,
            busy: false,
            tx,
        }
    }

    pub async fn stop(&self) {
        if let Some(session) = &self.session {
            session.stop().await;
        }
    }

    pub fn session(&self) -> Option<Session> {
        self.session.clone()
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.popup, Popup::None) && !self.console.focused
    }

    pub fn cancel_input(&mut self) -> bool {
        if !matches!(self.popup, Popup::None) {
            self.popup = Popup::None;
            return true;
        }
        if !self.console.input.text().is_empty() {
            self.console.input.clear();
            return true;
        }
        false
    }

    pub fn paste(&mut self, text: &str) {
        match &mut self.popup {
            Popup::Add(form) => form.paste(text),
            Popup::Ticket(ticket) => ticket.form.paste(text),
            Popup::None => self.console.paste(text),
            Popup::Confirm(_) => {}
        }
    }

    fn account(&self) -> Option<crate::openapi::AccountClient> {
        let session = self.session.as_ref()?;
        Some(session.client()?.account(session.account_id()))
    }

    fn notify(&mut self, tone: Tone, text: impl Into<String>) {
        self.console.push(tone, text);
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

    fn spawn_load(&mut self) {
        if self.loading {
            return;
        }
        let Some(session) = self.session.clone() else {
            return;
        };
        self.loading = true;
        self.load_id = self.load_id.wrapping_add(1);
        let load_id = self.load_id;
        let wanted = self.wanted.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(Msg::Loaded(load_id, Box::new(load(session, wanted).await)));
        });
    }

    pub fn on_loaded(&mut self, id: u64, loaded: Result<Loaded, String>) {
        if id != self.load_id {
            return;
        }
        self.loading = false;
        let loaded = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                self.notify(Tone::Error, format!("Could not load the account: {error}"));
                self.status = "Load failed".to_owned();
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
        self.busy = false;
        match result {
            Ok(text) => self.notify(Tone::Success, text),
            Err(text) => self.notify(Tone::Error, text),
        }
    }

    pub fn on_added(&mut self, result: Result<(Symbol, String), String>) {
        self.busy = false;
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
        if let Some(docs) = &self.docs
            && let Err(error) = docs.save(WATCHLIST, &Watchlist { symbols })
        {
            self.notify(
                Tone::Warning,
                format!("Could not save the watchlist: {error}"),
            );
        }
    }

    // ---- session events ----

    pub fn on_session(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Ready => {
                self.quotes.clear();
                self.price_history.clear();
                self.spread_history.clear();
                self.status = "Loading...".to_owned();
                self.spawn_load();
            }
            SessionEvent::Reconnecting {
                attempt, retry_in, ..
            } => {
                self.loading = false;
                self.load_id = self.load_id.wrapping_add(1);
                self.quotes.clear();
                self.price_history.clear();
                self.spread_history.clear();
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
                    format!("Session ended: {error}. Use /logout and sign in again."),
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
                let quote = self.quotes.apply(&spot);
                if let Some(bid) = quote.bid {
                    let history = self.price_history.entry(quote.symbol_id).or_default();
                    if history.back() != Some(&bid) {
                        history.push_back(bid);
                    }
                    if history.len() > 20 {
                        history.pop_front();
                    }
                }
                if let (Some(bid), Some(ask)) = (quote.bid, quote.ask) {
                    let history = self.spread_history.entry(quote.symbol_id).or_default();
                    history.push_back(ask.saturating_sub(bid));
                    if history.len() > 20 {
                        history.pop_front();
                    }
                }
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
        if key.code == KeyCode::F(6) || key.code == KeyCode::BackTab {
            self.console.focused = !self.console.focused;
            return Outcome::None;
        }
        if key.code == KeyCode::Esc {
            if self.console.input.text().is_empty() {
                self.console.focused = !self.console.focused;
            } else {
                self.console.input.clear();
            }
            return Outcome::None;
        }
        if matches!(key.code, KeyCode::F(1..=4)) {
            if let KeyCode::F(n) = key.code {
                self.tab = TABS[usize::from(n - 1)];
            }
            return Outcome::None;
        }
        if self.console.focused || matches!(key.code, KeyCode::PageUp | KeyCode::PageDown) {
            if let Some(text) = self.console.key(key) {
                return self.execute_command(&text);
            }
            return Outcome::None;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Outcome::None;
        }
        match key.code {
            KeyCode::Char('1') => self.tab = Tab::Watchlist,
            KeyCode::Char('2') => self.tab = Tab::Positions,
            KeyCode::Char('3') => self.tab = Tab::Orders,
            KeyCode::Char('4') => self.tab = Tab::Console,
            KeyCode::Char('/') => {
                self.console.focused = true;
                self.console.input.set("/");
            }
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
            Tab::Console => 0,
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
                Tab::Console => 0,
            };
            self.selected[index] = self.selected[index].min(count.saturating_sub(1));
        }
    }

    fn remove_selected(&mut self) {
        if self.loading || self.busy {
            self.notify(Tone::Warning, "Wait for the current operation to finish");
            return;
        }
        let index = self.selected[0];
        if index >= self.watch.len() {
            return;
        }
        let id = self.watch.remove(index);
        self.save_watchlist();
        self.clamp();
        if self.book.symbols().contains(&id) {
            return;
        }
        self.quotes.forget(id);
        self.price_history.remove(&id);
        self.spread_history.remove(&id);
        let Some(session) = self.session.clone() else {
            return;
        };
        tokio::spawn(async move {
            let _ = session.unsubscribe_spots(&[id]).await;
        });
    }

    fn key_add(&mut self, mut form: Form, key: KeyEvent) {
        match form.key(key) {
            FormAction::Cancel => {}
            FormAction::None => self.popup = Popup::Add(form),
            FormAction::Submit => {
                if self.busy || self.loading {
                    form.error = Some("Wait for the current operation to finish".to_owned());
                    self.popup = Popup::Add(form);
                    return;
                }
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
                if self.watch.contains(&id) {
                    self.notify(Tone::Info, format!("{name} is already in the watchlist"));
                    return;
                }
                let Some(session) = self.session.clone() else {
                    self.notify(Tone::Info, "Preview: symbols are sample data");
                    return;
                };
                self.busy = true;
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
        if self.session.is_none() {
            return Err("Preview: trading is disabled".to_owned());
        }
        if self.busy {
            return Err("Wait for the current operation to finish".to_owned());
        }
        if self.status != "Connected" {
            return Err("Wait for the account to connect and load".to_owned());
        }
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
                .ok()
                .filter(|p| p.is_finite() && *p > 0.0)
                .map(Some)
                .ok_or_else(|| format!("{what} must be a finite price above zero"))
        };
        let stop_loss = price(ticket.form.value(1), "Stop loss")?;
        let take_profit = price(ticket.form.value(2), "Take profit")?;

        let id = ticket.symbol_id;
        let contract = self.book.contract(id);
        let quote = self.quotes.get(id);
        let request = super::ticket::MarketDraft {
            symbol_id: id,
            side: ticket.side,
            lots,
            stop_loss,
            take_profit,
        }
        .request(
            contract,
            quote.and_then(|s| s.bid).map(to_price),
            quote.and_then(|s| s.ask).map(to_price),
        )?;
        let volume = request.volume;
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
            return Err("Preview: trading is disabled".to_owned());
        };
        self.busy = true;
        self.notify(Tone::Info, "Sending order...");
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
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('n' | 'N')) {
            return Outcome::None;
        }
        if !matches!(key.code, KeyCode::Char('y' | 'Y') | KeyCode::Enter) {
            self.popup = Popup::Confirm(confirm);
            return Outcome::None;
        }
        if matches!(confirm, Confirm::SignOut) {
            if self.session.is_none() {
                self.notify(Tone::Info, "Preview: no saved connection");
                return Outcome::None;
            }
            return Outcome::SignOut;
        }
        if self.session.is_none() {
            self.notify(Tone::Error, "Preview: trading is disabled");
            return Outcome::None;
        }
        if self.busy || self.status != "Connected" {
            self.notify(Tone::Warning, "Wait for the account and current operation");
            self.popup = Popup::Confirm(confirm);
            return Outcome::None;
        }
        let account = self.account();
        if account.is_none() {
            self.notify(Tone::Error, "Preview: trading is disabled");
            return Outcome::None;
        }
        self.busy = true;
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
