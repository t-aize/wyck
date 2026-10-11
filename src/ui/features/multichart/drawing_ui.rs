//! The drawing tools of the multichart: the rail of tools on the left, the list of tools of a
//! family that opens beside it, and the bar of options over a selected drawing.
//!
//! What a drawing is and how the pointer makes one lives in [`crate::domain::drawings`]; this
//! file only shows the choices and turns clicks into calls on the shared drawings.

use crate::ui::kit::icon::IconName;
use crate::ui::kit::prelude::Disableable;
use gpui::prelude::*;
use gpui::{AnyElement, Context, MouseButton, SharedString, canvas, div, px};

use super::MultiChart;
use crate::domain::drawings::model::{Group, Tool};
use crate::ui::features::chart::object_tree::tool_icon;

/// The opacities the style bar steps through, most opaque first.
const OPACITY_STEPS: [f32; 4] = [1.0, 0.75, 0.5, 0.25];
/// The most tools the search lists.
const MAX_FOUND: usize = 40;
use crate::ui::kit::{controls, icon, menu as popup, theme, tokens};

/// The width of the rail of tools.
pub const RAIL_WIDTH: f32 = 46.0;

fn group_tools(group: Group) -> Vec<Tool> {
    Tool::ALL
        .into_iter()
        .filter(|tool| tool.group() == group)
        .collect()
}

/// A drawing color as the theme's color type.
fn swatch_color(color: u32) -> gpui::Rgba {
    gpui::rgb(color)
}

impl MultiChart {
    /// The tool a family shows on its button: the last one used, or its first.
    fn group_tool(&self, group: Group) -> Tool {
        self.last_tool
            .get(&group)
            .copied()
            .unwrap_or_else(|| group_tools(group)[0])
    }

    /// The vertical strip of tools.
    pub(super) fn render_rail(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let symbol = self.symbol_name(cx);
        let book = self.drawings.read(cx).book();
        let (tool, magnet, can_undo, can_redo) =
            (book.tool(), book.magnet(), book.can_undo(), book.can_redo());
        let keep = book.keep_tool();
        let favorites_bar = self.favorites_shown(cx);
        let has_any = symbol.as_ref().is_some_and(|symbol| book.count(symbol) > 0);
        // Some hidden is enough for the button to offer showing them again.
        let some_hidden = symbol
            .as_ref()
            .is_some_and(|symbol| book.drawings(symbol).iter().any(|d| d.hidden));

        let mut rail = div()
            .id("draw-rail")
            .overflow_y_scroll()
            .flex_none()
            .w(px(RAIL_WIDTH))
            .h_full()
            .py_2()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .border_r_1()
            .border_color(theme::border_hairline())
            .bg(theme::bg())
            .occlude()
            .child(
                crate::ui::kit::button::quiet("draw-pointer")
                    .icon(IconName::MousePointer2)
                    .tooltip("Pointer (Esc)")
                    .toggled(tool.is_none())
                    .on_click(cx.listener(|this, _event, _window, cx| this.pick_tool(None, cx))),
            );

        let searching = self.tool_search;
        rail = rail.child(
            crate::ui::kit::button::quiet("draw-search")
                .icon(IconName::Search)
                .tooltip(crate::ui::kit::shortcut::text("Find a tool (Ctrl+Shift+F)"))
                .toggled(searching)
                .on_click(cx.listener(|this, _event, window, cx| {
                    if this.tool_search {
                        this.tool_search = false;
                        cx.notify();
                    } else {
                        this.open_tool_search(window, cx);
                    }
                })),
        );

        for group in Group::ALL {
            let shown = self.group_tool(group);
            let active = tool.is_some_and(|t| t.group() == group);
            let slots = self.rail_slots.clone();
            let tip: SharedString = match shown.shortcut() {
                Some(keys) => format!("{}: {} ({keys})", group.label(), shown.label()).into(),
                None => format!("{}: {}", group.label(), shown.label()).into(),
            };
            // The wrapper only tells where the button is, for the list to open beside it.
            rail = rail.child(
                div()
                    .relative()
                    .flex_none()
                    .child(
                        canvas(
                            move |bounds, _window, _cx| {
                                slots.borrow_mut().insert(group, bounds);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(
                        crate::ui::kit::button::quiet(SharedString::from(format!(
                            "draw-group-{group:?}"
                        )))
                        .icon(tool_icon(shown))
                        .tooltip(tip)
                        .toggled(active || self.flyout == Some(group))
                        .on_click(cx.listener(
                            move |this, _event, _window, cx| {
                                this.tool_search = false;
                                this.flyout = if this.flyout == Some(group) {
                                    None
                                } else {
                                    Some(group)
                                };
                                cx.notify();
                            },
                        )),
                    ),
            );
        }

        rail.child(
            crate::ui::kit::layout::rule_h()
                .my_1()
                .w(px(tokens::height::compact())),
        )
        .child(
            crate::ui::kit::button::quiet("draw-favorites")
                .icon(IconName::Star)
                .tooltip(if favorites_bar {
                    "Hide the favorites bar"
                } else {
                    "Show the favorites bar"
                })
                .toggled(favorites_bar)
                .on_click(cx.listener(|this, _event, _window, cx| this.toggle_favorites_bar(cx))),
        )
        .child(
            crate::ui::kit::button::quiet("draw-tree")
                .icon(IconName::ListTree)
                .tooltip("Drawings on this symbol")
                .on_click(cx.listener(|this, _event, window, cx| {
                    let chart = this.active_chart().clone();
                    crate::ui::features::chart::open_object_tree(&chart, window, cx);
                })),
        )
        .child(
            crate::ui::kit::button::quiet("draw-hide-all")
                .icon(if some_hidden {
                    IconName::Eye
                } else {
                    IconName::EyeOff
                })
                .tooltip(if some_hidden {
                    "Show the drawings"
                } else {
                    "Hide the drawings"
                })
                .toggled(some_hidden)
                .disabled(!has_any)
                .when(!has_any, |button| button.cursor_not_allowed())
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.edit_book(cx, |book, symbol| book.set_all_hidden(symbol, !some_hidden));
                })),
        )
        .child(
            crate::ui::kit::button::quiet("draw-keep")
                .icon(IconName::PencilLine)
                .tooltip(if keep {
                    "Stay in drawing mode: on (draw several in a row, Esc to stop)"
                } else {
                    "Stay in drawing mode: off (back to the pointer after each drawing)"
                })
                .toggled(keep)
                .on_click(cx.listener(|this, _event, _window, cx| this.toggle_keep_drawing(cx))),
        )
        .child(
            crate::ui::kit::button::quiet("draw-magnet")
                .icon(IconName::Magnet)
                .tooltip("Magnet: snap to open, high, low and close")
                .toggled(magnet)
                .on_click(cx.listener(|this, _event, _window, cx| this.toggle_magnet(cx))),
        )
        .child(
            crate::ui::kit::button::quiet("draw-undo")
                .icon(IconName::Undo2)
                .tooltip(crate::ui::kit::shortcut::text("Undo (Ctrl+Z)"))
                .disabled(!can_undo)
                .when(!can_undo, |button| button.cursor_not_allowed())
                .on_click(cx.listener(|this, _event, _window, cx| this.undo_drawing(cx))),
        )
        .child(
            crate::ui::kit::button::quiet("draw-redo")
                .icon(IconName::Redo2)
                .tooltip(crate::ui::kit::shortcut::text("Redo (Ctrl+Shift+Z)"))
                .disabled(!can_redo)
                .when(!can_redo, |button| button.cursor_not_allowed())
                .on_click(cx.listener(|this, _event, _window, cx| this.redo_drawing(cx))),
        )
        .child(
            crate::ui::kit::button::quiet("draw-clear")
                .icon(IconName::Trash)
                .tooltip("Remove all drawings of this symbol")
                .disabled(!has_any)
                .when(!has_any, |button| button.cursor_not_allowed())
                .on_click(cx.listener(|this, _event, _window, cx| this.clear_drawings(cx))),
        )
    }

    /// One row of a list of tools: the icon, the name, the keys when it has some, and the star
    /// that pins it. The row is two boxes side by side, not one inside the other: the star must
    /// not also pick the tool.
    fn tool_row(
        &self,
        tool: Tool,
        current: Option<Tool>,
        favorites: &[Tool],
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let selected = current == Some(tool);
        let starred = favorites.contains(&tool);
        div()
            .flex()
            .flex_row()
            .items_center()
            .rounded_md()
            .when(selected, |el| el.bg(theme::accent_selected()))
            .hover(|style| style.bg(theme::surface_hover()))
            .child(
                div()
                    .id(SharedString::from(format!("draw-tool-{tool:?}")))
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(px(tokens::height::control()))
                    .pl_2()
                    .cursor_pointer()
                    .text_size(px(tokens::text::body()))
                    .text_color(if selected {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    })
                    .hover(|style| style.text_color(theme::fg()))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.pick_tool(Some(tool), cx);
                    }))
                    .child(icon::tinted(
                        tool_icon(tool),
                        15.,
                        if selected {
                            theme::fg()
                        } else {
                            theme::muted_fg()
                        },
                    ))
                    .child(div().flex_1().min_w_0().child(tool.label()))
                    .children(tool.shortcut().map(|keys| {
                        div()
                            .flex_none()
                            .pr_1()
                            .text_size(px(tokens::text::caption()))
                            .text_color(theme::muted_fg())
                            .child(keys)
                    })),
            )
            .child(
                div()
                    .id(SharedString::from(format!("draw-star-{tool:?}")))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(tokens::height::control()))
                    .cursor_pointer()
                    .tooltip(controls::tooltip(if starred {
                        "Unpin from the favorites"
                    } else {
                        "Pin to the favorites"
                    }))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.toggle_favorite(tool, cx);
                    }))
                    .child(icon::tinted(
                        if starred {
                            IconName::StarFill
                        } else {
                            IconName::Star
                        },
                        14.,
                        if starred {
                            theme::amber()
                        } else {
                            theme::muted_fg()
                        },
                    )),
            )
    }

    /// The tools of the open family, or the search over all of them, beside the rail, over a
    /// sheet that closes it when clicked.
    pub(super) fn render_flyout(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.flyout.is_none() && !self.tool_search {
            return None;
        }
        let current = self.drawings.read(cx).book().tool();
        let favorites = self.favorite_tools(cx);
        let root_top = self
            .root_bounds
            .get()
            .map_or(0.0, |bounds| f32::from(bounds.origin.y));
        // Beside the button of the family, where it was last drawn.
        let top = match self.flyout {
            Some(group) => self
                .rail_slots
                .borrow()
                .get(&group)
                .map_or(4.0, |bounds| f32::from(bounds.origin.y) - root_top),
            None => 4.0,
        };
        let mut list = popup::card()
            .id("draw-flyout-list")
            .absolute()
            .top(px(top.max(0.0)))
            .left(px(RAIL_WIDTH + 6.0))
            .w(px(tokens::menu::FLYOUT_WIDTH))
            .max_h(px(tokens::menu::MAX_HEIGHT))
            .overflow_y_scroll();

        if let Some(group) = self.flyout {
            list = list.child(popup::section_title(group.label()));
            for tool in group_tools(group) {
                list = list.child(self.tool_row(tool, current, &favorites, cx));
            }
        } else {
            let query = self.search_input.read(cx).text().trim().to_lowercase();
            list = list.child(div().p_1().child(self.search_input.clone()));
            if query.is_empty() {
                if !self.recent_tools.is_empty() {
                    list = list.child(popup::section_title("Recent"));
                    for tool in self.recent_tools.clone() {
                        list = list.child(self.tool_row(tool, current, &favorites, cx));
                    }
                }
                list = list.child(
                    div()
                        .px_2()
                        .py_2()
                        .text_size(px(tokens::text::small()))
                        .text_color(theme::muted_fg())
                        .child(format!("Type to search the {} tools.", Tool::ALL.len())),
                );
            } else {
                let found: Vec<Tool> = Tool::ALL
                    .into_iter()
                    .filter(|tool| {
                        tool.label().to_lowercase().contains(&query)
                            || tool.group().label().to_lowercase().contains(&query)
                    })
                    .take(MAX_FOUND)
                    .collect();
                if found.is_empty() {
                    list = list.child(
                        div()
                            .px_2()
                            .py_2()
                            .text_size(px(tokens::text::small()))
                            .text_color(theme::muted_fg())
                            .child("No tool by that name."),
                    );
                }
                for tool in found {
                    list = list.child(self.tool_row(tool, current, &favorites, cx));
                }
            }
        }
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.flyout = None;
                        this.tool_search = false;
                        cx.notify();
                    }),
                )
                .child(list)
                .into_any_element(),
        )
    }
}

mod bar_options;
mod style_bar;
