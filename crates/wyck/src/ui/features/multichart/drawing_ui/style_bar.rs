//! The style bar over a selected drawing: its colors and its line.

use super::bar_place::Movable;
use super::{OPACITY_STEPS, swatch_color};

use crate::ui::features::multichart::MultiChart;
use crate::ui::kit::icon::IconName;
use crate::ui::kit::{controls, layout, theme, tokens};
use gpui::prelude::*;
use gpui::{AnyElement, Context, Window, canvas, div, px};
use wyck_drawings::model::style::{Dash, PALETTE};
use wyck_drawings::model::tool::Tool;

impl MultiChart {
    /// The bar over a selected drawing, or nothing. Only the cheap test lives here: the bar itself
    /// is built by [`Self::build_style_bar`], whose frame is large, so a view with nothing
    /// selected (the usual case) does not pay for it on the stack.
    pub(in crate::ui::features::multichart) fn render_style_bar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let symbol = self.symbol_name(cx)?;
        let book = self.drawings.read(cx).book();
        // The bar stays while a tool is armed (the one kept for the next drawing): the drawing
        // just made is still there to restyle.
        book.selected().and_then(|id| book.get(&symbol, id))?;
        self.build_style_bar(window, cx)
    }

    /// The bar over a selected drawing: color, width, line style, fill, words, lock, copy, delete.
    #[inline(never)]
    pub(super) fn build_style_bar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let symbol = self.symbol_name(cx)?;
        let book = self.drawings.read(cx).book();
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
            .child(self.bar_grip(Movable::StyleBar, cx))
            .child(
                div()
                    .px_2()
                    .text_size(px(tokens::text::small()))
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

        // The bar belongs to the whole area of charts: it sits under the legends along the top, or
        // where the user moved it.
        let (dx, dy) = self.bar.offset(self.area.get());
        let bounds = self.bar.bounds_cell();
        let bar = bar.relative().left(px(dx)).child(
            canvas(
                move |bounds_now, _window, _cx| bounds.set(Some(bounds_now)),
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        );
        Some(
            div()
                .absolute()
                .top(px(self.bar.home() + dy))
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
    pub(super) fn bar_colors(
        &self,
        mut bar: gpui::Div,
        st: &BarState,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
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
                        .size(px(tokens::height::compact()))
                        .rounded_full()
                        .cursor_pointer()
                        .when(selected, |el| el.border_1().border_color(theme::fg()))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.set_drawing_color(color, cx);
                        }))
                        .child(
                            div()
                                .size(px(crate::ui::kit::tokens::swatch::medium()))
                                .rounded_full()
                                .bg(swatch_color(color)),
                        ),
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
                        .size(px(tokens::height::compact()))
                        .rounded_full()
                        .cursor_pointer()
                        .tooltip(controls::tooltip(format!("#{color:06x}")))
                        .when(selected, |el| el.border_1().border_color(theme::fg()))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.set_drawing_color(color, cx);
                        }))
                        .child(
                            div()
                                .size(px(crate::ui::kit::tokens::swatch::medium()))
                                .rounded_full()
                                .bg(swatch_color(color)),
                        ),
                );
            }
            // Any other color: the panel with the square, the hue bar and the typed values.
            let (open_this, pick_this, opacity_this) = (cx.entity(), cx.entity(), cx.entity());
            let opacity: crate::ui::kit::color_picker::ChangeOpacity =
                std::rc::Rc::new(move |value, _window, cx| {
                    opacity_this.update(cx, |this, cx| this.set_drawing_opacity(value, cx));
                });
            bar = bar.child(controls::color_swatch_with_opacity(
                "draw-color-custom",
                style.color,
                Some(crate::ui::kit::color_picker::Opacity {
                    value: style.opacity,
                    label: "LINES",
                    change: opacity,
                }),
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
    pub(super) fn bar_line_options(
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
                    crate::ui::kit::button::quiet("draw-fill")
                        .icon(IconName::PaintBucket)
                        .tooltip("Fill")
                        .toggled(style.fill)
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
                crate::ui::kit::button::quiet("draw-opacity")
                    .icon(IconName::Droplet)
                    .label(format!("{}%", (opacity * 100.0).round() as u32))
                    .tooltip("Opacity of the lines (click to step down)")
                    .toggled(opacity < 0.99)
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.edit_book(cx, |book, symbol| {
                            book.edit_style(symbol, |s| s.opacity = next)
                        });
                    })),
            );
        }
        bar
    }
}

/// What the style bar knows of the selected drawing, gathered once for the pieces that build it.
pub(super) struct BarState {
    pub(super) style: wyck_drawings::model::style::Style,
    pub(super) tool: Tool,
    pub(super) locked: bool,
    pub(super) hidden: bool,
    pub(super) id: u64,
    pub(super) has_fill: bool,
    pub(super) has_dash: bool,
    pub(super) has_default: bool,
    pub(super) template_names: Vec<String>,
}
