//! How the order ticket looks: its fields, the menus that pick the units, the quick values and
//! the summary of what the order risks.

use gpui::prelude::*;
use gpui::{Context, SharedString, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable, Sizable, StyledExt as _};

use super::{HIGH_RISK, Kind, OrderTicket, TicketEvent, nice, side_of};
use crate::app::connection::ui;
use crate::app::trading::account::Busy;
use crate::app::trading::math::{self, Contract, Limit, Offset, SizeMode};
use crate::app::{theme, widgets};

impl OrderTicket {
    fn size_label(mode: SizeMode, currency: &str) -> String {
        match mode {
            SizeMode::Lots => "Lots".to_owned(),
            SizeMode::Units => "Units".to_owned(),
            SizeMode::RiskBalance => "Risk % of balance".to_owned(),
            SizeMode::RiskEquity => "Risk % of equity".to_owned(),
            SizeMode::RiskMoney => format!("Risk in {}", or_money(currency)),
            SizeMode::FreeMargin => "% of free margin".to_owned(),
        }
    }

    fn unit_label(unit: Offset, currency: &str) -> String {
        match unit {
            Offset::Price => "Price".to_owned(),
            Offset::Pips => "Pips".to_owned(),
            Offset::Money => or_money(currency).to_owned(),
            Offset::Percent => "% balance".to_owned(),
            Offset::Ratio => "R".to_owned(),
        }
    }

    /// The button that picks how the volume is sized.
    fn size_menu(&self, currency: &str, cx: &Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        let current = self.size_mode;
        let currency = currency.to_owned();
        Button::new("ticket-size-mode")
            .ghost()
            .xsmall()
            .label(Self::size_label(current, &currency))
            .icon(IconName::ChevronDown)
            .dropdown_menu(move |mut menu, _window, _cx| {
                for (index, mode) in SizeMode::ALL.into_iter().enumerate() {
                    if index == 2 || index == 5 {
                        menu = menu.separator();
                    }
                    let this = this.clone();
                    menu = menu.item(
                        PopupMenuItem::new(Self::size_label(mode, &currency))
                            .checked(mode == current)
                            .on_click(move |_, window, cx| {
                                this.update(cx, |t, cx| t.set_size_mode(mode, window, cx));
                            }),
                    );
                }
                menu
            })
    }

    /// The button that picks the unit of a stop loss or take profit.
    fn unit_menu(&self, stop: bool, currency: &str, cx: &Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        let current = if stop {
            self.stop_unit
        } else {
            self.target_unit
        };
        let risk_sized = self.size_mode.is_risk();
        let currency = currency.to_owned();
        Button::new(if stop {
            "ticket-stop-unit"
        } else {
            "ticket-target-unit"
        })
        .ghost()
        .xsmall()
        .label(Self::unit_label(current, &currency))
        .icon(IconName::ChevronDown)
        .dropdown_menu(move |mut menu, _window, _cx| {
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
            for unit in units.iter().copied() {
                let this = this.clone();
                let text = match unit {
                    Offset::Price => "Price".to_owned(),
                    Offset::Pips => "Distance in pips".to_owned(),
                    Offset::Money => format!("Amount in {}", or_money(&currency)),
                    Offset::Percent => "Percent of the balance".to_owned(),
                    Offset::Ratio => "Multiple of the risk (R)".to_owned(),
                };
                menu = menu.item(
                    PopupMenuItem::new(text)
                        .checked(unit == current)
                        // A stop loss sizing the volume cannot depend on it.
                        .disabled(stop && risk_sized && unit.needs_volume())
                        .on_click(move |_, window, cx| {
                            this.update(cx, |t, cx| t.set_unit(stop, unit, window, cx));
                        }),
                );
            }
            menu
        })
    }

    /// Quick values for the volume, in its mode.
    fn presets(&self, contract: &Contract, cx: &Context<Self>) -> impl IntoElement {
        let balance = self.account.read(cx).summary().balance;
        let lot_units = contract.lot_size as f64 / 100.0;
        let values: Vec<(String, f64)> = match self.size_mode {
            SizeMode::Lots => [0.01, 0.1, 0.5, 1.0]
                .map(|v| (math::format_lots(v), v))
                .to_vec(),
            SizeMode::Units => [0.01, 0.1, 0.5, 1.0]
                .map(|v| {
                    let units = v * lot_units;
                    (format_units(units), units)
                })
                .to_vec(),
            SizeMode::RiskBalance | SizeMode::RiskEquity => [0.25, 0.5, 1.0, 2.0]
                .map(|v| (format!("{}%", widgets::format_number(v, 2)), v))
                .to_vec(),
            SizeMode::RiskMoney => [0.0025, 0.005, 0.01, 0.02]
                .map(|share| {
                    let amount = nice(balance * share);
                    (widgets::format_number(amount, 2), amount)
                })
                .to_vec(),
            SizeMode::FreeMargin => [5.0, 10.0, 25.0, 50.0]
                .map(|v| (format!("{}%", widgets::format_number(v, 0)), v))
                .to_vec(),
        };
        let current = Self::read(&self.size, cx);
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
                    .text_size(px(11.))
                    .text_color(if chosen {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    })
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::surface_hover()))
                    .on_click(move |_, window, cx| {
                        this.update(cx, |t, cx| {
                            let text = widgets::format_number(value, 2);
                            t.write(&t.size.clone(), text, window, cx);
                        });
                    })
                    .child(label),
            );
        }
        row
    }
}

impl Render for OrderTicket {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let contract = self.contract(cx);
        let plan = self.plan(cx);
        // The margin follows the volume.
        if let Some(symbol) = self.symbol.as_ref().map(|s| s.id) {
            let volume = plan.sized.map_or(contract.min_volume, |s| s.volume);
            if self.margin_asked != Some((symbol, volume)) {
                self.request_margin(symbol, volume, cx);
            }
        }
        let (bid, ask) = self.quote(cx);
        let busy = self.account.read(cx).is_busy(Busy::Placing);
        let summary = self.account.read(cx).summary();
        let currency = self.account.read(cx).book.currency.clone();
        let quote_currency = self
            .symbol
            .as_ref()
            .and_then(|s| {
                self.account
                    .read(cx)
                    .book
                    .quote_currency
                    .get(&s.id)
                    .cloned()
            })
            .unwrap_or_default();
        let name = self
            .symbol
            .as_ref()
            .map_or_else(|| SharedString::from("No symbol"), |s| s.name.clone());
        let price_text =
            |p: Option<f64>| p.map_or_else(|| "-".to_owned(), |p| contract.format_price(p));
        let spread = bid
            .zip(ask)
            .map(|(b, a)| format!("{:.1}", contract.pips(a - b)))
            .unwrap_or_default();
        let this = cx.entity();

        let side_button = |buy: bool| {
            let chosen = self.buy == buy;
            let color = if buy {
                theme::chart_up()
            } else {
                theme::chart_down()
            };
            let this = this.clone();
            div()
                .id(if buy { "ticket-buy" } else { "ticket-sell" })
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .gap_0p5()
                .py_2()
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
                    this.update(cx, |t, cx| t.set_side(buy, window, cx));
                })
                .child(
                    div()
                        .text_size(px(11.))
                        .font_semibold()
                        .text_color(color)
                        .child(if buy { "BUY" } else { "SELL" }),
                )
                .child(
                    div()
                        .text_size(px(15.))
                        .font_semibold()
                        .text_color(theme::fg())
                        .child(price_text(if buy { ask } else { bid })),
                )
        };

        let kinds = ["Market", "Limit", "Stop"];
        let kind_index = match self.kind {
            Kind::Market => 0,
            Kind::Limit => 1,
            Kind::Stop => 2,
        };
        let kind_this = this.clone();
        let action_color = if self.buy {
            theme::chart_up()
        } else {
            theme::chart_down()
        };
        let set_market_this = this.clone();
        let money = |amount: f64| math::format_money(amount, &currency);
        let share = |amount: f64| {
            if summary.balance > 0.0 {
                format!("{:.2}%", amount / summary.balance * 100.0)
            } else {
                String::new()
            }
        };
        let lots = plan.sized.map(|s| contract.lots_of_volume(s.volume));
        let units = plan.sized.map(|s| s.volume as f64 / 100.0);

        // What the volume comes to, beside its field.
        let size_note = match self.size_mode {
            SizeMode::Lots => units.map(|u| format!("{} units", format_units(u))),
            _ => lots.map(|l| format!("{} lots", math::format_lots(l))),
        };

        let protection = |stop: bool, cx: &Context<Self>| {
            let (label, on, state, price) = if stop {
                ("Stop loss", self.stop_on, &self.stop_loss, plan.stop)
            } else {
                (
                    "Take profit",
                    self.target_on,
                    &self.take_profit,
                    plan.target,
                )
            };
            let this = this.clone();
            let distance = price
                .zip(plan.entry)
                .map(|(p, e)| format!("{:.1} pips", contract.pips((p - e).abs())));
            let amount = if stop {
                plan.risk.map(|r| -r)
            } else {
                plan.reward
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
                price.map(|p| format!("at {}", contract.format_price(p)))
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
                            Switch::new(SharedString::from(format!("ticket-{label}")))
                                .small()
                                .checked(on)
                                .label(label)
                                .on_click(move |_, window, cx| {
                                    this.update(cx, |t, cx| t.toggle_protection(stop, window, cx));
                                }),
                        )
                        .child(div().flex_1())
                        .when(on, |el| el.child(self.unit_menu(stop, &currency, cx))),
                )
                .when(on, |el| {
                    el.child(gpui_kit::component::input::NumberInput::new(state).small())
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .justify_between()
                                .text_size(px(11.))
                                .text_color(theme::muted_fg())
                                .child(detail.unwrap_or_default())
                                .children(amount.map(|a| {
                                    div()
                                        .text_color(if a < 0.0 {
                                            theme::chart_down()
                                        } else {
                                            theme::chart_up()
                                        })
                                        .child(format!("{}  {}", money(a), share(a.abs())))
                                })),
                        )
                })
        };

        // Warnings that do not stop the order.
        let mut warnings: Vec<String> = Vec::new();
        match plan.sized.and_then(|s| s.limit) {
            Some(Limit::Min) => warnings.push(format!(
                "Raised to the least volume, {} lots: more at stake than asked",
                math::format_lots(contract.lots_of_volume(contract.min_volume))
            )),
            Some(Limit::Max) => warnings.push("Cut to the most volume the broker takes".into()),
            None => {}
        }
        let margin = self
            .margin
            .filter(|m| Some(m.volume) == plan.sized.map(|s| s.volume))
            .map(|m| side_of(m.order, self.buy));
        if let Some(m) = margin
            && m > summary.free_margin
        {
            warnings.push("Not enough free margin".into());
        }
        if let Some(risk) = plan.risk
            && summary.balance > 0.0
            && risk / summary.balance * 100.0 > HIGH_RISK
        {
            warnings.push(format!("Risks more than {HIGH_RISK:.0}% of the balance"));
        }
        let ratio = plan
            .risk
            .zip(plan.reward)
            .filter(|(r, _)| *r > 0.0)
            .map(|(r, w)| format!("1 : {:.2}", w / r));
        let pip_value = match (units, plan.rate) {
            (Some(u), Some(rate)) => Some(money(contract.pip() * u * rate)),
            (Some(u), None) => Some(math::format_money(contract.pip() * u, &quote_currency)),
            _ => None,
        };
        let problem = plan.problem.clone();

        div()
            .id("order-ticket")
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .h_full()
            .overflow_y_scroll()
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
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::muted_fg())
                                    .child("NEW ORDER"),
                            )
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_semibold()
                                    .text_color(theme::fg())
                                    .child(name),
                            ),
                    )
                    .child(
                        Button::new("ticket-close")
                            .ghost()
                            .small()
                            .icon(IconName::X)
                            .tooltip("Close the ticket")
                            .on_click(cx.listener(|_this, _, _, cx| cx.emit(TicketEvent::Close))),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(side_button(false))
                    .child(side_button(true))
                    .when(!spread.is_empty(), |el| {
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
                                        .text_size(px(10.))
                                        .text_color(theme::muted_fg())
                                        .child(spread.clone()),
                                ),
                        )
                    }),
            )
            .child(widgets::segmented(
                "ticket-kind",
                &kinds,
                kind_index,
                move |choice, window, cx| {
                    kind_this.update(cx, |t, cx| {
                        t.kind = [Kind::Market, Kind::Limit, Kind::Stop][choice];
                        if t.kind != Kind::Market && Self::read(&t.price, cx).is_none() {
                            let (bid, ask) = t.quote(cx);
                            let market = if t.buy { ask } else { bid };
                            if let Some(price) = market {
                                let value = t.contract(cx).format_price(price);
                                t.write(&t.price.clone(), value, window, cx);
                            }
                        }
                        cx.emit(TicketEvent::LinesChanged);
                        cx.notify();
                    });
                },
            ))
            .when(self.kind != Kind::Market, |el| {
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
                                .text_size(px(12.))
                                .text_color(theme::muted_fg())
                                .child("Price")
                                .child(
                                    div()
                                        .id("ticket-at-market")
                                        .cursor_pointer()
                                        .text_color(theme::accent())
                                        .hover(|s| s.underline())
                                        .on_click(move |_, window, cx| {
                                            set_market_this.update(cx, |t, cx| {
                                                let (bid, ask) = t.quote(cx);
                                                if let Some(price) = if t.buy { ask } else { bid } {
                                                    let value = t.contract(cx).format_price(price);
                                                    t.write(&t.price.clone(), value, window, cx);
                                                }
                                            });
                                        })
                                        .child("At market"),
                                ),
                        )
                        .child(gpui_kit::component::input::Input::new(&self.price).small()),
                )
            })
            .child(
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
                            .text_size(px(12.))
                            .text_color(theme::muted_fg())
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child("Size")
                                    .child(self.size_menu(&currency, cx)),
                            )
                            .child(
                                div()
                                    .text_color(theme::fg())
                                    .child(size_note.unwrap_or_default()),
                            ),
                    )
                    .child(gpui_kit::component::input::NumberInput::new(&self.size).small())
                    .child(self.presets(&contract, cx)),
            )
            .child(protection(true, cx))
            .child(protection(false, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(theme::surface())
                    .text_size(px(12.))
                    .child(summary_row(
                        "Volume",
                        lots.zip(units).map_or_else(
                            || "-".into(),
                            |(l, u)| {
                                format!("{} lots, {} units", math::format_lots(l), format_units(u))
                            },
                        ),
                    ))
                    .child(summary_row(
                        "Risk",
                        match plan.risk {
                            Some(r) => format!("{}  {}", money(r), share(r)),
                            None if self.stop_on => "-".into(),
                            None => "No stop loss".into(),
                        },
                    ))
                    .child(summary_row(
                        "Reward",
                        plan.reward
                            .map_or_else(|| "-".into(), |r| format!("{}  {}", money(r), share(r))),
                    ))
                    .child(summary_row(
                        "Risk to reward",
                        ratio.unwrap_or_else(|| "-".into()),
                    ))
                    .child(summary_row(
                        "Margin",
                        margin.map_or_else(|| "-".into(), &money),
                    ))
                    .child(summary_row(
                        "Pip value",
                        pip_value.unwrap_or_else(|| "-".into()),
                    )),
            )
            .children(warnings.into_iter().chain(problem.clone()).map(|message| {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .text_color(theme::amber())
                    .child(ui::icon_colored(
                        IconName::TriangleAlert,
                        13.,
                        theme::amber(),
                    ))
                    .child(message)
            }))
            .child(
                Button::new("ticket-send")
                    .label(self.describe(&plan, cx))
                    .with_size(gpui_kit::component::Size::Large)
                    .disabled(problem.is_some() || busy)
                    .loading(busy)
                    .bg(action_color)
                    .text_color(theme::bg())
                    .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
            )
            .child(
                Switch::new("ticket-one-click")
                    .small()
                    .checked(self.one_click)
                    .label("One-click trading (no confirmation)")
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.one_click = *checked;
                        cx.emit(TicketEvent::LinesChanged);
                        cx.notify();
                    })),
            )
    }
}

/// The deposit currency, or a word for it before it is known.
fn or_money(currency: &str) -> &str {
    if currency.is_empty() {
        "money"
    } else {
        currency
    }
}

fn summary_row(label: &'static str, value: String) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .justify_between()
        .child(div().text_color(theme::muted_fg()).child(label))
        .child(div().text_color(theme::fg()).child(value))
}

fn format_units(units: f64) -> String {
    math::format_money(units, "")
        .trim_end_matches("00")
        .trim_end_matches('.')
        .to_owned()
}
