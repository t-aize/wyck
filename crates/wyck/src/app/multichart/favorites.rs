//! The bar of favorite drawing tools: the tools drawn most, pinned in a strip over the charts so
//! they are one click away (or Alt and a number), whatever family they belong to.
//!
//! The bar is docked and always there, unless it is turned off, so it is never in the way of a
//! drag and never has to be found. A tool is pinned or unpinned with the star beside it in the
//! list of its family, and a right click on a favorite moves it along the bar or removes it. The
//! favorites are kept with the rest of the workspace.

use gpui::prelude::*;
use gpui::{AnyElement, Context, MouseButton, SharedString, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tooltip::Tooltip;

use super::MultiChart;
use crate::app::chart::drawing::model::Tool;
use crate::app::chart::object_tree::tool_icon;
use crate::app::connection::ui;
use crate::app::menu::{self as popup, Entry, Item};
use crate::app::theme;
use crate::app::workspace::MAX_FAVORITE_TOOLS;

/// How tall the bar is.
const BAR_HEIGHT: f32 = 36.0;

impl MultiChart {
    /// The favorite tools, in the order the bar shows them.
    pub(super) fn favorite_tools(&self, cx: &gpui::App) -> Vec<Tool> {
        self.workspace.read(cx).preferences().favorite_tool_list()
    }

    /// Pins the tool, or unpins it. A full bar says so and takes no more.
    pub(crate) fn toggle_favorite(&mut self, tool: Tool, cx: &mut Context<Self>) {
        let list = self.favorite_tools(cx);
        if !list.contains(&tool) && list.len() >= MAX_FAVORITE_TOOLS {
            crate::app::toast::show(
                cx,
                crate::app::toast::Kind::Info,
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
            .w_full()
            .h(px(BAR_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .px_2()
            .overflow_hidden()
            .border_b_1()
            .border_color(theme::border_hairline())
            .bg(theme::bg())
            .child(div().flex_none().px_1().child(ui::icon_colored(
                IconName::StarFill,
                14.,
                theme::amber(),
            )));

        if favorites.is_empty() {
            bar = bar.child(
                div()
                    .text_size(px(12.))
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
                    .h(px(28.))
                    .px_2()
                    .rounded_md()
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(if selected {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    })
                    .when(selected, |el| el.bg(theme::accent_selected()))
                    .when(!selected, |el| {
                        el.hover(|s| s.bg(theme::surface_hover()).text_color(theme::fg()))
                    })
                    .tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.pick_favorite(index, cx);
                    }))
                    .child(ui::icon_colored(
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

        Some(
            bar.child(div().flex_1())
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
                        .icon(IconName::PanelTopClose)
                        .tooltip("Hide the favorites bar")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.toggle_favorites_bar(cx);
                        })),
                )
                .into_any_element(),
        )
    }
}
