//! The dashboard's trading side: the account, the order ticket beside the charts, the account
//! panel under them, the price alerts, and the lines all of these put on the charts.
//!
//! What the charts ask for (an order at a price, an alert, a line dragged or closed) arrives here
//! as a [`MultiChartEvent`] and is turned into a call on the account, the alerts or the ticket.
//! Some of those need the window (a confirmation, a field of the ticket), which a chart event does
//! not carry: they wait in [`Pending`] for the next render.

use gpui::prelude::*;
use gpui::{Context, MouseButton, MouseMoveEvent, Window, div, px};

use super::Dashboard;
use wyck::openapi::market::PRICE_SCALE;

use crate::app::chart::{ChartAction, LineId, now_ms};
use crate::app::confirm::confirm;
use crate::app::multichart::SymbolRef;
use crate::app::trading::panel::{PanelEvent, Tab};
use crate::app::trading::ticket::prefs::{Dock, WIDTH_DEFAULT};
use crate::app::trading::ticket::{OrderTicket, TicketEvent};
use crate::app::trading::{self, math};
use crate::app::{alerts, theme, toast};

/// Something to do once the window is at hand.
pub(super) enum Pending {
    /// Open the ticket, filled from the chart.
    Ticket {
        symbol: SymbolRef,
        buy: bool,
        entry: Option<f64>,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    },
    /// A line of the ticket was dragged.
    TicketLine(u8, f64),
    /// Ask before closing a position from its line.
    ClosePosition(i64),
    /// Open the editor of a new alert.
    EditAlert(u64),
}

/// The least height of the account panel.
const PANEL_MIN: f32 = 120.0;

impl Dashboard {
    /// Creates the ticket (which needs the window) and runs what waited for it.
    pub(super) fn trading_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ticket.is_none() {
            let prefs = self.workspace.read(cx).preferences().clone();
            let account = self.trading.clone();
            let ticket =
                cx.new(|cx| OrderTicket::new(account, prefs.ticket, prefs.one_click, window, cx));
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
        if let Some(ticket) = &self.ticket
            && ticket.read(cx).symbol().map(|s| s.id) != symbol_id
        {
            ticket.update(cx, |t, cx| t.set_symbol(symbol, window, cx));
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
            } => {
                self.set_ticket_open(true, cx);
                if let Some(ticket) = &self.ticket {
                    ticket.update(cx, |t, cx| {
                        t.set_symbol(Some(symbol), window, cx);
                        t.prefill(buy, entry, stop_loss, take_profit, window, cx);
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
                crate::app::trading::panel::open_alert(self.alerts.clone(), id, window, cx);
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
            PanelEvent::ShowSymbol(id) => {
                let entry = match &self.catalog {
                    super::Load::Ready(catalog) => catalog.by_id(*id).cloned(),
                    _ => None,
                };
                if let Some(entry) = entry {
                    self.picker_target = None;
                    self.select(entry, false, cx);
                }
            }
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
            } => {
                self.pending.push(Pending::Ticket {
                    symbol: symbol.clone(),
                    buy: *buy,
                    entry: *entry,
                    stop_loss: *stop_loss,
                    take_profit: *take_profit,
                });
            }
            ChartAction::AddAlert(price) => self.add_alert(symbol, *price, cx),
        }
        cx.notify();
    }

    /// Adds an alert on `symbol` at `price` and opens it, so its condition and message can be
    /// set straight away.
    pub(super) fn add_alert(&mut self, symbol: &SymbolRef, price: f64, cx: &mut Context<Self>) {
        let (id, name, digits) = (symbol.id, symbol.name.to_string(), symbol.digits);
        let added = self.alerts.update(cx, |alerts, cx| {
            alerts.digits.insert(id, digits);
            alerts.edit(cx, |book| {
                book.add(id, &name, price, alerts::Condition::Crossing, now_ms())
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
                "There are too many alerts. Remove some first.",
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
                .w(px(5.))
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
                .child(div().w(px(1.)).h_full().bg(theme::border_hairline()))
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
                .flex_1()
                .min_w_0()
                .h_full()
                .overflow_y_scroll()
                .bg(theme::bg())
                .child(ticket);
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
                    .flex_none()
                    .h(px(self.panel_height))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id("panel-resize")
                            .flex_none()
                            .h(px(5.))
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

    fn drag_panel(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(last) = self.panel_drag else { return };
        if event.pressed_button != Some(MouseButton::Left) {
            self.end_panel_drag(cx);
            return;
        }
        let y = f32::from(event.position.y);
        self.panel_height = (self.panel_height - (y - last)).clamp(PANEL_MIN, 900.0);
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
