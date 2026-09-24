//! The order ticket: the panel beside the charts where an order is put together and sent.
//!
//! It shows the live bid and ask of the symbol with the spread, the side, the kind of order
//! (market, limit, stop), the volume in lots (stepped the way the broker accepts), the price of a
//! pending order, and a stop loss and take profit with their distance in pips. The margin the
//! order would use is asked from the server as the volume changes.
//!
//! While a pending price or a protection is set, it shows on the chart as a line that can be
//! dragged; the ticket follows the line (see [`OrderTicket::lines`]).
//!
//! An order is sent after a confirmation, unless one-click trading is on.

use std::time::Duration;

use gpui::prelude::*;
use gpui::{App, Context, Entity, EventEmitter, SharedString, Subscription, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable, Sizable, StyledExt as _};
use wyck::openapi::account::TradeSide;
use wyck::openapi::trading::NewOrderReq;

use super::account::{Account, Busy};
use super::math::{self, Contract, Pending};
use crate::app::chart::drawing::model::Dash;
use crate::app::chart::{ChartLine, LineId};
use crate::app::connection::ui;
use crate::app::multichart::SymbolRef;
use crate::app::{runtime, theme, widgets};

/// The kind of order the ticket sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Market,
    Limit,
    Stop,
}

/// Which line of the ticket a pending line on the chart stands for.
pub const LINE_ENTRY: u8 = 0;
pub const LINE_STOP: u8 = 1;
pub const LINE_TARGET: u8 = 2;

pub enum TicketEvent {
    /// The ticket's lines on the chart changed.
    LinesChanged,
    /// The user closed the ticket.
    Close,
}

pub struct OrderTicket {
    account: Entity<Account>,
    symbol: Option<SymbolRef>,
    buy: bool,
    kind: Kind,
    lots: Entity<InputState>,
    price: Entity<InputState>,
    stop_loss: Entity<InputState>,
    take_profit: Entity<InputState>,
    stop_on: bool,
    target_on: bool,
    one_click: bool,
    /// The margin a buy and a sell of the volume would use, once the server said.
    margin: Option<(f64, f64)>,
    /// Bumped per margin request, so an old answer is ignored.
    margin_epoch: u64,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TicketEvent> for OrderTicket {}

fn field(value: &str, window: &mut Window, cx: &mut Context<InputState>) -> InputState {
    InputState::new(window, cx).default_value(value.to_owned())
}

impl OrderTicket {
    pub fn new(
        account: Entity<Account>,
        lots: f64,
        one_click: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let lots_state = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(math::format_lots(lots))
                .min(0.0)
                .step(0.01)
        });
        let price = cx.new(|cx| field("", window, cx));
        let stop_loss = cx.new(|cx| field("", window, cx));
        let take_profit = cx.new(|cx| field("", window, cx));
        let mut subscriptions = vec![cx.observe(&account, |_this, _account, cx| cx.notify())];
        subscriptions.push(
            cx.subscribe(&lots_state, |this, _state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.request_margin(cx);
                    cx.notify();
                }
            }),
        );
        for state in [&price, &stop_loss, &take_profit] {
            subscriptions.push(
                cx.subscribe(state, |_this, _state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.emit(TicketEvent::LinesChanged);
                        cx.notify();
                    }
                }),
            );
        }
        Self {
            account,
            symbol: None,
            buy: true,
            kind: Kind::Market,
            lots: lots_state,
            price,
            stop_loss,
            take_profit,
            stop_on: false,
            target_on: false,
            one_click,
            margin: None,
            margin_epoch: 0,
            _subscriptions: subscriptions,
        }
    }

    pub fn symbol(&self) -> Option<&SymbolRef> {
        self.symbol.as_ref()
    }

    /// The ticket is for this symbol now.
    pub fn set_symbol(
        &mut self,
        symbol: Option<SymbolRef>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.symbol.as_ref().map(|s| s.id) == symbol.as_ref().map(|s| s.id) {
            self.symbol = symbol;
            return;
        }
        self.symbol = symbol;
        if let Some(symbol) = &self.symbol {
            let id = symbol.id;
            self.account
                .update(cx, |account, cx| account.ensure_contract(id, cx));
        }
        self.kind = Kind::Market;
        self.stop_on = false;
        self.target_on = false;
        for state in [&self.price, &self.stop_loss, &self.take_profit] {
            state.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.request_margin(cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// Fills the ticket from the chart: a side, a price (a pending order, limit or stop by where
    /// it is), and protection.
    pub fn prefill(
        &mut self,
        buy: bool,
        entry: Option<f64>,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buy = buy;
        let contract = self.contract(cx);
        let text = |value: Option<f64>| value.map(|v| contract.format_price(v)).unwrap_or_default();
        match entry {
            Some(price) => {
                let (bid, ask) = self.quote(cx);
                self.kind = match math::pending_kind(buy, price, bid, ask) {
                    Pending::Limit => Kind::Limit,
                    Pending::Stop => Kind::Stop,
                };
                let value = text(Some(price));
                self.price
                    .update(cx, |s, cx| s.set_value(value, window, cx));
            }
            None => self.kind = Kind::Market,
        }
        self.stop_on = stop_loss.is_some();
        self.target_on = take_profit.is_some();
        let (sl, tp) = (text(stop_loss), text(take_profit));
        self.stop_loss
            .update(cx, |s, cx| s.set_value(sl, window, cx));
        self.take_profit
            .update(cx, |s, cx| s.set_value(tp, window, cx));
        self.request_margin(cx);
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    /// A line of the ticket was dragged on the chart.
    pub fn line_moved(
        &mut self,
        line: u8,
        price: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self.contract(cx).format_price(price);
        let state = match line {
            LINE_ENTRY => &self.price,
            LINE_STOP => &self.stop_loss,
            _ => &self.take_profit,
        };
        state.update(cx, |s, cx| s.set_value(value, window, cx));
        cx.emit(TicketEvent::LinesChanged);
        cx.notify();
    }

    fn contract(&self, cx: &App) -> Contract {
        self.symbol
            .as_ref()
            .map(|s| {
                let mut contract = self.account.read(cx).book.contract(s.id);
                contract.digits = s.digits;
                contract
            })
            .unwrap_or_default()
    }

    fn quote(&self, cx: &App) -> (Option<f64>, Option<f64>) {
        self.symbol
            .as_ref()
            .map_or((None, None), |s| self.account.read(cx).quote(s.id))
    }

    fn read(state: &Entity<InputState>, cx: &App) -> Option<f64> {
        widgets::parse_number(&state.read(cx).value())
    }

    fn lots(&self, cx: &App) -> f64 {
        Self::read(&self.lots, cx).unwrap_or(0.0)
    }

    /// The price the order would enter at: its own for a pending order, the market's for a
    /// market order.
    fn entry(&self, cx: &App) -> Option<f64> {
        match self.kind {
            Kind::Market => {
                let (bid, ask) = self.quote(cx);
                if self.buy { ask } else { bid }
            }
            Kind::Limit | Kind::Stop => Self::read(&self.price, cx),
        }
    }

    /// The lines the ticket shows on the chart of its symbol.
    pub fn lines(&self, cx: &App) -> Vec<ChartLine> {
        let Some(_) = &self.symbol else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        let line = |id: u8, price: f64, color: u32, label: &str| ChartLine {
            id: LineId::Pending(id),
            price,
            color,
            label: label.to_owned(),
            detail: None,
            dash: Dash::Dashed,
            draggable: true,
            closable: false,
        };
        if self.kind != Kind::Market
            && let Some(price) = Self::read(&self.price, cx)
        {
            let label = match (self.buy, self.kind) {
                (true, Kind::Limit) => "Buy limit",
                (true, _) => "Buy stop",
                (false, Kind::Limit) => "Sell limit",
                (false, _) => "Sell stop",
            };
            lines.push(line(LINE_ENTRY, price, 0x5b8def, label));
        }
        if self.stop_on
            && let Some(price) = Self::read(&self.stop_loss, cx)
        {
            lines.push(line(LINE_STOP, price, 0xef5350, "SL"));
        }
        if self.target_on
            && let Some(price) = Self::read(&self.take_profit, cx)
        {
            lines.push(line(LINE_TARGET, price, 0x26a69a, "TP"));
        }
        lines
    }

    /// Asks the server what margin the volume would use, a moment after the last change.
    fn request_margin(&mut self, cx: &mut Context<Self>) {
        self.margin_epoch += 1;
        let epoch = self.margin_epoch;
        let Some(symbol) = self.symbol.as_ref().map(|s| s.id) else {
            self.margin = None;
            return;
        };
        let volume = self.contract(cx).volume_of_lots(self.lots(cx));
        let digits = self.account.read(cx).book.money_digits();
        let session = self.account.read(cx).session();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let current = this
                .update(cx, |this, _| this.margin_epoch == epoch)
                .unwrap_or(false);
            if !current {
                return;
            }
            let answer = runtime::spawn(async move {
                let client = session
                    .client()
                    .ok_or(wyck::openapi::OpenApiError::Closed)?;
                client
                    .account(session.account_id())
                    .margin()
                    .expected_margin(symbol, &[volume])
                    .await
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                if this.margin_epoch != epoch {
                    return;
                }
                this.margin = answer.ok().and_then(Result::ok).and_then(|m| {
                    m.first().map(|m| {
                        (
                            wyck::openapi::account::money(m.buy_margin, digits),
                            wyck::openapi::account::money(m.sell_margin, digits),
                        )
                    })
                });
                cx.notify();
            });
        })
        .detach();
    }

    /// The order as it would be sent, or why it cannot be.
    fn order(&self, cx: &App) -> Result<NewOrderReq, String> {
        let symbol = self.symbol.as_ref().ok_or("Pick a symbol first")?;
        let contract = self.contract(cx);
        let lots = self.lots(cx);
        if lots <= 0.0 {
            return Err("Set the volume".into());
        }
        let volume = contract.volume_of_lots(lots);
        let side = if self.buy {
            TradeSide::Buy
        } else {
            TradeSide::Sell
        };
        let entry = self
            .entry(cx)
            .ok_or_else(|| math::TicketProblem::NoPrice.to_string())?;
        let stop_loss = if self.stop_on {
            Some(Self::read(&self.stop_loss, cx).ok_or("Set the stop loss price")?)
        } else {
            None
        };
        let take_profit = if self.target_on {
            Some(Self::read(&self.take_profit, cx).ok_or("Set the take profit price")?)
        } else {
            None
        };
        math::check_protection(self.buy, entry, stop_loss, take_profit)
            .map_err(|p| p.to_string())?;
        let round = |p: f64| contract.round_price(p);
        let mut order = match self.kind {
            Kind::Market => {
                let mut order = NewOrderReq::market(symbol.id, side, volume);
                // A market order's protection is a distance from where it fills.
                order.relative_stop_loss = stop_loss.map(|sl| math::relative_distance(entry - sl));
                order.relative_take_profit =
                    take_profit.map(|tp| math::relative_distance(tp - entry));
                order
            }
            Kind::Limit => NewOrderReq::limit(symbol.id, side, volume, round(entry))
                .with_protection(stop_loss.map(round), take_profit.map(round)),
            Kind::Stop => NewOrderReq::stop(symbol.id, side, volume, round(entry))
                .with_protection(stop_loss.map(round), take_profit.map(round)),
        };
        order.time_in_force = (self.kind != Kind::Market).then_some(2);
        Ok(order)
    }

    fn describe(&self, cx: &App) -> String {
        let name = self
            .symbol
            .as_ref()
            .map_or_else(String::new, |s| s.name.to_string());
        let side = if self.buy { "Buy" } else { "Sell" };
        let lots = math::format_lots(self.lots(cx));
        let contract = self.contract(cx);
        match self.kind {
            Kind::Market => format!("{side} {lots} {name} at market"),
            Kind::Limit | Kind::Stop => format!(
                "{side} {lots} {name} {} at {}",
                if self.kind == Kind::Limit {
                    "limit"
                } else {
                    "stop"
                },
                self.entry(cx)
                    .map(|p| contract.format_price(p))
                    .unwrap_or_default()
            ),
        }
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let order = match self.order(cx) {
            Ok(order) => order,
            Err(message) => {
                crate::app::toast::show(
                    cx,
                    crate::app::toast::Kind::Warning,
                    "Order not sent",
                    message,
                );
                return;
            }
        };
        if self.one_click {
            self.account
                .update(cx, |account, cx| account.place(order, cx));
            return;
        }
        let account = self.account.clone();
        let text = self.describe(cx);
        confirm(window, cx, "Send this order?", text, move |_window, cx| {
            account.update(cx, |account, cx| account.place(order.clone(), cx));
        });
    }
}

/// Asks for a confirmation before `then` runs.
pub fn confirm(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    text: impl Into<SharedString>,
    then: impl Fn(&mut Window, &mut App) + 'static,
) {
    let (title, text) = (title.into(), text.into());
    let then = std::rc::Rc::new(then);
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let then = then.clone();
        dialog
            .title(title.clone())
            .w(px(420.))
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(theme::fg())
                    .child(text.clone()),
            )
            .footer(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("confirm-cancel")
                            .ghost()
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("confirm-ok")
                            .primary()
                            .label("Confirm")
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                then(window, cx);
                            }),
                    ),
            )
    });
}

impl Render for OrderTicket {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let contract = self.contract(cx);
        let (bid, ask) = self.quote(cx);
        let busy = self.account.read(cx).is_busy(Busy::Placing);
        let currency = self.account.read(cx).book.currency.clone();
        // What a pip is worth is in the currency the symbol is quoted in.
        let quote_currency = self
            .symbol
            .as_ref()
            .and_then(|s| {
                self.account
                    .read(cx)
                    .book
                    .quote_currency
                    .get(&s.id)
                    .cloned()
            })
            .unwrap_or_default();
        let name = self
            .symbol
            .as_ref()
            .map_or_else(|| SharedString::from("No symbol"), |s| s.name.clone());
        let price_text =
            |p: Option<f64>| p.map_or_else(|| "-".to_owned(), |p| contract.format_price(p));
        let spread = bid
            .zip(ask)
            .map(|(b, a)| format!("{:.1}", contract.pips(a - b)))
            .unwrap_or_default();
        let this = cx.entity();

        let side_button = |buy: bool| {
            let chosen = self.buy == buy;
            let color = if buy {
                theme::chart_up()
            } else {
                theme::chart_down()
            };
            let this = this.clone();
            div()
                .id(if buy { "ticket-buy" } else { "ticket-sell" })
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .gap_0p5()
                .py_2()
                .rounded_lg()
                .cursor_pointer()
                .border_1()
                .border_color(if chosen {
                    color
                } else {
                    theme::border_subtle()
                })
                .bg(if chosen {
                    let mut tint: gpui::Hsla = color.into();
                    tint.a = 0.16;
                    tint
                } else {
                    theme::bg().into()
                })
                .hover(|s| s.border_color(color))
                .on_click(move |_, _window, cx| {
                    this.update(cx, |t, cx| {
                        t.buy = buy;
                        cx.emit(TicketEvent::LinesChanged);
                        cx.notify();
                    });
                })
                .child(
                    div()
                        .text_size(px(11.))
                        .font_semibold()
                        .text_color(color)
                        .child(if buy { "BUY" } else { "SELL" }),
                )
                .child(
                    div()
                        .text_size(px(15.))
                        .font_semibold()
                        .text_color(theme::fg())
                        .child(price_text(if buy { ask } else { bid })),
                )
        };

        let kinds = ["Market", "Limit", "Stop"];
        let kind_index = match self.kind {
            Kind::Market => 0,
            Kind::Limit => 1,
            Kind::Stop => 2,
        };
        let kind_this = this.clone();
        let entry = self.entry(cx);
        let distance = |price: Option<f64>| -> String {
            match (price, entry) {
                (Some(p), Some(e)) => format!("{:.1} pips", contract.pips((p - e).abs())),
                _ => String::new(),
            }
        };
        let stop_price = Self::read(&self.stop_loss, cx);
        let target_price = Self::read(&self.take_profit, cx);
        let problem = self.order(cx).err();
        let lots = self.lots(cx);
        let volume = contract.volume_of_lots(lots);
        let units = volume as f64 / 100.0;
        let margin = self
            .margin
            .map(|(b, s)| math::format_money(if self.buy { b } else { s }, &currency));
        let action_color = if self.buy {
            theme::chart_up()
        } else {
            theme::chart_down()
        };
        let set_market_this = this.clone();

        let protection = |label: &'static str,
                          on: bool,
                          state: &Entity<InputState>,
                          price: Option<f64>,
                          toggle: fn(&mut OrderTicket)| {
            let this = this.clone();
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            Switch::new(SharedString::from(format!("ticket-{label}")))
                                .small()
                                .checked(on)
                                .label(label)
                                .on_click(move |_, _window, cx| {
                                    this.update(cx, |t, cx| {
                                        toggle(t);
                                        cx.emit(TicketEvent::LinesChanged);
                                        cx.notify();
                                    });
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::muted_fg())
                                .child(if on { distance(price) } else { String::new() }),
                        ),
                )
                .when(on, |el| {
                    el.child(gpui_kit::component::input::Input::new(state).small())
                })
        };

        div()
            .id("order-ticket")
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .h_full()
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::muted_fg())
                                    .child("NEW ORDER"),
                            )
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_semibold()
                                    .text_color(theme::fg())
                                    .child(name),
                            ),
                    )
                    .child(
                        Button::new("ticket-close")
                            .ghost()
                            .small()
                            .icon(IconName::X)
                            .tooltip("Close the ticket")
                            .on_click(cx.listener(|_this, _, _, cx| cx.emit(TicketEvent::Close))),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(side_button(false))
                    .child(side_button(true))
                    .when(!spread.is_empty(), |el| {
                        el.child(
                            div()
                                .absolute()
                                .top(px(-8.))
                                .left_0()
                                .right_0()
                                .flex()
                                .justify_center()
                                .child(
                                    div()
                                        .px_1p5()
                                        .rounded_sm()
                                        .bg(theme::surface())
                                        .border_1()
                                        .border_color(theme::border_subtle())
                                        .text_size(px(10.))
                                        .text_color(theme::muted_fg())
                                        .child(spread.clone()),
                                ),
                        )
                    }),
            )
            .child(widgets::segmented(
                "ticket-kind",
                &kinds,
                kind_index,
                move |choice, window, cx| {
                    kind_this.update(cx, |t, cx| {
                        t.kind = [Kind::Market, Kind::Limit, Kind::Stop][choice];
                        if t.kind != Kind::Market && Self::read(&t.price, cx).is_none() {
                            let (bid, ask) = t.quote(cx);
                            let market = if t.buy { ask } else { bid };
                            if let Some(price) = market {
                                let value = t.contract(cx).format_price(price);
                                t.price.update(cx, |s, cx| s.set_value(value, window, cx));
                            }
                        }
                        cx.emit(TicketEvent::LinesChanged);
                        cx.notify();
                    });
                },
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .text_size(px(12.))
                            .text_color(theme::muted_fg())
                            .child("Volume (lots)")
                            .child(format!("{} units", format_units(units))),
                    )
                    .child(gpui_kit::component::input::NumberInput::new(&self.lots).small()),
            )
            .when(self.kind != Kind::Market, |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .justify_between()
                                .items_center()
                                .text_size(px(12.))
                                .text_color(theme::muted_fg())
                                .child("Price")
                                .child(
                                    div()
                                        .id("ticket-at-market")
                                        .cursor_pointer()
                                        .text_color(theme::accent())
                                        .hover(|s| s.underline())
                                        .on_click(move |_, window, cx| {
                                            set_market_this.update(cx, |t, cx| {
                                                let (bid, ask) = t.quote(cx);
                                                if let Some(price) = if t.buy { ask } else { bid } {
                                                    let value = t.contract(cx).format_price(price);
                                                    t.price.update(cx, |s, cx| {
                                                        s.set_value(value, window, cx)
                                                    });
                                                }
                                            });
                                        })
                                        .child("At market"),
                                ),
                        )
                        .child(gpui_kit::component::input::Input::new(&self.price).small()),
                )
            })
            .child(protection(
                "Stop loss",
                self.stop_on,
                &self.stop_loss,
                stop_price,
                |t| t.stop_on = !t.stop_on,
            ))
            .child(protection(
                "Take profit",
                self.target_on,
                &self.take_profit,
                target_price,
                |t| t.target_on = !t.target_on,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(theme::surface())
                    .text_size(px(12.))
                    .child(summary_row("Margin", margin.unwrap_or_else(|| "-".into())))
                    .child(summary_row(
                        "Pip value",
                        format!(
                            "{} per pip",
                            math::format_money(contract.pip() * units, &quote_currency)
                        ),
                    )),
            )
            .children(problem.clone().map(|message| {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .text_color(theme::amber())
                    .child(ui::icon_colored(
                        IconName::TriangleAlert,
                        13.,
                        theme::amber(),
                    ))
                    .child(message)
            }))
            .child(
                Button::new("ticket-send")
                    .label(self.describe(cx))
                    .with_size(gpui_kit::component::Size::Large)
                    .disabled(problem.is_some() || busy)
                    .loading(busy)
                    .bg(action_color)
                    .text_color(theme::bg())
                    .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
            )
            .child(
                Switch::new("ticket-one-click")
                    .small()
                    .checked(self.one_click)
                    .label("One-click trading (no confirmation)")
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.one_click = *checked;
                        cx.emit(TicketEvent::LinesChanged);
                        cx.notify();
                    })),
            )
    }
}

impl OrderTicket {
    pub fn one_click(&self) -> bool {
        self.one_click
    }
}

fn summary_row(label: &'static str, value: String) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .justify_between()
        .child(div().text_color(theme::muted_fg()).child(label))
        .child(div().text_color(theme::fg()).child(value))
}

fn format_units(units: f64) -> String {
    math::format_money(units, "")
        .trim_end_matches("00")
        .trim_end_matches('.')
        .to_owned()
}
