//! The open positions of the symbol, under the order ticket.

use super::*;

impl OrderTicket {
    /// What is open on the symbol, with what can be done to it.
    pub(super) fn positions(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
        let Some(symbol) = self.symbol.as_ref().map(|s| s.id) else {
            return div().into_any_element();
        };
        let account = self.account.read(cx);
        let one_click = self.one_click;
        let mut rows: Vec<AnyElement> = Vec::new();
        let icon_button = |id: SharedString,
                           icon: IconName,
                           tip: &'static str,
                           enabled: bool,
                           on: bool,
                           run: Action| {
            Button::new(id)
                .cursor_pointer()
                .when(!enabled, |button| button.cursor_not_allowed())
                .ghost()
                .xsmall()
                .icon(icon)
                .tooltip(tip)
                .selected(on)
                .disabled(!enabled)
                .on_click(move |_, window, cx| run(window, cx))
        };
        let mut count = 0;
        for position in account.book.positions.values() {
            if position.trade_data.symbol_id != symbol {
                continue;
            }
            count += 1;
            let id = position.position_id;
            let buy = is_buy(position.trade_data.trade_side);
            let volume = position.trade_data.volume;
            let contract = &f.contract;
            let profit = account.net_profit(id);
            let tone = match profit {
                Some(p) if p > 0.0 => theme::chart_up(),
                Some(p) if p < 0.0 => theme::chart_down(),
                _ => theme::fg(),
            };
            let busy = account.is_busy(Busy::Closing(id)) || account.is_busy(Busy::Amending(id));
            let step = contract.step_volume.max(1);
            let half = volume / 2 / step * step;
            let half = (half >= contract.min_volume && half > 0 && half < volume).then_some(half);
            let entry = position.price;
            let at_entry = entry
                .zip(position.stop_loss)
                .is_some_and(|(e, s)| (e - s).abs() < contract.pip() / 10.0);
            let describe = format!(
                "{} {} {}",
                if buy { "Buy" } else { "Sell" },
                math::format_lots(contract.lots_of_volume(volume)),
                f.name
            );
            let (take_profit, stop_loss, trailing) = (
                position.take_profit,
                position.stop_loss,
                position.trailing_stop_loss.unwrap_or(false),
            );
            let run =
                |what: &'static str, title: &'static str, action: Rc<dyn Fn(&mut App)>| -> Action {
                    let text = format!("{what}: {describe}");
                    Rc::new(move |window, cx| {
                        if one_click {
                            action(cx);
                        } else {
                            let action = action.clone();
                            confirm(window, cx, title, text.clone(), move |_, cx| action(cx));
                        }
                    })
                };
            let (a, b, c, d, e) = (
                self.account.clone(),
                self.account.clone(),
                self.account.clone(),
                self.account.clone(),
                self.account.clone(),
            );
            let close = run(
                "Close",
                "Close this position?",
                Rc::new(move |cx| a.update(cx, |acc, cx| acc.close_position(id, None, cx))),
            );
            let close_half = run(
                "Close half of",
                "Close half of this position?",
                Rc::new(move |cx| {
                    if let Some(half) = half {
                        b.update(cx, |acc, cx| acc.close_position(id, Some(half), cx));
                    }
                }),
            );
            let reverse = run(
                "Reverse",
                "Reverse this position?",
                Rc::new(move |cx| c.update(cx, |acc, cx| acc.reverse_position(id, cx))),
            );
            let break_even: Action = Rc::new(move |_, cx| {
                if let Some(entry) = entry {
                    d.update(cx, |acc, cx| {
                        acc.protect_position(id, Some(entry), take_profit, cx);
                    });
                }
            });
            let trail: Action = Rc::new(move |_, cx| {
                e.update(cx, |acc, cx| acc.trail_position(id, !trailing, cx));
            });
            let symbol_id = SharedString::from(format!("{id}"));
            rows.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .p_2()
                    .rounded_md()
                    .bg(theme::bg())
                    .border_1()
                    .border_color(theme::border_hairline())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .text_size(px(tokens::text::body()))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .font_semibold()
                                            .text_color(if buy {
                                                theme::chart_up()
                                            } else {
                                                theme::chart_down()
                                            })
                                            .child(if buy { "Buy" } else { "Sell" }),
                                    )
                                    .child(div().text_color(theme::fg()).child(format!(
                                        "{} @ {}",
                                        math::format_lots(contract.lots_of_volume(volume)),
                                        f.price(entry)
                                    ))),
                            )
                            .child(
                                div()
                                    .font_semibold()
                                    .text_color(tone)
                                    .child(profit.map_or_else(|| "...".to_owned(), |p| f.money(p))),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(tokens::text::small()))
                                    .text_color(theme::muted_fg())
                                    .child(format!(
                                        "SL {}  TP {}{}",
                                        f.price(stop_loss),
                                        f.price(take_profit),
                                        if trailing { "  trailing" } else { "" }
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .child(icon_button(
                                        SharedString::from(format!("pos-half-{symbol_id}")),
                                        IconName::Scissors,
                                        "Close half",
                                        half.is_some() && !busy,
                                        false,
                                        close_half,
                                    ))
                                    .child(icon_button(
                                        SharedString::from(format!("pos-be-{symbol_id}")),
                                        IconName::ShieldCheck,
                                        "Move the stop loss to the entry",
                                        entry.is_some() && !at_entry && !busy,
                                        at_entry,
                                        break_even,
                                    ))
                                    .child(icon_button(
                                        SharedString::from(format!("pos-trail-{symbol_id}")),
                                        IconName::TrendingUp,
                                        if trailing {
                                            "Stop trailing the stop loss"
                                        } else {
                                            "Trail the stop loss"
                                        },
                                        (stop_loss.is_some() || trailing) && !busy,
                                        trailing,
                                        trail,
                                    ))
                                    .child(icon_button(
                                        SharedString::from(format!("pos-reverse-{symbol_id}")),
                                        IconName::ArrowUpDown,
                                        "Reverse",
                                        !busy,
                                        false,
                                        reverse,
                                    ))
                                    .child(icon_button(
                                        SharedString::from(format!("pos-close-{symbol_id}")),
                                        IconName::X,
                                        "Close",
                                        !busy,
                                        false,
                                        close,
                                    )),
                            ),
                    )
                    .into_any_element(),
            );
        }
        // Working orders of the symbol, small, with a way to cancel each.
        for order in account.book.orders.values() {
            if order.trade_data.symbol_id != symbol {
                continue;
            }
            count += 1;
            let id = order.order_id;
            let buy = is_buy(order.trade_data.trade_side);
            let price = order.limit_price.or(order.stop_price);
            let cancelling = account.is_busy(Busy::Cancelling(id));
            let cancel_account = self.account.clone();
            let cancel: Action = Rc::new(move |_, cx| {
                cancel_account.update(cx, |acc, cx| acc.cancel_order(id, cx));
            });
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .text_size(px(tokens::text::small()))
                    .text_color(theme::muted_fg())
                    .child(format!(
                        "{} {} {} @ {}",
                        if buy { "Buy" } else { "Sell" },
                        order
                            .kind()
                            .map_or("order", crate::domain::trading::OrderType::label),
                        math::format_lots(f.contract.lots_of_volume(order.trade_data.volume)),
                        f.price(price)
                    ))
                    .child(icon_button(
                        SharedString::from(format!("order-cancel-{id}")),
                        IconName::X,
                        "Cancel the order",
                        !cancelling,
                        false,
                        cancel,
                    ))
                    .into_any_element(),
            );
        }
        let close_all = self.account.clone();
        let text = format!("Every position of {} is closed at the market.", f.name);
        let close_all_button = (count > 1).then(|| {
            text_button("ticket-close-all", "Close all")
                .on_click(move |_, window, cx| {
                    let account = close_all.clone();
                    let run = move |cx: &mut App| {
                        account.update(cx, |a, cx| a.close_all(Some(symbol), cx));
                    };
                    if one_click {
                        run(cx);
                    } else {
                        confirm(
                            window,
                            cx,
                            "Close every position on this symbol?",
                            text.clone(),
                            move |_, cx| run(cx),
                        );
                    }
                })
                .into_any_element()
        });
        card(f.m)
            .child(card_head(format!("Open on {}", f.name), close_all_button))
            .children(rows)
            .when(count == 0, |el| {
                el.child(hint("Nothing open on this symbol."))
            })
            .into_any_element()
    }
}
