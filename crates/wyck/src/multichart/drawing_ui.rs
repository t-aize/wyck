//! The drawing tools of the multichart: the rail of tools on the left, the list of tools of a
//! family that opens beside it, and the bar of options over a selected drawing.
//!
//! What a drawing is and how the pointer makes one lives in [`crate::chart::drawing`]; this
//! file only shows the choices and turns clicks into calls on the shared drawings.

use gpui::prelude::*;
use gpui::{AnyElement, Context, MouseButton, SharedString, Window, canvas, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::Disableable;
use gpui_kit::component::button::{Button, ButtonVariants};

use super::MultiChart;
use crate::chart::DrawingCommand;
use crate::chart::object_tree::tool_icon;
use wyck_chart::drawing::look::Cap;
use wyck_chart::drawing::model::{Dash, Group, PALETTE, Tool};

/// The opacities the style bar steps through, most opaque first.
const OPACITY_STEPS: [f32; 4] = [1.0, 0.75, 0.5, 0.25];
/// The most tools the search lists.
const MAX_FOUND: usize = 40;
use wyck_ui::{controls, icon, layout, menu as popup, theme, tokens};

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
                Button::new("draw-pointer")
                    .ghost()
                    .compact()
                    .icon(IconName::MousePointer2)
                    .tooltip("Pointer (Esc)")
                    .toggled(tool.is_none())
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _event, _window, cx| this.pick_tool(None, cx))),
            );

        let searching = self.tool_search;
        rail = rail.child(
            Button::new("draw-search")
                .ghost()
                .compact()
                .icon(IconName::Search)
                .tooltip("Find a tool (Ctrl+Shift+K)")
                .toggled(searching)
                .cursor_pointer()
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
                        Button::new(SharedString::from(format!("draw-group-{group:?}")))
                            .ghost()
                            .compact()
                            .icon(tool_icon(shown))
                            .tooltip(tip)
                            .toggled(active || self.flyout == Some(group))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.tool_search = false;
                                this.flyout = if this.flyout == Some(group) {
                                    None
                                } else {
                                    Some(group)
                                };
                                cx.notify();
                            })),
                    ),
            );
        }

        rail.child(
            div()
                .my_1()
                .w(px(24.))
                .h(px(1.))
                .bg(theme::border_hairline()),
        )
        .child(
            Button::new("draw-favorites")
                .ghost()
                .compact()
                .icon(IconName::Star)
                .tooltip(if favorites_bar {
                    "Hide the favorites bar"
                } else {
                    "Show the favorites bar"
                })
                .toggled(favorites_bar)
                .cursor_pointer()
                .on_click(cx.listener(|this, _event, _window, cx| this.toggle_favorites_bar(cx))),
        )
        .child(
            Button::new("draw-tree")
                .ghost()
                .compact()
                .icon(IconName::ListTree)
                .tooltip("Drawings on this symbol")
                .cursor_pointer()
                .on_click(cx.listener(|this, _event, window, cx| {
                    let chart = this.active_chart().clone();
                    crate::chart::open_object_tree(&chart, window, cx);
                })),
        )
        .child(
            Button::new("draw-hide-all")
                .ghost()
                .compact()
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
                .cursor_pointer()
                .when(!has_any, |button| button.cursor_not_allowed())
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.edit_book(cx, |book, symbol| book.set_all_hidden(symbol, !some_hidden));
                })),
        )
        .child(
            Button::new("draw-keep")
                .ghost()
                .compact()
                .icon(IconName::PencilLine)
                .tooltip(if keep {
                    "Stay in drawing mode: on (draw several in a row, Esc to stop)"
                } else {
                    "Stay in drawing mode: off (back to the pointer after each drawing)"
                })
                .toggled(keep)
                .cursor_pointer()
                .on_click(cx.listener(|this, _event, _window, cx| this.toggle_keep_drawing(cx))),
        )
        .child(
            Button::new("draw-magnet")
                .ghost()
                .compact()
                .icon(IconName::Magnet)
                .tooltip("Magnet: snap to open, high, low and close")
                .toggled(magnet)
                .cursor_pointer()
                .on_click(cx.listener(|this, _event, _window, cx| this.toggle_magnet(cx))),
        )
        .child(
            Button::new("draw-undo")
                .ghost()
                .compact()
                .icon(IconName::Undo2)
                .tooltip("Undo (Ctrl+Z)")
                .disabled(!can_undo)
                .cursor_pointer()
                .when(!can_undo, |button| button.cursor_not_allowed())
                .on_click(cx.listener(|this, _event, _window, cx| this.undo_drawing(cx))),
        )
        .child(
            Button::new("draw-redo")
                .ghost()
                .compact()
                .icon(IconName::Redo2)
                .tooltip("Redo (Ctrl+Shift+Z)")
                .disabled(!can_redo)
                .cursor_pointer()
                .when(!can_redo, |button| button.cursor_not_allowed())
                .on_click(cx.listener(|this, _event, _window, cx| this.redo_drawing(cx))),
        )
        .child(
            Button::new("draw-clear")
                .ghost()
                .compact()
                .icon(IconName::Trash)
                .tooltip("Remove all drawings of this symbol")
                .disabled(!has_any)
                .cursor_pointer()
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
                    .h(px(tokens::height::CONTROL))
                    .pl_2()
                    .cursor_pointer()
                    .text_size(px(tokens::text::BODY))
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
                            .text_size(px(tokens::text::CAPTION))
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
                    .size(px(tokens::height::CONTROL))
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
            .w(px(tokens::menu::CONTEXT_WIDTH + 40.0))
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
                        .py_1p5()
                        .text_size(px(tokens::text::SMALL))
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
                            .py_1p5()
                            .text_size(px(tokens::text::SMALL))
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

    /// The bar over a selected drawing, or nothing. Only the cheap test lives here: the bar itself
    /// is built by [`Self::build_style_bar`], whose frame is large, so a view with nothing
    /// selected (the usual case) does not pay for it on the stack.
    pub(super) fn render_style_bar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let symbol = self.symbol_name(cx)?;
        let book = self.drawings.read(cx).book();
        if book.tool().is_some()
            || book
                .selected()
                .and_then(|id| book.get(&symbol, id))
                .is_none()
        {
            return None;
        }
        self.build_style_bar(window, cx)
    }

    /// The bar over a selected drawing: color, width, line style, fill, words, lock, copy, delete.
    #[inline(never)]
    fn build_style_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let symbol = self.symbol_name(cx)?;
        let book = self.drawings.read(cx).book();
        if book.tool().is_some() {
            return None;
        }
        let drawing = book.selected().and_then(|id| book.get(&symbol, id))?;
        let tool = drawing.tool;
        let count = book.selected_count();
        let st = BarState {
            style: drawing.style.clone(),
            tool,
            locked: drawing.locked,
            hidden: drawing.hidden,
            id: drawing.id,
            has_fill: tool.has_fill(),
            has_dash: tool.has_dash(),
            has_default: book.has_template(tool),
            template_names: book
                .named_templates(tool)
                .iter()
                .map(|t| t.name.clone())
                .collect(),
        };
        let bar_label = if count > 1 {
            format!("{} and {} more", tool.label(), count - 1)
        } else {
            tool.label().to_owned()
        };

        let mut bar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .p_1()
            .rounded_xl()
            .bg(theme::surface_alpha(0.95))
            .border_1()
            .border_color(theme::border_subtle())
            .shadow_lg()
            .occlude()
            .child(
                div()
                    .px_2()
                    .text_size(px(tokens::text::SMALL))
                    .text_color(theme::muted_fg())
                    .child(bar_label),
            )
            .child(layout::divider());

        if !st.locked {
            if tool.has_quick_color() {
                bar = self.bar_colors(bar, &st, cx);
            }
            bar = self.bar_line_options(bar, &st, cx);
            bar = self.bar_extras(bar, &st, cx);
        }
        if tool.is_position() {
            bar = self.bar_position(bar, &st, cx);
        }
        if !st.locked {
            bar = self.bar_look(bar, &st, window, cx);
        }
        bar = self.bar_actions(bar, &st, cx);

        let top = self.active_chart().read(cx).legend_bottom();
        Some(
            div()
                .absolute()
                .top(px(top))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(bar)
                .into_any_element(),
        )
    }

    #[allow(unused_variables)]
    #[inline(never)]
    fn bar_colors(&self, mut bar: gpui::Div, st: &BarState, cx: &mut Context<Self>) -> gpui::Div {
        let (style, tool, locked, hidden, id) = (&st.style, st.tool, st.locked, st.hidden, st.id);
        let (has_fill, has_dash) = (st.has_fill, st.has_dash);
        let (has_default, template_names) = (st.has_default, st.template_names.clone());
        if tool.has_quick_color() {
            for color in PALETTE {
                let selected = style.color == color;
                bar = bar.child(
                    div()
                        .id(("draw-color", u64::from(color)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(tokens::height::COMPACT))
                        .rounded_full()
                        .cursor_pointer()
                        .when(selected, |el| el.border_1().border_color(theme::fg()))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.set_drawing_color(color, cx);
                        }))
                        .child(div().size(px(14.)).rounded_full().bg(swatch_color(color))),
                );
            }
            // The colors set last that the palette does not hold, one click to use again.
            for color in self
                .recent_colors
                .iter()
                .copied()
                .filter(|color| !PALETTE.contains(color))
            {
                let selected = style.color == color;
                bar = bar.child(
                    div()
                        .id(("draw-recent-color", u64::from(color)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(tokens::height::COMPACT))
                        .rounded_full()
                        .cursor_pointer()
                        .tooltip(controls::tooltip(format!("#{color:06x}")))
                        .when(selected, |el| el.border_1().border_color(theme::fg()))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.set_drawing_color(color, cx);
                        }))
                        .child(div().size(px(14.)).rounded_full().bg(swatch_color(color))),
                );
            }
            // Any other color: the panel with the square, the hue bar and the typed values.
            let (open_this, pick_this) = (cx.entity(), cx.entity());
            bar = bar.child(controls::color_swatch(
                "draw-color-custom",
                style.color,
                self.color_open == Some(id),
                cx,
                move |_window, cx| {
                    open_this.update(cx, |this, cx| {
                        this.color_open = if this.color_open == Some(id) {
                            None
                        } else {
                            Some(id)
                        };
                        cx.notify();
                    });
                },
                move |color, _window, cx| {
                    pick_this.update(cx, |this, cx| this.set_drawing_color(color, cx));
                },
            ));
            bar = bar.child(layout::divider());
        }
        bar
    }

    #[allow(unused_variables)]
    #[inline(never)]
    fn bar_line_options(
        &self,
        mut bar: gpui::Div,
        st: &BarState,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let (style, tool, locked, hidden, id) = (&st.style, st.tool, st.locked, st.hidden, st.id);
        let (has_fill, has_dash) = (st.has_fill, st.has_dash);
        let (has_default, template_names) = (st.has_default, st.template_names.clone());
        if tool.has_width() {
            let widths = tool.widths();
            let selected = widths.iter().position(|w| (style.width - w).abs() < 0.01);
            let (this, picked) = (cx.entity(), widths);
            bar = bar
                .child(controls::width_picker(
                    "draw-width",
                    &widths,
                    selected,
                    move |index, _window, cx| {
                        let width = picked[index];
                        this.update(cx, |this, cx| this.set_drawing_width(width, cx));
                    },
                ))
                .child(layout::divider());
        }
        if has_dash {
            const DASHES: [Dash; 3] = [Dash::Solid, Dash::Dashed, Dash::Dotted];
            let selected = DASHES.iter().position(|d| *d == style.dash).unwrap_or(0);
            let this = cx.entity();
            bar = bar
                .child(controls::dash_picker(
                    "draw-dash",
                    selected,
                    move |index, _window, cx| {
                        this.update(cx, |this, cx| this.set_drawing_dash(DASHES[index], cx));
                    },
                ))
                .child(layout::divider());
        }
        if has_fill {
            bar = bar
                .child(
                    Button::new("draw-fill")
                        .ghost()
                        .compact()
                        .icon(IconName::PaintBucket)
                        .tooltip("Fill")
                        .toggled(style.fill)
                        .cursor_pointer()
                        .on_click(
                            cx.listener(|this, _event, _window, cx| this.toggle_drawing_fill(cx)),
                        ),
                )
                .child(layout::divider());
        }
        if tool.has_width() || has_fill {
            // Steps through 100, 75, 50 and 25 percent, and back.
            let opacity = style.opacity;
            let next = OPACITY_STEPS
                .iter()
                .copied()
                .find(|step| *step < opacity - 0.01)
                .unwrap_or(OPACITY_STEPS[0]);
            bar = bar.child(
                Button::new("draw-opacity")
                    .ghost()
                    .compact()
                    .icon(IconName::Droplet)
                    .label(format!("{}%", (opacity * 100.0).round() as u32))
                    .tooltip("Opacity of the lines (click to step down)")
                    .toggled(opacity < 0.99)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.edit_book(cx, |book, symbol| {
                            book.edit_style(symbol, |s| s.opacity = next)
                        });
                    })),
            );
        }
        bar
    }

    #[allow(unused_variables)]
    #[inline(never)]
    fn bar_extras(&self, mut bar: gpui::Div, st: &BarState, cx: &mut Context<Self>) -> gpui::Div {
        let (style, tool, locked, hidden, id) = (&st.style, st.tool, st.locked, st.hidden, st.id);
        let (has_fill, has_dash) = (st.has_fill, st.has_dash);
        let (has_default, template_names) = (st.has_default, st.template_names.clone());
        if tool.has_extend() {
            let (left, right) = (style.extend_left, style.extend_right);
            bar = bar
                .child(
                    Button::new("draw-extend-left")
                        .ghost()
                        .compact()
                        .icon(IconName::ArrowLeftToLine)
                        .tooltip("Extend to the left")
                        .toggled(left)
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.edit_book(cx, |book, symbol| {
                                book.edit_style(symbol, |s| s.extend_left = !s.extend_left)
                            });
                        })),
                )
                .child(
                    Button::new("draw-extend-right")
                        .ghost()
                        .compact()
                        .icon(IconName::ArrowRightToLine)
                        .tooltip("Extend to the right")
                        .toggled(right)
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.edit_book(cx, |book, symbol| {
                                book.edit_style(symbol, |s| s.extend_right = !s.extend_right)
                            });
                        })),
                );
        }
        if tool.has_end_cap() {
            let arrow = style.caps.end == Cap::Arrow;
            bar = bar.child(
                Button::new("draw-end-arrow")
                    .ghost()
                    .compact()
                    .icon(IconName::MoveRight)
                    .tooltip("Arrow at the end")
                    .toggled(arrow)
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.edit_book(cx, |book, symbol| {
                            book.edit_style(symbol, |s| {
                                s.caps.end = if s.caps.end == Cap::Arrow {
                                    Cap::None
                                } else {
                                    Cap::Arrow
                                };
                            })
                        });
                    })),
            );
        }
        if tool.has_width() || has_fill || tool.has_extend() || tool.has_end_cap() {
            bar = bar.child(layout::divider());
        }
        if tool.has_text() || tool.takes_label() {
            let size = style.text_size;
            let bold = style.bold;
            bar = bar
                .child(
                    Button::new("draw-text-smaller")
                        .ghost()
                        .compact()
                        .icon(IconName::Minus)
                        .tooltip("Smaller text")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.edit_book(cx, |book, symbol| {
                                book.edit_style(symbol, |s| {
                                    s.text_size = (s.text_size - 2.0).clamp(8.0, 48.0);
                                })
                            });
                        })),
                )
                .child(
                    div()
                        .min_w(px(22.))
                        .flex()
                        .justify_center()
                        .text_size(px(tokens::text::SMALL))
                        .text_color(theme::muted_fg())
                        .child(format!("{}", size.round() as u32)),
                )
                .child(
                    Button::new("draw-text-bigger")
                        .ghost()
                        .compact()
                        .icon(IconName::Plus)
                        .tooltip("Bigger text")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.edit_book(cx, |book, symbol| {
                                book.edit_style(symbol, |s| {
                                    s.text_size = (s.text_size + 2.0).clamp(8.0, 48.0);
                                })
                            });
                        })),
                )
                .child(
                    Button::new("draw-text-bold")
                        .ghost()
                        .compact()
                        .icon(IconName::Bold)
                        .tooltip("Bold")
                        .toggled(bold)
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.edit_book(cx, |book, symbol| {
                                book.edit_style(symbol, |s| s.bold = !s.bold)
                            });
                        })),
                )
                .child(layout::divider());
        }
        if tool.has_text() {
            bar = bar
                .child(div().w(px(220.)).child(self.text_input.clone()))
                .child(layout::divider());
        }
        bar
    }

    #[allow(unused_variables)]
    #[inline(never)]
    fn bar_position(&self, mut bar: gpui::Div, st: &BarState, cx: &mut Context<Self>) -> gpui::Div {
        let (style, tool, locked, hidden, id) = (&st.style, st.tool, st.locked, st.hidden, st.id);
        let (has_fill, has_dash) = (st.has_fill, st.has_dash);
        let (has_default, template_names) = (st.has_default, st.template_names.clone());
        if tool.is_position() {
            bar = bar
                .child(
                    Button::new("draw-trade")
                        .primary()
                        .compact()
                        .icon(IconName::ArrowLeftRight)
                        .label(if tool == Tool::LongPosition {
                            "Buy"
                        } else {
                            "Sell"
                        })
                        .tooltip("Open the order ticket with this entry, stop and target")
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.trade_drawing(id, cx);
                        })),
                )
                .child(
                    Button::new("draw-flip")
                        .ghost()
                        .compact()
                        .icon(IconName::ArrowUpDown)
                        .tooltip(if tool == Tool::LongPosition {
                            "Flip to a short position"
                        } else {
                            "Flip to a long position"
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.drawing_command(id, DrawingCommand::Flip, cx);
                        })),
                )
                .child(layout::divider());
        }
        bar
    }

    #[allow(unused_variables)]
    #[inline(never)]
    fn bar_look(
        &self,
        mut bar: gpui::Div,
        st: &BarState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let (style, tool, locked, hidden, id) = (&st.style, st.tool, st.locked, st.hidden, st.id);
        let (has_fill, has_dash) = (st.has_fill, st.has_dash);
        let (has_default, template_names) = (st.has_default, st.template_names.clone());
        if !locked {
            let menu = popup::Menu::new(("draw-style-menu", 0usize), window, cx);
            let items: Vec<popup::Item> = if menu.is_open(cx) {
                let this = cx.entity();
                let mut items = vec![popup::Item::Title("Look".into())];
                let entry =
                    |label: &str, icon: IconName| popup::Entry::new(label.to_owned()).icon(icon);
                {
                    let (this, menu) = (this.clone(), menu.clone());
                    items.push(
                        entry("Use as the default for this tool", IconName::Save)
                            .on_click(move |_window, cx| {
                                menu.close(cx);
                                this.update(cx, |this, cx| {
                                    this.edit_book(cx, |book, symbol| {
                                        book.save_template(symbol, id)
                                    });
                                });
                            })
                            .into(),
                    );
                }
                {
                    let (this, menu) = (this.clone(), menu.clone());
                    items.push(
                        entry("Back to the default look", IconName::Undo2)
                            .on_click(move |_window, cx| {
                                menu.close(cx);
                                this.update(cx, |this, cx| {
                                    this.edit_book(cx, |book, symbol| {
                                        book.apply_starting_style(symbol, tool)
                                    });
                                });
                            })
                            .into(),
                    );
                }
                if has_default {
                    let (this, menu) = (this.clone(), menu.clone());
                    items.push(
                        entry("Forget the saved default", IconName::StarOff)
                            .on_click(move |_window, cx| {
                                menu.close(cx);
                                this.update(cx, |this, cx| {
                                    this.edit_book(cx, |book, _| book.forget_template(tool));
                                });
                            })
                            .into(),
                    );
                }
                if !template_names.is_empty() {
                    items.push(popup::Item::Separator);
                    items.push(popup::Item::Title("Saved looks".into()));
                    for name in template_names {
                        let (this, menu, applied) = (this.clone(), menu.clone(), name.clone());
                        items.push(
                            entry(&name, IconName::LayoutTemplate)
                                .on_click(move |_window, cx| {
                                    menu.close(cx);
                                    let applied = applied.clone();
                                    this.update(cx, |this, cx| {
                                        this.edit_book(cx, |book, symbol| {
                                            book.apply_named_template(symbol, tool, &applied)
                                        });
                                    });
                                })
                                .into(),
                        );
                    }
                }
                items
            } else {
                Vec::new()
            };
            let toggle = menu.clone();
            bar = bar.child(
                div()
                    .relative()
                    .child(
                        Button::new("draw-look")
                            .ghost()
                            .compact()
                            .icon(IconName::Palette)
                            .tooltip("Save or reuse this look")
                            .cursor_pointer()
                            .on_click(move |_event, _window, cx| toggle.toggle(cx)),
                    )
                    .children(menu.popup(
                        items,
                        popup::Placement::Below(tokens::height::COMPACT),
                        window,
                        cx,
                    )),
            );
        }
        bar
    }

    #[allow(unused_variables)]
    #[inline(never)]
    fn bar_actions(&self, mut bar: gpui::Div, st: &BarState, cx: &mut Context<Self>) -> gpui::Div {
        let (style, tool, locked, hidden, id) = (&st.style, st.tool, st.locked, st.hidden, st.id);
        let (has_fill, has_dash) = (st.has_fill, st.has_dash);
        let (has_default, template_names) = (st.has_default, st.template_names.clone());
        bar = bar
            .child(
                Button::new("draw-settings")
                    .ghost()
                    .compact()
                    .icon(IconName::Settings2)
                    .tooltip("Settings (double click)")
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        let chart = this.active_chart().clone();
                        crate::chart::open_drawing_settings(&chart, id, window, cx);
                    })),
            )
            .child(
                Button::new("draw-hide")
                    .ghost()
                    .compact()
                    .icon(if hidden {
                        IconName::Eye
                    } else {
                        IconName::EyeOff
                    })
                    .tooltip(if hidden { "Show" } else { "Hide" })
                    .toggled(hidden)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.drawing_command(id, DrawingCommand::Hidden(!hidden), cx);
                    })),
            )
            .child(
                Button::new("draw-lock")
                    .ghost()
                    .compact()
                    .icon(if locked {
                        IconName::Lock
                    } else {
                        IconName::LockOpen
                    })
                    .tooltip(if locked { "Unlock" } else { "Lock" })
                    .toggled(locked)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(|this, _event, _window, cx| this.toggle_drawing_lock(cx)),
                    ),
            )
            .child(
                Button::new("draw-copy")
                    .ghost()
                    .compact()
                    .icon(IconName::Copy)
                    .tooltip("Duplicate (Ctrl+D)")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _event, _window, cx| this.duplicate_drawing(cx))),
            )
            .child(
                Button::new("draw-delete")
                    .ghost()
                    .compact()
                    .icon(IconName::Trash)
                    .tooltip("Delete (Del)")
                    .disabled(locked)
                    .cursor_pointer()
                    .when(locked, |button| button.cursor_not_allowed())
                    .on_click(cx.listener(|this, _event, _window, cx| this.delete_drawing(cx))),
            );
        bar
    }
}

/// What the style bar knows of the selected drawing, gathered once for the pieces that build it.
struct BarState {
    style: wyck_chart::drawing::model::Style,
    tool: Tool,
    locked: bool,
    hidden: bool,
    id: u64,
    has_fill: bool,
    has_dash: bool,
    has_default: bool,
    template_names: Vec<String>,
}
