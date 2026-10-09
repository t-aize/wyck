//! Which part of the data is on screen: the horizontal window and the price scale.
//!
//! The horizontal axis counts points, not time. Each bar or tick takes the same width, so a
//! weekend without prices leaves no hole, as on any trading chart. Point `i` is centered at
//! `i + 0.5` in "index space", and the right edge of the plot sits at `len + offset`.

pub const MIN_BAR_PX: f64 = 0.2;
/// Wide enough for a footprint's numbers.
pub const MAX_BAR_PX: f64 = 260.0;
/// Points that must stay on screen when scrolled all the way back.
const KEEP_VISIBLE: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PriceScale {
    /// Fits the prices on screen.
    Auto,
    /// A range the user set, in raw price units.
    Manual { lo: f64, hi: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub bar_px: f64,
    /// Where the right edge of the plot is, in points past the last one. Negative when scrolled
    /// back in time.
    pub offset: f64,
    pub price: PriceScale,
}

impl View {
    pub fn new(bar_px: f64) -> Self {
        Self {
            bar_px,
            offset: Self::home_offset(),
            price: PriceScale::Auto,
        }
    }

    /// The offset that leaves a little air after the last point.
    pub fn home_offset() -> f64 {
        6.0
    }

    /// The index-space position of the right edge.
    pub fn right(&self, len: usize) -> f64 {
        len as f64 + self.offset
    }

    /// How many points the plot width holds.
    pub fn span(&self, plot_w: f64) -> f64 {
        plot_w / self.bar_px
    }

    /// The x of the center of point `index`, from the left of the plot.
    pub fn x_of(&self, index: f64, len: usize, plot_w: f64) -> f64 {
        plot_w - (self.right(len) - index - 0.5) * self.bar_px
    }

    /// The (fractional) point under `x`.
    pub fn index_at(&self, x: f64, len: usize, plot_w: f64) -> f64 {
        self.right(len) - (plot_w - x) / self.bar_px - 0.5
    }

    /// The points that reach the plot: `first..last`.
    pub fn visible(&self, len: usize, plot_w: f64) -> (usize, usize) {
        let right = self.right(len);
        let left = right - self.span(plot_w);
        let first = (left.floor() - 1.0).max(0.0) as usize;
        let last = ((right.ceil() + 1.0).max(0.0) as usize).min(len);
        (first.min(last), last)
    }

    /// Keeps the view inside what makes sense: a sane zoom, and some data always on screen.
    pub fn clamp(&mut self, len: usize, plot_w: f64) {
        self.bar_px = self.bar_px.clamp(MIN_BAR_PX, MAX_BAR_PX);
        let span = self.span(plot_w.max(1.0));
        let max_offset = span * 0.6;
        let min_right = KEEP_VISIBLE.min(len as f64);
        let min_offset = min_right - len as f64;
        self.offset = self.offset.min(max_offset).max(min_offset);
    }

    /// Scrolls by `dx` pixels: positive shows older points.
    pub fn pan(&mut self, dx: f64, len: usize, plot_w: f64) {
        self.offset -= dx / self.bar_px;
        self.clamp(len, plot_w);
    }

    /// Put a point at the middle of the plot without changing the zoom.
    pub fn center_on(&mut self, index: f64, len: usize, plot_w: f64) {
        self.offset = index + 0.5 + self.span(plot_w) / 2.0 - len as f64;
        self.clamp(len, plot_w);
    }

    /// Zooms by `factor` (above 1 magnifies) keeping the point under `anchor_x` where it is.
    pub fn zoom(&mut self, factor: f64, anchor_x: f64, len: usize, plot_w: f64) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let anchor = self.index_at(anchor_x, len, plot_w);
        self.bar_px = (self.bar_px * factor).clamp(MIN_BAR_PX, MAX_BAR_PX);
        let right = anchor + 0.5 + (plot_w - anchor_x) / self.bar_px;
        self.offset = right - len as f64;
        self.clamp(len, plot_w);
    }

    /// Whether the newest point is on screen: a new one should then scroll in, not be left off.
    pub fn is_following(&self) -> bool {
        self.offset > -1.0
    }

    /// Called when `added` points were put after the last: keeps what the user looks at in place
    /// unless they are following the newest point.
    pub fn on_appended(&mut self, added: usize) {
        if !self.is_following() {
            self.offset -= added as f64;
        }
    }

    pub fn jump_to_latest(&mut self) {
        self.offset = Self::home_offset();
    }
}

/// Widens a price range so it never collapses to a line and leaves room around the data.
/// `min_range` is the smallest range worth showing, in raw units, and `share` the room left on
/// each side as a share of the range.
pub fn padded_range(lo: f64, hi: f64, min_range: f64, share: f64) -> (f64, f64) {
    let mut range = hi - lo;
    let mut lo = lo;
    let mut hi = hi;
    if range < min_range {
        let mid = (lo + hi) / 2.0;
        lo = mid - min_range / 2.0;
        hi = mid + min_range / 2.0;
        range = min_range;
    }
    let pad = range * share;
    (lo - pad, hi + pad)
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f64 = 800.0;

    #[test]
    fn a_point_maps_to_x_and_back() {
        let view = View::new(8.0);
        for i in [0.0, 10.0, 99.5] {
            let x = view.x_of(i, 100, W);
            assert!((view.index_at(x, 100, W) - i).abs() < 1e-9);
        }
    }

    #[test]
    fn the_last_point_sits_left_of_the_right_edge_by_the_offset() {
        let view = View::new(10.0);
        let x = view.x_of(99.0, 100, W);
        // offset 6: the right edge is 6 points past the end, the last center is 6.5 points in.
        assert!((x - (W - 6.5 * 10.0)).abs() < 1e-9);
    }

    #[test]
    fn visible_points_are_bounded_by_the_data() {
        let view = View::new(8.0);
        let (first, last) = view.visible(50, W);
        assert_eq!((first, last), (0, 50));
        let (first, last) = view.visible(10_000, W);
        assert!(first > 9_000 && last == 10_000, "{first} {last}");
        assert_eq!(View::new(8.0).visible(0, W), (0, 0));
    }

    #[test]
    fn zoom_keeps_the_point_under_the_pointer() {
        let mut view = View::new(8.0);
        let len = 1_000;
        let anchor_x = 300.0;
        let before = view.index_at(anchor_x, len, W);
        view.zoom(1.5, anchor_x, len, W);
        let after = view.index_at(anchor_x, len, W);
        assert!((before - after).abs() < 1e-6, "{before} {after}");
        assert!(view.bar_px > 8.0);
    }

    #[test]
    fn centering_a_point_keeps_the_zoom() {
        let mut view = View::new(8.0);
        view.center_on(250.0, 1_000, W);
        assert!((view.x_of(250.0, 1_000, W) - W / 2.0).abs() < 1e-9);
        assert_eq!(view.bar_px, 8.0);
    }

    #[test]
    fn zoom_and_pan_stay_within_limits() {
        let mut view = View::new(8.0);
        view.zoom(1e9, 100.0, 500, W);
        assert!(view.bar_px <= MAX_BAR_PX);
        view.zoom(1e-9, 100.0, 500, W);
        assert!(view.bar_px >= MIN_BAR_PX);
        view.pan(1e9, 500, W);
        assert!(view.right(500) >= KEEP_VISIBLE);
        view.pan(-1e9, 500, W);
        assert!(view.offset <= view.span(W) * 0.6 + 1e-9);
        view.zoom(f64::NAN, 0.0, 500, W);
        assert!(view.bar_px.is_finite());
    }

    #[test]
    fn a_short_series_can_still_be_shown() {
        let mut view = View::new(8.0);
        view.pan(1e9, 2, W);
        assert!(view.right(2) >= 2.0 - 1e-9);
        assert_eq!(view.visible(2, W).1, 2);
    }

    #[test]
    fn following_the_end_survives_new_points_scrolling_back_does_not() {
        let mut view = View::new(8.0);
        let before = view.offset;
        view.on_appended(3);
        assert_eq!(view.offset, before, "following: the end stays in view");
        view.offset = -50.0;
        view.on_appended(2);
        assert_eq!(
            view.offset, -52.0,
            "scrolled back: the same points stay put"
        );
    }

    #[test]
    fn a_flat_range_is_widened() {
        let (lo, hi) = padded_range(100.0, 100.0, 10.0, 0.07);
        assert!(hi - lo > 10.0);
        assert!(lo < 100.0 && hi > 100.0);
        let (lo, hi) = padded_range(0.0, 100.0, 1.0, 0.07);
        assert!((lo + 7.0).abs() < 1e-9 && (hi - 107.0).abs() < 1e-9);
        let (lo, hi) = padded_range(0.0, 100.0, 1.0, 0.16);
        assert!((lo + 16.0).abs() < 1e-9 && (hi - 116.0).abs() < 1e-9);
    }
}
