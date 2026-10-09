//! The panel that customizes the order ticket: where it sits and how wide it is, which blocks
//! show and in what order, the shortcuts under the volume, and what a new order starts with.
//!
//! It is built like the other settings panels (see [`crate::ui::kit::form`]) and shows in the
//! same modal. Every change applies to the ticket at once and is remembered; there is nothing to
//! confirm. The numbers are read as they are typed, and a value that is not a number leaves the
//! setting as it was.

use crate::ui::kit::field;
use crate::ui::kit::icon::IconName;
use crate::ui::kit::input::{InputEvent, InputState};
use crate::ui::kit::prelude::Disableable;
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div, px};

use super::OrderTicket;
use crate::app::prefs::ticket::{Density, Dock, Kind, Layout, Placed, Slot, Span, Tif, shift};
use crate::domain::trading::math::SizeMode;
use crate::ui::kit::{button, controls, form, form::Head, form::Tab, modal, number, tokens};

/// The ways of sizing that have a list of shortcuts of their own, with what the list is for.
const PRESET_MODES: [(SizeMode, &str, &str); 5] = [
    (SizeMode::Lots, "Lots", "For example 0.01, 0.1, 0.5, 1"),
    (
        SizeMode::Units,
        "Units",
        "Leave empty to work them out from the symbol",
    ),
    (
        SizeMode::RiskBalance,
        "Risk in percent",
        "Of the balance or of the equity",
    ),
    (
        SizeMode::RiskMoney,
        "Risk in money",
        "Leave empty to work them out from the balance",
    ),
    (
        SizeMode::FreeMargin,
        "Share of the free margin",
        "In percent",
    ),
];

type NumberSetter = fn(&mut Layout, f64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Panel,
    Blocks,
    Shortcuts,
    Defaults,
}

impl Page {
    const ALL: [Self; 4] = [Self::Panel, Self::Blocks, Self::Shortcuts, Self::Defaults];

    fn tab(self) -> Tab {
        match self {
            Self::Panel => Tab {
                label: "Panel",
                icon: IconName::PanelRight,
            },
            Self::Blocks => Tab {
                label: "Blocks",
                icon: IconName::LayoutList,
            },
            Self::Shortcuts => Tab {
                label: "Size shortcuts",
                icon: IconName::Ruler,
            },
            Self::Defaults => Tab {
                label: "New order",
                icon: IconName::SlidersHorizontal,
            },
        }
    }
}

pub struct Customizer {
    ticket: Entity<OrderTicket>,
    page: Page,
    width: Entity<InputState>,
    presets: Vec<Entity<InputState>>,
    numbers: Vec<Entity<InputState>>,
    _subscriptions: Vec<Subscription>,
}

/// Opens the panel for a ticket.
pub fn open(ticket: Entity<OrderTicket>, window: &mut Window, cx: &mut App) {
    // Opened once the click that asked for it is done with the ticket.
    window.defer(cx, move |window, cx| {
        let editor = cx.new(|cx| Customizer::new(ticket, window, cx));
        modal::open(
            editor,
            modal::Options::new(760.0, 600.0).label("Customize the order ticket"),
            window,
            cx,
        );
    });
}

/// The numbers of a list, as they are typed: separated by commas, semicolons or spaces. What is
/// not a positive number is left out.
fn parse_list(text: &str) -> Vec<f64> {
    text.split([',', ';', ' '])
        .filter_map(number::parse)
        .filter(|v| *v > 0.0)
        .collect()
}

fn list_text(values: &[f64]) -> String {
    values
        .iter()
        .map(|v| number::format(*v, 4))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The number fields of the defaults page: the value now, what it holds, its range, and what
/// changes in the layout when it is typed.
type NumberSpec = (fn(&Layout) -> f64, number::Kind, f64, f64, NumberSetter);

const NUMBERS: [NumberSpec; 5] = [
    (
        |l| l.defaults.stop_pips,
        number::Kind::Pips,
        0.1,
        100_000.,
        |l, v| {
            l.defaults.stop_pips = v;
        },
    ),
    (
        |l| l.defaults.target_ratio,
        number::Kind::Ratio,
        0.1,
        1_000.,
        |l, v| {
            l.defaults.target_ratio = v;
        },
    ),
    (
        |l| l.defaults.expiry,
        number::Kind::Count,
        1.,
        100_000.,
        |l, v| {
            l.defaults.expiry = v;
        },
    ),
    (
        |l| l.defaults.slippage_pips,
        number::Kind::Slippage,
        0.,
        10_000.,
        |l, v| {
            l.defaults.slippage_pips = v;
        },
    ),
    (
        |l| l.high_risk,
        number::Kind::Percent,
        0.1,
        100.,
        |l, v| l.high_risk = v,
    ),
];

impl Customizer {
    fn new(ticket: Entity<OrderTicket>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let layout = ticket.read(cx).layout().clone();
        let mut subscriptions = vec![cx.observe(&ticket, |_this, _ticket, cx| cx.notify())];

        let width = cx.new(|cx| {
            number::state(
                number::Kind::PanelWidth,
                f64::from(layout.width),
                window,
                cx,
            )
            .min(f64::from(crate::app::prefs::ticket::WIDTH_MIN))
            .max(f64::from(crate::app::prefs::ticket::WIDTH_MAX))
        });
        subscriptions.push(cx.subscribe(&width, |this, state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change)
                && let Some(value) = number::parse(&state.read(cx).value())
            {
                this.edit(cx, |l| l.width = value as f32);
            }
        }));

        let mut presets = Vec::new();
        for (mode, _, _) in PRESET_MODES {
            let text = list_text(layout.presets.of(mode));
            let state = cx.new(|cx| InputState::new(window, cx).default_value(text));
            subscriptions.push(
                cx.subscribe(&state, move |this, state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        let values = parse_list(&state.read(cx).value());
                        this.edit(cx, |l| *l.presets.of_mut(mode) = values);
                    }
                }),
            );
            presets.push(state);
        }

        let mut numbers = Vec::new();
        for (get, kind, min, max, set) in NUMBERS {
            let state = cx.new(|cx| {
                number::state(kind, get(&layout), window, cx)
                    .min(min)
                    .max(max)
            });
            subscriptions.push(
                cx.subscribe(&state, move |this, state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change)
                        && let Some(value) = number::parse(&state.read(cx).value())
                    {
                        this.edit(cx, |l| set(l, value));
                    }
                }),
            );
            numbers.push(state);
        }

        Self {
            ticket,
            page: Page::Panel,
            width,
            presets,
            numbers,
            _subscriptions: subscriptions,
        }
    }

    /// Puts everything back as it was first, fields included.
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ticket.update(cx, |t, cx| t.reset_layout(cx));
        let layout = self.ticket.read(cx).layout().clone();
        self.width.update(cx, |s, cx| {
            s.set_value(number::format(f64::from(layout.width), 0), window, cx);
        });
        for (state, (mode, _, _)) in self.presets.iter().zip(PRESET_MODES) {
            let text = list_text(layout.presets.of(mode));
            state.update(cx, |s, cx| s.set_value(text, window, cx));
        }
        for (state, (get, kind, _, _, _)) in self.numbers.iter().zip(NUMBERS) {
            let text = number::format(get(&layout), kind.spec().decimals);
            state.update(cx, |s, cx| s.set_value(text, window, cx));
        }
        cx.notify();
    }

    fn edit(&self, cx: &mut App, change: impl FnOnce(&mut Layout)) {
        self.ticket.update(cx, |t, cx| t.edit_layout(cx, change));
    }

    /// A switch that sets a flag of the layout.
    fn flag(
        &self,
        id: &'static str,
        on: bool,
        cx: &Context<Self>,
        set: fn(&mut Layout, bool),
    ) -> AnyElement {
        let this = cx.entity();
        controls::toggle(id, on, move |value, _, cx| {
            this.update(cx, |c, cx| c.edit(cx, |l| set(l, value)));
        })
        .into_any_element()
    }

    /// Buttons side by side that set a choice of the layout.
    fn choice(
        &self,
        id: &'static str,
        options: &[&str],
        selected: usize,
        cx: &Context<Self>,
        set: fn(&mut Layout, usize),
    ) -> AnyElement {
        let this = cx.entity();
        controls::segmented(id, options, selected, move |index, _, cx| {
            this.update(cx, |c, cx| c.edit(cx, |l| set(l, index)));
        })
        .into_any_element()
    }

    fn panel_page(&self, layout: &Layout, cx: &Context<Self>) -> AnyElement {
        form::page()
            .child(form::group(
                IconName::PanelRight,
                "Placement",
                [
                    form::field(
                        "Side of the charts",
                        None,
                        self.choice(
                            "custom-dock",
                            &["Left", "Right"],
                            usize::from(layout.dock == Dock::Right),
                            cx,
                            |l, i| l.dock = if i == 0 { Dock::Left } else { Dock::Right },
                        ),
                    ),
                    form::field(
                        "Width",
                        Some("Also dragged from the edge of the panel"),
                        number::field(&self.width, tokens::field::wide()),
                    ),
                    form::field(
                        "Spacing",
                        Some("Compact fits more of the panel on a small screen"),
                        self.choice(
                            "custom-density",
                            &["Comfortable", "Compact"],
                            usize::from(layout.density == Density::Compact),
                            cx,
                            |l, i| {
                                l.density = if i == 0 {
                                    Density::Comfortable
                                } else {
                                    Density::Compact
                                };
                            },
                        ),
                    ),
                ],
            ))
            .child(form::group(
                IconName::ArrowLeftRight,
                "Buy and sell buttons",
                [
                    form::field(
                        "Buy on the left, sell on the right",
                        None,
                        self.flag("custom-buy-first", layout.buy_first, cx, |l, v| {
                            l.buy_first = v;
                        }),
                    ),
                    form::field(
                        "Prices on the buttons",
                        None,
                        self.flag("custom-prices", layout.show_prices, cx, |l, v| {
                            l.show_prices = v;
                        }),
                    ),
                    form::field(
                        "Spread between the buttons",
                        None,
                        self.flag("custom-spread", layout.show_spread, cx, |l, v| {
                            l.show_spread = v;
                        }),
                    ),
                ],
            ))
            .into_any_element()
    }

    /// A list of slots, each with its arrows and its switch.
    fn slots<T: Slot>(
        &self,
        id: &'static str,
        list: &[Placed<T>],
        pick: fn(&mut Layout) -> &mut Vec<Placed<T>>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let last = list.len().saturating_sub(1);
        list.iter()
            .enumerate()
            .map(|(index, placed)| {
                let this = cx.entity();
                let (up, down, toggle) = (this.clone(), this.clone(), this);
                form::field(
                    placed.item.label(),
                    None,
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(
                            crate::ui::kit::button::dense(SharedString::from(format!(
                                "{id}-up-{index}"
                            )))
                            .icon(IconName::ChevronUp)
                            .tooltip("Move up")
                            .disabled(index == 0)
                            .on_click(move |_, _, cx| {
                                up.update(cx, |c, cx| {
                                    c.edit(cx, |l| {
                                        shift(pick(l), index, -1);
                                    });
                                });
                            }),
                        )
                        .child(
                            crate::ui::kit::button::dense(SharedString::from(format!(
                                "{id}-down-{index}"
                            )))
                            .icon(IconName::ChevronDown)
                            .tooltip("Move down")
                            .disabled(index == last)
                            .on_click(move |_, _, cx| {
                                down.update(cx, |c, cx| {
                                    c.edit(cx, |l| {
                                        shift(pick(l), index, 1);
                                    });
                                });
                            }),
                        )
                        .child(controls::toggle(
                            SharedString::from(format!("{id}-{index}")),
                            placed.shown,
                            move |shown, _, cx| {
                                toggle.update(cx, |c, cx| {
                                    c.edit(cx, |l| pick(l)[index].shown = shown);
                                });
                            },
                        )),
                )
            })
            .collect()
    }

    fn blocks_page(&self, layout: &Layout, cx: &Context<Self>) -> AnyElement {
        form::page()
            .child(form::group(
                IconName::LayoutList,
                "Blocks of the panel",
                self.slots("custom-sections", &layout.sections, |l| &mut l.sections, cx),
            ))
            .child(form::note(
                "Switch a block off to hide it, and move it with the arrows to change where it sits.",
            ))
            .child(form::group(
                IconName::ListChecks,
                "Lines of the summary",
                self.slots("custom-lines", &layout.lines, |l| &mut l.lines, cx),
            ))
            .into_any_element()
    }

    fn shortcuts_page(&self) -> AnyElement {
        let rows: Vec<AnyElement> = self
            .presets
            .iter()
            .zip(PRESET_MODES)
            .map(|(state, (_, label, hint))| form::field(label, Some(hint), field::text(state)))
            .collect();
        form::page()
            .child(form::group(
                IconName::Ruler,
                "Buttons under the size field",
                rows,
            ))
            .child(form::note(
                "Values separated by commas, up to six for each way of sizing.",
            ))
            .into_any_element()
    }

    fn defaults_page(&self, layout: &Layout, cx: &Context<Self>) -> AnyElement {
        let d = &layout.defaults;
        let kinds: Vec<&str> = Kind::ALL.iter().map(|k| k.label()).collect();
        let spans: Vec<&str> = Span::ALL.iter().map(|s| s.label()).collect();
        let number = |index: usize| {
            div()
                .w(px(130.))
                .child(crate::ui::kit::input::number(&self.numbers[index]))
        };
        form::page()
            .child(form::group(
                IconName::SlidersHorizontal,
                "A new order starts with",
                [
                    form::field(
                        "Order type",
                        None,
                        self.choice(
                            "custom-kind",
                            &kinds,
                            Kind::ALL.iter().position(|k| *k == d.kind).unwrap_or(0),
                            cx,
                            |l, i| l.defaults.kind = Kind::ALL[i],
                        ),
                    ),
                    form::field(
                        "Stop loss turned on",
                        None,
                        self.flag("custom-stop-on", d.stop_on, cx, |l, v| {
                            l.defaults.stop_on = v;
                        }),
                    ),
                    form::field(
                        "Take profit turned on",
                        None,
                        self.flag("custom-target-on", d.target_on, cx, |l, v| {
                            l.defaults.target_on = v;
                        }),
                    ),
                    form::field(
                        "Stop loss distance",
                        Some("In pips, when a stop loss is turned on"),
                        number(0),
                    ),
                    form::field(
                        "Take profit distance",
                        Some("As a multiple of the risk"),
                        number(1),
                    ),
                ],
            ))
            .child(form::group(
                IconName::Timer,
                "Pending orders and slippage",
                [
                    form::field(
                        "A pending order lasts",
                        None,
                        self.choice(
                            "custom-tif",
                            &["Until cancelled", "Good till date"],
                            usize::from(d.tif == Tif::GoodTillDate),
                            cx,
                            |l, i| {
                                l.defaults.tif = if i == 1 {
                                    Tif::GoodTillDate
                                } else {
                                    Tif::GoodTillCancel
                                };
                            },
                        ),
                    ),
                    form::field("Good till date: lasts", None, number(2)),
                    form::field(
                        "Good till date: unit",
                        None,
                        self.choice(
                            "custom-span",
                            &spans,
                            Span::ALL
                                .iter()
                                .position(|s| *s == d.expiry_span)
                                .unwrap_or(1),
                            cx,
                            |l, i| l.defaults.expiry_span = Span::ALL[i],
                        ),
                    ),
                    form::field(
                        "Slippage",
                        Some(
                            "In pips: the most a market order may slip, and the range of the limit of a stop limit",
                        ),
                        number(3),
                    ),
                ],
            ))
            .child(form::group(
                IconName::TriangleAlert,
                "Warnings",
                [form::field(
                    "Warn when an order risks more than",
                    Some("In percent of the balance"),
                    number(4),
                )],
            ))
            .into_any_element()
    }
}

impl Render for Customizer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = self.ticket.read(cx).layout().clone();
        let tabs: Vec<Tab> = Page::ALL.iter().map(|p| p.tab()).collect();
        let active = Page::ALL.iter().position(|p| *p == self.page).unwrap_or(0);
        let body = match self.page {
            Page::Panel => self.panel_page(&layout, cx),
            Page::Blocks => self.blocks_page(&layout, cx),
            Page::Shortcuts => self.shortcuts_page(),
            Page::Defaults => self.defaults_page(&layout, cx),
        };
        let this = cx.entity();
        let reset = cx.entity();
        form::frame(
            Head {
                icon: IconName::PanelRight,
                title: "Order panel".into(),
                subtitle: "Every change applies at once".into(),
            },
            &tabs,
            active,
            move |index, _, cx| {
                this.update(cx, |c, cx| {
                    c.page = Page::ALL[index];
                    cx.notify();
                });
            },
            modal::dismiss,
            body,
            form::footer(
                vec![
                    button::action(
                        "ticket-customize-reset",
                        "Reset everything",
                        Some(IconName::RotateCcw),
                        false,
                        move |window, cx| reset.update(cx, |c, cx| c.reset(window, cx)),
                    )
                    .into_any_element(),
                ],
                vec![
                    button::action("ticket-customize-done", "Done", None, true, |window, cx| {
                        modal::close(window, cx);
                    })
                    .into_any_element(),
                ],
            ),
        )
    }
}
