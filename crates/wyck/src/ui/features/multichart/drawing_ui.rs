//! The drawing tools of the multichart: the rail of tools on the left, the list of tools of a
//! family that opens beside it, and the bar of options over a selected drawing.
//!
//! What a drawing is and how the pointer makes one lives in [`wyck_drawings`]; this
//! file only shows the choices and turns clicks into calls on the shared drawings.

pub mod bar_options;
pub(super) mod bar_place;
pub mod style_bar;

use super::MultiChart;
use crate::ui::features::chart::object_tree::tool_icon;
use gpui::prelude::*;
use gpui::{AnyElement, Context, MouseButton, SharedString, canvas, div, px};
use wyck_drawings::model::tool::{Group, Tool};
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::prelude::Disableable;
use wyck_ui_kit::{controls, icon, menu as popup, theme, tokens};

/// The opacities the style bar steps through, most opaque first.
const OPACITY_STEPS: [f32; 4] = [1.0, 0.75, 0.5, 0.25];
/// The most tools the search lists.
const MAX_FOUND: usize = 40;

/// The width of the rail of tools: a rail button and room on each side.
pub fn rail_width() -> f32 {
    tokens::height::rail() + 2.0 * tokens::space::sm()
}

/// A short line between two blocks of the rail.
fn rail_rule() -> gpui::Div {
    wyck_ui_kit::layout::rule_h()
        .my_1()
        .w(px(tokens::height::compact()))
}

fn group_tools(group: Group) -> Vec<Tool> {
    Tool::ALL
        .into_iter()
        .filter(|tool| tool.group() == group)
        .collect()
}

/// The keys that pick a drawing tool, or `None` for a tool with none.
fn tool_hint(tool: Tool) -> Option<String> {
    let id = match tool {
        Tool::TrendLine => "tool_trend_line",
        Tool::HorizontalLine => "tool_horizontal",
        Tool::VerticalLine => "tool_vertical",
        Tool::FibRetracement => "tool_fib",
        Tool::Rectangle => "tool_rectangle",
        Tool::Text => "tool_text",
        Tool::Measure => "tool_measure",
        Tool::Brush => "tool_brush",
        _ => return None,
    };
    wyck_ui_kit::hints::hint(id)
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

    /// The vertical strip of tools: the families of drawing tools on top, and under them, apart,
    /// the modes (stay in drawing mode, magnet), what is done to the drawings of the symbol and
    /// the history. The buttons are the large size of the kit, so they are easy to hit.
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
        let all_locked = symbol.as_ref().is_some_and(|symbol| {
            let list = book.drawings(symbol);
            !list.is_empty() && list.iter().all(|d| d.locked)
        });

        let mut tools = div()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .child(
                wyck_ui_kit::button::rail("draw-pointer")
                    .icon(IconName::MousePointer2)
                    .tooltip("Pointer (Esc)")
                    .toggled(tool.is_none())
                    .on_click(cx.listener(|this, _event, _window, cx| this.pick_tool(None, cx))),
            );

        let searching = self.tool_search;
        tools = tools.child(
            wyck_ui_kit::button::rail("draw-search")
                .icon(IconName::Search)
                .tooltip(gpui::SharedString::from(format!(
                    "Find a tool{}",
                    wyck_ui_kit::hints::suffix("find_tool")
                )))
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
            let tip: SharedString = match tool_hint(shown) {
                Some(keys) => format!(
                    "{} ({keys}). The arrow lists the {}",
                    shown.label(),
                    group.label().to_lowercase()
                )
                .into(),
                None => format!(
                    "{}. The arrow lists the {}",
                    shown.label(),
                    group.label().to_lowercase()
                )
                .into(),
            };
            // The wrapper tells where the button is, for the list to open beside it, and holds
            // the small arrow that says there is a list.
            tools = tools.child(
                div()
                    .relative()
                    .flex_none()
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _event, _window, cx| this.toggle_flyout(group, cx)),
                    )
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
                        wyck_ui_kit::button::rail(SharedString::from(format!(
                            "draw-group-{group:?}"
                        )))
                        .icon(tool_icon(shown))
                        .tooltip(tip)
                        .toggled(active || self.flyout == Some(group))
                        // A click draws with the tool the family shows (a second click goes back
                        // to the pointer); the arrow, or a right click, opens the list.
                        .on_click(cx.listener(
                            move |this, _event, _window, cx| {
                                let current = this.drawings.read(cx).book().tool();
                                this.pick_tool(
                                    if current == Some(shown) {
                                        None
                                    } else {
                                        Some(shown)
                                    },
                                    cx,
                                );
                            },
                        )),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("draw-arrow-{group:?}")))
                            .absolute()
                            .right_0()
                            .bottom_0()
                            .size(px(tokens::swatch::medium()))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::surface_hover()))
                            .tooltip(controls::tooltip(format!(
                                "{}: the list of tools",
                                group.label()
                            )))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.toggle_flyout(group, cx);
                            }))
                            .child(icon::tinted(IconName::ChevronRight, 9., theme::muted_fg())),
                    ),
            );
        }

        let utilities = div()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .child(
                wyck_ui_kit::button::rail("draw-keep")
                    .icon(IconName::PencilLine)
                    .tooltip(if keep {
                        "Stay in drawing mode: on (draw several in a row, Esc to stop)"
                    } else {
                        "Stay in drawing mode: off (back to the pointer after each drawing)"
                    })
                    .toggled(keep)
                    .on_click(
                        cx.listener(|this, _event, _window, cx| this.toggle_keep_drawing(cx)),
                    ),
            )
            .child(
                wyck_ui_kit::button::rail("draw-magnet")
                    .icon(IconName::Magnet)
                    .tooltip("Magnet: snap to open, high, low and close")
                    .toggled(magnet)
                    .on_click(cx.listener(|this, _event, _window, cx| this.toggle_magnet(cx))),
            )
            .child(rail_rule())
            .child(
                wyck_ui_kit::button::rail("draw-favorites")
                    .icon(IconName::Star)
                    .tooltip(if favorites_bar {
                        "Hide the favorites bar"
                    } else {
                        "Show the favorites bar"
                    })
                    .toggled(favorites_bar)
                    .on_click(
                        cx.listener(|this, _event, _window, cx| this.toggle_favorites_bar(cx)),
                    ),
            )
            .child(
                wyck_ui_kit::button::rail("draw-tree")
                    .icon(IconName::ListTree)
                    .tooltip("Drawings on this symbol")
                    .on_click(cx.listener(|this, _event, window, cx| {
                        let chart = this.active_chart().clone();
                        crate::ui::features::chart::open_object_tree(&chart, window, cx);
                    })),
            )
            .child(
                wyck_ui_kit::button::rail("draw-lock-all")
                    .icon(if all_locked {
                        IconName::Lock
                    } else {
                        IconName::LockOpen
                    })
                    .tooltip(if all_locked {
                        "Unlock the drawings"
                    } else {
                        "Lock the drawings"
                    })
                    .toggled(all_locked)
                    .disabled(!has_any)
                    .when(!has_any, |button| button.cursor_not_allowed())
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.edit_book(cx, |book, symbol| book.set_all_locked(symbol, !all_locked));
                    })),
            )
            .child(
                wyck_ui_kit::button::rail("draw-hide-all")
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
                        this.edit_book(cx, |book, symbol| {
                            book.set_all_hidden(symbol, !some_hidden)
                        });
                    })),
            )
            .child(rail_rule())
            .child(
                wyck_ui_kit::button::rail("draw-undo")
                    .icon(IconName::Undo2)
                    .tooltip(gpui::SharedString::from(format!(
                        "Undo{}",
                        wyck_ui_kit::hints::suffix("undo_drawing")
                    )))
                    .disabled(!can_undo)
                    .when(!can_undo, |button| button.cursor_not_allowed())
                    .on_click(cx.listener(|this, _event, _window, cx| this.undo_drawing(cx))),
            )
            .child(
                wyck_ui_kit::button::rail("draw-redo")
                    .icon(IconName::Redo2)
                    .tooltip(gpui::SharedString::from(format!(
                        "Redo{}",
                        wyck_ui_kit::hints::suffix("redo_drawing")
                    )))
                    .disabled(!can_redo)
                    .when(!can_redo, |button| button.cursor_not_allowed())
                    .on_click(cx.listener(|this, _event, _window, cx| this.redo_drawing(cx))),
            )
            .child(
                wyck_ui_kit::button::rail("draw-clear")
                    .icon(IconName::Trash)
                    .tooltip("Remove all drawings of this symbol")
                    .disabled(!has_any)
                    .when(!has_any, |button| button.cursor_not_allowed())
                    .on_click(cx.listener(|this, _event, _window, cx| this.clear_drawings(cx))),
            );

        // On a low window the whole strip scrolls; on a tall one the utilities sit at the bottom.
        div()
            .id("draw-rail")
            .overflow_y_scroll()
            .flex_none()
            .w(px(rail_width()))
            .h_full()
            .py_2()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .border_r_1()
            .border_color(theme::border_hairline())
            .bg(theme::bg())
            .occlude()
            .child(tools)
            .child(div().flex_1().min_h_0())
            .child(utilities)
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
                    .h(px(tokens::height::large()))
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
                    .children(tool_hint(tool).map(|keys| {
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
                    .size(px(tokens::height::large()))
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
        let mut list = popup::parts::card()
            .id("draw-flyout-list")
            .absolute()
            .top(px(top.max(0.0)))
            .left(px(rail_width() + tokens::space::xs()))
            .w(px(tokens::menu::FLYOUT_WIDTH))
            .max_h(px(tokens::menu::MAX_HEIGHT))
            .overflow_y_scroll();

        if let Some(group) = self.flyout {
            list = list.child(popup::parts::section_title(group.label()));
            // A long family is cut in sections: a ray, a pitchfork or a level is found by what it
            // is.
            let sections = group.sections();
            let several = sections.len() > 1;
            for (name, tools) in sections {
                if several {
                    list = list.child(
                        div()
                            .px_2()
                            .pt_2()
                            .pb_1()
                            .text_size(px(tokens::text::caption()))
                            .text_color(theme::muted_fg())
                            .child(name.to_uppercase()),
                    );
                }
                for tool in tools {
                    list = list.child(self.tool_row(tool, current, &favorites, cx));
                }
            }
        } else {
            let query = self.search_input.read(cx).text().trim().to_lowercase();
            list = list.child(div().p_1().child(self.search_input.clone()));
            if query.is_empty() {
                if !self.recent_tools.is_empty() {
                    list = list.child(popup::parts::section_title("Recent"));
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
