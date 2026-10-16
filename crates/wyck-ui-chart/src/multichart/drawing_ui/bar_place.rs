//! Where the bars that float over the charts sit, and where the user dragged them: the style bar of
//! the selected drawing (at the top) and the bar of the favorite tools (at the bottom).
//!
//! They belong to the whole area of charts, not to the chart that was clicked: a chart low in the
//! layout may be too small to hold them. The grip at the left edge of a bar moves it, a double
//! click on the grip brings it back.

use crate::multichart::MultiChart;
use gpui::prelude::*;
use gpui::{
    AnyElement, Bounds, Context, DispatchPhase, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, canvas, div, px,
};
use std::cell::Cell;
use std::rc::Rc;
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::{controls, icon, theme, tokens};

/// Where a bar tells the layout it was last drawn.
pub(in crate::multichart) type BoundsCell = Rc<Cell<Option<Bounds<Pixels>>>>;

/// Which bar is meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::multichart) enum Movable {
    /// The bar over the selected drawing, which sits at the top.
    StyleBar,
    /// The bar of the favorite tools, which sits at the bottom.
    Favorites,
}

#[derive(Default)]
pub(in crate::multichart) struct BarPlace {
    /// Whether the bar is anchored to the bottom of the charts rather than to the top.
    from_bottom: bool,
    /// How far the bar was moved from its home, in pixels (down and right are positive).
    offset: (f32, f32),
    /// The pointer and the offset at the moment the grip was pressed.
    drag: Option<((f32, f32), (f32, f32))>,
    /// Where the bar was last drawn, to keep it inside the charts.
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The distance of the bar from its edge before it is moved: from the top, under the legend of
    /// the charts along the top, or from the bottom, clear of the time axis.
    home: f32,
}

impl BarPlace {
    /// A bar anchored to the bottom of the charts, moved by `offset`.
    pub(in crate::multichart) fn at_bottom(home: f32, offset: (f32, f32)) -> Self {
        Self {
            from_bottom: true,
            offset,
            home,
            ..Self::default()
        }
    }

    pub(in crate::multichart) fn set_home(&mut self, home: f32) {
        self.home = home;
    }

    /// The cell the bar reports its bounds to.
    pub(in crate::multichart) fn bounds_cell(&self) -> Rc<Cell<Option<Bounds<Pixels>>>> {
        Rc::clone(&self.bounds)
    }

    pub(in crate::multichart) fn home(&self) -> f32 {
        self.home
    }

    /// The offset to draw with: the one set, pulled back inside `area` when the charts shrank or
    /// the bar changed width.
    pub(in crate::multichart) fn offset(&self, area: Option<Bounds<Pixels>>) -> (f32, f32) {
        self.clamp(self.offset, area)
    }

    fn clamp(&self, offset: (f32, f32), area: Option<Bounds<Pixels>>) -> (f32, f32) {
        let (Some(area), Some(bar)) = (area, self.bounds.get()) else {
            return offset;
        };
        let (area_w, area_h) = (f32::from(area.size.width), f32::from(area.size.height));
        let (bar_w, bar_h) = (f32::from(bar.size.width), f32::from(bar.size.height));
        let reach_x = ((area_w - bar_w) / 2.0).max(0.0);
        // The bar may go from the very top to the very bottom of the area.
        let (min_y, max_y) = if self.from_bottom {
            let up = -(area_h - bar_h - self.home).max(0.0);
            (up, self.home.max(0.0))
        } else {
            let min_y = -self.home;
            (min_y, (area_h - bar_h - self.home).max(min_y))
        };
        (
            offset.0.clamp(-reach_x, reach_x),
            offset.1.clamp(min_y, max_y),
        )
    }
}

impl MultiChart {
    fn place(&self, bar: Movable) -> &BarPlace {
        match bar {
            Movable::StyleBar => &self.bar,
            Movable::Favorites => &self.pill,
        }
    }

    fn place_mut(&mut self, bar: Movable) -> &mut BarPlace {
        match bar {
            Movable::StyleBar => &mut self.bar,
            Movable::Favorites => &mut self.pill,
        }
    }

    /// The bar being dragged now, if any.
    fn dragged_bar(&self) -> Option<Movable> {
        [Movable::StyleBar, Movable::Favorites]
            .into_iter()
            .find(|bar| self.place(*bar).drag.is_some())
    }

    /// The grip was pressed: the bar follows the pointer until the button is released.
    pub(super) fn begin_bar_drag(
        &mut self,
        bar: Movable,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let area = self.area.get();
        let place = self.place_mut(bar);
        let start = place.offset(area);
        place.offset = start;
        place.drag = Some(((f32::from(position.x), f32::from(position.y)), start));
        cx.notify();
    }

    /// The bar goes back to its home.
    pub(super) fn reset_bar_place(&mut self, bar: Movable, cx: &mut Context<Self>) {
        let place = self.place_mut(bar);
        place.offset = (0.0, 0.0);
        place.drag = None;
        self.keep_bar_place(bar, cx);
        cx.notify();
    }

    /// Keeps where the favorites were put for the next start. The style bar is not kept: it
    /// follows the drawing it is over.
    fn keep_bar_place(&mut self, bar: Movable, cx: &mut Context<Self>) {
        if bar != Movable::Favorites {
            return;
        }
        let offset = self.pill.offset;
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.favorites_place = [offset.0, offset.1]);
        });
    }

    /// Follows the pointer while the grip is held.
    fn drag_bar(&mut self, bar: Movable, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let area = self.area.get();
        let place = self.place_mut(bar);
        let Some((pointer, start)) = place.drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            // The button was released where nobody heard it.
            place.drag = None;
            self.keep_bar_place(bar, cx);
            cx.notify();
            return;
        }
        let moved = (
            start.0 + f32::from(event.position.x) - pointer.0,
            start.1 + f32::from(event.position.y) - pointer.1,
        );
        let place = self.place_mut(bar);
        place.offset = place.clamp(moved, area);
        cx.notify();
    }

    /// The button was released, whether over the bar or not.
    fn end_bar_drag(&mut self, bar: Movable, cx: &mut Context<Self>) {
        if self.place_mut(bar).drag.take().is_some() {
            self.keep_bar_place(bar, cx);
            cx.notify();
        }
    }

    /// The grip at the left of a bar: hold it to move the bar, double click to bring it back.
    pub(in crate::multichart) fn bar_grip(
        bar: Movable,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(match bar {
                Movable::StyleBar => "draw-bar-grip",
                Movable::Favorites => "favorites-grip",
            })
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .h(px(tokens::height::compact()))
            .px_1()
            .cursor_grab()
            .tooltip(controls::tooltip(
                "Move the bar (double click to put it back)",
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    if event.click_count >= 2 {
                        this.reset_bar_place(bar, cx);
                    } else {
                        this.begin_bar_drag(bar, event.position, cx);
                    }
                }),
            )
            .child(icon::tinted(
                IconName::GripVertical,
                tokens::text::body(),
                theme::muted_fg(),
            ))
    }

    /// The position the bar is drawn at: its offset kept inside the charts, and the cell where it
    /// reports its bounds.
    pub(in crate::multichart) fn place_of(&self, bar: Movable) -> ((f32, f32), BoundsCell) {
        let place = self.place(bar);
        (place.offset(self.area.get()), place.bounds_cell())
    }

    /// While a bar is being dragged: a layer over the charts that shows the grabbing cursor and
    /// keeps the charts from reacting to the pointer, with window-wide listeners that follow it.
    /// Listening on an element would not do: the bar blocks the mouse for the elements behind it,
    /// so the moves over the bar itself would never arrive.
    pub(in crate::multichart) fn bar_drag_layer(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let bar = self.dragged_bar()?;
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
                                    moves.update(cx, |this, cx| this.drag_bar(bar, event, cx));
                                }
                            });
                            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble
                                    && event.button == MouseButton::Left
                                {
                                    ups.update(cx, |this, cx| this.end_bar_drag(bar, cx));
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
