//! The stop loss and take profit of the order ticket, and the ATR stop options.

use super::*;

impl OrderTicket {
    pub(super) fn protection(
        &self,
        stop: bool,
        f: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (label, on, state, price) = if stop {
            ("Stop loss", self.stop_on, &self.stop_loss, f.plan.stop)
        } else {
            (
                "Take profit",
                self.target_on,
                &self.take_profit,
                f.plan.target,
            )
        };
        let this = cx.entity();
        let distance = price
            .zip(f.plan.entry)
            .map(|(p, e)| format!("{:.1} pips", f.contract.pips((p - e).abs())));
        let amount = if stop {
            f.plan.risk.map(|r| -r)
        } else {
            f.plan.reward
        };
        // The price it comes to, when it is given as a distance.
        let unit = if stop {
            self.stop_unit
        } else {
            self.target_unit
        };
        let detail = if unit == Offset::Price {
            distance
        } else {
            price.map(|p| format!("at {}", f.contract.format_price(p)))
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(
                        controls::switch(SharedString::from(format!("ticket-{label}")), on)
                            .label(label)
                            .on_click(move |_, window, cx| {
                                this.update(cx, |t, cx| t.toggle_protection(stop, window, cx));
                            }),
                    )
                    .child(div().flex_1())
                    .when(on, |el| {
                        el.child(self.unit_menu(stop, &f.currency, window, cx))
                    }),
            )
            .when(on, |el| {
                el.child(if stop && self.stop_atr {
                    div()
                        .text_size(px(f.m.small))
                        .child(
                            price
                                .map(|p| format!("ATR stop: {}", f.contract.format_price(p)))
                                .unwrap_or_else(|| "Waiting for ATR data".to_owned()),
                        )
                        .into_any_element()
                } else {
                    NumberInput::new(state).small().into_any_element()
                })
                .when(stop && self.stop_atr, |el| {
                    el.child(self.atr_options(f.m.small, window, cx))
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .text_size(px(f.m.small))
                        .text_color(theme::muted_fg())
                        .child(detail.unwrap_or_default())
                        .children(amount.map(|a| {
                            div()
                                .text_color(if a < 0.0 {
                                    theme::chart_down()
                                } else {
                                    theme::chart_up()
                                })
                                .child(format!("{}  {}", f.money(a), f.share(a.abs())))
                        })),
                )
            })
            .into_any_element()
    }

    pub(super) fn atr_options(
        &self,
        font_size: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let smoothing = self.atr.smoothing;
        let smoothing_this = cx.entity();
        let smoothing_menu = self.select(
            "ticket-atr-smoothing",
            smoothing.label().to_owned(),
            move || {
                Smoothing::ALL
                    .into_iter()
                    .map(|choice| {
                        let this = smoothing_this.clone();
                        Entry::new(choice.label())
                            .checked(choice == smoothing)
                            .on_click(move |_, cx| {
                                this.update(cx, |t, cx| {
                                    t.atr.smoothing = choice;
                                    t.settings_changed(cx);
                                    cx.emit(TicketEvent::LinesChanged);
                                    cx.notify();
                                });
                            })
                            .into()
                    })
                    .collect()
            },
            window,
            cx,
        );
        let current = self.atr.timeframe.clone();
        let timeframe_this = cx.entity();
        let timeframe_menu = self.select(
            "ticket-atr-timeframe",
            current.clone().unwrap_or_else(|| "Chart TF".to_owned()),
            move || {
                let mut items: Vec<Item> = Vec::new();
                let this = timeframe_this.clone();
                items.push(
                    Entry::new("Chart TF")
                        .checked(current.is_none())
                        .on_click(move |_, cx| {
                            this.update(cx, |t, cx| {
                                t.atr.timeframe = None;
                                t.settings_changed(cx);
                                cx.emit(TicketEvent::LinesChanged);
                                cx.notify();
                            });
                        })
                        .into(),
                );
                for (_, group) in crate::chart::GROUPS {
                    for &tf in group {
                        if tf == crate::chart::Timeframe::Ticks {
                            continue;
                        }
                        let code = tf.code();
                        let this = timeframe_this.clone();
                        items.push(
                            Entry::new(code.clone())
                                .checked(current.as_deref() == Some(code.as_str()))
                                .on_click(move |_, cx| {
                                    this.update(cx, |t, cx| {
                                        t.atr.timeframe = Some(code.clone());
                                        t.request_atr(cx);
                                        t.settings_changed(cx);
                                        cx.emit(TicketEvent::LinesChanged);
                                        cx.notify();
                                    });
                                })
                                .into(),
                        );
                    }
                }
                items
            },
            window,
            cx,
        );
        let bar_this = cx.entity();
        let bar_label_this = bar_this.clone();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(font_size))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child("Length")
                    .child(NumberInput::new(&self.atr_length).xsmall())
                    .child("x")
                    .child(NumberInput::new(&self.atr_multiplier).xsmall()),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(smoothing_menu)
                    .child(timeframe_menu),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(
                        controls::switch("ticket-atr-current", self.atr.current_bar)
                            .accessibility_label("Current bar (off: last closed)")
                            .on_click(move |on: &bool, _, cx| {
                                let on = *on;
                                bar_this.update(cx, |t, cx| {
                                    t.atr.current_bar = on;
                                    t.settings_changed(cx);
                                    cx.emit(TicketEvent::LinesChanged);
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        div()
                            .id("ticket-atr-current-label")
                            .cursor_pointer()
                            .text_size(px(font_size))
                            .child("Current bar (off: last closed)")
                            .on_click(move |_, _, cx| {
                                bar_label_this.update(cx, |t, cx| {
                                    t.atr.current_bar = !t.atr.current_bar;
                                    t.settings_changed(cx);
                                    cx.emit(TicketEvent::LinesChanged);
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .into_any_element()
    }
}
