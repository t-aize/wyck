//! The top of the order ticket: the header, the buy and sell buttons, the kind of order, the size and its presets.

use super::*;

impl OrderTicket {
    /// A small button that opens a list under it, showing what is picked.
    pub(super) fn select(
        &self,
        key: &'static str,
        text: String,
        items: impl FnOnce() -> Vec<Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let menu = popup::Menu::new(key, window, cx);
        let entries = if menu.is_open(cx) {
            items()
        } else {
            Vec::new()
        };
        let toggle = menu.clone();
        div()
            .relative()
            .child(
                div()
                    .id(key)
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .h(px(tokens::height::COMPACT))
                    .px_2()
                    .rounded_sm()
                    .cursor_pointer()
                    .text_size(px(tokens::text::SMALL))
                    .text_color(theme::muted_fg())
                    .hover(|s| s.bg(theme::surface_hover()).text_color(theme::fg()))
                    .on_click(move |_, _, cx| toggle.toggle(cx))
                    .child(text)
                    .child(icon::tinted(IconName::ChevronDown, 12., theme::muted_fg())),
            )
            .children(menu.popup(
                entries,
                popup::Placement::Below(tokens::height::COMPACT),
                window,
                cx,
            ))
            .into_any_element()
    }

    /// The menu that picks how the volume is sized.
    pub(super) fn size_menu(
        &self,
        currency: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let this = cx.entity();
        let current = self.size_mode;
        let currency = currency.to_owned();
        let text = Self::size_label(current, &currency);
        self.select(
            "ticket-size-mode",
            text,
            move || {
                let mut items = Vec::new();
                for (index, mode) in SizeMode::ALL.into_iter().enumerate() {
                    if index == 2 || index == 5 {
                        items.push(Item::Separator);
                    }
                    let this = this.clone();
                    items.push(
                        Entry::new(Self::size_label(mode, &currency))
                            .checked(mode == current)
                            .on_click(move |window, cx| {
                                this.update(cx, |t, cx| t.set_size_mode(mode, window, cx));
                            })
                            .into(),
                    );
                }
                items
            },
            window,
            cx,
        )
    }

    /// The menu that picks the unit of a stop loss or take profit.
    pub(super) fn unit_menu(
        &self,
        stop: bool,
        currency: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let this = cx.entity();
        let current = if stop {
            self.stop_unit
        } else {
            self.target_unit
        };
        let risk_sized = self.size_mode.is_risk();
        let currency = currency.to_owned();
        let text = if stop && self.stop_atr {
            "ATR x".to_owned()
        } else {
            Self::unit_label(current, &currency)
        };
        let atr_on = self.stop_atr;
        self.select(
            if stop {
                "ticket-stop-unit"
            } else {
                "ticket-target-unit"
            },
            text,
            move || {
                let units: &[Offset] = if stop {
                    &[Offset::Price, Offset::Pips, Offset::Money, Offset::Percent]
                } else {
                    &[
                        Offset::Price,
                        Offset::Pips,
                        Offset::Money,
                        Offset::Percent,
                        Offset::Ratio,
                    ]
                };
                let mut items: Vec<Item> = units
                    .iter()
                    .copied()
                    .map(|unit| {
                        let this = this.clone();
                        let text = match unit {
                            Offset::Price => "Price".to_owned(),
                            Offset::Pips => "Distance in pips".to_owned(),
                            Offset::Money => format!("Amount in {}", or_money(&currency)),
                            Offset::Percent => "Percent of the balance".to_owned(),
                            Offset::Ratio => "Multiple of the risk (R)".to_owned(),
                        };
                        Entry::new(text)
                            .checked(unit == current && !(stop && atr_on))
                            // A stop loss sizing the volume cannot depend on it.
                            .disabled(stop && risk_sized && unit.needs_volume())
                            .on_click(move |window, cx| {
                                this.update(cx, |t, cx| t.set_unit(stop, unit, window, cx));
                            })
                            .into()
                    })
                    .collect();
                if stop {
                    let this = this.clone();
                    items.push(
                        Entry::new("ATR x multiplier")
                            .checked(atr_on)
                            .on_click(move |window, cx| {
                                this.update(cx, |t, cx| t.set_atr_stop(window, cx))
                            })
                            .into(),
                    );
                }
                items
            },
            window,
            cx,
        )
    }

    /// The quick values for the volume, in its mode: the ones the user set, or worked out.
    pub(super) fn preset_values(&self, f: &Frame, cx: &App) -> Vec<(String, f64)> {
        let lot_units = f.contract.lot_size as f64 / 100.0;
        let mode = self.size_mode;
        let custom = self.layout.presets.of(mode);
        if !custom.is_empty() {
            return custom
                .iter()
                .map(|v| {
                    let label = match mode {
                        SizeMode::Lots => math::format_lots(*v),
                        SizeMode::Units => format_units(*v),
                        SizeMode::RiskBalance | SizeMode::RiskEquity | SizeMode::FreeMargin => {
                            format!("{}%", number::format(*v, 2))
                        }
                        SizeMode::RiskMoney => number::format(*v, 2),
                    };
                    (label, *v)
                })
                .collect();
        }
        let balance = self.account.read(cx).summary().balance;
        match mode {
            SizeMode::Units => [0.01, 0.1, 0.5, 1.0]
                .map(|v| {
                    let units = v * lot_units;
                    (format_units(units), units)
                })
                .to_vec(),
            SizeMode::RiskMoney => [0.0025, 0.005, 0.01, 0.02]
                .map(|share| {
                    let amount = nice(balance * share);
                    (number::format(amount, 2), amount)
                })
                .to_vec(),
            // Lots, the risk and the margin always have a list of their own.
            _ => Vec::new(),
        }
    }

    /// Quick values for the volume, in its mode.
    pub(super) fn presets(&self, f: &Frame, cx: &Context<Self>) -> AnyElement {
        let values = self.preset_values(f, cx);
        let current = Self::read(&self.size, cx);
        let decimals = if self.size_mode == SizeMode::Units {
            0
        } else {
            2
        };
        let mut row = div().flex().flex_row().gap_1();
        for (index, (label, value)) in values.into_iter().enumerate() {
            let chosen = current.is_some_and(|c| (c - value).abs() < 1e-9);
            let this = cx.entity();
            row = row.child(
                div()
                    .id(("ticket-preset", index))
                    .flex_1()
                    .flex()
                    .justify_center()
                    .py_0p5()
                    .rounded_sm()
                    .border_1()
                    .border_color(if chosen {
                        theme::accent()
                    } else {
                        theme::border_subtle()
                    })
                    .text_size(px(f.m.small))
                    .text_color(if chosen {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    })
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::surface_hover()))
                    .on_click(move |_, window, cx| {
                        this.update(cx, |t, cx| {
                            let text = number::format(value, decimals);
                            t.write(&t.size.clone(), text, window, cx);
                        });
                    })
                    .child(label),
            );
        }
        row.into_any_element()
    }

    pub(super) fn header(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(f.m.small))
                            .text_color(theme::muted_fg())
                            .child("NEW ORDER"),
                    )
                    .child(
                        div()
                            .text_size(px(tokens::text::HEADING))
                            .font_semibold()
                            .text_color(theme::fg())
                            .truncate()
                            .child(f.name.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(
                        Button::new("ticket-customize")
                            .cursor_pointer()
                            .ghost()
                            .small()
                            .icon(IconName::SlidersHorizontal)
                            .tooltip("Customize the panel")
                            .on_click(move |_, window, cx| {
                                customize::open(this.clone(), window, cx);
                            }),
                    )
                    .child(
                        Button::new("ticket-close")
                            .cursor_pointer()
                            .ghost()
                            .small()
                            .icon(IconName::X)
                            .tooltip("Close the ticket")
                            .on_click(cx.listener(|_this, _, _, cx| cx.emit(TicketEvent::Close))),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn sides(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
        let layout = &self.layout;
        let one_click = self.one_click;
        let button = |buy: bool| {
            let chosen = self.buy == buy;
            let color = if buy {
                theme::chart_up()
            } else {
                theme::chart_down()
            };
            let this = cx.entity();
            let price = if buy { f.ask } else { f.bid };
            div()
                .id(if buy { "ticket-buy" } else { "ticket-sell" })
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .gap_0p5()
                .py(px(f.m.side_pad))
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
                .on_click(move |_, window, cx| {
                    this.update(cx, |t, cx| {
                        if t.one_click {
                            t.send_side(buy, window, cx);
                        } else {
                            t.set_side(buy, window, cx);
                        }
                    });
                })
                .child(
                    div()
                        .text_size(px(f.m.small))
                        .font_semibold()
                        .text_color(color)
                        .child(if buy { "BUY" } else { "SELL" }),
                )
                .when(layout.show_prices, |el| {
                    el.child(
                        div()
                            .text_size(px(if f.m.compact {
                                tokens::text::EMPHASIS
                            } else {
                                tokens::text::HEADING
                            }))
                            .font_semibold()
                            .text_color(theme::fg())
                            .child(f.price(price)),
                    )
                })
                .when(one_click, |el| {
                    el.child(
                        div()
                            .text_size(px(tokens::text::CAPTION))
                            .text_color(theme::muted_fg())
                            .child("click to send"),
                    )
                })
        };
        let order = if layout.buy_first {
            [true, false]
        } else {
            [false, true]
        };
        div()
            .relative()
            .flex()
            .flex_row()
            .gap_2()
            .children(order.map(button))
            .when(layout.show_spread && !f.spread.is_empty(), |el| {
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
                                .text_size(px(tokens::text::CAPTION))
                                .text_color(theme::muted_fg())
                                .child(f.spread.clone()),
                        ),
                )
            })
            .into_any_element()
    }

    pub(super) fn order_block(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
        let labels: Vec<&str> = Kind::ALL.iter().map(|k| k.label()).collect();
        let index = Kind::ALL.iter().position(|k| *k == self.kind).unwrap_or(0);
        let this = cx.entity();
        let at_market = cx.entity();
        let gap = f.m.gap - 4.;
        let limit_at = self.slippage_pips(cx).map(|range| {
            let entry = f.plan.entry;
            entry.map(|e| {
                f.contract
                    .format_price(stop_limit_price(self.buy, e, range * f.contract.pip()))
            })
        });
        div()
            .flex()
            .flex_col()
            .gap(px(gap))
            .child(controls::segmented_fill(
                "ticket-kind",
                &labels,
                index,
                move |choice, window, cx| {
                    this.update(cx, |t, cx| t.set_kind(Kind::ALL[choice], window, cx));
                },
            ))
            .when(self.kind.is_pending(), |el| {
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
                                .text_size(px(f.m.text))
                                .text_color(theme::muted_fg())
                                .child(if self.kind == Kind::StopLimit {
                                    "Stop price"
                                } else {
                                    "Price"
                                })
                                .child(
                                    div()
                                        .id("ticket-at-market")
                                        .cursor_pointer()
                                        .text_color(theme::accent())
                                        .hover(|s| s.underline())
                                        .on_click(move |_, window, cx| {
                                            at_market.update(cx, |t, cx| {
                                                t.price_at_market(window, cx);
                                                cx.emit(TicketEvent::LinesChanged);
                                            });
                                        })
                                        .child("At market"),
                                ),
                        )
                        .child(Input::new(&self.price).small()),
                )
            })
            .when(self.kind == Kind::StopLimit, |el| {
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
                                .text_size(px(f.m.text))
                                .text_color(theme::muted_fg())
                                .child("Range of the limit (pips)")
                                .children(limit_at.flatten().map(|p| {
                                    div()
                                        .text_size(px(f.m.small))
                                        .child(format!("limit at {p}"))
                                })),
                        )
                        .child(NumberInput::new(&self.slippage).small()),
                )
            })
            .into_any_element()
    }

    pub(super) fn size_block(
        &self,
        f: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // What the volume comes to, beside its field.
        let size_note = match self.size_mode {
            SizeMode::Lots => f.units.map(|u| format!("{} units", format_units(u))),
            _ => f.lots.map(|l| format!("{} lots", math::format_lots(l))),
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
                    .justify_between()
                    .text_size(px(f.m.text))
                    .text_color(theme::muted_fg())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child("Size")
                            .child(self.size_menu(&f.currency, window, cx)),
                    )
                    .child(
                        div()
                            .text_color(theme::fg())
                            .child(size_note.unwrap_or_default()),
                    ),
            )
            .child(NumberInput::new(&self.size).small())
            .child(self.presets(f, cx))
            .into_any_element()
    }
}
