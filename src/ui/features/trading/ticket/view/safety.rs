//! The banners that tell the user an order is risky or cannot be sent as it stands.

use super::{
    AnyElement, Context, IconName, OrderTicket, THIN_METER_HEIGHT, Tone, confirm_as, div, icon,
    math, px, text_button, theme, tokens,
};
use gpui::prelude::*;

impl OrderTicket {
    /// What the safety limits say now: a lock that stops new orders (with the way out), a request
    /// that got no answer, and how much of the day's loss limit is used.
    pub(super) fn safety_banners(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let account_entity = self.account.clone();
        let (lock, uncertain, standing, limit, positions, currency, text) = {
            let account = self.account.read(cx);
            let standing = account.standing();
            let lock = account.lock();
            (
                lock,
                account.is_uncertain(),
                standing,
                account.risk().daily_loss_limit(standing.day_start_balance),
                account.book.positions.len(),
                account.book.currency.clone(),
                lock.map(|lock| {
                    lock.text(
                        &|m| math::format_money(m, &account.book.currency),
                        standing.now,
                    )
                }),
            )
        };
        let mut out: Vec<AnyElement> = Vec::new();
        if let (Some(_), Some(text)) = (lock, text) {
            let close = account_entity.clone();
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme::destructive())
                    .bg(theme::destructive_bg())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_start()
                            .gap_2()
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::destructive())
                            .child(
                                div()
                                    .flex_none()
                                    .pt(px(crate::ui::kit::tokens::HAIRLINE))
                                    .child(icon::tinted(
                                        IconName::ShieldAlert,
                                        13.,
                                        theme::destructive(),
                                    )),
                            )
                            .child(div().flex_1().min_w_0().child(text)),
                    )
                    .when(positions > 0, |el| {
                        el.child(
                            text_button("ticket-lock-close-all", "Close all positions").on_click(
                                move |_, window, cx| {
                                    let close = close.clone();
                                    confirm_as(
                                        window,
                                        cx,
                                        Tone::Danger,
                                        "Close every position?",
                                        "All open positions are closed at market.",
                                        move |_, cx| {
                                            close.update(cx, |a, cx| a.close_all(None, cx))
                                        },
                                    );
                                },
                            ),
                        )
                    })
                    .into_any_element(),
            );
        }
        if uncertain {
            let refresh = account_entity.clone();
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(theme::amber_bg())
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::amber())
                    .child("A request got no answer. It may have gone through: check the positions before sending again.")
                    .child(text_button("ticket-uncertain-refresh", "Check the account").on_click(
                        move |_, _, cx| refresh.update(cx, |a, cx| a.on_ready(cx)),
                    ))
                    .into_any_element(),
            );
        }
        if let Some(limit) = limit.filter(|l| *l > 0.0) {
            let used = (-standing.day_pnl).max(0.0);
            let share = (used / limit).clamp(0.0, 1.0) as f32;
            let color = if share >= 1.0 {
                theme::destructive()
            } else if share >= 0.7 {
                theme::amber()
            } else {
                theme::emerald()
            };
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .text_size(px(tokens::text::small()))
                            .text_color(theme::muted_fg())
                            .child("Daily loss")
                            .child(format!(
                                "{} of {}",
                                math::format_money(used, &currency),
                                math::format_money(limit, &currency)
                            )),
                    )
                    .child(
                        div()
                            .h(px(THIN_METER_HEIGHT))
                            .w_full()
                            .rounded_full()
                            .bg(theme::fg_alpha(0.12))
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(color)
                                    .w(gpui::relative(share.max(0.02))),
                            ),
                    )
                    .into_any_element(),
            );
        }
        out
    }
}
