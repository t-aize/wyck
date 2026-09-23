//! The pointer and the keys: what a press, a drag, the wheel or a pinch does where it happens.
//!
//! - On the plot: a drawing takes the press if it is under the pointer (or a tool is picked), a
//!   line (order, stop, alert) is grabbed if it is under it, anything else scrolls the chart
//!   (with Shift, the prices too).
//! - On the line between two panes: drags it, resizing both.
//! - On the price axis: drags zoom the prices, the wheel too; a double click fits them again.
//! - On the time axis: drags and the wheel zoom the time; a double click goes to the newest.
//! - A double click on the plot goes back to the newest prices.

use gpui::{Context, CursorStyle};

use super::lines::{LineId, to_real};
use super::view::PriceScale;
use super::{Chart, ChartEvent};

/// Where a position is on the chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// The plot, in band `n` (0 is the prices).
    Plot(usize),
    /// The price axis of band `n`.
    PriceAxis(usize),
    TimeAxis,
    Corner,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragKind {
    Pan,
    Price,
    Time,
    /// The line between band `n` and the one under it.
    Separator(usize),
    /// A line being moved, and where it is now (raw price).
    Line(LineId, f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drag {
    pub kind: DragKind,
    pub last: (f32, f32),
    /// Whether the pointer moved since the press.
    pub moved: bool,
}

/// How close (in pixels) the pointer must be to a line to grab it.
const LINE_REACH: f32 = 5.0;

impl Chart {
    pub(super) fn region(&self, x: f32, y: f32) -> Region {
        let geometry = self.geometry();
        let (x, y) = (f64::from(x), f64::from(y));
        let band = geometry.band_at(y);
        match (x < geometry.plot_w(), band) {
            (true, Some(band)) => Region::Plot(band),
            (false, Some(band)) => Region::PriceAxis(band),
            (true, None) => Region::TimeAxis,
            (false, None) => Region::Corner,
        }
    }

    fn moved(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        self.emit_span(cx);
        self.load_older_if_needed(cx);
    }

    pub(super) fn pan_by(&mut self, dx: f32, cx: &mut Context<Self>) {
        let plot_w = self.geometry().plot_w();
        let len = self.shown().len();
        self.view.pan(f64::from(dx), len, plot_w);
        self.moved(cx);
    }

    pub(super) fn zoom_by(&mut self, factor: f64, anchor_x: f32, cx: &mut Context<Self>) {
        let plot_w = self.geometry().plot_w();
        let len = self.shown().len();
        self.view.zoom(factor, f64::from(anchor_x), len, plot_w);
        self.moved(cx);
    }

    fn zoom_price_by(&mut self, factor: f64, anchor_y: Option<f32>, cx: &mut Context<Self>) {
        let Some(map) = self.main_map() else {
            return;
        };
        let (lo, hi) = map.zoomed(factor, anchor_y.map(f64::from));
        if lo.is_finite() && hi.is_finite() && hi > lo {
            self.view.price = PriceScale::Manual { lo, hi };
            cx.notify();
        }
    }

    fn pan_price_by(&mut self, dy: f32, cx: &mut Context<Self>) {
        let Some(map) = self.main_map() else {
            return;
        };
        let (lo, hi) = map.panned(f64::from(dy));
        if lo.is_finite() && hi.is_finite() && hi > lo {
            self.view.price = PriceScale::Manual { lo, hi };
            cx.notify();
        }
    }

    /// Fits the prices on screen again.
    pub fn reset_price_scale(&mut self, cx: &mut Context<Self>) {
        self.view.price = PriceScale::Auto;
        cx.notify();
    }

    pub fn pan_keys(&mut self, forward: bool, cx: &mut Context<Self>) {
        let step = (self.view.bar_px * 6.0) as f32;
        self.pan_by(if forward { -step } else { step }, cx);
    }

    pub fn zoom_keys(&mut self, magnify: bool, cx: &mut Context<Self>) {
        let anchor = self.geometry().plot_w() as f32;
        self.zoom_by(if magnify { 1.25 } else { 0.8 }, anchor, cx);
    }

    pub fn jump_to_latest(&mut self, cx: &mut Context<Self>) {
        self.view.jump_to_latest();
        self.view.price = PriceScale::Auto;
        cx.notify();
        self.emit_span(cx);
    }

    /// The draggable line within reach of the pointer, if any.
    fn line_at(&self, y: f32) -> Option<(LineId, f64)> {
        let map = self.main_map()?;
        self.lines
            .iter()
            .filter(|line| line.draggable)
            .map(|line| (line, (map.y(line.raw_price()) as f32 - y).abs()))
            .filter(|(_, distance)| *distance <= LINE_REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(line, _)| (line.id, line.raw_price()))
    }

    /// The price under a height of the prices band, in raw units, rounded to the symbol's
    /// decimals.
    pub(super) fn price_at(&self, y: f32) -> Option<f64> {
        let map = self.main_map()?;
        let unit = self.unit() as f64;
        Some((map.price(f64::from(y)) / unit).round() * unit)
    }

    pub(super) fn on_mouse_down(&mut self, x: f32, y: f32, clicks: usize, cx: &mut Context<Self>) {
        cx.emit(ChartEvent::Activated);
        if self.menu.take().is_some() {
            cx.notify();
        }
        let region = self.region(x, y);
        if region == Region::Corner {
            self.menu = Some(super::Menu::Zone);
            cx.notify();
            return;
        }
        if let Region::Plot(_) = region
            && let Some(separator) = self.geometry().separator_at(f64::from(y))
        {
            self.drag = Some(Drag {
                kind: DragKind::Separator(separator),
                last: (x, y),
                moved: false,
            });
            return;
        }
        if region == Region::Plot(0) {
            let tool_active = self
                .drawings
                .as_ref()
                .is_some_and(|d| d.read(cx).book().tool().is_some());
            if !tool_active && let Some((id, price)) = self.line_at(y) {
                self.drag = Some(Drag {
                    kind: DragKind::Line(id, price),
                    last: (x, y),
                    moved: false,
                });
                cx.notify();
                return;
            }
            if self.drawing_press(x, y, cx) {
                self.drawing_drag = true;
                cx.notify();
                return;
            }
        }
        if clicks >= 2 {
            match region {
                Region::Plot(_) | Region::TimeAxis => self.jump_to_latest(cx),
                Region::PriceAxis(0) => self.reset_price_scale(cx),
                Region::PriceAxis(_) | Region::Corner => {}
            }
            return;
        }
        let kind = match region {
            Region::Plot(_) => DragKind::Pan,
            Region::PriceAxis(0) => DragKind::Price,
            Region::PriceAxis(_) => return,
            Region::TimeAxis => DragKind::Time,
            Region::Corner => return,
        };
        self.drag = Some(Drag {
            kind,
            last: (x, y),
            moved: false,
        });
        cx.notify();
    }

    pub(super) fn on_mouse_move(
        &mut self,
        x: f32,
        y: f32,
        left_down: bool,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        if self.drawing_drag {
            if left_down {
                self.set_hover(x, y, cx);
                self.drawing_moved(x, y, cx);
            } else {
                self.drawing_released(x, y, cx);
            }
            return;
        }
        if let Some(drag) = self.drag {
            if !left_down {
                self.finish_drag(cx);
                return;
            }
            let (dx, dy) = (x - drag.last.0, y - drag.last.1);
            let mut next = Drag {
                last: (x, y),
                moved: drag.moved || dx.abs() + dy.abs() > 0.0,
                ..drag
            };
            match drag.kind {
                DragKind::Pan => {
                    self.set_hover(x, y, cx);
                    if shift {
                        self.pan_price_by(dy, cx);
                    }
                    self.pan_by(dx, cx);
                }
                DragKind::Price => self.zoom_price_by((-f64::from(dy) * 0.006).exp(), None, cx),
                DragKind::Time => {
                    let anchor = self.geometry().plot_w() as f32;
                    self.zoom_by((f64::from(dx) * 0.006).exp(), anchor, cx);
                }
                DragKind::Separator(index) => self.drag_separator(index, dy, cx),
                DragKind::Line(id, _) => {
                    self.set_hover(x, y, cx);
                    if let Some(price) = self.price_at(y) {
                        next.kind = DragKind::Line(id, price);
                        cx.notify();
                    }
                }
            }
            self.drag = Some(next);
            return;
        }
        self.set_hover(x, y, cx);
        self.drawing_moved(x, y, cx);
        cx.notify();
    }

    /// A drag ended: a line that moved says where it went.
    fn finish_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.drag.take() {
            if let DragKind::Line(id, price) = drag.kind
                && drag.moved
            {
                cx.emit(ChartEvent::LineMoved(id, to_real(price)));
            }
            cx.notify();
        }
    }

    fn drag_separator(&mut self, index: usize, dy: f32, cx: &mut Context<Self>) {
        let geometry = self.geometry();
        let panes = self.settings.panes();
        let mut weights = vec![self.settings.main_weight];
        weights.extend(panes.iter().map(|pane| pane.weight));
        let Some((a, b)) = geometry.drag_separator(&weights, index, f64::from(dy)) else {
            return;
        };
        self.edit_settings(cx, |settings| {
            let mut set = |band: usize, weight: f32| {
                if band == 0 {
                    settings.main_weight = weight;
                } else if let Some(pane) = panes.get(band - 1) {
                    settings.studies[pane.study].weight = weight;
                }
            };
            set(index, a);
            set(index + 1, b);
        });
    }

    fn set_hover(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        self.hover = Some((x, y));
        let info = self.hover_info(x, y);
        cx.emit(ChartEvent::Hover(info));
    }

    pub(super) fn on_mouse_up(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        if self.drawing_drag {
            self.drawing_released(x, y, cx);
        }
        self.finish_drag(cx);
    }

    pub(super) fn on_right_down(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        cx.emit(ChartEvent::Activated);
        self.context_at = Some((x, y));
    }

    pub(super) fn on_pointer_left(&mut self, cx: &mut Context<Self>) {
        if self.hover.take().is_some() {
            cx.emit(ChartEvent::Hover(None));
            cx.notify();
        }
    }

    pub(super) fn on_wheel(
        &mut self,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        match self.region(x, y) {
            Region::PriceAxis(0) => {
                self.zoom_price_by((f64::from(dy) * 0.002).exp(), Some(y), cx);
            }
            Region::PriceAxis(_) | Region::Corner => {}
            region @ (Region::Plot(_) | Region::TimeAxis) => {
                if shift || dx.abs() > dy.abs() {
                    self.pan_by(if shift { dy } else { dx }, cx);
                } else {
                    let anchor = if region == Region::TimeAxis {
                        self.geometry().plot_w() as f32
                    } else {
                        x
                    };
                    self.zoom_by((f64::from(dy) * 0.002).exp(), anchor, cx);
                }
            }
        }
    }

    /// The raw price a line is drawn at: where it is being dragged to, or its own.
    pub(super) fn line_price(&self, id: LineId, own: f64) -> f64 {
        match self.drag.map(|d| d.kind) {
            Some(DragKind::Line(dragged, price)) if dragged == id => price,
            _ => own,
        }
    }

    pub(super) fn cursor(&self, cx: &gpui::App) -> CursorStyle {
        if self.drawing_drag {
            return CursorStyle::ClosedHand;
        }
        if let Some(drag) = self.drag {
            return match drag.kind {
                DragKind::Pan => CursorStyle::ClosedHand,
                DragKind::Price | DragKind::Separator(_) | DragKind::Line(..) => {
                    CursorStyle::ResizeUpDown
                }
                DragKind::Time => CursorStyle::ResizeLeftRight,
            };
        }
        let Some((x, y)) = self.hover else {
            return CursorStyle::Arrow;
        };
        let region = self.region(x, y);
        if matches!(region, Region::Plot(_)) && self.geometry().separator_at(f64::from(y)).is_some()
        {
            return CursorStyle::ResizeUpDown;
        }
        let tool = self
            .drawings
            .as_ref()
            .is_some_and(|drawings| drawings.read(cx).book().tool().is_some());
        if region == Region::Plot(0) {
            if tool {
                return CursorStyle::Crosshair;
            }
            if self.line_at(y).is_some() {
                return CursorStyle::ResizeUpDown;
            }
            if self.over_drawing {
                return CursorStyle::PointingHand;
            }
        }
        match region {
            Region::Plot(_) => CursorStyle::Crosshair,
            Region::PriceAxis(0) => CursorStyle::ResizeUpDown,
            Region::PriceAxis(_) => CursorStyle::Arrow,
            Region::TimeAxis => CursorStyle::ResizeLeftRight,
            Region::Corner => CursorStyle::PointingHand,
        }
    }
}
