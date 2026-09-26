//! The dialogs the account panel opens: the one that modifies a position or an order, and the one
//! that edits an alert.

use gpui::prelude::*;
use gpui::{App, Context, Entity, Window};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Input, InputState};

use crate::app::alerts::{Alerts, Condition};
use crate::app::settings_ui::{self as ui, Head};
use crate::app::trading::account::Account;
use crate::app::{modal, widgets};

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
    modal::open(editor, modal::Options::new(420.0, 400.0), window, cx);
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, subtitle) = match self.target {
            Target::Position(_) => ("Modify the position", "Stop loss and take profit"),
            Target::Order(_) => ("Modify the order", "Price, stop loss and take profit"),
        };
        let head = Head {
            icon: IconName::ShieldCheck,
            title: title.into(),
            subtitle: subtitle.into(),
        };
        let mut rows = Vec::new();
        if matches!(self.target, Target::Order(_)) {
            rows.push(ui::field("Price", None, ui::text_field(&self.price, 160.)));
        }
        rows.push(ui::field(
            "Stop loss",
            Some("Leave it empty to remove it"),
            ui::text_field(&self.stop_loss, 160.),
        ));
        rows.push(ui::field(
            "Take profit",
            Some("Leave it empty to remove it"),
            ui::text_field(&self.take_profit, 160.),
        ));
        let body = ui::page()
            .child(ui::group(IconName::Target, "Levels", rows))
            .child(ui::note("Lines can also be dragged on the chart."));
        let save = cx.entity();
        let footer = ui::footer(
            Vec::new(),
            vec![
                ui::action("protection-cancel", "Cancel", None, false, modal::close)
                    .into_any_element(),
                ui::action("protection-save", "Save", None, true, move |window, cx| {
                    save.update(cx, |editor, cx| editor.save(cx));
                    modal::close(window, cx);
                })
                .into_any_element(),
            ],
        );
        ui::dialog(head, modal::dismiss, body, footer)
    }
}

// ---- editing an alert ----

struct AlertEditor {
    alerts: Entity<Alerts>,
    id: u64,
    symbol: String,
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
        symbol: alert.symbol.clone(),
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
    modal::open(editor, modal::Options::new(460.0, 480.0), window, cx);
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
        let (condition, repeat, save) = (cx.entity(), cx.entity(), cx.entity());
        let head = Head {
            icon: IconName::BellRing,
            title: format!("Alert on {}", self.symbol).into(),
            subtitle: "Fires when the price crosses the level".into(),
        };
        let level = vec![
            ui::field(
                "Condition",
                None,
                widgets::segmented(
                    "alert-condition",
                    &labels,
                    index,
                    move |choice, _window, cx| {
                        condition.update(cx, |e, cx| {
                            e.condition = Condition::ALL[choice];
                            cx.notify();
                        });
                    },
                ),
            ),
            ui::field("Price", None, ui::text_field(&self.price, 160.)),
        ];
        let message = vec![ui::block(Input::new(&self.message).small())];
        let options = vec![ui::field(
            "Keep watching after it fires",
            Some("The alert stays on and can fire again"),
            ui::toggle("alert-repeat", self.repeat, move |on, _window, cx| {
                repeat.update(cx, |e, cx| {
                    e.repeat = on;
                    cx.notify();
                });
            }),
        )];
        let body = ui::page()
            .child(ui::group(IconName::Target, "Level", level))
            .child(ui::group(IconName::MessageSquare, "Message", message))
            .child(ui::group(IconName::SlidersHorizontal, "Options", options))
            .child(ui::note(
                "The line of an alert can also be dragged on the chart.",
            ));
        let footer = ui::footer(
            Vec::new(),
            vec![
                ui::action("alert-cancel", "Cancel", None, false, modal::close).into_any_element(),
                ui::action("alert-save", "Save", None, true, move |window, cx| {
                    save.update(cx, |editor, cx| editor.save(cx));
                    modal::close(window, cx);
                })
                .into_any_element(),
            ],
        );
        ui::dialog(head, modal::dismiss, body, footer)
    }
}
