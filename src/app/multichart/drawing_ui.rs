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

use super::MultiChart;
use crate::app::chart::drawing::model::{Dash, Group, PALETTE, Tool, WIDTHS};
use crate::app::connection::ui;
use crate::app::theme;

/// The width of the rail of tools.
pub const RAIL_WIDTH: f32 = 46.0;

pub fn tool_icon(tool: Tool) -> IconName {
    match tool {
        Tool::TrendLine => IconName::TrendingUp,
        Tool::Ray => IconName::MoveUpRight,
        Tool::ExtendedLine => IconName::MoveDiagonal,
        Tool::HorizontalLine => IconName::Minus,
        Tool::HorizontalRay => IconName::MoveRight,
        Tool::VerticalLine => IconName::SeparatorVertical,
        Tool::CrossLine => IconName::Plus,
        Tool::Arrow => IconName::ArrowUpRight,
        Tool::ParallelChannel => IconName::Rows2,
        Tool::FibRetracement | Tool::FibExtension => IconName::ChartNoAxesGantt,
        Tool::Rectangle => IconName::Square,
        Tool::Ellipse => IconName::Ellipse,
        Tool::Brush => IconName::Brush,
        Tool::Measure => IconName::Ruler,
        Tool::LongPosition => IconName::ArrowBigUp,
        Tool::ShortPosition => IconName::ArrowBigDown,
        Tool::Text => IconName::Type,
        Tool::PriceLabel => IconName::Tag,
        Tool::Unknown => IconName::Pen,
    }
}

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
        let has_any = symbol.is_some_and(|symbol| book.count(&symbol) > 0);

        let mut rail = div()
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
                .on_click(cx.listener(|this, _event, _window, cx| this.clear_drawings(cx))),
        )
    }

    /// The tools of the open family, beside the rail, over a sheet that closes it when clicked.
    pub(super) fn render_flyout(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let group = self.flyout?;
        let current = self.drawings.read(cx).book().tool();
        let mut list = div()
            .absolute()
            .top(px(8.))
            .left(px(RAIL_WIDTH + 6.0))
            .w(px(230.))
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
        for tool in group_tools(group) {
            let selected = current == Some(tool);
            list = list.child(
                div()
                    .id(SharedString::from(format!("draw-tool-{tool:?}")))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(px(30.))
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
                    .hover(|style| style.bg(theme::surface_hover()).text_color(theme::fg()))
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
        let (tool, locked) = (drawing.tool, drawing.locked);
        let has_fill = matches!(
            tool,
            Tool::Rectangle
                | Tool::Ellipse
                | Tool::ParallelChannel
                | Tool::FibRetracement
                | Tool::FibExtension
        );
        let has_dash = !matches!(tool, Tool::Text | Tool::PriceLabel | Tool::Brush);

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
            bar = bar.child(divider());
            if !matches!(tool, Tool::Text | Tool::PriceLabel) {
                for (index, width) in WIDTHS.into_iter().enumerate() {
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
                    bar = bar.child(
                        div()
                            .id(SharedString::from(format!("draw-dash-{label}")))
                            .flex()
                            .items_center()
                            .justify_center()
                            .h(px(24.))
                            .px_2()
                            .rounded_md()
                            .cursor_pointer()
                            .text_size(px(11.))
                            .text_color(if selected {
                                theme::fg()
                            } else {
                                theme::muted_fg()
                            })
                            .when(selected, |el| el.bg(theme::accent_selected()))
                            .hover(|style| style.bg(theme::surface_hover()))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.set_drawing_dash(dash, cx);
                            }))
                            .child(match dash {
                                Dash::Solid => "----",
                                Dash::Dashed => "- - -",
                                Dash::Dotted => ". . .",
                            }),
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

        bar = bar
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
                    .on_click(cx.listener(|this, _event, _window, cx| this.delete_drawing(cx))),
            );

        Some(
            div()
                .absolute()
                .top(px(8.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(bar)
                .into_any_element(),
        )
    }
}
