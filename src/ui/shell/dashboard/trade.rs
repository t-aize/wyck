//! The dashboard's trading side: the account, the order ticket beside the charts, the account
//! panel under them, the price alerts, and the lines all of these put on the charts.
//!
//! What the charts ask for (an order at a price, an alert, a line dragged or closed) arrives here
//! as a `MultiChartEvent` and is turned into a call on the account, the alerts or the ticket.
//! Some of those need the window (a confirmation, a field of the ticket), which a chart event does
//! not carry: they wait in [`Pending`] for the next render.

use gpui::prelude::*;
use gpui::{Context, MouseButton, MouseMoveEvent, Window, div, px};

use super::Dashboard;
use crate::domain::market::PRICE_SCALE;

use crate::app::alerts;
use crate::app::market_data::now_ms;
use crate::app::prefs::ticket::{Dock, WIDTH_DEFAULT};
use crate::domain::trading::math;
use crate::ui::features::chart::{ChartAction, LineId, PositionLink};
use crate::ui::features::multichart::SymbolRef;
use crate::ui::features::trading;
use crate::ui::features::trading::panel::{PanelEvent, Tab};
use crate::ui::features::trading::ticket::{OrderTicket, TicketEvent};
use crate::ui::kit::{confirm::confirm, theme, toast, tokens};

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
    pub(super) fn trading_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                            if trading::book::is_buy(p.trade_data.trade_side) {
                                "Buy"
                            } else {
                                "Sell"
                            },
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
    pub(super) fn push_lines(&mut self, cx: &mut Context<Self>) {
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
    pub(super) fn ticket_key(
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
    pub(super) fn ask_close_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn toggle_kill_switch(&mut self, cx: &mut Context<Self>) {
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
    pub(super) fn sync_risk(&mut self, cx: &mut Context<Self>) {
        let prefs = self.workspace.read(cx).preferences();
        let (risk, zone) = (prefs.risk.clone(), prefs.zone);
        self.trading
            .update(cx, |account, cx| account.set_risk(risk, zone, cx));
    }

    pub(super) fn set_ticket_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.ticket_open = open;
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.ticket_open = open);
        });
        self.push_lines(cx);
        cx.notify();
    }

    pub(super) fn set_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.panel_open = open;
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.panel_open = open);
        });
        cx.notify();
    }

    pub(super) fn on_panel_event(&mut self, event: &PanelEvent, cx: &mut Context<Self>) {
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

    /// Makes an alert on a spread or a profit, with a level that fits what it sees now, and
    /// opens it so the level and the condition can be set straight away.
    fn add_measure_alert(
        &mut self,
        kind: crate::ui::features::trading::panel::NewAlert,
        cx: &mut Context<Self>,
    ) {
        use crate::app::alerts::model::PnlScope;
        use crate::ui::features::trading::panel::NewAlert;
        let now = now_ms();
        let (alert, digits) = match kind {
            NewAlert::Spread => {
                let Some(symbol) = self.multi.read(cx).active_symbol(cx) else {
                    toast::show(
                        cx,
                        toast::Kind::Info,
                        "No symbol",
                        "Open a chart to watch the spread of its symbol.",
                    );
                    return;
                };
                let pip = self.alerts.read(cx).pip_of(symbol.id);
                let (bid, ask) = self.trading.read(cx).quote(symbol.id);
                // Twice the spread it has now, a round number, as a first level.
                let level = bid.zip(ask).map_or(2.0, |(bid, ask)| {
                    alerts::eval::spread_pips(bid, ask, pip) * 2.0
                });
                let level = (level.max(1.0) * 10.0).round() / 10.0;
                (
                    alerts::Alert::spread(0, symbol.id, &symbol.name, pip, level, now),
                    symbol.digits,
                )
            }
            NewAlert::Profit => {
                // One percent of the balance lost, to start with.
                let balance = self.trading.read(cx).summary().balance;
                let level = -(balance * 0.01).round().max(1.0);
                (
                    alerts::Alert::pnl(0, 0, "", PnlScope::Account, level, now),
                    5,
                )
            }
            NewAlert::Position { id, symbol_id } => {
                let name = self.trading.read(cx).book.name(symbol_id);
                let digits = self.trading.read(cx).book.contract(symbol_id).digits;
                (
                    alerts::Alert::pnl(0, symbol_id, &name, PnlScope::Position { id }, 0.0, now),
                    digits,
                )
            }
        };
        self.insert_alert(alert, digits, cx);
    }

    /// Adds an alert made here and opens it, or says the limit is reached.
    fn insert_alert(&mut self, alert: alerts::Alert, digits: u32, cx: &mut Context<Self>) {
        let limit = self.workspace.read(cx).preferences().limits.alerts;
        let symbol_id = alert.symbol_id;
        let added = self.alerts.update(cx, |alerts, cx| {
            if symbol_id > 0 {
                alerts.digits.entry(symbol_id).or_insert(digits);
            }
            alerts.edit(cx, |book| book.insert(alert, limit))
        });
        match added {
            Some(id) => {
                self.pending.push(Pending::EditAlert(id));
                if let Some(panel) = &self.panel {
                    panel.update(cx, |panel, cx| panel.show_tab(Tab::Alerts, cx));
                }
            }
            None => toast::show(
                cx,
                toast::Kind::Warning,
                "No alert added",
                crate::ui::kit::shortcut::text(&format!(
                    "The limit is {limit} alerts. Change it in Settings (Ctrl+,)."
                )),
            ),
        }
        cx.notify();
    }

    /// Puts a symbol on the active chart.
    pub(super) fn show_symbol(&mut self, id: i64, cx: &mut Context<Self>) {
        let entry = match &self.catalog {
            super::Load::Ready(catalog) => catalog.by_id(id).cloned(),
            _ => None,
        };
        if let Some(entry) = entry {
            self.picker_target = None;
            self.select(entry, false, cx);
        }
    }

    /// Gives the alerts what they read from the account: how the symbols they watch are written
    /// (from the contracts, which are asked for when they are not known yet) and, for the ones on
    /// a profit, what the positions make now.
    pub(super) fn feed_alerts(&mut self, cx: &mut Context<Self>) {
        let symbols = self.alerts.read(cx).symbols();
        for id in symbols {
            self.trading
                .update(cx, |account, cx| account.ensure_contract(id, cx));
            let contract = self.trading.read(cx).book.contracts.get(&id).cloned();
            // Before the answer a contract is a placeholder: it must not hide what is known.
            let Some(contract) =
                contract.filter(|c| *c != crate::domain::trading::math::Contract::default())
            else {
                continue;
            };
            let (digits, pip) = (contract.digits, contract.pip());
            self.alerts
                .update(cx, |alerts, cx| alerts.learn(id, digits, pip, cx));
        }
        if self.alerts.read(cx).watches_profit() {
            let profits = self.trading.read(cx).profits();
            self.alerts
                .update(cx, |alerts, cx| alerts.on_profit(&profits, cx));
        }
    }

    /// An alert fired or ran out: says so, with the buttons that help.
    pub(super) fn on_alert_event(&mut self, event: &alerts::AlertsEvent, cx: &mut Context<Self>) {
        match event {
            alerts::AlertsEvent::Fired(alert, text) => {
                let title = if alert.source.is_indicator() {
                    "Indicator alert"
                } else if alert.drawing().is_some() {
                    "Drawing alert"
                } else {
                    "Price alert"
                };
                let (id, symbol) = (alert.id, alert.symbol_id);
                let dashboard = cx.entity();
                let snooze = self.alerts.clone();
                let mut toast = toast::Toast::warning(title, text.clone())
                    .sticky(alert.sticky)
                    .action("Show chart", move |_, cx| {
                        dashboard.update(cx, |d, cx| d.show_symbol(symbol, cx));
                    })
                    .action("Snooze 1 h", move |_, cx| {
                        snooze.update(cx, |a, cx| a.snooze(id, now_ms() + 3_600_000, cx));
                    });
                if alert.repeats() {
                    let off = self.alerts.clone();
                    toast = toast.action("Turn off", move |_, cx| {
                        off.update(cx, |a, cx| {
                            a.edit(cx, |book| {
                                if let Some(alert) = book.get_mut(id) {
                                    alert.active = false;
                                }
                            })
                        });
                    });
                }
                if !alert.tag.is_empty() {
                    toast = toast.hint(format!("Tag: {}", alert.tag));
                }
                toast.show(cx);
                self.alert_output(alert, title, text, cx);
            }
            alerts::AlertsEvent::Expired(alert) => {
                let digits = self
                    .alerts
                    .read(cx)
                    .digits
                    .get(&alert.symbol_id)
                    .copied()
                    .unwrap_or(5);
                toast::Toast::info("Alert expired", alert.describe(digits)).show(cx);
            }
            alerts::AlertsEvent::Changed => {}
        }
    }

    /// The sound and the desktop notification of an alert that fired, as the user set them.
    fn alert_output(&self, alert: &alerts::Alert, title: &str, text: &str, cx: &mut Context<Self>) {
        let output = self.workspace.read(cx).preferences().alert_output.clone();
        // No window of the app is active when the user is elsewhere.
        let in_use = cx.active_window().is_some();
        let plan = alerts::sound::plan(&output, alert.sound, in_use);
        let speaker = alerts::sound::speaker();
        if let Some(kind) = plan.sound {
            speaker.play(kind, output.custom.as_deref(), output.volume);
        }
        if plan.notification {
            speaker.notify(&format!("{title}: {}", alert.symbol), text);
        }
    }

    /// What a chart asked for.
    pub(super) fn on_chart_action(
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

    /// Adds an alert on `symbol` at `price` and opens it, so its condition and message can be
    /// set straight away.
    pub(super) fn add_alert(&mut self, symbol: &SymbolRef, price: f64, cx: &mut Context<Self>) {
        let (id, name, digits) = (symbol.id, symbol.name.to_string(), symbol.digits);
        let limit = self.workspace.read(cx).preferences().limits.alerts;
        let pip = symbol.pip_position.map_or_else(
            || alerts::model::pip_from_digits(digits),
            crate::domain::market::pip_size,
        );
        let added = self.alerts.update(cx, |alerts, cx| {
            alerts.digits.insert(id, digits);
            alerts.set_pip(id, pip);
            alerts.edit(cx, |book| {
                book.add_with_limit(
                    id,
                    &name,
                    price,
                    alerts::Condition::Crossing,
                    now_ms(),
                    limit,
                )
            })
        });
        match added {
            Some(alert) => {
                self.pending.push(Pending::EditAlert(alert));
                if let Some(panel) = &self.panel {
                    panel.update(cx, |panel, cx| panel.show_tab(Tab::Alerts, cx));
                }
            }
            None => toast::show(
                cx,
                toast::Kind::Warning,
                "No alert added",
                crate::ui::kit::shortcut::text(&format!(
                    "The limit is {limit} alerts. Change it in Settings (Ctrl+,)."
                )),
            ),
        }
        cx.notify();
    }

    /// Adds an alert on an indicator or a drawing and opens it, so its condition can be set.
    pub(super) fn add_alert_on(
        &mut self,
        symbol: &SymbolRef,
        seed: &crate::ui::features::chart::AlertSeed,
        cx: &mut Context<Self>,
    ) {
        use crate::app::alerts::{Alert, Condition, Source};
        let limit = self.workspace.read(cx).preferences().limits.alerts;
        let mut alert = Alert::price(0, symbol.id, &symbol.name, 0.0, now_ms());
        match seed {
            crate::ui::features::chart::AlertSeed::Indicator { study, timeframe } => {
                alert.timeframe = timeframe.clone();
                let spec = study.kind.spec();
                if matches!(spec.format, crate::domain::indicators::ValueFormat::Price) {
                    // An average or a band on the prices: the price crosses it.
                    alert.versus = Some(Source::Indicator {
                        study: study.clone(),
                        plot: 0,
                    });
                } else {
                    // An oscillator: its value crosses a level, a high one to start with.
                    alert.source = Source::Indicator {
                        study: study.clone(),
                        plot: 0,
                    };
                    alert.price = spec
                        .range
                        .map_or(0.0, |(low, high)| low + (high - low) * 0.7);
                }
            }
            crate::ui::features::chart::AlertSeed::Drawing {
                id,
                zone,
                timeframe,
            } => {
                alert.timeframe = timeframe.clone();
                alert.versus = Some(Source::Drawing { id: *id });
                alert.condition = if *zone {
                    Condition::EntersZone
                } else {
                    Condition::Crossing
                };
            }
        }
        let digits = symbol.digits;
        let pip = symbol.pip_position.map_or_else(
            || alerts::model::pip_from_digits(digits),
            crate::domain::market::pip_size,
        );
        let added = self.alerts.update(cx, |alerts, cx| {
            alerts.digits.insert(symbol.id, digits);
            alerts.set_pip(symbol.id, pip);
            alerts.edit(cx, |book| book.insert(alert, limit))
        });
        match added {
            Some(id) => {
                self.pending.push(Pending::EditAlert(id));
                if let Some(panel) = &self.panel {
                    panel.update(cx, |panel, cx| panel.show_tab(Tab::Alerts, cx));
                }
            }
            None => toast::show(
                cx,
                toast::Kind::Warning,
                "No alert added",
                crate::ui::kit::shortcut::text(&format!(
                    "The limit is {limit} alerts. Change it in Settings (Ctrl+,)."
                )),
            ),
        }
        cx.notify();
    }

    /// An alert at the active chart's last price.
    pub(super) fn add_alert_here(&mut self, cx: &mut Context<Self>) {
        let chart = self.multi.read(cx).active_chart().clone();
        let Some(symbol) = self.multi.read(cx).active_symbol(cx) else {
            return;
        };
        let Some(bid) = chart.read(cx).quote().0 else {
            return;
        };
        self.add_alert(&symbol, bid as f64 / PRICE_SCALE as f64, cx);
    }

    /// A line was dragged on a chart.
    pub(super) fn on_line_moved(&mut self, id: LineId, price: f64, cx: &mut Context<Self>) {
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
    pub(super) fn on_line_closed(&mut self, id: LineId, cx: &mut Context<Self>) {
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

    /// The charts with the ticket beside them, and the account panel under both.
    pub(super) fn trading_layout(
        &self,
        charts: gpui::AnyElement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let panel = self.panel.clone().filter(|_| self.panel_open);
        let ticket = self.ticket.clone().filter(|_| self.ticket_open);
        let dragging = self.panel_drag.is_some();
        let editor_dragging = self.editor_drag.is_some();
        let editor_panel = self.editor_dock(cx);
        let ticket_dragging = self.ticket_drag.is_some();
        let (dock, width) = ticket.as_ref().map_or((Dock::Right, WIDTH_DEFAULT), |t| {
            let layout = t.read(cx).layout();
            (layout.dock, layout.width)
        });
        let charts = div().flex_1().min_w_0().h_full().flex().child(charts);
        let edge = |id: &'static str| {
            // The edge toward the charts drags to make the panel wider or narrower.
            div()
                .id(id)
                .flex_none()
                .w(px(crate::ui::kit::tokens::splitter()))
                .h_full()
                .flex()
                .justify_center()
                .cursor_col_resize()
                .bg(if ticket_dragging {
                    theme::accent_alpha(0.4)
                } else {
                    gpui::rgba(0x00000000)
                })
                .hover(|s| s.bg(theme::accent_alpha(0.4)))
                .child(crate::ui::kit::layout::rule_v().h_full())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                        this.ticket_drag = Some((f32::from(event.position.x), width));
                        cx.notify();
                    }),
                )
        };
        let column = ticket.map(|ticket| {
            let body = div()
                .id("ticket-column")
                .relative()
                .flex_1()
                .min_w_0()
                .h_full()
                .bg(theme::bg())
                .child(
                    div()
                        .id("ticket-scroll")
                        .h_full()
                        .overflow_hidden()
                        .child(ticket),
                );
            let column = div().flex_none().w(px(width)).h_full().flex().flex_row();
            if dock == Dock::Right {
                column.child(edge("ticket-resize")).child(body)
            } else {
                column.child(body).child(edge("ticket-resize"))
            }
        });
        let (before, after) = if dock == Dock::Left {
            (column, None)
        } else {
            (None, column)
        };
        div()
            .id("trading-layout")
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .when(dragging, |el| {
                el.cursor_row_resize()
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        this.drag_panel(event, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_panel_drag(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_panel_drag(cx)),
                    )
            })
            .when(editor_dragging, |el| {
                el.cursor_col_resize()
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        this.drag_editor(event, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_editor_drag(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_editor_drag(cx)),
                    )
            })
            .when(ticket_dragging, |el| {
                el.cursor_col_resize()
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        this.drag_ticket(event, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_ticket_drag(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_ticket_drag(cx)),
                    )
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .children(before)
                    .child(charts)
                    .children(after),
            )
            .children(panel.map(|panel| {
                div()
                    .relative()
                    .flex_none()
                    .h(px(self.panel_height_now()))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id("panel-resize")
                            .flex_none()
                            .h(px(crate::ui::kit::tokens::splitter()))
                            .w_full()
                            .cursor_row_resize()
                            .border_t_1()
                            .border_color(if dragging {
                                theme::accent()
                            } else {
                                theme::border_hairline()
                            })
                            .hover(|s| s.border_color(theme::accent()))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                    this.panel_drag = Some(f32::from(event.position.y));
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(div().flex_1().min_h_0().child(panel))
            }))
            // Last, so it is drawn over everything else of the layout.
            .children(editor_panel)
    }

    /// The edge of the ticket is being dragged: its width follows the pointer, from where the
    /// edge was grabbed.
    fn drag_ticket(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some((start, width)) = self.ticket_drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.end_ticket_drag(cx);
            return;
        }
        let Some(ticket) = self.ticket.clone() else {
            return;
        };
        let moved = f32::from(event.position.x) - start;
        ticket.update(cx, |t, cx| {
            // The panel grows as its edge goes away from the charts.
            let width = if t.layout().dock == Dock::Right {
                width - moved
            } else {
                width + moved
            };
            t.edit_layout(cx, |layout| layout.width = width);
        });
    }

    fn end_ticket_drag(&mut self, cx: &mut Context<Self>) {
        if self.ticket_drag.take().is_some() {
            cx.notify();
        }
    }

    /// The tallest the panel can be in this window: the charts keep their least height.
    fn panel_room(&self) -> f32 {
        (self.viewport_height - bars() - CHARTS_MIN).clamp(PANEL_MIN, 900.0)
    }

    /// The height the panel has now: the one the user chose, or less in a short window.
    fn panel_height_now(&self) -> f32 {
        self.panel_height.clamp(PANEL_MIN, self.panel_room())
    }

    fn drag_panel(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(last) = self.panel_drag else { return };
        if event.pressed_button != Some(MouseButton::Left) {
            self.end_panel_drag(cx);
            return;
        }
        let y = f32::from(event.position.y);
        // Start from the height shown, so a panel held down by a short window follows the pointer.
        let room = self.panel_room();
        self.panel_height = (self.panel_height_now() - (y - last)).clamp(PANEL_MIN, room);
        self.panel_drag = Some(y);
        cx.notify();
    }

    fn end_panel_drag(&mut self, cx: &mut Context<Self>) {
        if self.panel_drag.take().is_some() {
            let height = self.panel_height;
            self.workspace.update(cx, |workspace, cx| {
                workspace.edit_preferences(cx, |prefs| prefs.panel_height = height);
            });
            cx.notify();
        }
    }
}
