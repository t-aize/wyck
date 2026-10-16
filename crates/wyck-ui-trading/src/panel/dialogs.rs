//! The dialogs the account panel opens: the one that modifies a position or an order, and the one
//! that edits an alert.

pub mod alert_editor;
pub mod alert_render;

use gpui::prelude::*;
use gpui::{App, Context, Entity, Window};
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::input::InputState;

use wyck_app::account::Account;
use wyck_ui_kit::{button, form, form::Head, modal, number, tokens};

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
    modal::open(
        editor,
        modal::Options::new(420.0, 400.0).label("Edit the stop loss and take profit"),
        window,
        cx,
    );
}

impl ProtectionEditor {
    fn save(&self, cx: &mut App) {
        let read = |state: &Entity<InputState>| number::parse(&state.read(cx).value());
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
            rows.push(form::field(
                "Price",
                None,
                form::text_field(&self.price, tokens::field::text()),
            ));
        }
        rows.push(form::field(
            "Stop loss",
            Some("Leave it empty to remove it"),
            form::text_field(&self.stop_loss, tokens::field::text()),
        ));
        rows.push(form::field(
            "Take profit",
            Some("Leave it empty to remove it"),
            form::text_field(&self.take_profit, tokens::field::text()),
        ));
        let body = form::page()
            .child(form::group(IconName::Target, "Levels", rows))
            .child(form::note("Lines can also be dragged on the chart."));
        let save = cx.entity();
        let footer = form::footer(
            Vec::new(),
            vec![
                button::action("protection-cancel", "Cancel", None, false, modal::close)
                    .into_any_element(),
                button::action("protection-save", "Save", None, true, move |window, cx| {
                    save.update(cx, |editor, cx| editor.save(cx));
                    modal::close(window, cx);
                })
                .into_any_element(),
            ],
        );
        form::dialog(head, modal::dismiss, body, footer)
    }
}
