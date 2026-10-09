//! The favorite drawing tools: the tools drawn most, pinned in a pill over the charts so they
//! are one click away (or Alt and a number), whatever family they belong to.
//!
//! The pill is anchored at the bottom center of the chart area, not draggable. It fades while
//! the pointer is elsewhere and is always there, unless it is turned off. A tool is pinned or unpinned with the star beside it in the
//! list of its family, and a right click on a favorite moves it along the bar or removes it. The
//! favorites are kept with the rest of the workspace.

use gpui::prelude::*;
use gpui::{AnyElement, Context, MouseButton, SharedString, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};

use super::MultiChart;
use crate::app::workspace::MAX_FAVORITE_TOOLS;
use crate::domain::drawings::model::Tool;
use crate::ui::features::chart::object_tree::tool_icon;
use crate::ui::kit::{
    controls, icon,
    menu::{self as popup, Entry, Item},
    theme, tokens,
};

/// How tall the pill is.
const PILL_HEIGHT: f32 = 36.0;
/// How far the pill sits above the bottom of the chart area, clear of the time axis.
const PILL_BOTTOM: f32 = 44.0;
/// The opacity of the pill while the pointer is elsewhere.
const IDLE_OPACITY: f32 = 0.7;

/// How far from the bottom of the chart area the pill reaches, and so how high a hint must sit.
pub(super) const PILL_TOP: f32 = PILL_BOTTOM + PILL_HEIGHT + 8.0;

impl MultiChart {
    /// The favorite tools, in the order the bar shows them.
    pub(super) fn favorite_tools(&self, cx: &gpui::App) -> Vec<Tool> {
        self.workspace.read(cx).preferences().favorite_tool_list()
    }

    /// Pins the tool, or unpins it. A full bar says so and takes no more.
    pub(crate) fn toggle_favorite(&mut self, tool: Tool, cx: &mut Context<Self>) {
        let list = self.favorite_tools(cx);
        if !list.contains(&tool) && list.len() >= MAX_FAVORITE_TOOLS {
            crate::ui::kit::toast::show(
                cx,
                crate::ui::kit::toast::Kind::Info,
                "The favorites are full",
                format!("The bar holds {MAX_FAVORITE_TOOLS} tools. Unpin one to make room."),
            );
            return;
        }
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.toggle_favorite_tool(tool));
        });
        cx.notify();
    }

    /// Moves the favorite at `index` along the bar.
    pub(crate) fn move_favorite(&mut self, index: usize, delta: isize, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.move_favorite_tool(index, delta));
        });
        cx.notify();
    }

    /// Picks the favorite at `index` (Alt and the number of it), or goes back to the pointer when
    /// it is the tool in hand already.
    pub(crate) fn pick_favorite(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(tool) = self.favorite_tools(cx).get(index).copied() else {
            return;
        };
        let current = self.drawings.read(cx).book().tool();
        self.pick_tool(
            if current == Some(tool) {
                None
            } else {
                Some(tool)
            },
            cx,
        );
    }

    /// Whether the bar shows.
    pub(super) fn favorites_shown(&self, cx: &gpui::App) -> bool {
        self.workspace.read(cx).preferences().favorites_bar
    }

    pub(crate) fn toggle_favorites_bar(&mut self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.favorites_bar = !prefs.favorites_bar);
        });
        cx.notify();
    }

    pub(crate) fn toggle_favorite_names(&mut self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| {
                prefs.favorites_labels = !prefs.favorites_labels;
            });
        });
        cx.notify();
    }

    /// The bar over the charts, or nothing when it is turned off.
    pub(super) fn render_favorites(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.favorites_shown(cx) {
            return None;
        }
        let favorites = self.favorite_tools(cx);
        let names = self.workspace.read(cx).preferences().favorites_labels;
        let current = self.drawings.read(cx).book().tool();
        let count = favorites.len();
        let this = cx.entity();

        let mut bar = div()
            .id("favorites-bar")
            .flex_none()
            .h(px(PILL_HEIGHT))
            .max_w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_0p5()
            .px_1p5()
            .overflow_hidden()
            .rounded_full()
            .border_1()
            .border_color(theme::border_subtle())
            .bg(theme::surface_alpha(0.92))
            .shadow_lg()
            .occlude()
            .opacity(IDLE_OPACITY)
            .hover(|s| s.opacity(1.0))
            .child(div().flex_none().px_1p5().child(icon::tinted(
                IconName::StarFill,
                13.,
                theme::amber(),
            )));

        if favorites.is_empty() {
            bar = bar.child(
                div()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::muted_fg())
                    .child("Star a tool in the list of its family to pin it here."),
            );
        }
        for (index, tool) in favorites.iter().copied().enumerate() {
            let selected = current == Some(tool);
            let hint: SharedString = if index < 9 {
                format!("{} (Alt+{})", tool.label(), index + 1).into()
            } else {
                tool.label().into()
            };
            let entity = this.clone();
            let menu = popup::Menu::new(("favorite-menu", index), window, cx);
            let opener = menu.clone();
            let items: Vec<Item> = if menu.is_open(cx) {
                let (left, right, unpin) = (entity.clone(), entity.clone(), entity.clone());
                vec![
                    Item::Title(tool.label().into()),
                    Entry::new("Move left")
                        .icon(IconName::ArrowLeft)
                        .disabled(index == 0)
                        .on_click(move |_, cx| {
                            left.update(cx, |m, cx| m.move_favorite(index, -1, cx));
                        })
                        .into(),
                    Entry::new("Move right")
                        .icon(IconName::ArrowRight)
                        .disabled(index + 1 >= count)
                        .on_click(move |_, cx| {
                            right.update(cx, |m, cx| m.move_favorite(index, 1, cx));
                        })
                        .into(),
                    Item::Separator,
                    Entry::new("Unpin")
                        .icon(IconName::StarOff)
                        .on_click(move |_, cx| {
                            unpin.update(cx, |m, cx| m.toggle_favorite(tool, cx));
                        })
                        .into(),
                ]
            } else {
                Vec::new()
            };
            bar = bar.child(
                div()
                    .id(("favorite", index))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .h(px(tokens::height::control()))
                    .px_2()
                    .rounded_full()
                    .cursor_pointer()
                    .text_size(px(tokens::text::body()))
                    .text_color(if selected {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    })
                    .when(selected, |el| el.bg(theme::accent_selected()))
                    .when(!selected, |el| {
                        el.hover(|s| s.bg(theme::surface_hover()).text_color(theme::fg()))
                    })
                    .tooltip(controls::tooltip(hint.clone()))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.pick_favorite(index, cx);
                    }))
                    .child(icon::tinted(
                        tool_icon(tool),
                        16.,
                        if selected {
                            theme::fg()
                        } else {
                            theme::muted_fg()
                        },
                    ))
                    .when(names, |el| el.child(tool.label()))
                    .on_mouse_down(MouseButton::Right, move |event, _, cx| {
                        opener.open(Some(event.position), cx);
                    })
                    .children(menu.popup(items, popup::Placement::Cursor, window, cx)),
            );
        }

        // The pill floats at the bottom of the chart area, over the time axis, centered. The
        // wrapper takes no mouse events, so the chart stays live around the pill.
        let pill = bar
            .child(
                div()
                    .flex_none()
                    .w(px(1.))
                    .h(px(16.))
                    .mx_1()
                    .bg(theme::border_subtle()),
            )
            .child(
                Button::new("favorites-names")
                    .ghost()
                    .compact()
                    .icon(IconName::Type)
                    .tooltip(if names {
                        "Show icons only"
                    } else {
                        "Show the names of the tools"
                    })
                    .toggled(names)
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.toggle_favorite_names(cx);
                    })),
            )
            .child(
                Button::new("favorites-hide")
                    .ghost()
                    .compact()
                    .icon(IconName::X)
                    .tooltip("Hide the favorites")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.toggle_favorites_bar(cx);
                    })),
            );

        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(PILL_BOTTOM))
                .px_2()
                .flex()
                .flex_row()
                .justify_center()
                .child(pill)
                .into_any_element(),
        )
    }
}
