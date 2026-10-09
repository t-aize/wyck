//! The stop loss and take profit of the order ticket, and the ATR stop options.

use super::*;

/// The multiples of the ATR offered as shortcuts for the ATR stop.
const ATR_CHOICES: [f64; 4] = [1.0, 1.5, 2.0, 3.0];

/// The multiples of the risk offered as shortcuts for the take profit.
const RATIO_CHOICES: [f64; 4] = [1.0, 1.5, 2.0, 3.0];

impl OrderTicket {
    pub(super) fn protection(
        &self,
        stop: bool,
        f: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (label, on, price) = if stop {
            ("Stop loss", self.stop_on, f.plan.stop)
        } else {
            ("Take profit", self.target_on, f.plan.target)
        };
        let linked = self.link.is_some();
        let this = cx.entity();
        let unit = if stop {
            self.stop_unit
        } else {
            self.target_unit
        };
        let head = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_2()
            .h(px(tokens::height::compact()))
            .child(
                controls::switch(SharedString::from(format!("ticket-{label}")), on)
                    .label(label)
                    .disabled(linked)
                    .on_click(move |_, window, cx| {
                        this.update(cx, |t, cx| t.toggle_protection(stop, window, cx));
                    }),
            )
            .when(linked, |el| {
                el.child(icon::tinted(IconName::Link, 12., theme::accent()))
            });

        let distance = price
            .zip(f.plan.entry)
            .map(|(p, e)| format!("{:.1} pips", f.contract.pips((p - e).abs())));
        let amount = if stop {
            f.plan.risk.map(|r| -r)
        } else {
            f.plan.reward
        };
        // The price it comes to, when it is given as a distance.
        let detail = if unit == Offset::Price && !(stop && self.stop_atr) {
            distance
        } else {
            price.map(|p| {
                let pips = f
                    .plan
                    .entry
                    .map(|e| format!(", {:.1} pips", f.contract.pips((p - e).abs())))
                    .unwrap_or_default();
                format!("at {}{pips}", f.contract.format_price(p))
            })
        };
        let result = div()
            .flex()
            .flex_row()
            .justify_between()
            .gap_2()
            .text_size(px(tokens::text::small()))
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
            }));

        let body: Vec<AnyElement> = if !on {
            Vec::new()
        } else if linked {
            vec![self.followed_level(stop, f), result.into_any_element()]
        } else if stop && self.stop_atr {
            vec![
                self.stop_mode(true, cx),
                self.atr_options(f, window, cx),
                result.into_any_element(),
            ]
        } else {
            let state = if stop {
                &self.stop_loss
            } else {
                &self.take_profit
            };
            let mut rows = Vec::new();
            if stop {
                rows.push(self.stop_mode(false, cx));
            }
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(NumberInput::new(state).small()),
                    )
                    .child(self.unit_menu(stop, &f.currency, window, cx))
                    .into_any_element(),
            );
            if !stop {
                rows.push(self.ratio_chips(cx));
            }
            rows.push(result.into_any_element());
            rows
        };
        card(f.m).child(head).children(body).into_any_element()
    }

    /// A level that a followed drawing sets: what it is, not a field to type in.
    fn followed_level(&self, stop: bool, f: &Frame) -> AnyElement {
        let Some(link) = &self.link else {
            return div().into_any_element();
        };
        let plan = &link.plan;
        let text = if stop {
            match &plan.atr {
                Some(atr) => format!(
                    "ATR {} x {}  {}, {}",
                    atr.length,
                    number::format(atr.multiplier, 2),
                    atr.smoothing.label(),
                    atr.timeframe.as_deref().unwrap_or("chart timeframe")
                ),
                None => "Fixed price".to_owned(),
            }
        } else {
            match plan.rr {
                Some(rr) => format!("{}R of the risk", number::format(rr, 2)),
                None => "Fixed price".to_owned(),
            }
        };
        let price = if stop { f.plan.stop } else { f.plan.target };
        div()
            .flex()
            .flex_col()
            .gap_0p5()
            .child(
                div()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::fg())
                    .child(text),
            )
            .child(hint(match price {
                Some(p) => format!("Level {}, set by the drawing", f.contract.format_price(p)),
                None => "Waiting for the drawing".to_owned(),
            }))
            .into_any_element()
    }

    /// The choice between a stop loss given as a distance and one that follows the ATR.
    fn stop_mode(&self, atr: bool, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        controls::segmented_fill(
            "ticket-stop-mode",
            &["Distance", "ATR"],
            usize::from(atr),
            move |choice, window, cx| {
                this.update(cx, |t, cx| {
                    if choice == 1 {
                        t.set_atr_stop(window, cx);
                    } else {
                        // Back to a distance: the stop keeps its price.
                        let unit = t.stop_unit;
                        t.set_unit(true, unit, window, cx);
                    }
                });
            },
        )
        .into_any_element()
    }

    /// Shortcuts for the take profit in multiples of the risk.
    fn ratio_chips(&self, cx: &mut Context<Self>) -> AnyElement {
        let current =
            Self::read(&self.take_profit, cx).filter(|_| self.target_unit == Offset::Ratio);
        let mut row = div().flex().flex_row().gap_1();
        for (index, value) in RATIO_CHOICES.into_iter().enumerate() {
            let chosen = current.is_some_and(|c| (c - value).abs() < 1e-9);
            let this = cx.entity();
            row = row.child(
                controls::chip(("ticket-ratio", index), chosen)
                    .flex_1()
                    .justify_center()
                    .px_1()
                    .on_click(move |_, window, cx| {
                        this.update(cx, |t, cx| t.set_target_ratio(value, window, cx));
                    })
                    .child(format!("{}R", number::format(value, 1))),
            );
        }
        row.into_any_element()
    }

    /// The settings of the ATR stop: how many ATRs, with shortcuts, and behind a fold what the
    /// ATR is made of.
    pub(super) fn atr_options(
        &self,
        f: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut chips = div().flex().flex_row().gap_1();
        for (index, value) in ATR_CHOICES.into_iter().enumerate() {
            let chosen = (self.atr.multiplier - value).abs() < 1e-9;
            let this = cx.entity();
            chips = chips.child(
                controls::chip(("ticket-atr-mult", index), chosen)
                    .flex_1()
                    .justify_center()
                    .px_1()
                    .on_click(move |_, window, cx| {
                        this.update(cx, |t, cx| t.set_atr_multiplier(value, window, cx));
                    })
                    .child(format!("{}x", number::format(value, 1))),
            );
        }
        let atr_now = match f.atr {
            Some(atr) => format!(
                "ATR now {} ({:.1} pips)",
                f.contract.format_price(atr),
                f.contract.pips(atr)
            ),
            None => "Waiting for ATR data".to_owned(),
        };
        let open = self.atr_open;
        let fold = cx.entity();
        let mut column = div()
            .flex()
            .flex_col()
            .gap(px(f.m.inner))
            .child(line(
                "Times the ATR",
                number::field(&self.atr_multiplier, tokens::field::narrow()),
            ))
            .child(chips)
            .child(hint(atr_now))
            .child(
                div()
                    .id("ticket-atr-fold")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::muted_fg())
                    .hover(|s| s.text_color(theme::fg()))
                    .on_click(move |_, _, cx| {
                        fold.update(cx, |t, cx| {
                            t.atr_open = !t.atr_open;
                            cx.notify();
                        });
                    })
                    .child(icon::tinted(
                        if open {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        },
                        12.,
                        theme::muted_fg(),
                    ))
                    .child("ATR settings"),
            );
        if open {
            column = column.child(self.atr_details(window, cx));
        }
        column.into_any_element()
    }

    /// What the ATR is made of: its length, its smoothing, its timeframe and its bar.
    fn atr_details(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
            current.clone().unwrap_or_else(|| "Chart".to_owned()),
            move || {
                let mut items: Vec<Item> = Vec::new();
                let this = timeframe_this.clone();
                items.push(
                    Entry::new("Chart timeframe")
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
                for (_, group) in crate::ui::features::chart::GROUPS {
                    for &tf in group {
                        if tf == crate::ui::features::chart::Timeframe::Ticks {
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
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(line(
                "Length",
                number::field(&self.atr_length, tokens::field::narrow()),
            ))
            .child(line("Smoothing", smoothing_menu))
            .child(line("Timeframe", timeframe_menu))
            .child(line(
                "Bar",
                controls::segmented(
                    "ticket-atr-bar",
                    &["Last closed", "Current"],
                    usize::from(self.atr.current_bar),
                    move |choice, _, cx| {
                        bar_this.update(cx, |t, cx| {
                            t.atr.current_bar = choice == 1;
                            t.settings_changed(cx);
                            cx.emit(TicketEvent::LinesChanged);
                            cx.notify();
                        });
                    },
                ),
            ))
            .into_any_element()
    }
}
