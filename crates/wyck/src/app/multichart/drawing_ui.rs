//! The drawing tools of the multichart: the rail of tools on the left, the list of tools of a
//! family that opens beside it, and the bar of options over a selected drawing.
//!
//! What a drawing is and how the pointer makes one lives in [`crate::app::chart::drawing`]; this
//! file only shows the choices and turns clicks into calls on the shared drawings.

use gpui::prelude::*;
use gpui::{AnyElement, Context, MouseButton, SharedString, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::Disableable;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tooltip::Tooltip;

use super::MultiChart;
use crate::app::chart::DrawingCommand;
use crate::app::chart::drawing::model::{Dash, Group, PALETTE, Tool};
use crate::app::chart::object_tree::tool_icon;
use crate::app::connection::ui;
use crate::app::{theme, widgets};

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

/// A short sample of a line style, drawn as shapes so every style sits on the same middle line
/// and has the same width (text dots sit on the baseline and drift off center).
fn dash_preview(dash: Dash, ink: gpui::Rgba) -> impl IntoElement {
    let row = div().flex().flex_row().items_center().justify_center();
    match dash {
        Dash::Solid => row.child(div().w(px(22.)).h(px(2.)).rounded_full().bg(ink)),
        Dash::Dashed => row
            .gap(px(3.))
            .children((0..3).map(|_| div().w(px(6.)).h(px(2.)).rounded_full().bg(ink))),
        Dash::Dotted => row
            .gap(px(4.))
            .children((0..4).map(|_| div().size(px(2.)).rounded_full().bg(ink))),
    }
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

        for group in Group::ALL {
            let shown = self.group_tool(group);
            let active = tool.is_some_and(|t| t.group() == group);
            rail = rail.child(
                Button::new(SharedString::from(format!("draw-group-{group:?}")))
                    .ghost()
                    .compact()
                    .icon(tool_icon(shown))
                    .tooltip(group.label())
                    .toggled(active || self.flyout == Some(group))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.flyout = if this.flyout == Some(group) {
                            None
                        } else {
                            Some(group)
                        };
                        cx.notify();
                    })),
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
                    crate::app::chart::open_object_tree(&chart, window, cx);
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

    /// The tools of the open family, beside the rail, over a sheet that closes it when clicked.
    pub(super) fn render_flyout(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let group = self.flyout?;
        let current = self.drawings.read(cx).book().tool();
        // Beside the family's button: the pointer's comes first, then one per family.
        let index = Group::ALL.iter().position(|g| *g == group).unwrap_or(0);
        let mut list = div()
            .id("draw-flyout-list")
            .absolute()
            .top(px(4.0 + 36.0 * (index as f32 + 1.0)))
            .left(px(RAIL_WIDTH + 6.0))
            .w(px(250.))
            .max_h(px(520.))
            .overflow_y_scroll()
            .p_1()
            .flex()
            .flex_col()
            .rounded_lg()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border_subtle())
            .occlude()
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_size(px(11.))
                    .text_color(theme::muted_fg())
                    .child(group.label()),
            );
        let favorites = self.favorite_tools(cx);
        for tool in group_tools(group) {
            let selected = current == Some(tool);
            let starred = favorites.contains(&tool);
            // The row is two boxes side by side, not one inside the other: the star must not also
            // pick the tool.
            list = list.child(
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
                            .h(px(30.))
                            .pl_2()
                            .cursor_pointer()
                            .text_size(px(12.))
                            .text_color(if selected {
                                theme::fg()
                            } else {
                                theme::muted_fg()
                            })
                            .hover(|style| style.text_color(theme::fg()))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.pick_tool(Some(tool), cx);
                            }))
                            .child(ui::icon_colored(
                                tool_icon(tool),
                                15.,
                                if selected {
                                    theme::fg()
                                } else {
                                    theme::muted_fg()
                                },
                            ))
                            .child(tool.label()),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("draw-star-{tool:?}")))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.))
                            .cursor_pointer()
                            .tooltip(move |window, cx| {
                                Tooltip::new(if starred {
                                    "Unpin from the favorites bar"
                                } else {
                                    "Pin to the favorites bar"
                                })
                                .build(window, cx)
                            })
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.toggle_favorite(tool, cx);
                            }))
                            .child(ui::icon_colored(
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
                    ),
            );
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
                        cx.notify();
                    }),
                )
                .child(list)
                .into_any_element(),
        )
    }

    /// The bar over a selected drawing: color, width, line style, fill, words, lock, copy, delete.
    pub(super) fn render_style_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let symbol = self.symbol_name(cx)?;
        let book = self.drawings.read(cx).book();
        if book.tool().is_some() {
            return None;
        }
        let drawing = book.selected().and_then(|id| book.get(&symbol, id))?;
        let style = drawing.style.clone();
        let (tool, locked, hidden, id) = (drawing.tool, drawing.locked, drawing.hidden, drawing.id);
        let has_fill = tool.has_fill();
        let has_dash = tool.has_dash();

        let divider = || {
            div()
                .mx_1()
                .w(px(1.))
                .h(px(18.))
                .bg(theme::border_hairline())
        };
        let mut bar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .p_1()
            .rounded_lg()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border_subtle())
            .occlude()
            .child(
                div()
                    .px_2()
                    .text_size(px(11.))
                    .text_color(theme::muted_fg())
                    .child(tool.label()),
            )
            .child(divider());

        if !locked {
            if tool.has_quick_color() {
                for color in PALETTE {
                    let selected = style.color == color;
                    bar = bar.child(
                        div()
                            .id(("draw-color", u64::from(color)))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(22.))
                            .rounded_full()
                            .cursor_pointer()
                            .when(selected, |el| el.border_1().border_color(theme::fg()))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.set_drawing_color(color, cx);
                            }))
                            .child(div().size(px(14.)).rounded_full().bg(swatch_color(color))),
                    );
                }
                // Any other color: the panel with the square, the hue bar and the typed values.
                let (open_this, pick_this) = (cx.entity(), cx.entity());
                bar = bar.child(widgets::color_swatch(
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
                bar = bar.child(divider());
            }
            if tool.has_width() {
                for (index, width) in tool.widths().into_iter().enumerate() {
                    let selected = (style.width - width).abs() < 0.01;
                    bar = bar.child(
                        div()
                            .id(("draw-width", index as u64))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(24.))
                            .rounded_md()
                            .cursor_pointer()
                            .when(selected, |el| el.bg(theme::accent_selected()))
                            .hover(|style| style.bg(theme::surface_hover()))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.set_drawing_width(width, cx);
                            }))
                            .child(
                                div()
                                    .w(px(14.))
                                    .h(px(width.max(1.0)))
                                    .rounded_full()
                                    .bg(theme::fg()),
                            ),
                    );
                }
                bar = bar.child(divider());
            }
            if has_dash {
                for (dash, label) in [
                    (Dash::Solid, "Solid"),
                    (Dash::Dashed, "Dashed"),
                    (Dash::Dotted, "Dotted"),
                ] {
                    let selected = style.dash == dash;
                    let ink = if selected {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    };
                    bar = bar.child(
                        div()
                            .id(SharedString::from(format!("draw-dash-{label}")))
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(38.))
                            .h(px(24.))
                            .rounded_md()
                            .cursor_pointer()
                            .when(selected, |el| el.bg(theme::accent_selected()))
                            .hover(|style| style.bg(theme::surface_hover()))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.set_drawing_dash(dash, cx);
                            }))
                            .child(dash_preview(dash, ink)),
                    );
                }
                bar = bar.child(divider());
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
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.toggle_drawing_fill(cx)
                            })),
                    )
                    .child(divider());
            }
            if tool.has_text() {
                bar = bar
                    .child(div().w(px(220.)).child(self.text_input.clone()))
                    .child(divider());
            }
        }

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
                .child(divider());
        }
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
                        crate::app::chart::open_drawing_settings(&chart, id, window, cx);
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
}
