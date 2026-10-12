//! Where the style bar sits: at the top of the charts, and wherever the user dragged it to.
//!
//! The drawings show on every chart of the symbol, so the bar belongs to the whole area of
//! charts, not to the chart that was clicked: a chart low in the layout may be too small to hold
//! it. The grip at its left edge moves it, a double click on the grip brings it back.

use crate::ui::features::multichart::MultiChart;
use gpui::prelude::*;
use gpui::{
    AnyElement, Bounds, Context, DispatchPhase, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels,
    canvas, div,
};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Default)]
pub(in crate::ui::features::multichart) struct BarPlace {
    /// How far the bar was moved from its home, in pixels.
    offset: (f32, f32),
    /// The pointer and the offset at the moment the grip was pressed.
    drag: Option<((f32, f32), (f32, f32))>,
    /// Where the bar was last drawn, to keep it inside the charts.
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The top edge of the bar before it is moved: under the legend of the charts along the top.
    home_top: f32,
}

impl BarPlace {
    pub(in crate::ui::features::multichart) fn set_home(&mut self, top: f32) {
        self.home_top = top;
    }

    /// The cell the bar reports its bounds to.
    pub(super) fn bounds_cell(&self) -> Rc<Cell<Option<Bounds<Pixels>>>> {
        self.bounds.clone()
    }

    pub(super) fn home_top(&self) -> f32 {
        self.home_top
    }

    /// The offset to draw with: the one set, pulled back inside `area` when the charts shrank or
    /// the bar changed width.
    pub(super) fn offset(&self, area: Option<Bounds<Pixels>>) -> (f32, f32) {
        self.clamp(self.offset, area)
    }

    fn clamp(&self, offset: (f32, f32), area: Option<Bounds<Pixels>>) -> (f32, f32) {
        let (Some(area), Some(bar)) = (area, self.bounds.get()) else {
            return offset;
        };
        let (area_w, area_h) = (f32::from(area.size.width), f32::from(area.size.height));
        let (bar_w, bar_h) = (f32::from(bar.size.width), f32::from(bar.size.height));
        let reach_x = ((area_w - bar_w) / 2.0).max(0.0);
        let min_y = -self.home_top;
        let max_y = (area_h - bar_h - self.home_top).max(min_y);
        (
            offset.0.clamp(-reach_x, reach_x),
            offset.1.clamp(min_y, max_y),
        )
    }
}

impl MultiChart {
    /// The grip was pressed: the bar follows the pointer until the button is released.
    pub(super) fn begin_bar_drag(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let area = self.area.get();
        let start = self.bar.offset(area);
        self.bar.offset = start;
        self.bar.drag = Some(((f32::from(position.x), f32::from(position.y)), start));
        cx.notify();
    }

    /// The bar goes back to its home.
    pub(super) fn reset_bar_place(&mut self, cx: &mut Context<Self>) {
        self.bar.offset = (0.0, 0.0);
        self.bar.drag = None;
        cx.notify();
    }

    /// Follows the pointer while the grip is held.
    fn drag_bar(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some((pointer, start)) = self.bar.drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            // The button was released where nobody heard it.
            self.bar.drag = None;
            cx.notify();
            return;
        }
        let moved = (
            start.0 + f32::from(event.position.x) - pointer.0,
            start.1 + f32::from(event.position.y) - pointer.1,
        );
        self.bar.offset = self.bar.clamp(moved, self.area.get());
        cx.notify();
    }

    /// The button was released, whether over the bar or not.
    fn end_bar_drag(&mut self, cx: &mut Context<Self>) {
        if self.bar.drag.take().is_some() {
            cx.notify();
        }
    }

    /// While the bar is being dragged: a layer over the charts that shows the grabbing cursor and
    /// keeps the charts from reacting to the pointer, with window-wide listeners that follow it.
    /// Listening on an element would not do: the bar blocks the mouse for the elements behind it,
    /// so the moves over the bar itself would never arrive.
    pub(in crate::ui::features::multichart) fn bar_drag_layer(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.bar.drag?;
        let this = cx.entity();
        Some(
            div()
                .absolute()
                .size_full()
                .occlude()
                .cursor_grabbing()
                .child(
                    canvas(
                        |_, _, _| (),
                        move |_, (), window, _cx| {
                            let (moves, ups) = (this.clone(), this.clone());
                            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble {
                                    moves.update(cx, |this, cx| this.drag_bar(event, cx));
                                }
                            });
                            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble
                                    && event.button == MouseButton::Left
                                {
                                    ups.update(cx, |this, cx| this.end_bar_drag(cx));
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_full(),
                )
                .into_any_element(),
        )
    }
}
