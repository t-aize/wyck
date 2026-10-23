//! How the charts are drawn together: the cells of the layout, the lines between them that can be
//! dragged, the style bar and the favorites bar over them.

use super::{MultiChart, SplitDrag};
use crate::kit::theme;
use gpui::prelude::*;
use gpui::{Context, MouseButton, MouseMoveEvent, SharedString, Window, canvas, div, px, relative};
use std::rc::Rc;

/// How wide the hit zone of a line between charts is.
const DIVIDER_HIT: f32 = 7.0;

impl Render for MultiChart {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A text drawing that was just made takes the keyboard, so its words can be typed.
        if self.drawings.read(cx).book().wants_text_focus() {
            self.drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, wyck_drawings::book::Book::take_text_focus);
            });
            window.focus(&gpui::Focusable::focus_handle(&self.text_input, cx), cx);
        }
        // A chart that changed symbol gets that symbol's hours (a no-op when it has them).
        self.give_hours(cx);
        let several = self.slots.len() > 1;
        // The symbol is named on a chart only when the charts do not all show the same one: with one
        // symbol the header says it already.
        let names: Vec<Option<String>> = self
            .slots
            .iter()
            .map(|slot| slot.chart.read(cx).symbol().map(|s| s.name.to_string()))
            .collect();
        let show_symbol = names
            .split_first()
            .is_some_and(|(first, rest)| rest.iter().any(|name| name != first));
        let floor = if self.favorites_shown(cx) {
            super::favorites::PILL_TOP
        } else {
            0.0
        };
        for (index, slot) in self.slots.iter().enumerate() {
            let selected = !several || index == self.active;
            slot.chart.update(cx, |chart, cx| {
                chart.set_selected(selected, cx);
                chart.set_hint_floor(floor, cx);
                chart.set_show_symbol(show_symbol, cx);
            });
        }
        let (rects, dividers) = self.tree.place(self.slots.len());
        // The bar of the selected drawing floats over the whole area, under the legends of the
        // charts along the top, so a small chart low in the layout does not have to hold it.
        let home_top = self
            .slots
            .iter()
            .zip(&rects)
            .filter(|(_, r)| r.y < 0.001)
            .map(|(slot, _)| slot.chart.read(cx).legend_bottom())
            .fold(0.0, f32::max);
        self.bar.set_home(home_top);
        let style_bar = self.render_style_bar(window, cx);
        let gap = if several { 1.0 } else { 0.0 };

        let cells = self
            .slots
            .iter()
            .zip(&rects)
            .enumerate()
            .map(|(index, (slot, r))| {
                let active = several && index == self.active;
                div()
                    .id(("chart-cell", index))
                    .absolute()
                    .left(relative(r.x))
                    .top(relative(r.y))
                    .w(relative(r.w))
                    .h(relative(r.h))
                    .p(px(gap))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            window.focus(&this.focus, cx);
                            this.activate(index, cx);
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _event, _window, cx| this.activate(index, cx)),
                    )
                    .child(
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .rounded(px(if several { 3.0 } else { 0.0 }))
                            .overflow_hidden()
                            .border_1()
                            .border_color(if active {
                                theme::accent()
                            } else if several {
                                theme::border_hairline()
                            } else {
                                gpui::rgba(0x0000_0000)
                            })
                            .child(slot.chart.clone()),
                    )
            })
            .collect::<Vec<_>>();

        let dragging = self.split_drag.map(|d| (d.divider.split, d.divider.index));
        let divider_elements = dividers
            .iter()
            .copied()
            .map(|divider| {
                let key = (divider.split, divider.index);
                let lit = dragging == Some(key) || self.hover_divider == Some(key);
                let base = div()
                    .id(SharedString::from(format!(
                        "chart-divider-{}-{}",
                        divider.split, divider.index
                    )))
                    .absolute()
                    .flex()
                    .items_center()
                    .justify_center()
                    .occlude()
                    .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                        let next = if *hovered { Some(key) } else { None };
                        if this.hover_divider != next && this.split_drag.is_none() {
                            this.hover_divider = next;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                            cx.stop_propagation();
                            if event.click_count >= 2 {
                                this.reset_splits(cx);
                                return;
                            }
                            this.split_drag = Some(SplitDrag {
                                divider,
                                last: (f32::from(event.position.x), f32::from(event.position.y)),
                            });
                            cx.notify();
                        }),
                    );
                let line = div().bg(if lit {
                    theme::accent()
                } else {
                    gpui::rgba(0x0000_0000)
                });
                if divider.vertical {
                    base.left(relative(divider.from))
                        .w(relative(divider.to - divider.from))
                        .top(relative(divider.at))
                        .mt(px(-DIVIDER_HIT / 2.0))
                        .h(px(DIVIDER_HIT))
                        .cursor_row_resize()
                        .child(line.w_full().h(px(if lit { 3.0 } else { 1.0 })))
                } else {
                    base.top(relative(divider.from))
                        .h(relative(divider.to - divider.from))
                        .left(relative(divider.at))
                        .ml(px(-DIVIDER_HIT / 2.0))
                        .w(px(DIVIDER_HIT))
                        .cursor_col_resize()
                        .child(line.h_full().w(px(if lit { 3.0 } else { 1.0 })))
                }
            })
            .collect::<Vec<_>>();

        let bar_drag = self.bar_drag_layer(cx);
        let rail = self.render_rail(cx);
        let favorites = self.render_favorites(window, cx);
        let flyout = self.render_flyout(cx);
        let area_cell = Rc::clone(&self.area);
        let root_cell = Rc::clone(&self.root_bounds);

        div()
            .relative()
            .flex()
            .flex_row()
            .flex_1()
            .w_full()
            .min_h_0()
            .bg(theme::bg())
            .track_focus(&self.focus)
            .child(
                canvas(
                    move |bounds, _window, _cx| root_cell.set(Some(bounds)),
                    |_, (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(rail)
            .child(
                div().flex_1().h_full().min_w_0().flex().flex_col().child(
                    div()
                        .id("chart-area")
                        .relative()
                        .flex_1()
                        .w_full()
                        .min_h_0()
                        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                            this.on_area_move(event, cx);
                        }))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| this.end_split_drag(cx)),
                        )
                        .on_mouse_up_out(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| this.end_split_drag(cx)),
                        )
                        .child(
                            canvas(
                                move |bounds, _window, _cx| area_cell.set(Some(bounds)),
                                |_, (), _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        )
                        .children(cells)
                        .children(divider_elements)
                        .children(style_bar)
                        .children(bar_drag)
                        .children(favorites),
                ),
            )
            .children(flyout)
    }
}
