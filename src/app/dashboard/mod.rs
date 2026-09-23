//! The dashboard: the screen the app lands on once an account is connected.
//!
//! For now that is the header bar (the symbol, its live price, the connection state, the account
//! and the window controls) and the symbol picker that opens from it. The chart and the trading
//! panels are not built yet.
//!
//! A [`Dashboard`] owns the [`Session`] for its account: the session reconnects, renews the
//! tokens and restores the price subscription by itself, and the dashboard only follows its events.
//! The two things the rest of the app has to act on come out as a [`DashboardEvent`].

mod catalog;
mod details;
mod header;
mod marks;
mod picker;

use std::collections::HashMap;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, KeyBinding, SharedString, Window, div, px,
};
use gpui_kit::assets::IconName;
use tokio::sync::broadcast::error::RecvError;
use wyck::openapi::market::{PRICE_SCALE, Spot, format_price};
use wyck::openapi::session::{Session, SessionEvent, SessionState};
use wyck::openapi::{Event, OpenApiError};

use self::catalog::{Catalog, Entry};
use self::picker::Picker;
use super::connection::ui;
use super::{runtime, theme};

gpui::actions!(
    wyck_dashboard,
    [
        OpenPicker,
        ClosePicker,
        PickerUp,
        PickerDown,
        PickerPageUp,
        PickerPageDown,
        PickerConfirm,
    ]
);

/// Registers the dashboard's key bindings. Call once, at startup.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-k", OpenPicker, Some("Dashboard")),
        KeyBinding::new("escape", ClosePicker, Some("Dashboard")),
        KeyBinding::new("up", PickerUp, Some("SymbolPicker")),
        KeyBinding::new("down", PickerDown, Some("SymbolPicker")),
        KeyBinding::new("ctrl-p", PickerUp, Some("SymbolPicker")),
        KeyBinding::new("ctrl-n", PickerDown, Some("SymbolPicker")),
        KeyBinding::new("pageup", PickerPageUp, Some("SymbolPicker")),
        KeyBinding::new("pagedown", PickerPageDown, Some("SymbolPicker")),
        KeyBinding::new("enter", PickerConfirm, Some("SymbolPicker")),
    ]);
}

/// Who the dashboard is connected as.
#[derive(Clone)]
pub struct AccountInfo {
    pub label: SharedString,
    pub is_live: bool,
}

/// What the rest of the app has to act on.
pub enum DashboardEvent {
    /// The user picked another symbol; remember it for the next start.
    SymbolChosen(String),
    /// The user asked to disconnect this account and forget its sign-in.
    Disconnect,
    /// The session ended for a reason only a new sign-in fixes.
    SignInAgain,
}

impl EventEmitter<DashboardEvent> for Dashboard {}

/// Where the connection stands, as the header shows it.
enum Conn {
    Connecting,
    Ready,
    Reconnecting,
    Failed(SharedString),
}

/// The list of symbols.
enum Load {
    Loading,
    Ready(Arc<Catalog>),
    Failed(SharedString),
}

/// The symbol the dashboard is on.
#[derive(Clone)]
struct Active {
    entry: Entry,
    /// Decimals the symbol is quoted with, and where its pip sits. Until the broker's details for
    /// the symbol arrive these are the common forex values.
    digits: u32,
    pip_position: i64,
}

#[derive(Default, Clone, Copy)]
struct Quote {
    bid: Option<i64>,
    ask: Option<i64>,
}

#[derive(Clone, Copy, PartialEq)]
enum Tick {
    Up,
    Down,
}

/// The symbol the picker is showing, and the prices it has had since: the dashboard follows its
/// price for as long as the picker highlights it.
struct Peek {
    id: i64,
    quote: Quote,
}

pub struct Dashboard {
    session: Session,
    account: AccountInfo,
    focus_handle: FocusHandle,
    focused: bool,
    conn: Conn,
    catalog: Load,
    catalog_requested: bool,
    /// The symbol to open on once the list is loaded: the one remembered from last time.
    initial_symbol: Option<String>,
    active: Option<Active>,
    quote: Quote,
    tick: Option<Tick>,
    picker: Option<Picker>,
    /// What the broker said about the symbols the picker highlighted, kept while the app runs.
    details: HashMap<i64, details::Detail>,
    peek: Option<Peek>,
    /// How many times the picker has been opened, so its entrance animation replays.
    picker_opens: u64,
    menu_open: bool,
}

impl Dashboard {
    pub fn new(
        session: Session,
        account: AccountInfo,
        initial_symbol: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut dashboard = Self {
            session,
            account,
            focus_handle: cx.focus_handle(),
            focused: false,
            conn: Conn::Connecting,
            catalog: Load::Loading,
            catalog_requested: false,
            initial_symbol,
            active: None,
            quote: Quote::default(),
            tick: None,
            picker: None,
            details: HashMap::new(),
            peek: None,
            picker_opens: 0,
            menu_open: false,
        };
        dashboard.follow_session(cx);
        dashboard
    }

    /// Stops the session: no more reconnects, and the connection closes.
    pub fn stop(&self) {
        let session = self.session.clone();
        runtime::spawn(async move { session.stop().await });
    }

    /// Feeds the session's events into this view, for as long as it lives.
    fn follow_session(&mut self, cx: &mut Context<Self>) {
        let mut events = self.session.events();
        if matches!(*self.session.state().borrow(), SessionState::Ready) {
            self.on_ready(cx);
        }
        cx.spawn(async move |this, cx| {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        if this
                            .update(cx, |this, cx| this.on_event(event, cx))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                }
            }
        })
        .detach();
    }

    fn on_event(&mut self, event: SessionEvent, cx: &mut Context<Self>) {
        match event {
            SessionEvent::Ready => self.on_ready(cx),
            SessionEvent::Data(Event::Spot(spot)) => self.apply_spot(Spot::from(&spot), cx),
            SessionEvent::Reconnecting { attempt, .. } => {
                tracing::debug!(attempt, "the session is reconnecting");
                self.conn = Conn::Reconnecting;
                cx.notify();
            }
            SessionEvent::Failed(error) => {
                tracing::warn!(%error, "the session failed");
                self.conn = Conn::Failed(failure_message(&error).into());
                cx.notify();
            }
            _ => {}
        }
    }

    fn on_ready(&mut self, cx: &mut Context<Self>) {
        self.conn = Conn::Ready;
        if !self.catalog_requested {
            self.catalog_requested = true;
            self.load_catalog(cx);
        }
        cx.notify();
    }

    fn load_catalog(&mut self, cx: &mut Context<Self>) {
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let loaded = runtime::spawn(async move {
                let client = session.client().ok_or(OpenApiError::Closed)?;
                let account = client.account(session.account_id());
                let market = account.market();
                let symbols = market.symbols().await?;
                let categories = market.symbol_categories().await?;
                let classes = market.asset_classes().await?;
                let assets = market.assets().await?;
                Ok::<_, OpenApiError>(Catalog::build(symbols, categories, classes, assets))
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                match loaded {
                    Ok(Ok(catalog)) => this.catalog_loaded(Arc::new(catalog), cx),
                    Ok(Err(error)) => {
                        tracing::warn!(%error, "could not load the symbol list");
                        this.catalog = Load::Failed(error.to_string().into());
                        // Ask again the next time the session comes back.
                        this.catalog_requested = false;
                    }
                    Err(_) => {
                        this.catalog = Load::Failed("the background runtime stopped".into());
                        this.catalog_requested = false;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn catalog_loaded(&mut self, catalog: Arc<Catalog>, cx: &mut Context<Self>) {
        let remembered = self
            .initial_symbol
            .take()
            .and_then(|name| catalog.by_name(&name).cloned());
        let start = remembered
            .or_else(|| catalog.by_name("EURUSD").cloned())
            .or_else(|| catalog.entry(0).cloned());
        self.catalog = Load::Ready(catalog);
        if let Some(entry) = start {
            self.select(entry, false, cx);
        }
    }

    /// Makes `entry` the symbol the dashboard is on: follows its price, reads how it is quoted,
    /// and (when the user picked it) tells the app to remember it.
    fn select(&mut self, entry: Entry, remember: bool, cx: &mut Context<Self>) {
        let previous = self.active.as_ref().map(|active| active.entry.id);
        if previous == Some(entry.id) {
            return;
        }
        let id = entry.id;
        if remember {
            cx.emit(DashboardEvent::SymbolChosen(entry.name.clone()));
        }
        self.active = Some(Active {
            entry,
            digits: 5,
            pip_position: 4,
        });
        self.quote = Quote::default();
        self.tick = None;
        cx.notify();

        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let details = runtime::spawn(async move {
                if let Some(previous) = previous {
                    let _ = session.unsubscribe_spots(&[previous]).await;
                }
                if let Err(error) = session.subscribe_spots(&[id]).await {
                    tracing::warn!(%error, symbol_id = id, "could not follow the symbol's price");
                }
                let client = session.client()?;
                let account = client.account(session.account_id());
                account.market().symbol_details(&[id]).await.ok()
            })
            .await;
            let Ok(Some(details)) = details else { return };
            let Some(details) = details.into_iter().find(|d| d.symbol_id == id) else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if let Some(active) = this.active.as_mut().filter(|a| a.entry.id == id) {
                    active.digits = u32::try_from(details.digits).unwrap_or(5);
                    active.pip_position = details.pip_position;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn apply_spot(&mut self, spot: Spot, cx: &mut Context<Self>) {
        // A symbol the picker is showing (not the one being followed) has its own quote.
        if let Some(peek) = self.peek.as_mut().filter(|peek| peek.id == spot.symbol_id) {
            peek.quote.bid = spot.bid.or(peek.quote.bid);
            peek.quote.ask = spot.ask.or(peek.quote.ask);
            cx.notify();
        }
        if self.active.as_ref().map(|a| a.entry.id) != Some(spot.symbol_id) {
            return;
        }
        let previous = self.quote.bid;
        // An event only carries what changed.
        self.quote.bid = spot.bid.or(self.quote.bid);
        self.quote.ask = spot.ask.or(self.quote.ask);
        if let (Some(before), Some(now)) = (previous, self.quote.bid) {
            if now > before {
                self.tick = Some(Tick::Up);
            } else if now < before {
                self.tick = Some(Tick::Down);
            }
        }
        cx.notify();
    }

    /// The bid, the ask and the spread in pips, as the followed symbol quotes them.
    fn price_text(&self) -> Option<details::Live> {
        let active = self.active.as_ref()?;
        quote_text(self.quote, active.digits, active.pip_position)
    }

    /// The same for the symbol the picker highlights, once its details say how it is quoted.
    fn peek_text(&self, id: i64) -> Option<details::Live> {
        let peek = self.peek.as_ref().filter(|peek| peek.id == id)?;
        let Some(details::Detail::Ready(symbol)) = self.details.get(&id) else {
            return None;
        };
        let digits = u32::try_from(symbol.digits).unwrap_or(5);
        quote_text(peek.quote, digits, symbol.pip_position)
    }

    /// Stops following the price of the symbol the picker was showing.
    fn drop_peek(&mut self) {
        let Some(peek) = self.peek.take() else {
            return;
        };
        if self.active.as_ref().map(|a| a.entry.id) == Some(peek.id) {
            return;
        }
        let session = self.session.clone();
        runtime::spawn(async move {
            if let Some(client) = session.client() {
                let account = client.account(session.account_id());
                let _ = account.market().unsubscribe_spots(&[peek.id]).await;
            }
        });
    }

    fn body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let centered = div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .px_6();
        match &self.conn {
            Conn::Failed(message) => centered
                .child(ui::icon_colored(
                    IconName::CircleAlert,
                    28.,
                    theme::destructive(),
                ))
                .child(
                    div()
                        .text_size(px(16.))
                        .text_color(theme::fg())
                        .child("The connection to cTrader ended"),
                )
                .child(
                    div()
                        .max_w(px(460.))
                        .text_center()
                        .text_size(px(13.))
                        .text_color(theme::muted_fg())
                        .child(message.clone()),
                )
                .child(div().pt_2().w(px(260.)).child(ui::primary_button(
                    "sign-in-again",
                    "Sign in again",
                    cx.listener(|_this, _event, _window, cx| {
                        cx.emit(DashboardEvent::SignInAgain);
                    }),
                )))
                .into_any_element(),
            _ => centered
                .child(ui::icon_colored(
                    IconName::ChartCandlestick,
                    28.,
                    theme::muted_fg(),
                ))
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(theme::muted_fg())
                        .child("The chart and trading panels will appear here."),
                )
                .into_any_element(),
        }
    }
}

/// What to tell the user about a session that ended for good.
fn failure_message(error: &OpenApiError) -> String {
    format!("{error}. Sign in again to keep going.")
}

impl Focusable for Dashboard {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Dashboard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focused {
            self.focused = true;
            window.focus(&self.focus_handle, cx);
        }
        let picker = self.render_picker(window, cx);
        let header = self.render_header(window, cx).into_any_element();
        let body = self.body(cx).into_any_element();
        let menu = self.render_menu(cx);

        div()
            .key_context("Dashboard")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &OpenPicker, window, cx| {
                this.open_picker(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ClosePicker, window, cx| {
                this.close_overlays(window, cx);
            }))
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .w_full()
            .h_full()
            .bg(theme::bg())
            .child(header)
            .child(body)
            .children(menu)
            .children(picker)
    }
}

/// The bid, the ask and the spread in pips of a quote, formatted for a symbol quoted with `digits`
/// decimals and a pip at `pip_position`.
fn quote_text(quote: Quote, digits: u32, pip_position: i64) -> Option<details::Live> {
    let bid = quote.bid?;
    let spread = quote.ask.map(|ask| {
        let pips = (ask - bid) as f64 / PRICE_SCALE as f64
            * 10f64.powi(i32::try_from(pip_position).unwrap_or(4));
        format!("{pips:.1}")
    });
    Some((
        format_price(bid, digits),
        quote.ask.map(|ask| format_price(ask, digits)),
        spread,
    ))
}
