//! The tabs of a long or short position: its trade and its look.

use super::*;

impl DrawingProps {
    /// What the position comes to with these settings, as rows of a card: the plan in figures.
    pub(super) fn position_result(&self, drawing: &Drawing, cx: &App) -> Vec<AnyElement> {
        let resolved = self
            .chart
            .as_ref()
            .and_then(|chart| chart.read(cx).resolved_position(drawing));
        if drawing.style.position.atr_stop.is_some() && resolved.is_none() {
            return vec![form::block(form::note(
                "ATR unavailable for the selected chart and timeframe.",
            ))];
        }
        let drawing = resolved.as_ref().unwrap_or(drawing);
        let p = &drawing.style.position;
        let real = |raw: f64| raw / PRICE_SCALE as f64;
        let tick = 10f64.powi(-(self.digits as i32));
        let stats = drawing.points.get(2).and_then(|_| {
            p.stats(
                real(drawing.points[0].p),
                real(drawing.points[1].p),
                real(drawing.points[2].p),
                tick,
            )
        });
        let Some(stats) = stats else {
            return vec![form::block(form::note(
                "Nothing to size: the stop sits on the entry.",
            ))];
        };
        let value = |text: String| {
            div()
                .text_size(px(tokens::text::EMPHASIS))
                .text_color(theme::fg())
                .child(text)
        };
        let mut rows = vec![
            form::field("Quantity", None, value(p.format_qty(stats.qty))),
            form::field(
                "Risk",
                Some(if stats.capped {
                    "Capped by the leverage: less than the risk asked for"
                } else {
                    "What the stop loses"
                }),
                value(p.format_plain(stats.loss)),
            ),
            form::field("Reward", None, value(p.format_plain(stats.profit))),
            form::field("Risk/reward", None, value(format!("{:.2}", stats.ratio))),
        ];
        if stats.qty <= 0.0 {
            rows.push(form::block(form::note(
                "The quantity rounds down to nothing: lower the lot size or raise the risk.",
            )));
        }
        rows
    }

    /// The inputs of a long or short position: the account, the risk, what the chart writes, and
    /// what it all comes to.
    pub(super) fn position_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let Some(pos) = &self.pos else {
            return form::page().into_any_element();
        };
        let p = &drawing.style.position;
        let this = cx.entity();

        let account = form::group(
            IconName::Wallet,
            "Account",
            [
                form::field(
                    "Account size",
                    Some("The balance the position is sized for"),
                    number::field(&pos.account, tokens::field::WIDE),
                ),
                form::field(
                    "Currency",
                    Some("Written after the amounts. Empty writes none"),
                    div().w(px(130.)).child(Input::new(&pos.currency).small()),
                ),
                form::field(
                    "Leverage",
                    Some("Caps the quantity at account x leverage / entry price"),
                    number::field(&pos.leverage, tokens::field::WIDE),
                ),
            ],
        );

        let mode_this = this.clone();
        let risk = form::group(
            IconName::ShieldAlert,
            "Risk and size",
            [
                form::field(
                    "Risk",
                    Some("Lost if the stop is hit"),
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(number::field(&pos.risk, tokens::field::NUMBER))
                        .child(controls::segmented(
                            "props-risk-mode",
                            &["%", "Amount"],
                            usize::from(!p.risk_percent),
                            move |choice, window, cx| {
                                mode_this.update(cx, |e, cx| {
                                    e.change(cx, |d| {
                                        d.style.position.risk_percent = choice == 0;
                                        d.style.position =
                                            std::mem::take(&mut d.style.position).normalized();
                                    });
                                    e.set_fields(window, cx);
                                });
                            },
                        )),
                ),
                form::field(
                    "Lot size",
                    Some("The step the quantity is rounded down to"),
                    number::field(&pos.lot_size, tokens::field::WIDE),
                ),
                form::field(
                    "Quantity decimals",
                    None,
                    number::field(&pos.qty_precision, tokens::field::WIDE),
                ),
                form::field(
                    "Point value",
                    Some(
                        "What one unit gains per 1.0 of price, in the account currency. 1 when the symbol is quoted in it",
                    ),
                    number::field(&pos.point_value, tokens::field::WIDE),
                ),
            ],
        );

        let atr_seed = self.atr_seed.clone().unwrap_or_default();
        let mode_this = this.clone();
        let target_this = this.clone();
        let mut levels = vec![
            form::field(
                "Stop loss",
                None,
                controls::segmented(
                    "pos-stop-mode",
                    &["Fixed price", "ATR x"],
                    usize::from(p.atr_stop.is_some()),
                    move |choice, _, cx| {
                        mode_this.update(cx, |editor, cx| {
                            editor.change(cx, |d| {
                                d.style.position.atr_stop = (choice == 1).then(|| atr_seed.clone());
                            })
                        });
                    },
                ),
            ),
            form::field(
                "Take profit",
                None,
                controls::segmented(
                    "pos-target-mode",
                    &["Fixed price", "Risk multiple"],
                    usize::from(p.target_rr.is_some()),
                    move |choice, _, cx| {
                        target_this.update(cx, |editor, cx| {
                            editor.change(cx, |d| {
                                d.style.position.target_rr = (choice == 1).then_some(2.0);
                            })
                        });
                    },
                ),
            ),
        ];
        if let Some(atr) = &p.atr_stop {
            let smooth_this = this.clone();
            let bar_this = this.clone();
            let smooth_index = Smoothing::ALL
                .iter()
                .position(|m| *m == atr.smoothing)
                .unwrap_or(0);
            levels.push(form::field(
                "ATR length",
                None,
                number::field(&pos.atr_length, tokens::field::NUMBER),
            ));
            levels.push(form::field(
                "ATR multiplier",
                None,
                number::field(&pos.atr_multiplier, tokens::field::NUMBER),
            ));
            levels.push(form::field(
                "ATR smoothing",
                None,
                controls::segmented(
                    "pos-atr-smoothing",
                    &["RMA", "SMA", "EMA", "WMA"],
                    smooth_index,
                    move |choice, _, cx| {
                        smooth_this.update(cx, |editor, cx| {
                            editor.change(cx, |d| {
                                if let Some(atr) = &mut d.style.position.atr_stop {
                                    atr.smoothing = Smoothing::ALL[choice];
                                }
                            })
                        });
                    },
                ),
            ));
            levels.push(form::field(
                "ATR timeframe",
                Some(
                    "Leave blank to follow this chart, or enter a timeframe code such as M15 or H1",
                ),
                div()
                    .w(px(130.))
                    .child(Input::new(&pos.atr_timeframe).small()),
            ));
            levels.push(form::field(
                "ATR bar",
                None,
                controls::segmented(
                    "pos-atr-bar",
                    &["Last closed", "Current"],
                    usize::from(atr.current_bar),
                    move |choice, _, cx| {
                        bar_this.update(cx, |editor, cx| {
                            editor.change(cx, |d| {
                                if let Some(atr) = &mut d.style.position.atr_stop {
                                    atr.current_bar = choice == 1;
                                }
                            })
                        });
                    },
                ),
            ));
        }
        if p.target_rr.is_some() {
            levels.push(form::field(
                "Risk multiple",
                None,
                number::field(&pos.rr, tokens::field::NUMBER),
            ));
        }
        let levels = form::group(IconName::ChartNoAxesCombined, "Protection levels", levels);

        let result = form::group(
            IconName::Calculator,
            "Result",
            self.position_result(drawing, cx),
        );

        let stat = |id: &str,
                    label: &'static str,
                    hint: Option<&'static str>,
                    on: bool,
                    cx: &mut Context<Self>,
                    set: fn(&mut Drawing, bool)| {
            form::field(label, hint, self.switch(id, on, cx, set))
        };
        let stats = form::group(
            IconName::ListChecks,
            "Written on the chart",
            [
                stat("pos-qty", "Quantity", None, p.show_qty, cx, |d, on| {
                    d.style.position.show_qty = on;
                }),
                stat("pos-risk", "Risk", None, p.show_risk, cx, |d, on| {
                    d.style.position.show_risk = on;
                }),
                stat(
                    "pos-amounts",
                    "Profit and loss",
                    Some("The amounts at the target and at the stop"),
                    p.show_amounts,
                    cx,
                    |d, on| d.style.position.show_amounts = on,
                ),
                stat(
                    "pos-ratio",
                    "Risk/reward ratio",
                    None,
                    p.show_ratio,
                    cx,
                    |d, on| {
                        d.style.position.show_ratio = on;
                    },
                ),
                stat(
                    "pos-price",
                    "Price of the levels",
                    None,
                    p.show_price,
                    cx,
                    |d, on| {
                        d.style.position.show_price = on;
                    },
                ),
                stat(
                    "pos-percent",
                    "Percent",
                    Some("Distance from the entry, in percent"),
                    p.show_percent,
                    cx,
                    |d, on| d.style.position.show_percent = on,
                ),
                stat(
                    "pos-ticks",
                    "Ticks",
                    Some("Distance from the entry, in the smallest price step"),
                    p.show_ticks,
                    cx,
                    |d, on| d.style.position.show_ticks = on,
                ),
                stat(
                    "pos-pips",
                    "Pips",
                    Some("Distance from the entry using the symbol's pip size, when available"),
                    p.show_pips,
                    cx,
                    |d, on| d.style.position.show_pips = on,
                ),
                stat(
                    "pos-compact",
                    "Compact tags",
                    Some("One short figure per tag"),
                    p.compact,
                    cx,
                    |d, on| d.style.position.compact = on,
                ),
                stat(
                    "pos-always",
                    "Always show the tags",
                    Some("Off: show the level tags only while the position is selected"),
                    p.always_stats,
                    cx,
                    |d, on| d.style.position.always_stats = on,
                ),
            ],
        );
        form::page()
            .child(account)
            .child(risk)
            .child(levels)
            .child(result)
            .child(stats)
            .child(form::note(
                "The figures are a plan, not a quote: there is no exchange rate in them.",
            ))
            .into_any_element()
    }

    /// The look of a long or short position: its three lines, its two zones, its tags.
    pub(super) fn position_style_page(
        &self,
        drawing: &Drawing,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = &drawing.style;
        let p = &style.position;
        let this = cx.entity();

        let dash_this = this.clone();
        let dash_index = DASHES.iter().position(|d| *d == style.dash).unwrap_or(0);
        let lines = form::group(
            IconName::PenLine,
            "Lines",
            [
                self.width_row(drawing, cx),
                form::field(
                    "Entry line",
                    None,
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(controls::dash_picker(
                            "props-dash",
                            dash_index,
                            move |choice, _w, cx| {
                                dash_this.update(cx, |e, cx| {
                                    e.change(cx, |d| d.style.dash = DASHES[choice])
                                });
                            },
                        ))
                        .child(self.swatch(Swatch::Entry, p.entry_color, "props-entry-color", cx)),
                ),
                form::field(
                    "Target",
                    Some("The profit line and its zone"),
                    self.swatch(Swatch::Target, p.target_color, "props-target-color", cx),
                ),
                form::field(
                    "Stop",
                    Some("The loss line and its zone"),
                    self.swatch(Swatch::Stop, p.stop_color, "props-stop-color", cx),
                ),
            ],
        );

        let background = form::group(
            IconName::PaintBucket,
            "Background",
            [
                form::field(
                    "Zones",
                    Some("Colors the profit and the loss zones"),
                    self.switch("props-fill", style.fill, cx, |d, on| d.style.fill = on),
                ),
                form::field(
                    "Zone opacity",
                    Some("In percent"),
                    number::field(&self.opacity, tokens::field::NUMBER),
                ),
            ],
        );

        let tags = form::group(
            IconName::Type,
            "Tags",
            [
                form::field(
                    "Show the tags",
                    Some("The words on the levels"),
                    self.switch("props-labels", style.labels, cx, |d, on| {
                        d.style.labels = on
                    }),
                ),
                form::field(
                    "Text color",
                    Some("Dark on the colored tags unless you pick one"),
                    self.swatch(
                        Swatch::Text,
                        style.text_color.unwrap_or(0x0a0a0a),
                        "props-text-color",
                        cx,
                    ),
                ),
                form::field(
                    "Text size",
                    None,
                    number::field(&self.text_size, tokens::field::NUMBER),
                ),
                form::field(
                    "Bold",
                    None,
                    self.switch("props-bold", style.bold, cx, |d, on| d.style.bold = on),
                ),
            ],
        );
        form::page()
            .child(lines)
            .child(background)
            .child(tags)
            .child(self.templates_group(drawing, cx))
            .into_any_element()
    }
}
