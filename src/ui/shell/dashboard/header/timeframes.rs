//! The timeframes of the header: the strip, the favorites, the custom ones and the menu of all.

use super::CUSTOM_TIMEFRAME_WIDTH;

use crate::ui::features::chart;
use crate::ui::kit::icon::IconName;
use crate::ui::kit::input::{InputEvent, InputState};
use crate::ui::kit::{controls, icon, menu, theme, tokens};
use crate::ui::shell::dashboard::{Dashboard, MenuKind};
use gpui::prelude::*;
use gpui::{Context, SharedString, Window, div, px};

impl Dashboard {
    /// The favorite timeframes as buttons, and a button that opens all of them: ticks, seconds,
    /// minutes, hours and days.
    pub(super) fn timeframe_strip(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if self.menu_is(MenuKind::Timeframes) && self.tf_custom.is_none() {
            let state = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. 7"));
            cx.subscribe_in(
                &state,
                window,
                |this, _state, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.add_custom_timeframe(window, cx);
                    }
                },
            )
            .detach();
            self.tf_custom = Some(state);
        }
        let current = self.multi.read(cx).active_timeframe(cx);
        let favorites = self.workspace.read(cx).preferences().favorites();
        let mut quick = Vec::new();
        for timeframe in favorites.iter().copied() {
            quick.push(timeframe_chip("tf", timeframe, current == timeframe, cx));
        }

        // When the chart is on a timeframe that has no button of its own, the menu button says which.
        let in_quick = favorites.contains(&current);
        let more = div()
            .id("tf-more")
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .h(px(tokens::height::control()))
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .text_size(px(tokens::text::body()))
            .text_color(if in_quick {
                theme::muted_fg()
            } else {
                theme::fg()
            })
            .when(!in_quick || self.menu_is(MenuKind::Timeframes), |el| {
                el.bg(theme::accent_selected())
            })
            .hover(|style| style.bg(theme::surface_hover()))
            .on_click(cx.listener(|this, _event, _window, cx| {
                this.toggle_menu(MenuKind::Timeframes, cx);
            }))
            .when(!in_quick, |el| el.child(current.label()))
            .child(icon::tinted(IconName::ChevronDown, 14., theme::muted_fg()));

        div()
            .relative()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .children(quick)
            .child(
                div().relative().flex_none().child(more).children(
                    self.menu_is(MenuKind::Timeframes)
                        .then(|| self.timeframe_menu(current, cx)),
                ),
            )
    }

    /// Moves the active chart to the next (`longer`) or previous timeframe of the favorites,
    /// going round at the ends.
    pub(in crate::ui::shell::dashboard) fn step_timeframe(
        &mut self,
        longer: bool,
        cx: &mut Context<Self>,
    ) {
        let list: Vec<chart::Timeframe> = self
            .workspace
            .read(cx)
            .preferences()
            .favorite_timeframes
            .iter()
            .filter_map(|code| chart::Timeframe::from_code(code))
            .collect();
        let current = self.multi.read(cx).active_timeframe(cx);
        if let Some(next) = current.step_in(&list, longer) {
            self.pick_timeframe(next, cx);
        }
    }

    pub(super) fn pick_timeframe(&mut self, timeframe: chart::Timeframe, cx: &mut Context<Self>) {
        self.close_menus();
        self.multi
            .update(cx, |multi, cx| multi.set_timeframe(timeframe, cx));
        cx.notify();
    }

    pub(super) fn toggle_favorite_timeframe(
        &mut self,
        timeframe: chart::Timeframe,
        cx: &mut Context<Self>,
    ) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.toggle_favorite_timeframe(timeframe));
        });
    }

    /// Adds the timeframe typed in the menu and shows it on the chart. The menu stays open, so
    /// it can be starred right away.
    pub(super) fn add_custom_timeframe(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.tf_custom.clone() else {
            return;
        };
        let text = state.read(cx).value().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let timeframe = if text.bytes().all(|b| b.is_ascii_digit()) {
            text.parse()
                .ok()
                .and_then(|count| chart::Timeframe::of(count, self.tf_unit))
        } else {
            chart::Timeframe::from_code(&text)
        };
        let Some(timeframe) = timeframe else {
            crate::ui::kit::toast::show(
                cx,
                crate::ui::kit::toast::Kind::Warning,
                "Not a timeframe",
                format!(
                    "\"{text}\" is not one: up to 300 seconds, a day in minutes or hours, 365 days, \
                     52 weeks or 24 months."
                ),
            );
            return;
        };
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.add_custom_timeframe(timeframe));
        });
        state.update(cx, |s, cx| s.set_value("", window, cx));
        self.multi
            .update(cx, |multi, cx| multi.set_timeframe(timeframe, cx));
        cx.notify();
    }

    pub(super) fn remove_custom_timeframe(
        &mut self,
        timeframe: chart::Timeframe,
        cx: &mut Context<Self>,
    ) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.remove_custom_timeframe(timeframe));
        });
    }

    /// A timeframe of the menu with its star, and a cross for one the user added.
    pub(super) fn menu_chip(
        &self,
        timeframe: chart::Timeframe,
        current: chart::Timeframe,
        favorite: bool,
        removable: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let small = |id: String, icon: IconName, color, tip: &'static str| {
            div()
                .id(SharedString::from(id))
                .flex()
                .items_center()
                .justify_center()
                .size(px(tokens::height::compact()))
                .rounded_md()
                .cursor_pointer()
                .hover(|style| style.bg(theme::surface_hover()))
                .tooltip(controls::tooltip(tip))
                .child(icon::tinted(icon, 13., color))
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .child(timeframe_chip(
                "tf-menu",
                timeframe,
                current == timeframe,
                cx,
            ))
            .child(
                small(
                    format!("tf-star-{}", timeframe.code()),
                    IconName::Star,
                    if favorite {
                        theme::amber()
                    } else {
                        theme::muted_fg()
                    },
                    "Keep in the header",
                )
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.toggle_favorite_timeframe(timeframe, cx);
                })),
            )
            .when(removable, |el| {
                el.child(
                    small(
                        format!("tf-remove-{}", timeframe.code()),
                        IconName::X,
                        theme::muted_fg(),
                        "Remove from the menu",
                    )
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.remove_custom_timeframe(timeframe, cx);
                    })),
                )
            })
    }

    /// Every timeframe, in sections, under the strip. A star keeps one in the header.
    pub(super) fn timeframe_menu(
        &self,
        current: chart::Timeframe,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let prefs = self.workspace.read(cx).preferences().clone();
        let mut card = menu::parts::panel(tokens::menu::PANEL_WIDTH);
        let section = |title: &'static str, chips: Vec<gpui::AnyElement>| {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(menu::parts::section_title(title).px_0())
                .child(div().flex().flex_row().flex_wrap().gap_1().children(chips))
        };
        for (title, items) in chart::GROUPS {
            let chips = items
                .iter()
                .copied()
                .map(|timeframe| {
                    let favorite = prefs.is_favorite_timeframe(timeframe);
                    self.menu_chip(timeframe, current, favorite, false, cx)
                        .into_any_element()
                })
                .collect();
            card = card.child(section(title, chips));
        }
        // The ones the user added, and the field that adds one.
        let customs: Vec<gpui::AnyElement> = prefs
            .customs()
            .into_iter()
            .map(|timeframe| {
                let favorite = prefs.is_favorite_timeframe(timeframe);
                self.menu_chip(timeframe, current, favorite, true, cx)
                    .into_any_element()
            })
            .collect();
        let mut units = div().flex().flex_row().gap_1();
        for unit in chart::Unit::ALL {
            let chosen = unit == self.tf_unit;
            units = units.child(
                div()
                    .id(SharedString::from(format!("tf-unit-{}", unit.label())))
                    .h(px(tokens::height::compact()))
                    .w(px(crate::ui::kit::tokens::height::compact()))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .text_size(px(tokens::text::body()))
                    .text_color(if chosen {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    })
                    .when(chosen, |el| el.bg(theme::accent_selected()))
                    .hover(|style| style.bg(theme::surface_hover()))
                    .tooltip(controls::tooltip(unit.label()))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.tf_unit = unit;
                        cx.notify();
                    }))
                    .child(unit.short()),
            );
        }
        let add_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .children(self.tf_custom.as_ref().map(|state| {
                div()
                    .w(px(CUSTOM_TIMEFRAME_WIDTH))
                    .child(crate::ui::kit::input::text(state))
            }))
            .child(units)
            .child(
                crate::ui::kit::button::accent("tf-add")
                    .label("Add")
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.add_custom_timeframe(window, cx);
                    })),
            );
        card = card.child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(menu::parts::section_title("Custom").px_0())
                .when(!customs.is_empty(), |el| {
                    el.child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap_1()
                            .children(customs),
                    )
                })
                .child(add_row),
        );
        card = card.child(
            div()
                .text_size(px(tokens::text::small()))
                .text_color(theme::muted_fg())
                .child(
                    "Star a timeframe to keep it in the header. A custom one is a number in the \
                     unit picked, or a code such as 45m, 2h or 3D.",
                ),
        );
        menu::parts::below(card, tokens::height::control(), 1)
    }
}

/// One timeframe as a small button; the active one is tinted.
pub(super) fn timeframe_chip(
    prefix: &str,
    timeframe: chart::Timeframe,
    active: bool,
    cx: &mut Context<Dashboard>,
) -> impl IntoElement + use<> {
    div()
        .id(SharedString::from(format!(
            "{prefix}-{}",
            timeframe.label()
        )))
        .h(px(tokens::height::control()))
        .px_2()
        .flex()
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .text_size(px(tokens::text::body()))
        .text_color(if active {
            theme::fg()
        } else {
            theme::muted_fg()
        })
        .when(active, |el| el.bg(theme::accent_selected()))
        .hover(|style| style.bg(theme::surface_hover()))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.pick_timeframe(timeframe, cx);
        }))
        .child(timeframe.label())
}
