//! The panel that customizes the order ticket: where it sits and how wide it is, which blocks
//! show and in what order, the shortcuts under the volume, and what a new order starts with.
//!
//! It is built like the other settings panels (see [`crate::app::settings_ui`]) and shows in the
//! same modal. Every change applies to the ticket at once and is remembered; there is nothing to
//! confirm. The numbers are read as they are typed, and a value that is not a number leaves the
//! setting as it was.

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState, NumberInput};
use gpui_kit::component::{Disableable, Sizable};

use super::OrderTicket;
use super::prefs::{Density, Dock, Kind, Layout, Placed, Slot, Span, Tif, shift};
use crate::app::settings_ui::{self as ui, Head, Tab};
use crate::app::trading::math::SizeMode;
use crate::app::{modal, widgets};

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
        modal::open(editor, modal::Options::new(760.0, 600.0), window, cx);
    });
}

/// The numbers of a list, as they are typed: separated by commas, semicolons or spaces. What is
/// not a positive number is left out.
fn parse_list(text: &str) -> Vec<f64> {
    text.split([',', ';', ' '])
        .filter_map(widgets::parse_number)
        .filter(|v| *v > 0.0)
        .collect()
}

fn list_text(values: &[f64]) -> String {
    values
        .iter()
        .map(|v| widgets::format_number(*v, 4))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The number fields of the defaults page: the value now, its range, its step, its decimals, and
/// what changes in the layout when it is typed.
type NumberSpec = (fn(&Layout) -> f64, f64, f64, f64, usize, NumberSetter);

const NUMBERS: [NumberSpec; 5] = [
    (
        |l| l.defaults.stop_pips,
        0.1,
        100_000.,
        1.,
        1,
        |l, v| {
            l.defaults.stop_pips = v;
        },
    ),
    (
        |l| l.defaults.target_ratio,
        0.1,
        1_000.,
        0.5,
        2,
        |l, v| {
            l.defaults.target_ratio = v;
        },
    ),
    (
        |l| l.defaults.expiry,
        1.,
        100_000.,
        1.,
        0,
        |l, v| {
            l.defaults.expiry = v;
        },
    ),
    (
        |l| l.defaults.slippage_pips,
        0.,
        10_000.,
        0.5,
        1,
        |l, v| {
            l.defaults.slippage_pips = v;
        },
    ),
    (|l| l.high_risk, 0.1, 100., 0.5, 1, |l, v| l.high_risk = v),
];

impl Customizer {
    fn new(ticket: Entity<OrderTicket>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let layout = ticket.read(cx).layout().clone();
        let mut subscriptions = vec![cx.observe(&ticket, |_this, _ticket, cx| cx.notify())];

        let width = cx.new(|cx| {
            widgets::number_state(
                f64::from(layout.width),
                f64::from(super::prefs::WIDTH_MIN),
                f64::from(super::prefs::WIDTH_MAX),
                10.,
                0,
                window,
                cx,
            )
        });
        subscriptions.push(cx.subscribe(&width, |this, state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change)
                && let Some(value) = widgets::parse_number(&state.read(cx).value())
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
        for (get, min, max, step, decimals, set) in NUMBERS {
            let state = cx.new(|cx| {
                widgets::number_state(get(&layout), min, max, step, decimals, window, cx)
            });
            subscriptions.push(
                cx.subscribe(&state, move |this, state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change)
                        && let Some(value) = widgets::parse_number(&state.read(cx).value())
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
            s.set_value(
                widgets::format_number(f64::from(layout.width), 0),
                window,
                cx,
            );
        });
        for (state, (mode, _, _)) in self.presets.iter().zip(PRESET_MODES) {
            let text = list_text(layout.presets.of(mode));
            state.update(cx, |s, cx| s.set_value(text, window, cx));
        }
        for (state, (get, _, _, _, decimals, _)) in self.numbers.iter().zip(NUMBERS) {
            let text = widgets::format_number(get(&layout), decimals);
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
        ui::toggle(id, on, move |value, _, cx| {
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
        widgets::segmented(id, options, selected, move |index, _, cx| {
            this.update(cx, |c, cx| c.edit(cx, |l| set(l, index)));
        })
        .into_any_element()
    }

    fn panel_page(&self, layout: &Layout, cx: &Context<Self>) -> AnyElement {
        ui::page()
            .child(ui::group(
                IconName::PanelRight,
                "Placement",
                [
                    ui::field(
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
                    ui::field(
                        "Width",
                        Some("Also dragged from the edge of the panel"),
                        widgets::number_field(&self.width, 130.),
                    ),
                    ui::field(
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
            .child(ui::group(
                IconName::ArrowLeftRight,
                "Buy and sell buttons",
                [
                    ui::field(
                        "Buy on the left, sell on the right",
                        None,
                        self.flag("custom-buy-first", layout.buy_first, cx, |l, v| {
                            l.buy_first = v;
                        }),
                    ),
                    ui::field(
                        "Prices on the buttons",
                        None,
                        self.flag("custom-prices", layout.show_prices, cx, |l, v| {
                            l.show_prices = v;
                        }),
                    ),
                    ui::field(
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
                ui::field(
                    placed.item.label(),
                    None,
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(
                            Button::new(SharedString::from(format!("{id}-up-{index}")))
                                .cursor_pointer()
                                .ghost()
                                .xsmall()
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
                            Button::new(SharedString::from(format!("{id}-down-{index}")))
                                .cursor_pointer()
                                .ghost()
                                .xsmall()
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
                        .child(ui::toggle(
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
        ui::page()
            .child(ui::group(
                IconName::LayoutList,
                "Blocks of the panel",
                self.slots("custom-sections", &layout.sections, |l| &mut l.sections, cx),
            ))
            .child(ui::note(
                "Switch a block off to hide it, and move it with the arrows to change where it sits.",
            ))
            .child(ui::group(
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
            .map(|(state, (_, label, hint))| {
                ui::field(
                    label,
                    Some(hint),
                    div().w(px(220.)).child(Input::new(state).small()),
                )
            })
            .collect();
        ui::page()
            .child(ui::group(
                IconName::Ruler,
                "Buttons under the size field",
                rows,
            ))
            .child(ui::note(
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
                .child(NumberInput::new(&self.numbers[index]).small())
        };
        ui::page()
            .child(ui::group(
                IconName::SlidersHorizontal,
                "A new order starts with",
                [
                    ui::field(
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
                    ui::field(
                        "Stop loss turned on",
                        None,
                        self.flag("custom-stop-on", d.stop_on, cx, |l, v| {
                            l.defaults.stop_on = v;
                        }),
                    ),
                    ui::field(
                        "Take profit turned on",
                        None,
                        self.flag("custom-target-on", d.target_on, cx, |l, v| {
                            l.defaults.target_on = v;
                        }),
                    ),
                    ui::field(
                        "Stop loss distance",
                        Some("In pips, when a stop loss is turned on"),
                        number(0),
                    ),
                    ui::field(
                        "Take profit distance",
                        Some("As a multiple of the risk"),
                        number(1),
                    ),
                ],
            ))
            .child(ui::group(
                IconName::Timer,
                "Pending orders and slippage",
                [
                    ui::field(
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
                    ui::field("Good till date: lasts", None, number(2)),
                    ui::field(
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
                    ui::field(
                        "Slippage",
                        Some(
                            "In pips: the most a market order may slip, and the range of the limit of a stop limit",
                        ),
                        number(3),
                    ),
                ],
            ))
            .child(ui::group(
                IconName::TriangleAlert,
                "Warnings",
                [ui::field(
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
        ui::frame(
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
            ui::footer(
                vec![
                    ui::action(
                        "ticket-customize-reset",
                        "Reset everything",
                        Some(IconName::RotateCcw),
                        false,
                        move |window, cx| reset.update(cx, |c, cx| c.reset(window, cx)),
                    )
                    .into_any_element(),
                ],
                vec![
                    ui::action("ticket-customize-done", "Done", None, true, |window, cx| {
                        modal::close(window, cx);
                    })
                    .into_any_element(),
                ],
            ),
        )
    }
}
