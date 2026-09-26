//! Following other charts: the chart says where the user is (the span on screen, the point under
//! the pointer), and is told to show another chart's span, right edge or pointer. A chart made to
//! follow never reports it, which keeps linked charts free of loops.

use gpui::Context;

use super::input::Region;
use super::{Chart, ChartEvent, Hover, Span};

impl Chart {
    pub(super) fn emit_span(&self, cx: &mut Context<Self>) {
        if let Some(span) = self.span() {
            cx.emit(ChartEvent::ViewChanged(span));
        }
    }

    /// The times at the edges of the plot, once there is data to tell them from.
    pub(super) fn span(&self) -> Option<Span> {
        let plot_w = self.geometry().plot_w();
        let series = self.shown();
        let len = series.len();
        let step = self.step_ms();
        let left = self.view.index_at(0.0, len, plot_w);
        let right = self.view.index_at(plot_w, len, plot_w);
        Some(Span {
            left_ms: series.time_of_index(left, step)?,
            right_ms: series.time_of_index(right, step)?,
        })
    }

    /// The time and price under a pointer position, when it is over a point of the prices.
    pub(super) fn hover_info(&self, x: f32, y: f32) -> Option<Hover> {
        if self.region(x, y) != Region::Plot(0) {
            return None;
        }
        let series = self.shown();
        let len = series.len();
        let plot_w = self.geometry().plot_w();
        let index = self
            .view
            .index_at(f64::from(x), len, plot_w)
            .round()
            .clamp(0.0, len.checked_sub(1)? as f64) as usize;
        let map = self.main_map()?;
        Some(Hover {
            time_ms: series.time_at(index)?,
            price: map.price(f64::from(y)),
        })
    }

    /// Another chart was scrolled: show the same time at the right edge. Does not tell anyone.
    pub fn follow_right_edge(&mut self, right_ms: i64, cx: &mut Context<Self>) {
        let step = self.step_ms();
        let series = self.shown();
        let len = series.len();
        let Some(index) = series.index_of_time(right_ms, step) else {
            return;
        };
        self.view.offset = index + 0.5 - len as f64;
        self.view.clamp(len, self.geometry().plot_w());
        cx.notify();
        self.load_older_if_needed(cx);
    }

    /// Another chart was scrolled or zoomed: show the same span of time. Does not tell anyone.
    pub fn follow_span(&mut self, span: Span, cx: &mut Context<Self>) {
        let step = self.step_ms();
        let series = self.shown();
        let len = series.len();
        let (Some(left), Some(right)) = (
            series.index_of_time(span.left_ms, step),
            series.index_of_time(span.right_ms, step),
        ) else {
            return;
        };
        let points = right - left;
        if !(points.is_finite() && points >= 1.0) {
            return;
        }
        let plot_w = self.geometry().plot_w();
        self.view.bar_px = plot_w / points;
        self.view.offset = right + 0.5 - len as f64;
        self.view.clamp(len, plot_w);
        cx.notify();
        self.load_older_if_needed(cx);
    }

    /// The pointer of another chart, drawn here as a crosshair.
    pub fn show_remote_pointer(&mut self, pointer: Option<Hover>, cx: &mut Context<Self>) {
        if self.remote != pointer {
            self.remote = pointer;
            cx.notify();
        }
    }

    /// The point the legend describes: the one under the pointer, or the newest.
    pub(super) fn shown_index(&self) -> Option<usize> {
        let series = self.shown();
        let len = series.len();
        let last = len.checked_sub(1)?;
        let Some((x, _)) = self
            .hover
            .filter(|(x, y)| matches!(self.region(*x, *y), Region::Plot(_)))
        else {
            if let Some(remote) = self.remote {
                let index = series
                    .index_of_time(remote.time_ms, self.step_ms())?
                    .round();
                return Some(index.clamp(0.0, last as f64) as usize);
            }
            return Some(last);
        };
        let index = self
            .view
            .index_at(f64::from(x), len, self.geometry().plot_w())
            .round();
        Some(index.clamp(0.0, last as f64) as usize)
    }
}
