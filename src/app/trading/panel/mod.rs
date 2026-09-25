//! The account panel under the charts: the account's totals, and tabs for the open positions,
//! the working orders, the recent deals, the exposure by symbol and the price alerts, with what
//! can be done to each (close, reverse, modify, cancel, delete).
//!
//! Every table has columns the user picks, orders and resizes, can be sorted by any of them,
//! searched and filtered, totalled, and copied as CSV. A right click on a row opens what can be
//! done to it, and on a header the columns. Which tabs, figures, columns and row buttons show, and
//! how the rows look, is kept between runs (see [`prefs`]) and edited in [`customize`].
//!
//! A row can be clicked to show its symbol on the active chart.

pub mod columns;
mod customize;
mod data;
mod dialogs;
pub mod prefs;
mod stats;
mod view;

use gpui::{Context, Entity, EventEmitter, Subscription};
use gpui_kit::component::input::InputState;

use super::account::Account;
use crate::app::alerts::Alerts;

pub use self::dialogs::open_alert;
pub use self::prefs::{PanelPrefs, Tab};

pub enum PanelEvent {
    /// Show this symbol on the active chart.
    ShowSymbol(i64),
    /// Fold the panel away.
    Hide,
    /// How the panel is arranged changed, to be remembered.
    Settings(Box<PanelPrefs>),
}

/// What a right click opened the menu for.
#[derive(Debug, Clone)]
enum MenuTarget {
    /// A row, by its key.
    Row(String),
    /// The header of the table.
    Header,
}

/// A column being resized: where the pointer was and how wide the column was when it was grabbed.
#[derive(Debug, Clone, Copy)]
struct Resize {
    tab: Tab,
    slot: usize,
    start_x: f32,
    start_width: f32,
}

pub struct AccountPanel {
    account: Entity<Account>,
    alerts: Entity<Alerts>,
    prefs: PanelPrefs,
    /// The symbol of the active chart, for the filter that keeps to it.
    symbol: Option<i64>,
    search: Option<Entity<InputState>>,
    query: String,
    menu_target: Option<MenuTarget>,
    resize: Option<Resize>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PanelEvent> for AccountPanel {}

impl AccountPanel {
    pub fn new(
        account: Entity<Account>,
        alerts: Entity<Alerts>,
        prefs: PanelPrefs,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions = vec![
            cx.observe(&account, |_this, _account, cx| cx.notify()),
            cx.observe(&alerts, |_this, _alerts, cx| cx.notify()),
        ];
        Self {
            account,
            alerts,
            prefs: prefs.normalized(),
            symbol: None,
            search: None,
            query: String::new(),
            menu_target: None,
            resize: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn prefs(&self) -> &PanelPrefs {
        &self.prefs
    }

    /// Changes how the panel is arranged, and remembers it.
    pub fn edit_prefs(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut PanelPrefs)) {
        let mut prefs = self.prefs.clone();
        change(&mut prefs);
        let prefs = prefs.normalized();
        if prefs != self.prefs {
            self.prefs = prefs;
            cx.emit(PanelEvent::Settings(Box::new(self.prefs.clone())));
            cx.notify();
        }
    }

    /// Puts the arrangement back as it was first.
    pub fn reset_prefs(&mut self, cx: &mut Context<Self>) {
        self.edit_prefs(cx, |prefs| *prefs = PanelPrefs::default());
    }

    /// Opens a tab, showing it first when the user had hidden it.
    pub fn show_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.edit_prefs(cx, |prefs| {
            if let Some(placed) = prefs.tabs.iter_mut().find(|t| t.item == tab) {
                placed.shown = true;
            }
            prefs.tab = tab;
        });
        cx.notify();
    }

    /// The symbol of the active chart changed.
    pub fn set_symbol(&mut self, symbol: Option<i64>, cx: &mut Context<Self>) {
        if self.symbol != symbol {
            self.symbol = symbol;
            if self.prefs.only_symbol {
                cx.notify();
            }
        }
    }

    /// A column is being dragged wider or narrower.
    fn drag_column(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(resize) = self.resize else { return };
        let width = resize.start_width + (x - resize.start_x);
        self.edit_prefs(cx, |prefs| prefs.set_width(resize.tab, resize.slot, width));
    }
}
