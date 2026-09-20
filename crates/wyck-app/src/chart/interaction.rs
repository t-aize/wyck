//! The chart as a user drives it: one model that owns the data and the view, and answers the
//! mouse and the keyboard.
//!
//! [`ChartModel`] holds the [`Series`], the [`Viewport`] and the [`PriceScale`], the size of the
//! drawing area, the pointer and the drag in progress. The view feeds it raw input (a wheel turn,
//! a press, a move) and it says whether anything changed. It does no drawing and knows no toolkit,
//! so every gesture below is covered by a plain unit test.
//!
//! # The three regions
//!
//! ```text
//! +-------------------------+-------+
//! |                         | price |
//! |          plot           | axis  |
//! |                         |       |
//! +-------------------------+-------+
//! |       time axis         | (corner)
//! +-------------------------+-------+
//! ```
//!
//! # Gestures
//!
//! | Where | Gesture | Effect |
//! |---|---|---|
//! | plot, time axis | wheel | zoom in time around the pointer |
//! | plot | Shift + wheel, or a sideways wheel | scroll in time |
//! | plot | Ctrl + wheel | zoom the price range around the pointer |
//! | price axis | wheel | zoom the price range around the pointer |
//! | plot | drag | scroll in time, and in price once the drag goes vertical |
//! | price axis | drag | stretch or squash the price range |
//! | time axis | drag | stretch or squash the bars |
//! | price axis | double click | back to the automatic price range |
//! | time axis, plot corner | double click | back to the newest bars |
//!
//! Anything that moves the price range by hand switches the scale to manual (see
//! [`scale`](super::scale)); the double click on the price axis switches it back.

use wyck_engine::domain::{Candle, Period, UnixMillis};

use super::scale::{PriceScale, ScaleMode};
use super::series::Series;
use super::viewport::Viewport;

/// Wheel pixels to a zoom exponent: a typical notch of 72 pixels zooms by about a fifth.
const WHEEL_ZOOM_RATE: f64 = 0.0025;

/// Drag pixels on an axis to a zoom exponent.
const AXIS_DRAG_RATE: f64 = 0.005;

/// A key zoom step.
const KEY_ZOOM: f64 = 1.2;

/// A vertical drag inside the plot is ignored until it has gone this far (pixels), so a
/// horizontal scroll that wobbles does not leave automatic scaling.
const VERTICAL_DEAD_ZONE: f64 = 4.0;

/// Which part of the chart a point is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// The candles.
    Plot,
    /// The price labels on the right.
    PriceAxis,
    /// The time labels along the bottom.
    TimeAxis,
    /// Where the two axes meet.
    Corner,
    /// Outside the chart.
    Outside,
}

/// A key the chart reacts to. The view maps its own key events to these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKey {
    /// Scroll towards older bars.
    Left,
    /// Scroll towards newer bars.
    Right,
    /// Zoom in.
    ZoomIn,
    /// Zoom out.
    ZoomOut,
    /// Jump to the newest bar.
    Latest,
    /// Default zoom, newest bar, automatic price range.
    Reset,
}

/// A drag in progress.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    /// Moving the plot. `origin_y` and `vertical` decide when the price range starts to follow.
    Pan {
        last: (f64, f64),
        origin_y: f64,
        vertical: bool,
    },
    /// Stretching the price range from the axis.
    PriceZoom { last_y: f64 },
    /// Stretching the bars from the axis.
    TimeZoom { last_x: f64 },
}

/// What the crosshair points at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hover {
    /// The pointer position in the plot.
    pub x: f64,
    /// The pointer position in the plot.
    pub y: f64,
    /// The price under the pointer.
    pub price: f64,
    /// The bar under the pointer, if it is over the data.
    pub bar: Option<usize>,
}

/// The chart's data, view and input state. See the [module docs](self).
#[derive(Debug, Clone)]
pub struct ChartModel {
    /// The bars.
    pub series: Series,
    /// The period of the bars.
    pub period: Period,
    /// The decimals the symbol quotes, for price labels.
    pub digits: u32,
    /// Zoom and pan in time.
    pub viewport: Viewport,
    /// The price range.
    pub scale: PriceScale,
    size: (f64, f64),
    axis: (f64, f64),
    pointer: Option<(f64, f64)>,
    drag: Option<Drag>,
}

impl ChartModel {
    /// An empty chart for `period`, with axes of the given size in pixels (width of the price axis,
    /// height of the time axis).
    #[must_use]
    pub fn new(period: Period, axis_width: f64, axis_height: f64) -> Self {
        Self {
            series: Series::new(),
            period,
            digits: 5,
            viewport: Viewport::default(),
            scale: PriceScale::default(),
            size: (0.0, 0.0),
            axis: (axis_width, axis_height),
            pointer: None,
            drag: None,
        }
    }

    // ---- size and regions ----

    /// The width of the plot, without the price axis.
    #[must_use]
    pub fn plot_width(&self) -> f64 {
        (self.size.0 - self.axis.0).max(1.0)
    }

    /// The height of the plot, without the time axis.
    #[must_use]
    pub fn plot_height(&self) -> f64 {
        (self.size.1 - self.axis.1).max(1.0)
    }

    /// The size of the whole drawing area.
    #[must_use]
    pub fn size(&self) -> (f64, f64) {
        self.size
    }

    /// Sets the size of the drawing area and the axes (they scale with the interface). Returns
    /// whether anything changed.
    pub fn set_size(&mut self, width: f64, height: f64, axis_width: f64, axis_height: f64) -> bool {
        if (width, height) == self.size && (axis_width, axis_height) == self.axis {
            return false;
        }
        self.size = (width.max(0.0), height.max(0.0));
        self.axis = (axis_width, axis_height);
        self.viewport
            .set_width(self.plot_width(), self.series.len());
        self.refit();
        true
    }

    /// The region `(x, y)` falls in, in the drawing area's own coordinates.
    #[must_use]
    pub fn region(&self, x: f64, y: f64) -> Region {
        let (w, h) = self.size;
        if x < 0.0 || y < 0.0 || x > w || y > h {
            return Region::Outside;
        }
        match (x < self.plot_width(), y < self.plot_height()) {
            (true, true) => Region::Plot,
            (false, true) => Region::PriceAxis,
            (true, false) => Region::TimeAxis,
            (false, false) => Region::Corner,
        }
    }

    // ---- data ----

    /// Starts over for another symbol or period: no bars, default zoom, automatic price range.
    pub fn clear(&mut self, period: Period, digits: u32) {
        self.series = Series::new();
        self.period = period;
        self.digits = digits;
        self.viewport = Viewport::new(self.plot_width());
        self.scale = PriceScale::default();
        self.drag = None;
    }

    /// Adds bars from the server or the cache. The chart keeps showing what the user was looking
    /// at: history added in front moves nothing, and new bars at the end push a scrolled back
    /// chart along (see [`Viewport::bars_appended`]). Returns whether anything changed.
    pub fn merge(&mut self, bars: impl IntoIterator<Item = Candle>) -> bool {
        let before = (self.series.first_time(), self.series.last().map(|b| b.time));
        let before_len = self.series.len();
        if !self.series.merge(bars) {
            return false;
        }
        if let (Some(_), Some(last)) = before {
            let older_than_last = self.series.lower_bound(last + 1);
            let appended = self.series.len() - older_than_last;
            self.viewport.bars_appended(appended);
        } else if before_len == 0 {
            // First bars of a fresh chart: land on the newest bar.
            self.viewport.scroll_to_latest(self.series.len());
        }
        self.viewport.clamp(self.series.len());
        self.refit();
        true
    }

    /// Folds a live price into the newest bar. Returns whether the chart changed.
    pub fn apply_price(&mut self, price: f64, at: UnixMillis) -> bool {
        let before = self.series.len();
        if !self.series.apply_price(self.period, price, at) {
            return false;
        }
        self.viewport
            .bars_appended(self.series.len().saturating_sub(before));
        self.refit();
        true
    }

    /// Drops the oldest bars beyond `keep`, keeping what is on screen where it is. Returns how many
    /// were dropped.
    pub fn trim(&mut self, keep: usize) -> usize {
        self.series.trim_front(keep)
    }

    /// Refits the price range to the visible bars when the scale is automatic.
    pub fn refit(&mut self) -> bool {
        let range = self.viewport.visible(self.series.len());
        self.scale.fit(&self.series.bars()[range])
    }

    /// Whether older bars should be fetched: the view is within a screenful of the oldest loaded.
    #[must_use]
    pub fn wants_older(&self) -> bool {
        !self.series.is_empty()
            && self
                .viewport
                .wants_older(self.series.len(), self.viewport.bars_across())
    }

    // ---- pointer ----

    /// The pointer moved to `(x, y)`. Returns whether the chart needs redrawing.
    pub fn hover(&mut self, x: f64, y: f64) -> bool {
        if self.drag.is_some() {
            return false;
        }
        let next = (self.region(x, y) == Region::Plot).then_some((x, y));
        let changed = next != self.pointer;
        self.pointer = next;
        changed
    }

    /// The pointer left the chart.
    pub fn leave(&mut self) -> bool {
        self.pointer.take().is_some()
    }

    /// What the crosshair points at, if it is shown.
    #[must_use]
    pub fn hover_info(&self) -> Option<Hover> {
        let (x, y) = self.pointer?;
        Some(Hover {
            x,
            y,
            price: self.scale.price_at(y, self.plot_height()),
            bar: self.viewport.bar_at(x, self.series.len()),
        })
    }

    // ---- wheel ----

    /// A wheel turn at `(x, y)` with a pixel delta (`dy` positive scrolling up, `dx` positive
    /// scrolling left) and the modifier keys. Returns whether the chart changed.
    pub fn wheel(&mut self, x: f64, y: f64, dx: f64, dy: f64, ctrl: bool, shift: bool) -> bool {
        let region = self.region(x, y);
        if region == Region::Outside || region == Region::Corner {
            return false;
        }
        let count = self.series.len();
        let height = self.plot_height();
        let zoom = (dy * WHEEL_ZOOM_RATE).exp();

        if region == Region::PriceAxis || (region == Region::Plot && ctrl) {
            self.scale.zoom_at(y.min(height), zoom, height);
        } else if shift || dx.abs() > dy.abs() {
            let along = if dx.abs() > dy.abs() { dx } else { dy };
            self.viewport.pan_by(along, count);
        } else {
            self.viewport.zoom_at(x.min(self.plot_width()), zoom, count);
        }
        self.refit();
        true
    }

    // ---- drag ----

    /// A button went down at `(x, y)`; `clicks` is 1 for a single click, 2 for a double click.
    /// Returns whether the chart changed.
    pub fn press(&mut self, x: f64, y: f64, clicks: usize) -> bool {
        let region = self.region(x, y);
        if clicks >= 2 {
            return self.double_click(region);
        }
        self.drag = match region {
            Region::Plot => Some(Drag::Pan {
                last: (x, y),
                origin_y: y,
                vertical: false,
            }),
            Region::PriceAxis => Some(Drag::PriceZoom { last_y: y }),
            Region::TimeAxis => Some(Drag::TimeZoom { last_x: x }),
            Region::Corner | Region::Outside => None,
        };
        // A press starts a drag: the crosshair steps aside until it ends.
        self.pointer.take().is_some()
    }

    fn double_click(&mut self, region: Region) -> bool {
        let count = self.series.len();
        match region {
            Region::PriceAxis => {
                self.scale.reset_auto();
                self.refit();
                true
            }
            Region::TimeAxis => {
                self.viewport.reset(count);
                self.refit();
                true
            }
            Region::Corner => {
                self.viewport.reset(count);
                self.scale.reset_auto();
                self.refit();
                true
            }
            Region::Plot | Region::Outside => false,
        }
    }

    /// The pointer moved to `(x, y)` with the button held. Returns whether the chart changed.
    pub fn drag_to(&mut self, x: f64, y: f64) -> bool {
        let count = self.series.len();
        let height = self.plot_height();
        let Some(drag) = self.drag else {
            return false;
        };
        match drag {
            Drag::Pan {
                last,
                origin_y,
                vertical,
            } => {
                let (dx, dy) = (x - last.0, y - last.1);
                let vertical = vertical || (y - origin_y).abs() >= VERTICAL_DEAD_ZONE;
                self.viewport.pan_by(dx, count);
                if vertical {
                    self.scale.pan_by(dy, height);
                }
                self.drag = Some(Drag::Pan {
                    last: (x, y),
                    origin_y,
                    vertical,
                });
            }
            Drag::PriceZoom { last_y } => {
                let factor = ((y - last_y) * AXIS_DRAG_RATE).exp();
                self.scale.zoom_at(height / 2.0, factor, height);
                self.drag = Some(Drag::PriceZoom { last_y: y });
            }
            Drag::TimeZoom { last_x } => {
                let factor = ((x - last_x) * AXIS_DRAG_RATE).exp();
                self.viewport.zoom_at_edge(factor, count);
                self.drag = Some(Drag::TimeZoom { last_x: x });
            }
        }
        self.refit();
        true
    }

    /// The button went up. Returns whether a drag ended.
    pub fn release(&mut self) -> bool {
        self.drag.take().is_some()
    }

    /// Whether a drag is in progress.
    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    // ---- keyboard ----

    /// A key. Returns whether the chart changed.
    pub fn key(&mut self, key: ChartKey) -> bool {
        let count = self.series.len();
        match key {
            ChartKey::Left => self.viewport.pan_by(self.viewport.width / 10.0, count),
            ChartKey::Right => self.viewport.pan_by(-self.viewport.width / 10.0, count),
            ChartKey::ZoomIn => self.viewport.zoom_at_edge(KEY_ZOOM, count),
            ChartKey::ZoomOut => self.viewport.zoom_at_edge(1.0 / KEY_ZOOM, count),
            ChartKey::Latest => self.viewport.scroll_to_latest(count),
            ChartKey::Reset => {
                self.viewport.reset(count);
                self.scale.reset_auto();
            }
        }
        self.refit();
        true
    }

    // ---- what to show ----

    /// The bar the legend describes: the one under the crosshair, else the newest.
    #[must_use]
    pub fn legend_bar(&self) -> Option<&Candle> {
        let hovered = self
            .hover_info()
            .and_then(|h| h.bar)
            .and_then(|i| self.series.bars().get(i));
        hovered.or_else(|| self.series.last())
    }

    /// Whether the price range follows the data.
    #[must_use]
    pub fn is_auto(&self) -> bool {
        self.scale.mode == ScaleMode::Auto
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;

    fn model_with(bars: usize) -> ChartModel {
        let mut m = ChartModel::new(Period::M1, 60.0, 24.0);
        m.set_size(860.0, 424.0, 60.0, 24.0); // plot 800 x 400
        m.merge((0..bars as i64).map(|i| {
            let p = 100.0 + (i % 20) as f64;
            Candle {
                time: i * MIN,
                open: p,
                high: p + 2.0,
                low: p - 2.0,
                close: p + 1.0,
                volume: 1.0,
            }
        }));
        m
    }

    #[test]
    fn regions_split_the_area() {
        let m = model_with(10);
        assert_eq!(m.region(100.0, 100.0), Region::Plot);
        assert_eq!(m.region(830.0, 100.0), Region::PriceAxis);
        assert_eq!(m.region(100.0, 410.0), Region::TimeAxis);
        assert_eq!(m.region(830.0, 410.0), Region::Corner);
        assert_eq!(m.region(-1.0, 5.0), Region::Outside);
        assert_eq!(m.region(100.0, 500.0), Region::Outside);
    }

    #[test]
    fn a_fresh_chart_lands_on_the_newest_bar_with_a_fitted_range() {
        let m = model_with(500);
        assert!(m.viewport.is_following());
        assert!(m.is_auto());
        assert!(m.scale.low < 98.0 && m.scale.high > 121.0);
    }

    #[test]
    fn the_wheel_zooms_time_around_the_pointer() {
        let mut m = model_with(500);
        let before = m.viewport.index_at(300.0, 500);
        assert!(m.wheel(300.0, 100.0, 0.0, 72.0, false, false));
        assert!(m.viewport.bar_spacing > 8.0);
        assert!((m.viewport.index_at(300.0, 500) - before).abs() < 1e-6);
        assert!(m.is_auto(), "a time zoom leaves the price range automatic");
    }

    #[test]
    fn scrolling_down_zooms_out() {
        let mut m = model_with(500);
        m.wheel(300.0, 100.0, 0.0, -72.0, false, false);
        assert!(m.viewport.bar_spacing < 8.0);
    }

    #[test]
    fn the_wheel_on_the_price_axis_zooms_the_price_and_goes_manual() {
        let mut m = model_with(500);
        let span = m.scale.span();
        assert!(m.wheel(830.0, 200.0, 0.0, 72.0, false, false));
        assert!(m.scale.span() < span);
        assert!(!m.is_auto());
        assert_eq!(m.viewport.bar_spacing, 8.0);
    }

    #[test]
    fn shift_and_sideways_wheels_scroll() {
        let mut m = model_with(500);
        let offset = m.viewport.right_offset;
        m.wheel(300.0, 100.0, 0.0, 40.0, false, true);
        assert!(
            m.viewport.right_offset < offset,
            "scrolled towards older bars"
        );
        let offset = m.viewport.right_offset;
        m.wheel(300.0, 100.0, -40.0, 0.0, false, false);
        assert!(m.viewport.right_offset > offset);
        assert_eq!(m.viewport.bar_spacing, 8.0, "no zoom happened");
    }

    #[test]
    fn dragging_the_plot_scrolls_and_stays_automatic_while_horizontal() {
        let mut m = model_with(500);
        m.press(400.0, 200.0, 1);
        assert!(m.is_dragging());
        m.drag_to(480.0, 201.0);
        assert!(m.viewport.right_offset < 0.0);
        assert!(m.is_auto(), "a wobble of one pixel is not a vertical drag");
        m.drag_to(480.0, 260.0);
        assert!(!m.is_auto(), "a real vertical drag frees the price range");
        assert!(m.release());
        assert!(!m.is_dragging());
    }

    #[test]
    fn dragging_the_price_axis_stretches_the_range() {
        let mut m = model_with(500);
        m.press(830.0, 100.0, 1);
        let span = m.scale.span();
        m.drag_to(830.0, 160.0);
        assert!(m.scale.span() < span, "down zooms in");
        m.drag_to(830.0, 40.0);
        assert!(m.scale.span() > span * 0.9);
        assert!(!m.is_auto());
    }

    #[test]
    fn dragging_the_time_axis_stretches_the_bars() {
        let mut m = model_with(500);
        m.press(300.0, 410.0, 1);
        m.drag_to(400.0, 410.0);
        assert!(m.viewport.bar_spacing > 8.0);
        m.drag_to(100.0, 410.0);
        assert!(m.viewport.bar_spacing < 8.0 * 1.7);
    }

    #[test]
    fn double_clicks_reset_what_belongs_to_the_axis() {
        let mut m = model_with(500);
        m.wheel(830.0, 200.0, 0.0, 200.0, false, false);
        m.wheel(300.0, 100.0, 0.0, 200.0, false, false);
        m.press(830.0, 100.0, 2);
        assert!(m.is_auto());
        assert!(
            m.viewport.bar_spacing > 8.0,
            "the price axis leaves the zoom"
        );
        m.press(300.0, 410.0, 2);
        assert_eq!(m.viewport.bar_spacing, 8.0);
        assert!(m.viewport.is_following());
    }

    #[test]
    fn the_crosshair_shows_in_the_plot_only_and_steps_aside_while_dragging() {
        let mut m = model_with(500);
        assert!(m.hover(400.0, 200.0));
        assert!(m.hover_info().is_some());
        assert!(!m.hover(400.0, 200.0), "same place, nothing to redraw");
        assert!(!m.hover(830.0, 200.0) || m.hover_info().is_none());
        m.hover(400.0, 200.0);
        m.press(400.0, 200.0, 1);
        assert!(m.hover_info().is_none());
        assert!(!m.hover(420.0, 200.0), "ignored during a drag");
        m.release();
        assert!(!m.leave());
    }

    #[test]
    fn the_crosshair_reads_the_bar_and_the_price() {
        let mut m = model_with(500);
        let x = m.viewport.x_of(499.0, 500);
        m.hover(x, 200.0);
        let info = m.hover_info().unwrap();
        assert_eq!(info.bar, Some(499));
        assert!((info.price - m.scale.price_at(200.0, 400.0)).abs() < 1e-9);
        assert_eq!(m.legend_bar().unwrap().time, 499 * MIN);
    }

    #[test]
    fn the_legend_shows_the_newest_bar_without_a_crosshair() {
        let m = model_with(50);
        assert_eq!(m.legend_bar().unwrap().time, 49 * MIN);
        assert!(
            ChartModel::new(Period::M1, 60.0, 24.0)
                .legend_bar()
                .is_none()
        );
    }

    #[test]
    fn a_live_price_follows_at_the_edge_and_leaves_a_scrolled_chart_alone() {
        let mut following = model_with(100);
        following.apply_price(150.0, 99 * MIN + 1);
        assert!(
            following.scale.high >= 150.0,
            "the range refits to the new high"
        );

        let mut scrolled = model_with(100);
        scrolled.press(400.0, 200.0, 1);
        scrolled.drag_to(700.0, 200.0);
        scrolled.release();
        let x = scrolled.viewport.x_of(50.0, 100);
        scrolled.apply_price(120.0, 100 * MIN + 5); // opens bar 100
        assert_eq!(scrolled.series.len(), 101);
        assert!(
            (scrolled.viewport.x_of(50.0, 101) - x).abs() < 1e-9,
            "bar 50 did not move"
        );
    }

    #[test]
    fn merged_history_keeps_the_view_still() {
        let mut m = model_with(300);
        m.press(400.0, 200.0, 1);
        m.drag_to(700.0, 200.0);
        m.release();
        let x = m.viewport.x_of(100.0, 300);
        let older: Vec<Candle> = (-200..0).map(|i| Candle::at(i * MIN, 100.0)).collect();
        assert!(m.merge(older));
        assert_eq!(m.series.len(), 500);
        assert!(
            (m.viewport.x_of(300.0, 500) - x).abs() < 1e-9,
            "old bar 100 is now 300"
        );
    }

    #[test]
    fn a_merge_that_changes_nothing_reports_nothing() {
        let mut m = model_with(50);
        let same: Vec<Candle> = m.series.bars().to_vec();
        assert!(!m.merge(same));
    }

    #[test]
    fn older_bars_are_wanted_only_near_the_left_end() {
        let mut m = model_with(5000);
        assert!(!m.wants_older());
        m.viewport.right_offset = -4850.0;
        m.viewport.clamp(5000);
        assert!(m.wants_older());
        assert!(!ChartModel::new(Period::M1, 60.0, 24.0).wants_older());
    }

    #[test]
    fn keys_scroll_zoom_and_reset() {
        let mut m = model_with(500);
        m.key(ChartKey::ZoomIn);
        assert!(m.viewport.bar_spacing > 8.0);
        m.key(ChartKey::Left);
        assert!(!m.viewport.is_following());
        m.key(ChartKey::Latest);
        assert!(m.viewport.is_following());
        m.scale.pan_by(10.0, 400.0);
        m.key(ChartKey::Reset);
        assert!(m.is_auto());
        assert_eq!(m.viewport.bar_spacing, 8.0);
    }

    #[test]
    fn resizing_keeps_working_and_refits() {
        let mut m = model_with(500);
        assert!(m.set_size(400.0, 300.0, 60.0, 24.0));
        assert_eq!(m.viewport.width, 340.0);
        assert!(
            !m.set_size(400.0, 300.0, 60.0, 24.0),
            "same size, nothing to do"
        );
        assert!(m.scale.high > m.scale.low);
    }

    #[test]
    fn clearing_starts_over() {
        let mut m = model_with(500);
        m.clear(Period::H1, 3);
        assert!(m.series.is_empty());
        assert_eq!((m.period, m.digits), (Period::H1, 3));
        assert!(m.is_auto());
        assert_eq!(m.viewport.bar_spacing, 8.0);
    }
}
