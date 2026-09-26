//! How the order ticket looks: its blocks in the order the user chose, the menus that pick the
//! units, the quick values and the summary of what the order risks.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, SharedString, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, NumberInput};
use gpui_kit::component::{Disableable, Selectable, Sizable, StyledExt as _};

use super::prefs::{Density, Kind, Line, Section, Slot, Span, Tif};
use super::{OrderTicket, Plan, TicketEvent, customize, nice, side_of, stop_limit_price};
use crate::app::confirm::confirm;
use crate::app::connection::ui;
use crate::app::menu::{self as popup, Entry, Item};
use crate::app::settings_ui;
use crate::app::trading::account::Busy;
use crate::app::trading::book::is_buy;
use crate::app::trading::math::{self, Contract, Limit, Offset, SizeMode};
use crate::app::{theme, widgets};

/// What a button of a position row does.
type Action = Rc<dyn Fn(&mut Window, &mut App)>;

/// Sizes that follow the density chosen for the panel.
#[derive(Debug, Clone, Copy)]
struct Metrics {
    gap: f32,
    pad: f32,
    text: f32,
    small: f32,
    side_pad: f32,
    compact: bool,
}

impl Metrics {
    fn of(density: Density) -> Self {
        match density {
            Density::Comfortable => Self {
                gap: 12.,
                pad: 12.,
                text: 12.,
                small: 11.,
                side_pad: 8.,
                compact: false,
            },
            Density::Compact => Self {
                gap: 8.,
                pad: 8.,
                text: 11.,
                small: 10.,
                side_pad: 4.,
                compact: true,
            },
        }
    }
}

/// What the blocks are drawn from, worked out once per frame.
struct Frame {
    m: Metrics,
    contract: Contract,
    plan: Plan,
    bid: Option<f64>,
    ask: Option<f64>,
    busy: bool,
    balance: f64,
    free_margin: f64,
    currency: String,
    quote_currency: String,
    name: SharedString,
    spread: String,
    lots: Option<f64>,
    units: Option<f64>,
    /// The margin of the order, when the server has said it for this volume.
    margin: Option<f64>,
}

impl Frame {
    fn money(&self, amount: f64) -> String {
        math::format_money(amount, &self.currency)
    }

    /// An amount and its share of the balance.
    fn share(&self, amount: f64) -> String {
        if self.balance > 0.0 {
            format!("{:.2}%", amount / self.balance * 100.0)
        } else {
            String::new()
        }
    }

    fn price(&self, price: Option<f64>) -> String {
        price.map_or_else(|| "-".to_owned(), |p| self.contract.format_price(p))
    }
}

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

    /// A small button that opens a list under it, showing what is picked.
    fn select(
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
                    .h(px(24.))
                    .px_2()
                    .rounded_sm()
                    .cursor_pointer()
                    .text_size(px(11.))
                    .text_color(theme::muted_fg())
                    .hover(|s| s.bg(theme::surface_hover()).text_color(theme::fg()))
                    .on_click(move |_, _, cx| toggle.toggle(cx))
                    .child(text)
                    .child(ui::icon_colored(
                        IconName::ChevronDown,
                        12.,
                        theme::muted_fg(),
                    )),
            )
            .children(menu.popup(entries, popup::Placement::Below(26.), window, cx))
            .into_any_element()
    }

    /// The menu that picks how the volume is sized.
    fn size_menu(&self, currency: &str, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
    fn unit_menu(
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
    fn preset_values(&self, f: &Frame, cx: &App) -> Vec<(String, f64)> {
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
                            format!("{}%", widgets::format_number(*v, 2))
                        }
                        SizeMode::RiskMoney => widgets::format_number(*v, 2),
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
                    (widgets::format_number(amount, 2), amount)
                })
                .to_vec(),
            // Lots, the risk and the margin always have a list of their own.
            _ => Vec::new(),
        }
    }

    /// Quick values for the volume, in its mode.
    fn presets(&self, f: &Frame, cx: &Context<Self>) -> AnyElement {
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
                            let text = widgets::format_number(value, decimals);
                            t.write(&t.size.clone(), text, window, cx);
                        });
                    })
                    .child(label),
            );
        }
        row.into_any_element()
    }

    // ---- the blocks ----

    fn header(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
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
                            .text_size(px(15.))
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

    fn sides(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
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
                            .text_size(px(if f.m.compact { 13. } else { 15. }))
                            .font_semibold()
                            .text_color(theme::fg())
                            .child(f.price(price)),
                    )
                })
                .when(one_click, |el| {
                    el.child(
                        div()
                            .text_size(px(9.))
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
                                .text_size(px(10.))
                                .text_color(theme::muted_fg())
                                .child(f.spread.clone()),
                        ),
                )
            })
            .into_any_element()
    }

    fn order_block(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
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
            .child(tabs(
                "ticket-kind",
                &labels,
                index,
                f.m.small,
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

    fn size_block(&self, f: &Frame, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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

    fn protection(
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
                        settings_ui::switch(SharedString::from(format!("ticket-{label}")), on)
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
                el.child(NumberInput::new(state).small()).child(
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

    /// The expiry of a pending order, the slippage of a market one, the trailing and the
    /// guarantee of a stop loss, and the comment.
    fn options(&self, f: &Frame, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        let span_this = cx.entity();
        let span = self.expiry_span;
        let span_menu = self.select(
            "ticket-expiry-span",
            span.label().to_owned(),
            move || {
                Span::ALL
                    .into_iter()
                    .map(|choice| {
                        let this = span_this.clone();
                        Entry::new(choice.label())
                            .checked(choice == span)
                            .on_click(move |_, cx| {
                                this.update(cx, |t, cx| {
                                    t.expiry_span = choice;
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
        let stop_on = self.stop_on;
        let switch = |id: &'static str, text: &'static str, on: bool, enabled: bool| {
            let this = this.clone();
            settings_ui::switch(id, on && enabled)
                .disabled(!enabled)
                .label(text)
                .on_click(move |checked: &bool, _, cx| {
                    let checked = *checked;
                    this.update(cx, |t, cx| {
                        match id {
                            "ticket-trailing" => t.trailing = checked,
                            "ticket-guaranteed" => t.guaranteed = checked,
                            _ => t.slippage_on = checked,
                        }
                        cx.notify();
                    });
                })
        };
        let tif_this = cx.entity();
        let tif_index = usize::from(self.tif == Tif::GoodTillDate);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when(self.kind.is_pending(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(f.m.text))
                                .text_color(theme::muted_fg())
                                .child("Expires"),
                        )
                        .child(tabs(
                            "ticket-tif",
                            &["Until cancelled", "Good till date"],
                            tif_index,
                            f.m.small,
                            move |choice, _, cx| {
                                tif_this.update(cx, |t, cx| {
                                    t.tif = if choice == 1 {
                                        Tif::GoodTillDate
                                    } else {
                                        Tif::GoodTillCancel
                                    };
                                    cx.notify();
                                });
                            },
                        ))
                        .when(self.tif == Tif::GoodTillDate, |el| {
                            el.child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .flex_1()
                                            .child(NumberInput::new(&self.expiry).small()),
                                    )
                                    .child(span_menu),
                            )
                        }),
                )
            })
            .when(self.kind == Kind::Market, |el| {
                el.child(switch(
                    "ticket-slippage",
                    "Limit the slippage",
                    self.slippage_on,
                    true,
                ))
                .when(self.slippage_on, |el| {
                    el.child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .child(NumberInput::new(&self.slippage).small()),
                            )
                            .child(
                                div()
                                    .text_size(px(f.m.small))
                                    .text_color(theme::muted_fg())
                                    .child("pips at most"),
                            ),
                    )
                })
            })
            .child(switch(
                "ticket-trailing",
                "Trailing stop",
                self.trailing,
                stop_on,
            ))
            .child(switch(
                "ticket-guaranteed",
                "Guaranteed stop loss",
                self.guaranteed,
                stop_on,
            ))
            .child(Input::new(&self.comment).small())
            .into_any_element()
    }

    /// What is open on the symbol, with what can be done to it.
    fn positions(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
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
                            .text_size(px(f.m.text))
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
                                    .text_size(px(f.m.small))
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
                    .text_size(px(f.m.small))
                    .text_color(theme::muted_fg())
                    .child(format!(
                        "{} {} {} @ {}",
                        if buy { "Buy" } else { "Sell" },
                        order
                            .kind()
                            .map_or("order", wyck_openapi::account::OrderType::label),
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
                    .text_size(px(f.m.small))
                    .text_color(theme::muted_fg())
                    .child(format!("OPEN ON {}", f.name.to_uppercase()))
                    .when(count > 1, |el| {
                        el.child(
                            div()
                                .id("ticket-close-all")
                                .cursor_pointer()
                                .text_color(theme::accent())
                                .hover(|s| s.underline())
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
                                .child("Close all"),
                        )
                    }),
            )
            .children(rows)
            .when(count == 0, |el| {
                el.child(
                    div()
                        .text_size(px(f.m.small))
                        .text_color(theme::muted_fg())
                        .child("Nothing open on this symbol."),
                )
            })
            .into_any_element()
    }

    fn summary(&self, f: &Frame) -> AnyElement {
        let plan = &f.plan;
        let ratio = plan
            .risk
            .zip(plan.reward)
            .filter(|(r, _)| *r > 0.0)
            .map(|(r, w)| format!("1 : {:.2}", w / r));
        let mut card = div()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .rounded_md()
            .bg(theme::surface())
            .text_size(px(f.m.text));
        for line in self.layout.visible_lines() {
            let text = match line {
                Line::Volume => f.lots.zip(f.units).map_or_else(
                    || "-".into(),
                    |(l, u)| format!("{} lots, {} units", math::format_lots(l), format_units(u)),
                ),
                Line::Notional => plan.sized.zip(plan.entry).zip(plan.rate).map_or_else(
                    || "-".into(),
                    |((s, e), r)| f.money(s.volume as f64 / 100.0 * e * r),
                ),
                Line::Risk => match plan.risk {
                    Some(r) => format!("{}  {}", f.money(r), f.share(r)),
                    None if self.stop_on => "-".into(),
                    None => "No stop loss".into(),
                },
                Line::Reward => plan
                    .reward
                    .map_or_else(|| "-".into(), |r| format!("{}  {}", f.money(r), f.share(r))),
                Line::RiskReward => ratio.clone().unwrap_or_else(|| "-".into()),
                Line::Margin => f.margin.map_or_else(|| "-".into(), |m| f.money(m)),
                Line::PipValue => match (f.units, plan.rate) {
                    (Some(u), Some(rate)) => f.money(f.contract.pip() * u * rate),
                    (Some(u), None) => math::format_money(f.contract.pip() * u, &f.quote_currency),
                    _ => "-".into(),
                },
                Line::SpreadCost => f
                    .bid
                    .zip(f.ask)
                    .zip(f.units)
                    .zip(plan.rate)
                    .map_or_else(|| "-".into(), |(((b, a), u), r)| f.money((a - b) * u * r)),
            };
            card = card.child(summary_row(line.label(), text));
        }
        card.into_any_element()
    }

    /// The warnings that do not stop the order, then what does.
    fn warnings(&self, f: &Frame) -> Vec<String> {
        let mut warnings: Vec<String> = Vec::new();
        match f.plan.sized.and_then(|s| s.limit) {
            Some(Limit::Min) => warnings.push(format!(
                "Raised to the least volume, {} lots: more at stake than asked",
                math::format_lots(f.contract.lots_of_volume(f.contract.min_volume))
            )),
            Some(Limit::Max) => warnings.push("Cut to the most volume the broker takes".into()),
            None => {}
        }
        if let Some(m) = f.margin
            && m > f.free_margin
        {
            warnings.push("Not enough free margin".into());
        }
        let high = self.layout.high_risk;
        if let Some(risk) = f.plan.risk
            && f.balance > 0.0
            && risk / f.balance * 100.0 > high
        {
            warnings.push(format!(
                "Risks more than {}% of the balance",
                widgets::format_number(high, 2)
            ));
        }
        warnings.extend(f.plan.problem.clone());
        warnings
    }

    fn send_button(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
        let blocked = f.plan.problem.is_some() || f.busy;
        Button::new("ticket-send")
            .cursor_pointer()
            .when(blocked, |button| button.cursor_not_allowed())
            .label(self.describe(&f.plan, cx))
            .with_size(gpui_kit::component::Size::Large)
            .disabled(blocked)
            .loading(f.busy)
            .bg(if self.buy {
                theme::chart_up()
            } else {
                theme::chart_down()
            })
            .text_color(theme::bg())
            .on_click(cx.listener(|this, _, window, cx| this.send(window, cx)))
            .into_any_element()
    }
}

impl Render for OrderTicket {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let contract = self.contract(cx);
        let mut plan = self.plan(cx);
        // The protections the defaults ask for start as soon as there is a price to start from.
        if self.autofill && plan.entry.is_some() {
            self.apply_defaults(window, cx);
            plan = self.plan(cx);
        }
        // The margin follows the volume.
        if let Some(symbol) = self.symbol.as_ref().map(|s| s.id) {
            let volume = plan.sized.map_or(contract.min_volume, |s| s.volume);
            if self.margin_asked != Some((symbol, volume)) {
                self.request_margin(symbol, volume, cx);
            }
        }
        let (bid, ask) = self.quote(cx);
        let (busy, summary, currency, quote_currency) = {
            let account = self.account.read(cx);
            (
                account.is_busy(Busy::Placing),
                account.summary(),
                account.book.currency.clone(),
                self.symbol
                    .as_ref()
                    .and_then(|s| account.book.quote_currency.get(&s.id).cloned())
                    .unwrap_or_default(),
            )
        };
        let margin = self
            .margin
            .filter(|m| Some(m.volume) == plan.sized.map(|s| s.volume))
            .map(|m| side_of(m.order, self.buy));
        let frame = Frame {
            m: Metrics::of(self.layout.density),
            spread: bid
                .zip(ask)
                .map(|(b, a)| format!("{:.1}", contract.pips(a - b)))
                .unwrap_or_default(),
            lots: plan.sized.map(|s| contract.lots_of_volume(s.volume)),
            units: plan.sized.map(|s| s.volume as f64 / 100.0),
            name: self
                .symbol
                .as_ref()
                .map_or_else(|| SharedString::from("No symbol"), |s| s.name.clone()),
            contract,
            plan,
            bid,
            ask,
            busy,
            balance: summary.balance,
            free_margin: summary.free_margin,
            currency,
            quote_currency,
            margin,
        };
        let m = frame.m;
        let warnings = self.warnings(&frame);

        let mut blocks: Vec<AnyElement> = Vec::new();
        for section in self.layout.clone().visible_sections() {
            blocks.push(match section {
                Section::Sides => self.sides(&frame, cx),
                Section::Order => self.order_block(&frame, cx),
                Section::Size => self.size_block(&frame, window, cx),
                Section::StopLoss => self.protection(true, &frame, window, cx),
                Section::TakeProfit => self.protection(false, &frame, window, cx),
                Section::Options => self.options(&frame, window, cx),
                Section::Positions => self.positions(&frame, cx),
                Section::Summary => self.summary(&frame),
                Section::Send => {
                    // The warnings sit over the button that sends, or at the end without one.
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(warning_rows(&warnings))
                        .child(self.send_button(&frame, cx))
                        .into_any_element()
                }
            });
        }
        let has_send = self.layout.shows(Section::Send);

        div()
            .id("order-ticket")
            .flex()
            .flex_col()
            .gap(px(m.gap))
            .p(px(m.pad))
            .h_full()
            .overflow_y_scroll()
            .child(self.header(&frame, cx))
            .children(blocks)
            .when(!has_send, |el| el.children(warning_rows(&warnings)))
            .child(
                settings_ui::switch("ticket-one-click", self.one_click)
                    .label("One-click trading (no confirmation)")
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.one_click = *checked;
                        cx.emit(TicketEvent::LinesChanged);
                        cx.notify();
                    })),
            )
    }
}

fn warning_rows(warnings: &[String]) -> Vec<AnyElement> {
    warnings
        .iter()
        .map(|message| {
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
                .child(message.clone())
                .into_any_element()
        })
        .collect()
}

/// Buttons side by side that share the width, one of them chosen.
fn tabs(
    id: &'static str,
    options: &[&str],
    selected: usize,
    text: f32,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let on_select = Rc::new(on_select);
    let mut strip = div()
        .flex()
        .flex_row()
        .items_center()
        .p_0p5()
        .gap_0p5()
        .rounded_md()
        .bg(theme::bg())
        .border_1()
        .border_color(theme::border_subtle());
    for (index, label) in options.iter().enumerate() {
        let chosen = index == selected;
        let on_select = on_select.clone();
        strip = strip.child(
            div()
                .id((id, index))
                .flex_1()
                .h(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .cursor_pointer()
                .text_size(px(text + 1.))
                .text_color(if chosen {
                    theme::fg()
                } else {
                    theme::muted_fg()
                })
                .when(chosen, |el| el.bg(theme::accent_selected()))
                .when(!chosen, |el| el.hover(|s| s.bg(theme::surface_hover())))
                .on_click(move |_, window, cx| on_select(index, window, cx))
                .child(SharedString::from((*label).to_owned())),
        );
    }
    strip
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
