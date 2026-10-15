//! The pages of the ticket customizer: the panel, the blocks, the shortcuts and the defaults.

use super::{Customizer, PRESET_MODES};
use crate::app::prefs::ticket::{Density, Dock, Kind, Layout, Placed, Slot, Span, Tif, shift};
use gpui::prelude::*;
use gpui::{AnyElement, Context, SharedString, div, px};
use wyck_ui_kit::field;
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::prelude::Disableable;
use wyck_ui_kit::{controls, form, number, tokens};

impl Customizer {
    pub(super) fn panel_page(&self, layout: &Layout, cx: &Context<Self>) -> AnyElement {
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
    pub(super) fn slots<T: Slot>(
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
                            wyck_ui_kit::button::dense(SharedString::from(format!(
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
                            wyck_ui_kit::button::dense(SharedString::from(format!(
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

    pub(super) fn blocks_page(&self, layout: &Layout, cx: &Context<Self>) -> AnyElement {
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

    pub(super) fn shortcuts_page(&self) -> AnyElement {
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

    pub(super) fn defaults_page(&self, layout: &Layout, cx: &Context<Self>) -> AnyElement {
        let d = &layout.defaults;
        let kinds: Vec<&str> = Kind::ALL.iter().map(|k| k.label()).collect();
        let spans: Vec<&str> = Span::ALL.iter().map(|s| s.label()).collect();
        let number = |index: usize| {
            div()
                .w(px(tokens::field::wide()))
                .child(wyck_ui_kit::input::number(&self.numbers[index]))
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
