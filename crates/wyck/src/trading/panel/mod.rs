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
mod nav;
pub mod prefs;
mod stats;
mod view;

use std::cell::Cell;
use std::rc::Rc;

use gpui::{Bounds, Context, Entity, EventEmitter, FocusHandle, Pixels, Subscription};
use gpui_kit::component::input::InputState;

use super::account::Account;
use crate::alerts::Alerts;

pub use self::dialogs::open_alert;
pub use self::prefs::{PanelPrefs, Tab};

/// An alert the panel asks for, that is not on a price.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NewAlert {
    /// On the spread of the symbol of the active chart.
    Spread,
    /// On the profit of the whole account.
    Profit,
    /// On the profit of one position.
    Position { id: i64, symbol_id: i64 },
}

pub enum PanelEvent {
    /// Show this symbol on the active chart.
    ShowSymbol(i64),
    /// Make an alert on a spread or a profit and open it.
    NewAlert(NewAlert),
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
    /// One focus handle for each row of the table on screen, made as rows appear. Only the current
    /// row is a stop of the Tab key: the arrows move within the table.
    row_focus: Vec<FocusHandle>,
    /// The keys of the rows on screen, as drawn last.
    row_keys: Vec<String>,
    /// The row that had the focus last (by its key), the one Tab arrives on.
    current_row: Option<String>,
    /// Where the current row is on screen, for a menu the keyboard opens.
    row_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
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
            row_focus: Vec::new(),
            row_keys: Vec::new(),
            current_row: None,
            row_bounds: Rc::new(Cell::new(None)),
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

    /// Has a focus handle for each of `keys`, and remembers them as the rows on screen.
    fn set_rows(&mut self, keys: Vec<String>, cx: &mut Context<Self>) {
        while self.row_focus.len() < keys.len() {
            self.row_focus.push(cx.focus_handle());
        }
        self.row_keys = keys;
    }

    /// The row of the table that keeps the focus for Tab.
    fn current_index(&self) -> usize {
        nav::current(&self.row_keys, self.current_row.as_deref())
    }

    /// A key pressed on row `index`: the arrows and their kin move the focus, and the menu key
    /// opens the menu of the row.
    fn row_key(
        &mut self,
        index: usize,
        event: &gpui::KeyDownEvent,
        menu: &wyck_ui::menu::Menu,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if let Some(step) = nav::step_for(key) {
            let to = nav::target(index, self.row_keys.len(), step);
            if let (Some(handle), Some(row)) = (self.row_focus.get(to), self.row_keys.get(to)) {
                self.current_row = Some(row.clone());
                window.focus(handle, cx);
                cx.notify();
            }
            cx.stop_propagation();
        } else if nav::asks_for_menu(key, event.keystroke.modifiers.shift)
            && let Some(row) = self.row_keys.get(index)
        {
            self.menu_target = Some(MenuTarget::Row(row.clone()));
            // Under the left part of the row, where the eye of a keyboard user is.
            let at = self
                .row_bounds
                .get()
                .map(|b| gpui::point(b.origin.x + gpui::px(24.0), b.origin.y + b.size.height));
            menu.open(at, cx);
            cx.stop_propagation();
        }
    }

    /// A column is being dragged wider or narrower.
    fn drag_column(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(resize) = self.resize else { return };
        // The pointer moves in scaled pixels; the width is kept at the base size.
        let width = resize.start_width
            + (x - resize.start_x) * 100.0 / wyck_ui::tokens::scale_percent() as f32;
        self.edit_prefs(cx, |prefs| prefs.set_width(resize.tab, resize.slot, width));
    }
}
