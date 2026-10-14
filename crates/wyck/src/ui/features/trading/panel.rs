//! The account panel under the charts: the account's totals, and tabs for the open positions,
//! the working orders, the closed trades, the exposure by symbol and the price alerts, with what
//! can be done to each (close, modify, cancel, delete).
//!
//! It has nothing to set: the tables always show the same columns, a row has its two buttons, and
//! the line under the table adds up what can be added. Only the tab that was open is kept between
//! runs (see [`prefs`]). A row can be clicked to show its symbol on the active chart.

mod data;
mod dialogs;
pub use crate::app::prefs::panel as prefs;
mod stats;
mod view;

use gpui::{Context, Entity, EventEmitter, Subscription};

use crate::app::account::Account;
use crate::app::alerts::Alerts;

pub use self::dialogs::alert_editor::open_alert;
pub use self::prefs::{PanelPrefs, Tab};

pub enum PanelEvent {
    /// Show this symbol on the active chart.
    ShowSymbol(i64),
    /// The tab that is open changed, to be remembered.
    Settings(Box<PanelPrefs>),
}

pub struct AccountPanel {
    account: Entity<Account>,
    alerts: Entity<Alerts>,
    prefs: PanelPrefs,
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
            prefs,
            _subscriptions: subscriptions,
        }
    }

    /// Opens a tab, and remembers it.
    pub fn show_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.prefs.tab != tab {
            self.prefs.tab = tab;
            cx.emit(PanelEvent::Settings(Box::new(self.prefs)));
        }
        cx.notify();
    }
}
