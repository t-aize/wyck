//! The top of the order ticket: the header, the buy and sell buttons, the kind of order, the size
//! and its presets.

use super::*;

impl OrderTicket {
    /// A button that opens a list under it, showing what is picked. It is the height and the
    /// look of a field, so it sits in a row with one.
    pub(super) fn select(
        &self,
        key: &'static str,
        text: String,
        items: impl FnOnce() -> Vec<Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let menu = popup::Menu::new(key, window, cx);
        let open = menu.is_open(cx);
        let entries = if open { items() } else { Vec::new() };
        let toggle = menu.clone();
        div()
            .relative()
            .flex_none()
            .child(
                div()
                    .id(key)
                    .keyboard()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .h(px(tokens::height::compact()))
                    .px_2()
                    .rounded_md()
                    .border_1()
                    .border_color(if open {
                        theme::accent()
                    } else {
                        theme::border_subtle()
                    })
                    .cursor_pointer()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::fg())
                    .hover(|s| s.bg(theme::surface_hover()))
                    .on_click(move |_, _, cx| toggle.toggle(cx))
                    .child(text)
                    .child(icon::tinted(IconName::ChevronDown, 12., theme::muted_fg())),
            )
            .children(menu.popup(
                entries,
                popup::Placement::Below(tokens::height::compact()),
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
        let text = Self::unit_label(current, &currency);
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
                units
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
                            .checked(unit == current)
                            // A stop loss sizing the volume cannot depend on it.
                            .disabled(stop && risk_sized && unit.needs_volume())
                            .on_click(move |window, cx| {
                                this.update(cx, |t, cx| t.set_unit(stop, unit, window, cx));
                            })
                            .into()
                    })
                    .collect()
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

    /// Quick values for the volume, in its mode: chips that share the width.
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
                controls::chip(("ticket-preset", index), chosen)
                    .flex_1()
                    .justify_center()
                    .px_1()
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

    /// The head of the panel: the symbol, the panel's buttons, and, while the ticket follows a
    /// drawing, what it follows and how to let go.
    pub(super) fn header(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .px(px(f.m.pad))
            .pt(px(f.m.pad - 4.))
            .pb(px(f.m.pad - 4.))
            .border_b_1()
            .border_color(theme::border_hairline())
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
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(tokens::text::caption()))
                                    .text_color(theme::muted_fg())
                                    .child("NEW ORDER"),
                            )
                            .child(
                                div()
                                    .text_size(px(tokens::text::heading()))
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
                                crate::ui::kit::button::quiet("ticket-customize")
                                    .icon(IconName::SlidersHorizontal)
                                    .tooltip("Customize the panel")
                                    .on_click(move |_, window, cx| {
                                        customize::open(this.clone(), window, cx);
                                    }),
                            )
                            .child(
                                crate::ui::kit::button::quiet("ticket-close")
                                    .icon(IconName::X)
                                    .tooltip("Close the ticket")
                                    .on_click(
                                        cx.listener(|_this, _, _, cx| cx.emit(TicketEvent::Close)),
                                    ),
                            ),
                    ),
            )
            .children(self.link_banner(cx))
            .into_any_element()
    }

    /// What a followed drawing sets, in a few words.
    fn link_summary(&self) -> String {
        let Some(link) = &self.link else {
            return String::new();
        };
        let plan = &link.plan;
        let stop = match &plan.atr {
            Some(atr) => format!(
                "stop {} ATR x {}",
                atr.length,
                number::format(atr.multiplier, 2)
            ),
            None => "stop at a price".to_owned(),
        };
        let target = match plan.rr {
            Some(rr) => format!("target {}R", number::format(rr, 2)),
            None => "target at a price".to_owned(),
        };
        format!("{stop}, {target}")
    }

    /// The banner shown while the ticket follows a position drawing.
    fn link_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let link = self.link.as_ref()?;
        let this = cx.entity();
        let name = if link.plan.buy {
            "long position"
        } else {
            "short position"
        };
        Some(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(theme::accent_alpha(0.5))
                .bg(theme::accent_selected())
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(icon::tinted(IconName::Link, 13., theme::accent()))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(tokens::text::body()))
                                .font_semibold()
                                .text_color(theme::fg())
                                .truncate()
                                .child(format!("Linked to the {name}")),
                        )
                        .child(text_button("ticket-unlink", "Unlink").on_click(
                            move |_, window, cx| {
                                this.update(cx, |t, cx| t.unlink(window, cx));
                            },
                        )),
                )
                .child(hint(format!(
                    "{}. Follows the drawing until the order is sent.",
                    self.link_summary()
                )))
                .into_any_element(),
        )
    }

    pub(super) fn sides(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
        let layout = &self.layout;
        let button = |buy: bool| {
            let chosen = self.buy == buy;
            let allowed = self.side_allowed(buy);
            let color = if buy {
                theme::chart_up()
            } else {
                theme::chart_down()
            };
            let this = cx.entity();
            let price = if buy { f.ask } else { f.bid };
            div()
                .id(if buy { "ticket-buy" } else { "ticket-sell" })
                .keyboard()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .gap_0p5()
                .py(px(f.m.inner))
                .rounded_lg()
                .border_1()
                .border_color(if chosen {
                    color
                } else {
                    theme::border_subtle()
                })
                .bg(if chosen {
                    tint(color, 0.16)
                } else {
                    theme::bg().into()
                })
                .when(allowed, |el| {
                    el.cursor_pointer()
                        .hover(|s| s.border_color(color))
                        .on_click(move |_, window, cx| {
                            this.update(cx, |t, cx| t.set_side(buy, window, cx));
                        })
                })
                .when(!allowed, |el| el.cursor_not_allowed().opacity(0.4))
                .child(
                    div()
                        .text_size(px(tokens::text::body()))
                        .font_semibold()
                        .text_color(color)
                        .child(if buy { "BUY" } else { "SELL" }),
                )
                .when(layout.show_prices, |el| {
                    el.child(
                        div()
                            .text_size(px(if f.m.compact {
                                tokens::text::emphasis()
                            } else {
                                tokens::text::heading()
                            }))
                            .font_semibold()
                            .text_color(theme::fg())
                            .child(f.price(price)),
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
                                .rounded_full()
                                .bg(theme::surface())
                                .border_1()
                                .border_color(theme::border_subtle())
                                .text_size(px(tokens::text::caption()))
                                .text_color(theme::muted_fg())
                                .child(format!("{} pips", f.spread)),
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
        let linked = self.link.is_some();
        let limit_at = self.slippage_pips(cx).and_then(|range| {
            f.plan.entry.map(|e| {
                f.contract
                    .format_price(stop_limit_price(self.buy, e, range * f.contract.pip()))
            })
        });
        let price_label = if self.kind == Kind::StopLimit {
            "Stop price"
        } else {
            "Price"
        };
        card(f.m)
            .child(card_head("Order", None))
            .child(controls::segmented_fill(
                "ticket-kind",
                &labels,
                index,
                move |choice, window, cx| {
                    this.update(cx, |t, cx| t.set_kind(Kind::ALL[choice], window, cx));
                },
            ))
            .when(self.kind.is_pending(), |el| {
                el.child(if linked {
                    // The drawing sets the entry: shown, not typed.
                    line(
                        price_label,
                        div()
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::fg())
                            .child(f.price(f.plan.entry)),
                    )
                    .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(line(
                            price_label,
                            text_button("ticket-at-market", "At market").on_click(
                                move |_, window, cx| {
                                    at_market.update(cx, |t, cx| {
                                        t.price_at_market(window, cx);
                                        cx.emit(TicketEvent::LinesChanged);
                                    });
                                },
                            ),
                        ))
                        .child(crate::ui::kit::input::text(&self.price))
                        .into_any_element()
                })
            })
            .when(linked && self.kind.is_pending(), |el| {
                el.child(hint(
                    "From the drawing. Pick Market to trade at the current price.",
                ))
            })
            .when(self.kind == Kind::StopLimit, |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(line(
                            "Range of the limit (pips)",
                            div()
                                .text_size(px(tokens::text::small()))
                                .text_color(theme::muted_fg())
                                .children(limit_at.map(|p| format!("limit at {p}"))),
                        ))
                        .child(crate::ui::kit::input::number(&self.slippage)),
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
        // What the volume comes to, under its field.
        let size_note = match self.size_mode {
            SizeMode::Lots => f.units.map(|u| format!("{} units", format_units(u))),
            _ => f.lots.map(|l| format!("{} lots", math::format_lots(l))),
        };
        let presets = self.presets(f, cx);
        let has_presets = !self.preset_values(f, cx).is_empty();
        card(f.m)
            .child(card_head(
                "Size",
                Some(self.size_menu(&f.currency, window, cx)),
            ))
            .child(crate::ui::kit::input::number(&self.size))
            .when(has_presets, |el| el.child(presets))
            .children(size_note.map(|note| {
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .text_size(px(tokens::text::small()))
                    .text_color(theme::muted_fg())
                    .child("Volume")
                    .child(div().text_color(theme::fg()).child(note))
            }))
            .into_any_element()
    }
}
