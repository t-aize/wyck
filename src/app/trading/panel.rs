//! The account panel under the charts: the account's totals, and tabs for the open positions, the
//! working orders, the recent deals and the price alerts, with what can be done to each (close,
//! reverse, modify, cancel, delete).
//!
//! A row can be clicked to show its symbol on the active chart.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FontWeight, SharedString, Subscription, Window,
    div, px,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable, Sizable, StyledExt as _};
use wyck::openapi::account::{OrderType, money};

use super::account::{Account, Busy, Status};
use super::book::is_buy;
use super::math::{self, format_money};
use super::ticket::confirm;
use crate::app::alerts::{Alerts, Condition};
use crate::app::connection::ui;
use crate::app::{theme, widgets};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Positions,
    Orders,
    History,
    Alerts,
}

pub enum PanelEvent {
    /// Show this symbol on the active chart.
    ShowSymbol(i64),
    /// Fold the panel away.
    Hide,
}

pub struct AccountPanel {
    account: Entity<Account>,
    alerts: Entity<Alerts>,
    tab: Tab,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PanelEvent> for AccountPanel {}

/// Column widths of the tables.
const W_SYMBOL: f32 = 110.;
const W_SIDE: f32 = 56.;
const W_NUM: f32 = 92.;
const W_WIDE: f32 = 150.;
const W_ACTIONS: f32 = 96.;
/// The width under which the lists scroll sideways instead of squeezing their columns.
const TABLE_MIN: f32 = 1_080.;

impl AccountPanel {
    pub fn new(account: Entity<Account>, alerts: Entity<Alerts>, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![
            cx.observe(&account, |_this, _account, cx| cx.notify()),
            cx.observe(&alerts, |_this, _alerts, cx| cx.notify()),
        ];
        Self {
            account,
            alerts,
            tab: Tab::Positions,
            _subscriptions: subscriptions,
        }
    }

    pub fn show_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        cx.notify();
    }

    fn tab_button(&self, tab: Tab, label: String, cx: &mut Context<Self>) -> impl IntoElement {
        let chosen = self.tab == tab;
        div()
            .id(SharedString::from(format!("panel-tab-{tab:?}")))
            .h_full()
            .px_3()
            .flex()
            .items_center()
            .cursor_pointer()
            .text_size(px(12.))
            .font_weight(if chosen {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(if chosen {
                theme::fg()
            } else {
                theme::muted_fg()
            })
            .border_b_2()
            .border_color(if chosen {
                theme::accent()
            } else {
                gpui::rgba(0x00000000)
            })
            .hover(|s| s.text_color(theme::fg()))
            .on_click(cx.listener(move |this, _, _, cx| this.show_tab(tab, cx)))
            .child(label)
    }

    fn totals(&self, cx: &App) -> impl IntoElement {
        let account = self.account.read(cx);
        let currency = account.book.currency.clone();
        let summary = account.summary();
        let tone = |value: f64| {
            if value > 0.0 {
                theme::chart_up()
            } else if value < 0.0 {
                theme::chart_down()
            } else {
                theme::fg()
            }
        };
        let item = |label: &'static str, value: String, color: gpui::Rgba| {
            div()
                .flex()
                .flex_row()
                .items_baseline()
                .gap_1p5()
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::muted_fg())
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .font_semibold()
                        .text_color(color)
                        .child(value),
                )
        };
        let level = summary
            .margin_level
            .map_or_else(|| "-".to_owned(), |l| format!("{l:.0}%"));
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_4()
            .child(item(
                "Balance",
                format_money(summary.balance, &currency),
                theme::fg(),
            ))
            .child(item(
                "Margin",
                format_money(summary.margin, &currency),
                theme::fg(),
            ))
            .child(item(
                "Level",
                level,
                match summary.margin_level {
                    Some(l) if l < 100.0 => theme::chart_down(),
                    Some(l) if l < 300.0 => theme::amber(),
                    _ => theme::fg(),
                },
            ))
            .child(item(
                "Free",
                format_money(summary.free_margin, &currency),
                theme::fg(),
            ))
            .child(item(
                "Equity",
                format_money(summary.equity, &currency),
                theme::fg(),
            ))
            .child(item(
                "Profit",
                format_money(summary.unrealized, &currency),
                tone(summary.unrealized),
            ))
    }

    fn header_cell(label: &'static str, width: f32, right: bool) -> impl IntoElement {
        div()
            .w(px(width))
            .flex_none()
            .when(right, |el| el.flex().justify_end())
            .child(label)
    }

    fn cell(
        text: impl Into<SharedString>,
        width: f32,
        right: bool,
        color: gpui::Rgba,
    ) -> impl IntoElement {
        div()
            .w(px(width))
            .flex_none()
            .truncate()
            .text_color(color)
            .when(right, |el| el.flex().justify_end())
            .child(text.into())
    }

    fn row(id: SharedString) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(30.))
            .px_3()
            .text_size(px(12.))
            .border_b_1()
            .border_color(theme::border_hairline())
            .hover(|s| s.bg(theme::surface_hover()))
    }

    fn header(cells: Vec<AnyElement>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(26.))
            .px_3()
            .text_size(px(11.))
            .text_color(theme::muted_fg())
            .border_b_1()
            .border_color(theme::border_hairline())
            .children(cells)
    }

    fn empty(text: &'static str) -> AnyElement {
        div()
            .py_6()
            .flex()
            .justify_center()
            .text_size(px(12.))
            .text_color(theme::muted_fg())
            .child(text)
            .into_any_element()
    }

    fn icon_action(
        id: SharedString,
        icon: IconName,
        tip: &'static str,
        busy: bool,
        on_click: impl Fn(&mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        Button::new(id)
            .cursor_pointer()
            .when(busy, |button| button.cursor_not_allowed())
            .ghost()
            .xsmall()
            .icon(icon)
            .tooltip(tip)
            .loading(busy)
            .disabled(busy)
            .on_click(move |_, window, cx| on_click(window, cx))
    }

    fn positions(&self, cx: &mut Context<Self>) -> AnyElement {
        let account = self.account.read(cx);
        if account.book.positions.is_empty() {
            return Self::empty("No open position.");
        }
        let currency = account.book.currency.clone();
        let digits = account.book.money_digits();
        let mut rows: Vec<AnyElement> = vec![
            Self::header(vec![
                Self::header_cell("Symbol", W_SYMBOL, false).into_any_element(),
                Self::header_cell("Side", W_SIDE, false).into_any_element(),
                Self::header_cell("Lots", W_NUM, true).into_any_element(),
                Self::header_cell("Entry", W_NUM, true).into_any_element(),
                Self::header_cell("Price", W_NUM, true).into_any_element(),
                Self::header_cell("Stop loss", W_NUM, true).into_any_element(),
                Self::header_cell("Take profit", W_NUM, true).into_any_element(),
                Self::header_cell("Swap", W_NUM, true).into_any_element(),
                Self::header_cell("Profit", W_WIDE, true).into_any_element(),
                Self::header_cell("", W_ACTIONS, true).into_any_element(),
            ])
            .into_any_element(),
        ];
        for position in account.book.positions.values() {
            let id = position.position_id;
            let symbol = position.trade_data.symbol_id;
            let contract = account.book.contract(symbol);
            let buy = is_buy(position.trade_data.trade_side);
            let (bid, ask) = account.quote(symbol);
            let now = if buy { bid } else { ask };
            let price =
                |p: Option<f64>| p.map_or_else(|| "-".to_owned(), |p| contract.format_price(p));
            let profit = account.net_profit(id);
            let profit_color = match profit {
                Some(p) if p > 0.0 => theme::chart_up(),
                Some(p) if p < 0.0 => theme::chart_down(),
                _ => theme::fg(),
            };
            let pips = match (position.price, now) {
                (Some(entry), Some(now)) => {
                    let move_ = if buy { now - entry } else { entry - now };
                    format!(" ({:+.1} pips)", contract.pips(move_))
                }
                _ => String::new(),
            };
            let closing = account.is_busy(Busy::Closing(id));
            let (close_account, reverse_account, edit_account) = (
                self.account.clone(),
                self.account.clone(),
                self.account.clone(),
            );
            let describe = format!(
                "{} {} {}",
                if buy { "Buy" } else { "Sell" },
                math::format_lots(contract.lots_of_volume(position.trade_data.volume)),
                account.book.name(symbol)
            );
            let close_text = describe.clone();
            rows.push(
                Self::row(SharedString::from(format!("position-{id}")))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_this, _, _, cx| cx.emit(PanelEvent::ShowSymbol(symbol))),
                    )
                    .child(Self::cell(
                        account.book.name(symbol),
                        W_SYMBOL,
                        false,
                        theme::fg(),
                    ))
                    .child(Self::cell(
                        if buy { "Buy" } else { "Sell" },
                        W_SIDE,
                        false,
                        if buy {
                            theme::chart_up()
                        } else {
                            theme::chart_down()
                        },
                    ))
                    .child(Self::cell(
                        math::format_lots(contract.lots_of_volume(position.trade_data.volume)),
                        W_NUM,
                        true,
                        theme::fg(),
                    ))
                    .child(Self::cell(price(position.price), W_NUM, true, theme::fg()))
                    .child(Self::cell(price(now), W_NUM, true, theme::fg()))
                    .child(Self::cell(
                        price(position.stop_loss),
                        W_NUM,
                        true,
                        theme::muted_fg(),
                    ))
                    .child(Self::cell(
                        price(position.take_profit),
                        W_NUM,
                        true,
                        theme::muted_fg(),
                    ))
                    .child(Self::cell(
                        position
                            .swap
                            .map_or_else(|| "-".to_owned(), |s| format_money(money(s, digits), "")),
                        W_NUM,
                        true,
                        theme::muted_fg(),
                    ))
                    .child(Self::cell(
                        profit.map_or_else(
                            || "...".to_owned(),
                            |p| format!("{}{pips}", format_money(p, &currency)),
                        ),
                        W_WIDE,
                        true,
                        profit_color,
                    ))
                    .child(
                        div()
                            .w(px(W_ACTIONS))
                            .flex_none()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap_0p5()
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .child(Self::icon_action(
                                SharedString::from(format!("position-edit-{id}")),
                                IconName::Pencil,
                                "Stop loss and take profit",
                                false,
                                move |window, cx| {
                                    open_protection(
                                        edit_account.clone(),
                                        Target::Position(id),
                                        window,
                                        cx,
                                    )
                                },
                            ))
                            .child(Self::icon_action(
                                SharedString::from(format!("position-reverse-{id}")),
                                IconName::ArrowUpDown,
                                "Reverse",
                                false,
                                move |window, cx| {
                                    let account = reverse_account.clone();
                                    confirm(
                                        window,
                                        cx,
                                        "Reverse this position?",
                                        describe.clone(),
                                        move |_, cx| {
                                            account.update(cx, |a, cx| a.reverse_position(id, cx));
                                        },
                                    );
                                },
                            ))
                            .child(Self::icon_action(
                                SharedString::from(format!("position-close-{id}")),
                                IconName::X,
                                "Close",
                                closing,
                                move |window, cx| {
                                    let account = close_account.clone();
                                    confirm(
                                        window,
                                        cx,
                                        "Close this position?",
                                        close_text.clone(),
                                        move |_, cx| {
                                            account
                                                .update(cx, |a, cx| a.close_position(id, None, cx));
                                        },
                                    );
                                },
                            )),
                    )
                    .into_any_element(),
            );
        }
        div().flex().flex_col().children(rows).into_any_element()
    }

    fn orders(&self, cx: &mut Context<Self>) -> AnyElement {
        let account = self.account.read(cx);
        if account.book.orders.is_empty() {
            return Self::empty("No working order.");
        }
        let mut rows: Vec<AnyElement> = vec![
            Self::header(vec![
                Self::header_cell("Symbol", W_SYMBOL, false).into_any_element(),
                Self::header_cell("Side", W_SIDE, false).into_any_element(),
                Self::header_cell("Type", W_NUM, false).into_any_element(),
                Self::header_cell("Lots", W_NUM, true).into_any_element(),
                Self::header_cell("Price", W_NUM, true).into_any_element(),
                Self::header_cell("Distance", W_NUM, true).into_any_element(),
                Self::header_cell("Stop loss", W_NUM, true).into_any_element(),
                Self::header_cell("Take profit", W_NUM, true).into_any_element(),
                Self::header_cell("", W_ACTIONS, true).into_any_element(),
            ])
            .into_any_element(),
        ];
        for order in account.book.orders.values() {
            let id = order.order_id;
            let symbol = order.trade_data.symbol_id;
            let contract = account.book.contract(symbol);
            let buy = is_buy(order.trade_data.trade_side);
            let price = order.limit_price.or(order.stop_price);
            let (bid, ask) = account.quote(symbol);
            let market = if buy { ask } else { bid };
            let distance = price.zip(market).map_or_else(String::new, |(p, m)| {
                format!("{:.1} pips", contract.pips((p - m).abs()))
            });
            let show =
                |p: Option<f64>| p.map_or_else(|| "-".to_owned(), |p| contract.format_price(p));
            let cancelling = account.is_busy(Busy::Cancelling(id));
            let (cancel_account, edit_account) = (self.account.clone(), self.account.clone());
            rows.push(
                Self::row(SharedString::from(format!("order-{id}")))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_this, _, _, cx| cx.emit(PanelEvent::ShowSymbol(symbol))),
                    )
                    .child(Self::cell(
                        account.book.name(symbol),
                        W_SYMBOL,
                        false,
                        theme::fg(),
                    ))
                    .child(Self::cell(
                        if buy { "Buy" } else { "Sell" },
                        W_SIDE,
                        false,
                        if buy {
                            theme::chart_up()
                        } else {
                            theme::chart_down()
                        },
                    ))
                    .child(Self::cell(
                        capitalized(order.kind().map_or("order", OrderType::label)),
                        W_NUM,
                        false,
                        theme::muted_fg(),
                    ))
                    .child(Self::cell(
                        math::format_lots(contract.lots_of_volume(order.trade_data.volume)),
                        W_NUM,
                        true,
                        theme::fg(),
                    ))
                    .child(Self::cell(show(price), W_NUM, true, theme::fg()))
                    .child(Self::cell(distance, W_NUM, true, theme::muted_fg()))
                    .child(Self::cell(
                        show(order.stop_loss),
                        W_NUM,
                        true,
                        theme::muted_fg(),
                    ))
                    .child(Self::cell(
                        show(order.take_profit),
                        W_NUM,
                        true,
                        theme::muted_fg(),
                    ))
                    .child(
                        div()
                            .w(px(W_ACTIONS))
                            .flex_none()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap_0p5()
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .child(Self::icon_action(
                                SharedString::from(format!("order-edit-{id}")),
                                IconName::Pencil,
                                "Price, stop loss and take profit",
                                false,
                                move |window, cx| {
                                    open_protection(
                                        edit_account.clone(),
                                        Target::Order(id),
                                        window,
                                        cx,
                                    )
                                },
                            ))
                            .child(Self::icon_action(
                                SharedString::from(format!("order-cancel-{id}")),
                                IconName::X,
                                "Cancel",
                                cancelling,
                                move |_window, cx| {
                                    cancel_account.update(cx, |a, cx| a.cancel_order(id, cx));
                                },
                            )),
                    )
                    .into_any_element(),
            );
        }
        div().flex().flex_col().children(rows).into_any_element()
    }

    fn history(&self, cx: &mut Context<Self>) -> AnyElement {
        let account = self.account.read(cx);
        if account.book.deals.is_empty() {
            return Self::empty("No trade in the last seven days.");
        }
        let currency = account.book.currency.clone();
        let mut rows: Vec<AnyElement> = vec![
            Self::header(vec![
                Self::header_cell("Time", W_WIDE, false).into_any_element(),
                Self::header_cell("Symbol", W_SYMBOL, false).into_any_element(),
                Self::header_cell("Side", W_SIDE, false).into_any_element(),
                Self::header_cell("Lots", W_NUM, true).into_any_element(),
                Self::header_cell("Price", W_NUM, true).into_any_element(),
                Self::header_cell("Commission", W_NUM, true).into_any_element(),
                Self::header_cell("Profit", W_WIDE, true).into_any_element(),
            ])
            .into_any_element(),
        ];
        for deal in account.book.deals.iter().take(200) {
            let contract = account.book.contract(deal.symbol_id);
            let buy = is_buy(deal.trade_side);
            let digits = deal.money_digits.and_then(|d| u32::try_from(d).ok());
            let profit = deal
                .close_position_detail
                .as_ref()
                .and_then(|d| d.get("grossProfit"))
                .and_then(serde_json::Value::as_i64)
                .map(|p| money(p, digits));
            let time = crate::app::chart::zone::Zone::Local.shift(deal.execution_timestamp);
            let when = time::OffsetDateTime::from_unix_timestamp(time.div_euclid(1_000))
                .map(|t| {
                    let month = t.month().to_string();
                    format!(
                        "{} {} {:02}:{:02}:{:02}",
                        &month[..3.min(month.len())],
                        t.day(),
                        t.hour(),
                        t.minute(),
                        t.second()
                    )
                })
                .unwrap_or_default();
            let symbol = deal.symbol_id;
            rows.push(
                Self::row(SharedString::from(format!("deal-{}", deal.deal_id)))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_this, _, _, cx| cx.emit(PanelEvent::ShowSymbol(symbol))),
                    )
                    .child(Self::cell(when, W_WIDE, false, theme::muted_fg()))
                    .child(Self::cell(
                        account.book.name(symbol),
                        W_SYMBOL,
                        false,
                        theme::fg(),
                    ))
                    .child(Self::cell(
                        if buy { "Buy" } else { "Sell" },
                        W_SIDE,
                        false,
                        if buy {
                            theme::chart_up()
                        } else {
                            theme::chart_down()
                        },
                    ))
                    .child(Self::cell(
                        math::format_lots(contract.lots_of_volume(deal.filled_volume)),
                        W_NUM,
                        true,
                        theme::fg(),
                    ))
                    .child(Self::cell(
                        deal.execution_price
                            .map_or_else(|| "-".to_owned(), |p| contract.format_price(p)),
                        W_NUM,
                        true,
                        theme::fg(),
                    ))
                    .child(Self::cell(
                        deal.commission
                            .map_or_else(|| "-".to_owned(), |c| format_money(money(c, digits), "")),
                        W_NUM,
                        true,
                        theme::muted_fg(),
                    ))
                    .child(Self::cell(
                        profit.map_or_else(|| "-".to_owned(), |p| format_money(p, &currency)),
                        W_WIDE,
                        true,
                        match profit {
                            Some(p) if p > 0.0 => theme::chart_up(),
                            Some(p) if p < 0.0 => theme::chart_down(),
                            _ => theme::muted_fg(),
                        },
                    ))
                    .into_any_element(),
            );
        }
        div().flex().flex_col().children(rows).into_any_element()
    }

    fn alert_rows(&self, cx: &mut Context<Self>) -> AnyElement {
        let alerts = self.alerts.read(cx);
        if alerts.book().alerts.is_empty() {
            return Self::empty(
                "No alert. Right click a chart, or press Alt+A, to add one at a price.",
            );
        }
        let mut rows: Vec<AnyElement> = vec![
            Self::header(vec![
                Self::header_cell("Symbol", W_SYMBOL, false).into_any_element(),
                Self::header_cell("Condition", W_WIDE, false).into_any_element(),
                Self::header_cell("Price", W_NUM, true).into_any_element(),
                Self::header_cell("State", W_WIDE, false).into_any_element(),
                Self::header_cell("Message", W_WIDE * 1.5, false).into_any_element(),
                Self::header_cell("", W_ACTIONS, true).into_any_element(),
            ])
            .into_any_element(),
        ];
        for alert in &alerts.book().alerts {
            let id = alert.id;
            let digits = alerts.digits.get(&alert.symbol_id).copied().unwrap_or(5);
            let state = match (alert.active, alert.fired_at) {
                (true, _) if alert.repeat => "Watching (repeats)".to_owned(),
                (true, _) => "Watching".to_owned(),
                (false, Some(_)) => "Fired".to_owned(),
                (false, None) => "Paused".to_owned(),
            };
            let (edit_alerts, toggle_alerts, delete_alerts) = (
                self.alerts.clone(),
                self.alerts.clone(),
                self.alerts.clone(),
            );
            let symbol = alert.symbol_id;
            rows.push(
                Self::row(SharedString::from(format!("alert-{id}")))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_this, _, _, cx| cx.emit(PanelEvent::ShowSymbol(symbol))),
                    )
                    .child(Self::cell(
                        alert.symbol.clone(),
                        W_SYMBOL,
                        false,
                        theme::fg(),
                    ))
                    .child(Self::cell(
                        alert.condition.label(),
                        W_WIDE,
                        false,
                        theme::muted_fg(),
                    ))
                    .child(Self::cell(
                        format!("{:.*}", digits as usize, alert.price),
                        W_NUM,
                        true,
                        theme::fg(),
                    ))
                    .child(Self::cell(
                        state,
                        W_WIDE,
                        false,
                        if alert.active {
                            theme::emerald()
                        } else {
                            theme::muted_fg()
                        },
                    ))
                    .child(Self::cell(
                        alert.message.clone(),
                        W_WIDE * 1.5,
                        false,
                        theme::muted_fg(),
                    ))
                    .child(
                        div()
                            .w(px(W_ACTIONS))
                            .flex_none()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap_0p5()
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .child(Self::icon_action(
                                SharedString::from(format!("alert-edit-{id}")),
                                IconName::Pencil,
                                "Edit",
                                false,
                                move |window, cx| open_alert(edit_alerts.clone(), id, window, cx),
                            ))
                            .child(Self::icon_action(
                                SharedString::from(format!("alert-toggle-{id}")),
                                if alert.active {
                                    IconName::BellOff
                                } else {
                                    IconName::BellRing
                                },
                                if alert.active { "Pause" } else { "Watch again" },
                                false,
                                move |_window, cx| {
                                    toggle_alerts.update(cx, |alerts, cx| {
                                        alerts.edit(cx, |book| {
                                            if let Some(alert) = book.get_mut(id) {
                                                alert.active = !alert.active;
                                                alert.fired_at = None;
                                            }
                                        });
                                    });
                                },
                            ))
                            .child(Self::icon_action(
                                SharedString::from(format!("alert-delete-{id}")),
                                IconName::Trash,
                                "Delete",
                                false,
                                move |_window, cx| {
                                    delete_alerts.update(cx, |alerts, cx| {
                                        alerts.edit(cx, |book| book.remove(id));
                                    });
                                },
                            )),
                    )
                    .into_any_element(),
            );
        }
        div().flex().flex_col().children(rows).into_any_element()
    }
}

impl Render for AccountPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (positions, orders, status) = {
            let account = self.account.read(cx);
            (
                account.book.positions.len(),
                account.book.orders.len(),
                account.status.clone(),
            )
        };
        let alerts = self
            .alerts
            .read(cx)
            .book()
            .alerts
            .iter()
            .filter(|a| a.active)
            .count();
        let count = |label: &str, n: usize| {
            if n > 0 {
                format!("{label} ({n})")
            } else {
                label.to_owned()
            }
        };
        // The alerts do not depend on the account, so they show while it loads or failed.
        let body = match &status {
            _ if self.tab == Tab::Alerts => self.alert_rows(cx),
            Status::Loading => div()
                .py_6()
                .flex()
                .justify_center()
                .text_size(px(12.))
                .text_color(theme::muted_fg())
                .child("Reading the account...")
                .into_any_element(),
            Status::Failed(message) => div()
                .py_6()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .text_size(px(12.))
                .child(
                    div()
                        .text_color(theme::destructive())
                        .child("Could not read the account"),
                )
                .child(div().text_color(theme::muted_fg()).child(message.clone()))
                .into_any_element(),
            Status::Ready => match self.tab {
                Tab::Positions => self.positions(cx),
                Tab::Orders => self.orders(cx),
                Tab::History => self.history(cx),
                Tab::Alerts => self.alert_rows(cx),
            },
        };
        let close_all_account = self.account.clone();
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::bg())
            .child(
                div()
                    .flex_none()
                    .h(px(36.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .pr_2()
                    .border_b_1()
                    .border_color(theme::border_hairline())
                    .child(
                        div()
                            .flex_none()
                            .h_full()
                            .flex()
                            .flex_row()
                            .child(self.tab_button(Tab::Positions, count("Positions", positions), cx))
                            .child(self.tab_button(Tab::Orders, count("Orders", orders), cx))
                            .child(self.tab_button(Tab::History, "History".into(), cx))
                            .child(self.tab_button(Tab::Alerts, count("Alerts", alerts), cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .gap_3()
                            // When the panel is narrow the totals give way from the left, where
                            // the least needed are.
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .flex()
                                    .flex_row()
                                    .justify_end()
                                    .child(self.totals(cx)),
                            )
                            .when(self.tab == Tab::Positions && positions > 0, |el| {
                                el.child(
                                    Button::new("close-all-positions")
                                        .cursor_pointer()
                                        .ghost()
                                        .xsmall()
                                        .label("Close all")
                                        .on_click(move |_, window, cx| {
                                            let account = close_all_account.clone();
                                            confirm(
                                                window,
                                                cx,
                                                "Close every position?",
                                                "Every open position of the account is closed at the market.",
                                                move |_, cx| account.update(cx, |a, cx| a.close_all(None, cx)),
                                            );
                                        }),
                                )
                            })
                            .child(
                                Button::new("panel-hide")
                                    .cursor_pointer()
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::ChevronDown)
                                    .tooltip("Hide the panel")
                                    .on_click(cx.listener(|_this, _, _, cx| cx.emit(PanelEvent::Hide))),
                            ),
                    ),
            )
            .child(
                div()
                    .id("panel-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_scroll()
                    .child(div().w_full().min_w(px(TABLE_MIN)).child(body)),
            )
    }
}

// ---- modifying a position or an order ----

#[derive(Debug, Clone, Copy)]
enum Target {
    Position(i64),
    Order(i64),
}

struct ProtectionEditor {
    account: Entity<Account>,
    target: Target,
    price: Entity<InputState>,
    stop_loss: Entity<InputState>,
    take_profit: Entity<InputState>,
}

fn open_protection(account: Entity<Account>, target: Target, window: &mut Window, cx: &mut App) {
    let (price, stop_loss, take_profit, digits) = {
        let book = &account.read(cx).book;
        match target {
            Target::Position(id) => match book.positions.get(&id) {
                Some(p) => (
                    None,
                    p.stop_loss,
                    p.take_profit,
                    book.contract(p.trade_data.symbol_id),
                ),
                None => return,
            },
            Target::Order(id) => match book.orders.get(&id) {
                Some(o) => (
                    o.limit_price.or(o.stop_price),
                    o.stop_loss,
                    o.take_profit,
                    book.contract(o.trade_data.symbol_id),
                ),
                None => return,
            },
        }
    };
    let text = |v: Option<f64>| v.map(|v| digits.format_price(v)).unwrap_or_default();
    let editor = cx.new(|cx| ProtectionEditor {
        account,
        target,
        price: cx.new(|cx| InputState::new(window, cx).default_value(text(price))),
        stop_loss: cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(text(stop_loss))
                .placeholder("None")
        }),
        take_profit: cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(text(take_profit))
                .placeholder("None")
        }),
    });
    let title = match target {
        Target::Position(_) => "Modify the position",
        Target::Order(_) => "Modify the order",
    };
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let editor = editor.clone();
        let save = editor.clone();
        dialog.title(title).w(px(380.)).child(editor).footer(
            div()
                .flex()
                .flex_row()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("protection-cancel")
                        .cursor_pointer()
                        .ghost()
                        .label("Cancel")
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .child(
                    Button::new("protection-save")
                        .cursor_pointer()
                        .primary()
                        .label("Save")
                        .on_click(move |_, window, cx| {
                            save.update(cx, |editor, cx| editor.save(cx));
                            window.close_dialog(cx);
                        }),
                ),
        )
    });
}

impl ProtectionEditor {
    fn save(&self, cx: &mut App) {
        let read = |state: &Entity<InputState>| widgets::parse_number(&state.read(cx).value());
        let (price, sl, tp) = (
            read(&self.price),
            read(&self.stop_loss),
            read(&self.take_profit),
        );
        let target = self.target;
        self.account.update(cx, |account, cx| match target {
            Target::Position(id) => account.protect_position(id, sl, tp, cx),
            Target::Order(id) => {
                let current = account
                    .book
                    .orders
                    .get(&id)
                    .and_then(|o| o.limit_price.or(o.stop_price));
                if let Some(price) = price.filter(|p| Some(*p) != current) {
                    account.move_order(id, price, cx);
                }
                account.protect_order(id, sl, tp, cx);
            }
        });
    }
}

impl Render for ProtectionEditor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .when(matches!(self.target, Target::Order(_)), |el| {
                el.child(widgets::row(
                    "Price",
                    div().w(px(160.)).child(Input::new(&self.price).small()),
                ))
            })
            .child(widgets::row(
                "Stop loss",
                div()
                    .w(px(160.))
                    .child(Input::new(&self.stop_loss).small().cleanable(true)),
            ))
            .child(widgets::row(
                "Take profit",
                div()
                    .w(px(160.))
                    .child(Input::new(&self.take_profit).small().cleanable(true)),
            ))
            .child(
                div()
                    .pt_1()
                    .text_size(px(11.))
                    .text_color(theme::muted_fg())
                    .child(
                        "Leave a field empty to remove it. Lines can also be dragged on the chart.",
                    ),
            )
    }
}

// ---- editing an alert ----

struct AlertEditor {
    alerts: Entity<Alerts>,
    id: u64,
    price: Entity<InputState>,
    message: Entity<InputState>,
    condition: Condition,
    repeat: bool,
}

pub fn open_alert(alerts: Entity<Alerts>, id: u64, window: &mut Window, cx: &mut App) {
    let Some(alert) = alerts
        .read(cx)
        .book()
        .alerts
        .iter()
        .find(|a| a.id == id)
        .cloned()
    else {
        return;
    };
    let digits = alerts
        .read(cx)
        .digits
        .get(&alert.symbol_id)
        .copied()
        .unwrap_or(5);
    let editor = cx.new(|cx| AlertEditor {
        alerts,
        id,
        price: cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(format!("{:.*}", digits as usize, alert.price))
        }),
        message: cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(alert.message.clone())
                .placeholder("What to say when it fires")
        }),
        condition: alert.condition,
        repeat: alert.repeat,
    });
    let title = format!("Alert on {}", alert.symbol);
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let save = editor.clone();
        dialog
            .title(title.clone())
            .w(px(420.))
            .child(editor.clone())
            .footer(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("alert-cancel")
                            .cursor_pointer()
                            .ghost()
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(Button::new("alert-save").primary().label("Save").on_click(
                        move |_, window, cx| {
                            save.update(cx, |editor, cx| editor.save(cx));
                            window.close_dialog(cx);
                        },
                    )),
            )
    });
}

impl AlertEditor {
    fn save(&self, cx: &mut App) {
        let price = widgets::parse_number(&self.price.read(cx).value());
        let message = self.message.read(cx).value().to_string();
        let (id, condition, repeat) = (self.id, self.condition, self.repeat);
        self.alerts.update(cx, |alerts, cx| {
            alerts.edit(cx, |book| {
                if let Some(alert) = book.get_mut(id) {
                    if let Some(price) = price.filter(|p| *p > 0.0) {
                        alert.price = price;
                    }
                    alert.message = message;
                    alert.condition = condition;
                    alert.repeat = repeat;
                    alert.active = true;
                    alert.fired_at = None;
                }
            });
        });
    }
}

impl Render for AlertEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let labels: Vec<&str> = Condition::ALL.iter().map(|c| c.label()).collect();
        let index = Condition::ALL
            .iter()
            .position(|c| *c == self.condition)
            .unwrap_or(0);
        let this = cx.entity();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(widgets::row(
                "Condition",
                widgets::segmented(
                    "alert-condition",
                    &labels,
                    index,
                    move |choice, _window, cx| {
                        this.update(cx, |e, cx| {
                            e.condition = Condition::ALL[choice];
                            cx.notify();
                        });
                    },
                ),
            ))
            .child(widgets::row(
                "Price",
                div().w(px(160.)).child(Input::new(&self.price).small()),
            ))
            .child(div().pt_1().child(Input::new(&self.message).small()))
            .child(widgets::row(
                "Keep watching after it fires",
                Switch::new("alert-repeat")
                    .cursor_pointer()
                    .checked(self.repeat)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.repeat = *checked;
                        cx.notify();
                    })),
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .text_size(px(11.))
                    .text_color(theme::muted_fg())
                    .child(ui::icon_colored(IconName::Info, 12., theme::muted_fg()))
                    .child("The line of an alert can also be dragged on the chart."),
            )
    }
}

/// A word with its first letter capital.
fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
