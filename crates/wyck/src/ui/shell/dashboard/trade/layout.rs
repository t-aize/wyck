//! Where the ticket and the panel sit around the charts, and dragging them.

use super::{CHARTS_MIN, PANEL_MIN, bars};

use crate::ui::shell::dashboard::Dashboard;
use gpui::prelude::*;
use gpui::{Context, MouseButton, MouseMoveEvent, div, px};
use wyck_app::prefs::ticket::{Dock, WIDTH_DEFAULT};
use wyck_ui_kit::theme;

impl Dashboard {
    /// The charts with the ticket beside them, and the account panel under both.
    pub(in crate::ui::shell::dashboard) fn trading_layout(
        &self,
        charts: gpui::AnyElement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let panel = self.panel.clone().filter(|_| self.panel_open);
        let ticket = self.ticket.clone().filter(|_| self.ticket_open);
        let dragging = self.panel_drag.is_some();
        let editor_dragging = self.editor_drag.is_some();
        let editor_panel = self.editor_dock(cx);
        let ticket_dragging = self.ticket_drag.is_some();
        let (dock, width) = ticket.as_ref().map_or((Dock::Right, WIDTH_DEFAULT), |t| {
            let layout = t.read(cx).layout();
            (layout.dock, layout.width)
        });
        let charts = div().flex_1().min_w_0().h_full().flex().child(charts);
        let edge = |id: &'static str| {
            // The edge toward the charts drags to make the panel wider or narrower.
            div()
                .id(id)
                .flex_none()
                .w(px(wyck_ui_kit::tokens::splitter()))
                .h_full()
                .flex()
                .justify_center()
                .cursor_col_resize()
                .bg(if ticket_dragging {
                    theme::accent_alpha(0.4)
                } else {
                    gpui::rgba(0x00000000)
                })
                .hover(|s| s.bg(theme::accent_alpha(0.4)))
                .child(wyck_ui_kit::layout::rule_v().h_full())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                        this.ticket_drag = Some((f32::from(event.position.x), width));
                        cx.notify();
                    }),
                )
        };
        let column = ticket.map(|ticket| {
            let body = div()
                .id("ticket-column")
                .relative()
                .flex_1()
                .min_w_0()
                .h_full()
                .bg(theme::bg())
                .child(
                    div()
                        .id("ticket-scroll")
                        .h_full()
                        .overflow_hidden()
                        .child(ticket),
                );
            let column = div().flex_none().w(px(width)).h_full().flex().flex_row();
            if dock == Dock::Right {
                column.child(edge("ticket-resize")).child(body)
            } else {
                column.child(body).child(edge("ticket-resize"))
            }
        });
        let (before, after) = if dock == Dock::Left {
            (column, None)
        } else {
            (None, column)
        };
        div()
            .id("trading-layout")
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .when(dragging, |el| {
                el.cursor_row_resize()
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        this.drag_panel(event, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_panel_drag(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_panel_drag(cx)),
                    )
            })
            .when(editor_dragging, |el| {
                el.cursor_col_resize()
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        this.drag_editor(event, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_editor_drag(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_editor_drag(cx)),
                    )
            })
            .when(ticket_dragging, |el| {
                el.cursor_col_resize()
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        this.drag_ticket(event, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_ticket_drag(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_ticket_drag(cx)),
                    )
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .children(before)
                    .child(charts)
                    .children(after),
            )
            .children(panel.map(|panel| {
                div()
                    .relative()
                    .flex_none()
                    .h(px(self.panel_height_now()))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id("panel-resize")
                            .flex_none()
                            .h(px(wyck_ui_kit::tokens::splitter()))
                            .w_full()
                            .cursor_row_resize()
                            .border_t_1()
                            .border_color(if dragging {
                                theme::accent()
                            } else {
                                theme::border_hairline()
                            })
                            .hover(|s| s.border_color(theme::accent()))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                    this.panel_drag = Some(f32::from(event.position.y));
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(div().flex_1().min_h_0().child(panel))
            }))
            // Last, so it is drawn over everything else of the layout.
            .children(editor_panel)
    }

    /// The edge of the ticket is being dragged: its width follows the pointer, from where the
    /// edge was grabbed.
    pub(super) fn drag_ticket(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some((start, width)) = self.ticket_drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.end_ticket_drag(cx);
            return;
        }
        let Some(ticket) = self.ticket.clone() else {
            return;
        };
        let moved = f32::from(event.position.x) - start;
        ticket.update(cx, |t, cx| {
            // The panel grows as its edge goes away from the charts.
            let width = if t.layout().dock == Dock::Right {
                width - moved
            } else {
                width + moved
            };
            t.edit_layout(cx, |layout| layout.width = width);
        });
    }

    pub(super) fn end_ticket_drag(&mut self, cx: &mut Context<Self>) {
        if self.ticket_drag.take().is_some() {
            cx.notify();
        }
    }

    /// The tallest the panel can be in this window: the charts keep their least height.
    pub(super) fn panel_room(&self) -> f32 {
        (self.viewport_height - bars() - CHARTS_MIN).clamp(PANEL_MIN, 900.0)
    }

    /// The height the panel has now: the one the user chose, or less in a short window.
    pub(super) fn panel_height_now(&self) -> f32 {
        self.panel_height.clamp(PANEL_MIN, self.panel_room())
    }

    pub(super) fn drag_panel(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(last) = self.panel_drag else { return };
        if event.pressed_button != Some(MouseButton::Left) {
            self.end_panel_drag(cx);
            return;
        }
        let y = f32::from(event.position.y);
        // Start from the height shown, so a panel held down by a short window follows the pointer.
        let room = self.panel_room();
        self.panel_height = (self.panel_height_now() - (y - last)).clamp(PANEL_MIN, room);
        self.panel_drag = Some(y);
        cx.notify();
    }

    pub(super) fn end_panel_drag(&mut self, cx: &mut Context<Self>) {
        if self.panel_drag.take().is_some() {
            let height = self.panel_height;
            self.workspace.update(cx, |workspace, cx| {
                workspace.edit_preferences(cx, |prefs| prefs.panel_height = height);
            });
            cx.notify();
        }
    }
}
