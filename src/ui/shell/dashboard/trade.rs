//! The dashboard's trading side: the account, the order ticket beside the charts, the account
//! panel under them, the price alerts, and the lines all of these put on the charts.
//!
//! What the charts ask for (an order at a price, an alert, a line dragged or closed) arrives here
//! as a `MultiChartEvent` and is turned into a call on the account, the alerts or the ticket.
//! Some of those need the window (a confirmation, a field of the ticket), which a chart event does
//! not carry: they wait in [`Pending`] for the next render.

pub mod layout;
pub mod price_alerts;

use super::Dashboard;
use crate::domain::trading::math;
use crate::ui::features::chart::{ChartAction, LineId, PositionLink};
use crate::ui::features::multichart::SymbolRef;
use crate::ui::features::trading;
use crate::ui::features::trading::panel::PanelEvent;
use crate::ui::features::trading::ticket::{OrderTicket, TicketEvent};
use crate::ui::kit::{confirm::confirm, toast, tokens};
use gpui::prelude::*;
use gpui::{Context, Window};

/// Something to do once the window is at hand.
pub(super) enum Pending {
    /// Open the ticket, filled from the chart.
    Ticket {
        symbol: SymbolRef,
        buy: bool,
        entry: Option<f64>,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        link: Option<PositionLink>,
    },
    /// A line of the ticket was dragged.
    TicketLine(u8, f64),
    /// Ask before closing a position from its line.
    ClosePosition(i64),
    /// Open the editor of a new alert.
    EditAlert(u64),
    /// Open the editor of the indicator scripts.
    Editor(crate::ui::features::chart::EditorRequest),
}

/// The least height of the account panel.
const PANEL_MIN: f32 = 120.0;

/// What the charts keep when the panel grows, and what the bars above them take.
const CHARTS_MIN: f32 = 240.0;
/// The height the bars above the charts take: the header and the strip under it.
fn bars() -> f32 {
    tokens::bar::header() + tokens::scaled(14.0)
}

impl Dashboard {
    /// Creates the ticket (which needs the window) and runs what waited for it.
    pub(in crate::ui::shell::dashboard) fn trading_frame(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ticket.is_none() {
            let prefs = self.workspace.read(cx).preferences().clone();
            let account = self.trading.clone();
            let ticket = cx.new(|cx| OrderTicket::new(account, prefs.ticket, false, window, cx));
            // One-click trading starts off at every launch, whatever it was left at.
            self.workspace.update(cx, |workspace, cx| {
                workspace.edit_preferences(cx, |prefs| prefs.one_click = false);
            });
            self.sync_risk(cx);
            cx.subscribe(
                &ticket,
                |this, ticket, event: &TicketEvent, cx| match event {
                    TicketEvent::LinesChanged => {
                        let one_click = ticket.read(cx).one_click();
                        this.workspace.update(cx, |workspace, cx| {
                            workspace.edit_preferences(cx, |prefs| prefs.one_click = one_click);
                        });
                        this.push_lines(cx);
                    }
                    TicketEvent::Settings(settings) => {
                        let settings = (**settings).clone();
                        this.workspace.update(cx, |workspace, cx| {
                            workspace.edit_preferences(cx, |prefs| prefs.ticket = settings);
                        });
                        // The side and the width of the panel are part of it.
                        cx.notify();
                    }
                    TicketEvent::Close => this.set_ticket_open(false, cx),
                },
            )
            .detach();
            self.ticket = Some(ticket);
        }
        // The ticket is for the active chart's symbol.
        let symbol = self.multi.read(cx).active_symbol(cx);
        let symbol_id = symbol.as_ref().map(|s| s.id);
        // Also when only the decimals changed: the broker's answer can come after the symbol.
        if let Some(ticket) = &self.ticket
            && ticket.read(cx).symbol() != symbol.as_ref()
        {
            ticket.update(cx, |t, cx| t.set_symbol(symbol, window, cx));
        }
        if let Some(ticket) = &self.ticket {
            let chart = self.multi.read(cx).active_chart().clone();
            ticket.update(cx, |t, cx| {
                t.set_chart(chart, cx);
                t.request_atr(cx);
            });
        }
        if let Some(panel) = &self.panel {
            panel.update(cx, |p, cx| p.set_symbol(symbol_id, cx));
        }
        for pending in std::mem::take(&mut self.pending) {
            self.run_pending(pending, window, cx);
        }
    }

    fn run_pending(&mut self, pending: Pending, window: &mut Window, cx: &mut Context<Self>) {
        match pending {
            Pending::Ticket {
                symbol,
                buy,
                entry,
                stop_loss,
                take_profit,
                link,
            } => {
                self.set_ticket_open(true, cx);
                if let Some(ticket) = &self.ticket {
                    ticket.update(cx, |t, cx| {
                        t.set_symbol(Some(symbol), window, cx);
                        t.prefill(buy, entry, stop_loss, take_profit, link, window, cx);
                    });
                }
            }
            Pending::TicketLine(line, price) => {
                if let Some(ticket) = &self.ticket {
                    ticket.update(cx, |t, cx| t.line_moved(line, price, window, cx));
                }
            }
            Pending::ClosePosition(id) => {
                let text = self
                    .trading
                    .read(cx)
                    .book
                    .positions
                    .get(&id)
                    .map(|p| {
                        let book = &self.trading.read(cx).book;
                        let contract = book.contract(p.trade_data.symbol_id);
                        format!(
                            "{} {} {}",
                            if p.trade_data.is_buy() { "Buy" } else { "Sell" },
                            math::format_lots(contract.lots_of_volume(p.trade_data.volume)),
                            book.name(p.trade_data.symbol_id)
                        )
                    })
                    .unwrap_or_default();
                let account = self.trading.clone();
                confirm(window, cx, "Close this position?", text, move |_, cx| {
                    account.update(cx, |a, cx| a.close_position(id, None, cx));
                });
            }
            Pending::EditAlert(id) => {
                crate::ui::features::trading::panel::open_alert(
                    self.alerts.clone(),
                    id,
                    window,
                    cx,
                );
            }
            Pending::Editor(request) => {
                use crate::ui::features::chart::EditorRequest;
                self.with_editor(window, cx, move |editor, window, cx| match request {
                    EditorRequest::Open => editor.focus_editor(window, cx),
                    EditorRequest::New => editor.ask_new(0, window, cx),
                    EditorRequest::Edit(id) => editor.open(&id, window, cx),
                });
            }
        }
        cx.notify();
    }

    /// The lines of the account, the alerts and the ticket, handed to the charts.
    pub(in crate::ui::shell::dashboard) fn push_lines(&mut self, cx: &mut Context<Self>) {
        let account = self.trading.read(cx);
        let mut lines = trading::lines(
            &account.book,
            self.alerts.read(cx).book(),
            &|id| account.net_profit(id),
            &account.book.currency,
        );
        if self.ticket_open
            && let Some(ticket) = &self.ticket
            && let Some(symbol) = ticket.read(cx).symbol().map(|s| s.id)
        {
            lines
                .entry(symbol)
                .or_default()
                .extend(ticket.read(cx).lines(cx));
        }
        self.multi
            .update(cx, |multi, cx| multi.set_lines(lines, cx));
    }

    /// The keyboard shortcuts of the ticket: Alt+B and Alt+S pick a side, Ctrl+Enter sends the
    /// order the way the send button does (with its confirmation, unless one-click is on).
    pub(in crate::ui::shell::dashboard) fn ticket_key(
        &mut self,
        side: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ticket_open {
            return;
        }
        let Some(ticket) = self.ticket.clone() else {
            return;
        };
        ticket.update(cx, |t, cx| match side {
            Some(buy) => t.choose_side(buy, window, cx),
            None => t.send_now(window, cx),
        });
    }

    /// The panic button: asks, then closes every open position. Working orders stay.
    pub(in crate::ui::shell::dashboard) fn ask_close_all(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.trading.read(cx).book.positions.len();
        if count == 0 {
            toast::show(
                cx,
                toast::Kind::Info,
                "Nothing to close",
                "No position is open.",
            );
            return;
        }
        let account = self.trading.clone();
        confirm(
            window,
            cx,
            "Close every position?",
            format!("{count} open position(s) will be closed at market."),
            move |_, cx| account.update(cx, |a, cx| a.close_all(None, cx)),
        );
    }

    /// Turns the kill switch on or off: while it is on, no new order can be sent.
    pub(in crate::ui::shell::dashboard) fn toggle_kill_switch(&mut self, cx: &mut Context<Self>) {
        let on = !self.workspace.read(cx).preferences().risk.kill_switch;
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.risk.kill_switch = on);
        });
        toast::show(
            cx,
            if on {
                toast::Kind::Warning
            } else {
                toast::Kind::Info
            },
            if on {
                "Kill switch on"
            } else {
                "Kill switch off"
            },
            if on {
                "No new order can be sent. Closing still works."
            } else {
                "New orders can be sent again."
            },
        );
    }

    /// Hands the safety limits and the zone the day is counted in to the account.
    pub(in crate::ui::shell::dashboard) fn sync_risk(&mut self, cx: &mut Context<Self>) {
        let prefs = self.workspace.read(cx).preferences();
        let (risk, zone) = (prefs.risk.clone(), prefs.zone);
        self.trading
            .update(cx, |account, cx| account.set_risk(risk, zone, cx));
    }

    pub(in crate::ui::shell::dashboard) fn set_ticket_open(
        &mut self,
        open: bool,
        cx: &mut Context<Self>,
    ) {
        self.ticket_open = open;
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.ticket_open = open);
        });
        self.push_lines(cx);
        cx.notify();
    }

    pub(in crate::ui::shell::dashboard) fn set_panel_open(
        &mut self,
        open: bool,
        cx: &mut Context<Self>,
    ) {
        self.panel_open = open;
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.panel_open = open);
        });
        cx.notify();
    }

    pub(in crate::ui::shell::dashboard) fn on_panel_event(
        &mut self,
        event: &PanelEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PanelEvent::Hide => self.set_panel_open(false, cx),
            PanelEvent::Settings(settings) => {
                let settings = (**settings).clone();
                self.workspace.update(cx, |workspace, cx| {
                    workspace.edit_preferences(cx, |prefs| prefs.account_panel = settings);
                });
            }
            PanelEvent::ShowSymbol(id) => self.show_symbol(*id, cx),
            PanelEvent::NewAlert(kind) => self.add_measure_alert(*kind, cx),
        }
    }

    /// Puts a symbol on the active chart.
    pub(in crate::ui::shell::dashboard) fn show_symbol(&mut self, id: i64, cx: &mut Context<Self>) {
        let entry = match &self.catalog {
            super::Load::Ready(catalog) => catalog.by_id(id).cloned(),
            _ => None,
        };
        if let Some(entry) = entry {
            self.picker_target = None;
            self.select(entry, false, cx);
        }
    }

    /// What a chart asked for.
    pub(in crate::ui::shell::dashboard) fn on_chart_action(
        &mut self,
        symbol: &SymbolRef,
        action: &ChartAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            ChartAction::Ticket {
                buy,
                entry,
                stop_loss,
                take_profit,
                link,
            } => {
                self.pending.push(Pending::Ticket {
                    symbol: symbol.clone(),
                    buy: *buy,
                    entry: *entry,
                    stop_loss: *stop_loss,
                    take_profit: *take_profit,
                    link: link.clone(),
                });
            }
            ChartAction::AddAlert(price) => self.add_alert(symbol, *price, cx),
            ChartAction::AddAlertOn(seed) => self.add_alert_on(symbol, seed, cx),
        }
        cx.notify();
    }

    /// A line was dragged on a chart.
    pub(in crate::ui::shell::dashboard) fn on_line_moved(
        &mut self,
        id: LineId,
        price: f64,
        cx: &mut Context<Self>,
    ) {
        let account = self.trading.clone();
        match id {
            LineId::Order(order) => account.update(cx, |a, cx| a.move_order(order, price, cx)),
            LineId::OrderStopLoss(order) => {
                let tp = account
                    .read(cx)
                    .book
                    .orders
                    .get(&order)
                    .and_then(|o| o.take_profit);
                account.update(cx, |a, cx| a.protect_order(order, Some(price), tp, cx));
            }
            LineId::OrderTakeProfit(order) => {
                let sl = account
                    .read(cx)
                    .book
                    .orders
                    .get(&order)
                    .and_then(|o| o.stop_loss);
                account.update(cx, |a, cx| a.protect_order(order, sl, Some(price), cx));
            }
            LineId::StopLoss(position) => {
                let tp = account
                    .read(cx)
                    .book
                    .positions
                    .get(&position)
                    .and_then(|p| p.take_profit);
                account.update(cx, |a, cx| {
                    a.protect_position(position, Some(price), tp, cx)
                });
            }
            LineId::TakeProfit(position) => {
                let sl = account
                    .read(cx)
                    .book
                    .positions
                    .get(&position)
                    .and_then(|p| p.stop_loss);
                account.update(cx, |a, cx| {
                    a.protect_position(position, sl, Some(price), cx)
                });
            }
            LineId::Position(_) => {}
            LineId::Alert(alert) => {
                self.alerts.update(cx, |alerts, cx| {
                    alerts.edit(cx, |book| {
                        if let Some(a) = book.get_mut(alert) {
                            a.price = price;
                        }
                    });
                });
            }
            LineId::Pending(line) => self.pending.push(Pending::TicketLine(line, price)),
        }
        cx.notify();
    }

    /// The close button of a line was clicked.
    pub(in crate::ui::shell::dashboard) fn on_line_closed(
        &mut self,
        id: LineId,
        cx: &mut Context<Self>,
    ) {
        match id {
            LineId::Position(position) => self.pending.push(Pending::ClosePosition(position)),
            LineId::Order(order) => self.trading.update(cx, |a, cx| a.cancel_order(order, cx)),
            LineId::Alert(alert) => {
                self.alerts.update(cx, |alerts, cx| {
                    alerts.edit(cx, |book| book.remove(alert));
                });
            }
            _ => {}
        }
        cx.notify();
    }
}
