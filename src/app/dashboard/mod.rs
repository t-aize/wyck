//! The dashboard: the screen the app lands on once an account is connected.
//!
//! That is the header bar (the symbol of the active chart, its live price, the timeframes, the
//! connection state, the account and the window controls), the symbol picker that opens from it,
//! and the charts.
//!
//! A [`Dashboard`] owns the [`Session`] for its account: the session reconnects, renews the
//! tokens and restores the price subscription by itself, and the dashboard only follows its events.
//! The two things the rest of the app has to act on come out as a [`DashboardEvent`].

mod catalog;
mod details;
mod header;
mod layout_menu;
mod lists;
mod marks;
mod picker;
mod trade;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding, SharedString, Window,
    deferred, div, px,
};
use gpui_kit::assets::IconName;
use tokio::sync::broadcast::error::RecvError;
use wyck::openapi::market::{PRICE_SCALE, Spot, format_price};
use wyck::openapi::session::{Session, SessionEvent, SessionState};
use wyck::openapi::{Event, OpenApiError};

use self::catalog::{Catalog, Entry};
use self::picker::Picker;
use super::alerts::Alerts;
use super::chart::drawing::Drawings;
use super::chart::live::{PEEK_OWNER, Wish};
use super::chart::{self, Chart, LiveHub};
use super::connection::ui;
use super::multichart::{MultiChart, MultiChartEvent, SymbolRef};
use super::trading::account::Account;
use super::trading::panel::AccountPanel;
use super::trading::ticket::OrderTicket;
use super::workspace::{Documents, Workspace};
use super::{runtime, theme, toast};

gpui::actions!(
    wyck_dashboard,
    [
        OpenPicker,
        OpenSettings,
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
        KeyBinding::new("secondary-,", OpenSettings, Some("Dashboard")),
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
    /// Every price and live bar subscription goes through it.
    hub: Rc<LiveHub>,
    /// The chart the picker chooses a symbol for.
    picker_target: Option<usize>,
    /// The last bid and ask of every symbol seen.
    quotes: HashMap<i64, Quote>,
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
    multi: Entity<MultiChart>,
    workspace: Entity<Workspace>,
    /// Whether the list of all timeframes is open.
    tf_menu_open: bool,
    /// The field of the menu where a custom timeframe is typed, made with the window, and the
    /// unit a bare number is in.
    tf_custom: Option<Entity<gpui_kit::component::input::InputState>>,
    tf_unit: chart::Unit,
    /// Whether the layout picker is open.
    layout_menu_open: bool,
    /// A chart asked for the picker: it opens at the next render, which has the window.
    pending_picker: bool,
    /// The trading account: positions, orders, balance.
    trading: Entity<Account>,
    alerts: Entity<Alerts>,
    /// The order ticket, made at the first render (its fields need the window).
    ticket: Option<Entity<OrderTicket>>,
    panel: Option<Entity<AccountPanel>>,
    ticket_open: bool,
    panel_open: bool,
    panel_height: f32,
    /// Where the pointer was while the panel's top edge is dragged.
    panel_drag: Option<f32>,
    /// What waits for the window.
    pending: Vec<trade::Pending>,
}

impl Dashboard {
    pub fn new(
        session: Session,
        account: AccountInfo,
        initial_symbol: Option<String>,
        documents: Documents,
        cx: &mut Context<Self>,
    ) -> Self {
        let workspace = cx.new(|cx| Workspace::new(&documents, cx));
        // The colors saved in the color panel live with the preferences.
        crate::app::color_picker::connect(
            workspace.read(cx).preferences().saved_colors.clone(),
            {
                let workspace = workspace.clone();
                move |colors, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace
                            .edit_preferences(cx, |prefs| prefs.saved_colors = colors.to_vec());
                    });
                }
            },
            cx,
        );
        // The header shows the favorite timeframes, so it follows the workspace.
        cx.observe(&workspace, |_this, _workspace, cx| cx.notify())
            .detach();
        let drawings = cx.new(|cx| Drawings::new(documents.account.clone(), cx));
        let hub = Rc::new(LiveHub::new(session.clone()));
        let trading = cx.new(|cx| Account::new(session.clone(), hub.clone(), cx));
        let alerts = cx.new(|cx| Alerts::new(documents.account.clone(), hub.clone(), cx));
        let panel = cx.new(|cx| AccountPanel::new(trading.clone(), alerts.clone(), cx));
        cx.subscribe(&panel, |this, _panel, event, cx| {
            this.on_panel_event(event, cx)
        })
        .detach();
        // The lines on the charts follow the account and the alerts.
        cx.observe(&trading, |this, _trading, cx| this.push_lines(cx))
            .detach();
        cx.observe(&alerts, |this, _alerts, cx| this.push_lines(cx))
            .detach();
        let prefs = workspace.read(cx).preferences().clone();
        let multi = cx.new(|cx| {
            MultiChart::new(
                session.clone(),
                hub.clone(),
                workspace.clone(),
                drawings,
                cx,
            )
        });
        cx.subscribe(&multi, |this, _multi, event: &MultiChartEvent, cx| {
            this.on_multi_event(event, cx);
        })
        .detach();
        let mut dashboard = Self {
            session,
            hub,
            picker_target: None,
            quotes: HashMap::new(),
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
            multi,
            workspace,
            tf_menu_open: false,
            tf_custom: None,
            tf_unit: chart::Unit::Minutes,
            layout_menu_open: false,
            pending_picker: false,
            trading,
            alerts,
            ticket: None,
            panel: Some(panel),
            ticket_open: prefs.ticket_open,
            panel_open: prefs.panel_open,
            panel_height: prefs.panel_height,
            panel_drag: None,
            pending: Vec::new(),
        };
        dashboard.follow_session(cx);
        dashboard
    }

    /// Runs `f` on the chart the user is working on.
    fn on_active_chart(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Chart, &mut Context<Chart>),
    ) {
        let chart = self.multi.read(cx).active_chart().clone();
        chart.update(cx, f);
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
            SessionEvent::Data(Event::Spot(spot)) => {
                self.multi.update(cx, |multi, cx| multi.on_spot(&spot, cx));
                self.alerts.update(cx, |alerts, cx| {
                    alerts.on_spot(spot.symbol_id, spot.bid, cx)
                });
                self.trading.update(cx, |account, cx| {
                    account.on_event(&Event::Spot(spot.clone()), cx)
                });
                self.apply_spot(Spot::from(&spot), cx);
            }
            SessionEvent::Data(event) => {
                self.trading
                    .update(cx, |account, cx| account.on_event(&event, cx));
            }
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
        self.multi.update(cx, |multi, cx| multi.on_ready(cx));
        self.trading.update(cx, |account, cx| account.on_ready(cx));
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
        self.catalog = Load::Ready(catalog.clone());
        let entries: Vec<&Entry> = (0..catalog.total())
            .filter_map(|i| catalog.entry(i))
            .collect();
        let names = entries.iter().map(|e| (e.id, e.name.clone())).collect();
        let quotes = entries
            .iter()
            .filter_map(|e| Some((e.id, e.quote.clone()?)))
            .collect();
        let quote_assets = entries
            .iter()
            .filter_map(|e| Some((e.id, e.quote_asset?)))
            .collect();
        self.trading.update(cx, |account, cx| {
            account.set_symbols(names, quotes, quote_assets, cx)
        });
        // Each chart gets the symbol it was saved with, or the one to start on.
        let wanted = self.multi.read(cx).wanted_symbols(cx);
        let linked = self.multi.read(cx).sync().symbol;
        for (index, name) in wanted {
            let entry = name
                .as_deref()
                .filter(|_| !linked)
                .and_then(|name| catalog.by_name(name).cloned())
                .or_else(|| start.clone());
            if let Some(entry) = entry {
                let symbol = self.symbol_ref(&entry);
                self.ensure_details(entry.id, cx);
                self.multi
                    .update(cx, |multi, cx| multi.restore_symbol(index, symbol, cx));
            }
        }
        self.refresh_active(cx);
    }

    /// Shows `entry` on the chart the picker was opened for (every chart when the symbol is
    /// linked), and (when the user picked it) tells the app to remember it.
    fn select(&mut self, entry: Entry, remember: bool, cx: &mut Context<Self>) {
        let target = self
            .picker_target
            .take()
            .unwrap_or_else(|| self.multi.read(cx).active_index());
        if remember {
            cx.emit(DashboardEvent::SymbolChosen(entry.name.clone()));
        }
        let symbol = self.symbol_ref(&entry);
        self.multi
            .update(cx, |multi, cx| multi.set_symbol(target, symbol, cx));
        self.ensure_details(entry.id, cx);
        self.refresh_active(cx);
    }

    /// A symbol of the list, with the decimals the broker gave for it (5 until it says).
    fn symbol_ref(&self, entry: &Entry) -> SymbolRef {
        let digits = match self.details.get(&entry.id) {
            Some(details::Detail::Ready(symbol)) => u32::try_from(symbol.digits).unwrap_or(5),
            _ => 5,
        };
        SymbolRef {
            id: entry.id,
            name: SharedString::from(entry.name.clone()),
            digits,
        }
    }

    /// Asks the broker how a symbol is quoted, once, and tells the charts.
    fn ensure_details(&mut self, id: i64, cx: &mut Context<Self>) {
        if matches!(
            self.details.get(&id),
            Some(details::Detail::Ready(_) | details::Detail::Loading)
        ) {
            return;
        }
        self.details.insert(id, details::Detail::Loading);
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let fetched = runtime::spawn(async move {
                let client = session.client().ok_or(OpenApiError::Closed)?;
                let account = client.account(session.account_id());
                let details = account.market().symbol_details(&[id]).await?;
                details
                    .into_iter()
                    .find(|d| d.symbol_id == id)
                    .ok_or_else(|| OpenApiError::Protocol("the broker sent no details".into()))
            })
            .await;
            let _ = this.update(cx, |this, cx| match fetched {
                Ok(Ok(symbol)) => {
                    let digits = u32::try_from(symbol.digits).unwrap_or(5);
                    let hours = wyck::openapi::market::TradingHours::from_symbol(&symbol);
                    this.multi
                        .update(cx, |multi, cx| multi.set_hours(id, hours, cx));
                    this.details.insert(id, details::Detail::Ready(symbol));
                    this.multi
                        .update(cx, |multi, cx| multi.set_digits(id, digits, cx));
                    this.refresh_active(cx);
                }
                Ok(Err(error)) => {
                    tracing::warn!(%error, symbol_id = id, "could not load a symbol's details");
                    this.details
                        .insert(id, details::Detail::Failed(error.to_string().into()));
                }
                Err(_) => {
                    this.details.remove(&id);
                }
            });
        })
        .detach();
    }

    /// The header follows the active chart: its symbol, how it is quoted, its last price.
    fn refresh_active(&mut self, cx: &mut Context<Self>) {
        let Some(symbol) = self.multi.read(cx).active_symbol(cx) else {
            self.active = None;
            cx.notify();
            return;
        };
        let entry = match &self.catalog {
            Load::Ready(catalog) => catalog.by_name(&symbol.name).cloned(),
            _ => None,
        };
        let Some(entry) = entry else {
            return;
        };
        let (digits, pip_position) = match self.details.get(&symbol.id) {
            Some(details::Detail::Ready(details)) => (
                u32::try_from(details.digits).unwrap_or(5),
                details.pip_position,
            ),
            _ => (5, 4),
        };
        if self.active.as_ref().map(|a| a.entry.id) != Some(symbol.id) {
            self.tick = None;
        }
        self.quote = self.quotes.get(&symbol.id).copied().unwrap_or_default();
        self.active = Some(Active {
            entry,
            digits,
            pip_position,
        });
        cx.notify();
    }

    fn on_multi_event(&mut self, event: &MultiChartEvent, cx: &mut Context<Self>) {
        match event {
            MultiChartEvent::PickSymbol(index) => {
                self.picker_target = Some(*index);
                self.pending_picker = true;
                cx.notify();
            }
            MultiChartEvent::ActiveChanged => self.refresh_active(cx),
            MultiChartEvent::Picture(png, name) => save_picture(png.clone(), name.clone(), cx),
            MultiChartEvent::PictureFailed(error) => {
                toast::show(
                    cx,
                    toast::Kind::Error,
                    "Could not take the picture",
                    error.clone(),
                );
            }
            MultiChartEvent::Action(symbol, action) => self.on_chart_action(symbol, action, cx),
            MultiChartEvent::LineMoved(id, price) => self.on_line_moved(*id, *price, cx),
            MultiChartEvent::LineClosed(id) => self.on_line_closed(*id, cx),
        }
    }

    fn apply_spot(&mut self, spot: Spot, cx: &mut Context<Self>) {
        let quote = self.quotes.entry(spot.symbol_id).or_default();
        quote.bid = spot.bid.or(quote.bid);
        quote.ask = spot.ask.or(quote.ask);
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
        if self.peek.take().is_some() {
            self.hub.set(PEEK_OWNER, None);
        }
    }

    /// Follows the price of the symbol the picker shows.
    pub(super) fn peek_at(&mut self, id: i64) {
        self.peek = Some(Peek {
            id,
            quote: self.quotes.get(&id).copied().unwrap_or_default(),
        });
        self.hub.set(PEEK_OWNER, Some(Wish::spots([id])));
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
            _ => self
                .trading_layout(self.multi.clone().into_any_element(), cx)
                .into_any_element(),
        }
    }
}

/// Saves a picture of a chart in the pictures folder (under `Wyck`) and puts it on the
/// clipboard, then says where it went.
fn save_picture(png: Vec<u8>, name: String, cx: &mut Context<Dashboard>) {
    cx.write_to_clipboard(gpui::ClipboardItem::new_image(&gpui::Image::from_bytes(
        gpui::ImageFormat::Png,
        png.clone(),
    )));
    let folder = directories::UserDirs::new()
        .and_then(|dirs| dirs.picture_dir().map(|p| p.to_path_buf()))
        .or_else(|| directories::UserDirs::new().map(|dirs| dirs.home_dir().to_path_buf()))
        .map(|dir| dir.join("Wyck"));
    let Some(folder) = folder else {
        toast::show(
            cx,
            toast::Kind::Success,
            "Picture copied",
            "The chart is on the clipboard.",
        );
        return;
    };
    cx.spawn(async move |_this, cx| {
        let path = folder.join(&name);
        let written = cx
            .background_executor()
            .spawn(async move {
                std::fs::create_dir_all(&folder)?;
                std::fs::write(&path, png)?;
                Ok::<_, std::io::Error>(path)
            })
            .await;
        cx.update(|cx| match written {
            Ok(path) => toast::show(
                cx,
                toast::Kind::Success,
                "Picture saved and copied",
                path.display().to_string(),
            ),
            Err(error) => toast::show(
                cx,
                toast::Kind::Warning,
                "Picture copied, not saved",
                error.to_string(),
            ),
        });
    })
    .detach();
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
        if std::mem::take(&mut self.pending_picker) {
            self.open_picker(window, cx);
        }
        self.trading_frame(window, cx);
        let picker = self.render_picker(window, cx);
        let header = self.render_header(window, cx).into_any_element();
        let body = self.body(cx).into_any_element();
        let menu = self.render_menu(cx);
        let menu_backdrop = (self.tf_menu_open || self.layout_menu_open).then(|| {
            deferred(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .occlude()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.tf_menu_open = false;
                            this.layout_menu_open = false;
                            cx.notify();
                        }),
                    ),
            )
            .with_priority(0)
        });

        // A second context name while a drawing is being made turns on the keys for that.
        let context = if self.multi.read(cx).drawing_in_progress(cx) {
            "Dashboard DrawingInProgress"
        } else {
            "Dashboard"
        };
        div()
            .key_context(context)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                crate::app::settings_hub::open(
                    this.workspace.clone(),
                    this.multi.clone(),
                    window,
                    cx,
                );
            }))
            .on_action(cx.listener(|this, _: &OpenPicker, window, cx| {
                this.open_picker(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ClosePicker, window, cx| {
                // Escape gives up a drawing in progress before it closes anything else.
                let nothing_open = this.picker.is_none()
                    && !this.menu_open
                    && !this.tf_menu_open
                    && !this.layout_menu_open;
                if nothing_open && this.multi.update(cx, |multi, cx| multi.cancel_drawing(cx)) {
                    // The field that had the keyboard may be gone with the selection.
                    window.focus(&this.focus_handle, cx);
                    return;
                }
                this.close_overlays(window, cx);
            }))
            .on_action(cx.listener(|this, _: &chart::DeleteDrawing, window, cx| {
                this.multi.update(cx, |multi, cx| multi.delete_drawing(cx));
                window.focus(&this.focus_handle, cx);
            }))
            .on_action(cx.listener(|this, _: &chart::FinishDrawing, _window, cx| {
                // Only an arrow path ends on Enter: for anything else the key goes on.
                if !this.multi.update(cx, |multi, cx| multi.finish_drawing(cx)) {
                    cx.propagate();
                }
            }))
            // Alt and a number picks the favorite tool at that place in the bar.
            .on_action(cx.listener(|this, _: &chart::Favorite1, _window, cx| {
                this.multi
                    .update(cx, |multi, cx| multi.pick_favorite(0, cx));
            }))
            .on_action(cx.listener(|this, _: &chart::Favorite2, _window, cx| {
                this.multi
                    .update(cx, |multi, cx| multi.pick_favorite(1, cx));
            }))
            .on_action(cx.listener(|this, _: &chart::Favorite3, _window, cx| {
                this.multi
                    .update(cx, |multi, cx| multi.pick_favorite(2, cx));
            }))
            .on_action(cx.listener(|this, _: &chart::Favorite4, _window, cx| {
                this.multi
                    .update(cx, |multi, cx| multi.pick_favorite(3, cx));
            }))
            .on_action(cx.listener(|this, _: &chart::Favorite5, _window, cx| {
                this.multi
                    .update(cx, |multi, cx| multi.pick_favorite(4, cx));
            }))
            .on_action(cx.listener(|this, _: &chart::Favorite6, _window, cx| {
                this.multi
                    .update(cx, |multi, cx| multi.pick_favorite(5, cx));
            }))
            .on_action(cx.listener(|this, _: &chart::Favorite7, _window, cx| {
                this.multi
                    .update(cx, |multi, cx| multi.pick_favorite(6, cx));
            }))
            .on_action(cx.listener(|this, _: &chart::Favorite8, _window, cx| {
                this.multi
                    .update(cx, |multi, cx| multi.pick_favorite(7, cx));
            }))
            .on_action(cx.listener(|this, _: &chart::Favorite9, _window, cx| {
                this.multi
                    .update(cx, |multi, cx| multi.pick_favorite(8, cx));
            }))
            .on_action(cx.listener(|this, _: &chart::UndoDrawing, _window, cx| {
                this.multi.update(cx, |multi, cx| multi.undo_drawing(cx));
            }))
            .on_action(cx.listener(|this, _: &chart::RedoDrawing, _window, cx| {
                this.multi.update(cx, |multi, cx| multi.redo_drawing(cx));
            }))
            .on_action(
                cx.listener(|this, _: &chart::DuplicateDrawing, _window, cx| {
                    this.multi
                        .update(cx, |multi, cx| multi.duplicate_drawing(cx));
                }),
            )
            .on_action(cx.listener(|this, _: &chart::ChartPanBack, _window, cx| {
                this.on_active_chart(cx, |chart, cx| chart.pan_keys(false, cx));
            }))
            .on_action(
                cx.listener(|this, _: &chart::ChartPanForward, _window, cx| {
                    this.on_active_chart(cx, |chart, cx| chart.pan_keys(true, cx));
                }),
            )
            .on_action(cx.listener(|this, _: &chart::ChartZoomIn, _window, cx| {
                this.on_active_chart(cx, |chart, cx| chart.zoom_keys(true, cx));
            }))
            .on_action(cx.listener(|this, _: &chart::ChartZoomOut, _window, cx| {
                this.on_active_chart(cx, |chart, cx| chart.zoom_keys(false, cx));
            }))
            .on_action(cx.listener(|this, _: &chart::ChartLatest, _window, cx| {
                this.on_active_chart(cx, |chart, cx| chart.jump_to_latest(cx));
            }))
            .on_action(
                cx.listener(|this, _: &chart::ChartResetScale, _window, cx| {
                    this.on_active_chart(cx, |chart, cx| chart.reset_price_scale(cx));
                }),
            )
            .on_action(cx.listener(|this, _: &chart::ChartAddAlert, _window, cx| {
                this.add_alert_here(cx);
            }))
            .on_action(
                cx.listener(|this, _: &chart::ChartScreenshot, _window, cx| {
                    this.multi.update(cx, |multi, cx| multi.picture(cx));
                }),
            )
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
            .children(menu_backdrop)
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
