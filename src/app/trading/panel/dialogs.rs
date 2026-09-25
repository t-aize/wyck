//! The dialogs the account panel opens: the one that modifies a position or an order, and the one
//! that edits an alert.

use gpui::prelude::*;
use gpui::{App, Entity, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;

use crate::app::alerts::{Alerts, Condition};
use crate::app::connection::ui;
use crate::app::trading::account::Account;
use crate::app::{theme, widgets};

// ---- modifying a position or an order ----

#[derive(Debug, Clone, Copy)]
pub enum Target {
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

pub fn open_protection(
    account: Entity<Account>,
    target: Target,
    window: &mut Window,
    cx: &mut App,
) {
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
