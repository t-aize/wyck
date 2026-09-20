//! Which bars are on screen, and where.
//!
//! A [`Viewport`] is two numbers and a width. `bar_spacing` is the horizontal zoom: how many
//! pixels one bar takes. `right_offset` is the pan: how many bars of empty space sit to the
//! right of the newest bar (negative when the newest bars have been pushed off screen). The
//! plot width is set from the layout.
//!
//! Everything is measured **from the right edge**, the way trading platforms do it:
//!
//! ```text
//! x(i) = width - (right_offset + 0.5 + (n - 1 - i)) * bar_spacing
//! ```
//!
//! for the centre of bar `i` out of `n`. This choice has two useful consequences. Bars added at
//! the front (older history arriving) change nothing on screen, because their distance to the
//! right edge is unchanged. And bars added at the end need one adjustment, done by
//! [`Viewport::bars_appended`], so a chart the user has scrolled back stays where it is while a
//! chart that follows the live edge keeps following.
//!
//! All the pixel maths lives here and is pure: it takes the bar count as an argument instead of
//! holding the data, so it is tested with plain numbers.

/// The narrowest a bar can be squeezed, in pixels. Below one pixel per bar the chart shows one
/// column per pixel anyway (see the drawing code's level of detail), so more zoom-out buys
/// nothing but speed loss.
pub const MIN_SPACING: f64 = 0.1;

/// The widest a bar can be stretched, in pixels.
pub const MAX_SPACING: f64 = 80.0;

/// The zoom a fresh chart starts at.
pub const DEFAULT_SPACING: f64 = 8.0;

/// Empty bars kept to the right of the newest bar on a fresh chart or after a reset.
pub const DEFAULT_RIGHT_OFFSET: f64 = 6.0;

/// How many bars of the data must stay on screen when panning away, so the user cannot lose the
/// chart entirely past either end.
const MIN_BARS_KEPT: f64 = 2.0;

/// Fraction of the plot width that may be left empty to the right of the newest bar.
const MAX_EMPTY_RIGHT: f64 = 0.7;

/// How zoomed and how panned the chart is. See the [module docs](self).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Pixels per bar.
    pub bar_spacing: f64,
    /// Empty bars right of the newest bar; negative when the newest bars are off screen.
    pub right_offset: f64,
    /// The plot's width in pixels.
    pub width: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            bar_spacing: DEFAULT_SPACING,
            right_offset: DEFAULT_RIGHT_OFFSET,
            width: 800.0,
        }
    }
}

impl Viewport {
    /// A viewport of the default zoom for a plot `width` pixels wide.
    #[must_use]
    pub fn new(width: f64) -> Self {
        Self {
            width: width.max(1.0),
            ..Self::default()
        }
    }

    /// The x of the centre of bar `index` among `count` bars. `index` may be fractional or lie
    /// outside `0..count` (a position left of the first bar, or in the empty space at the right).
    #[must_use]
    pub fn x_of(&self, index: f64, count: usize) -> f64 {
        let from_last = count as f64 - 1.0 - index;
        self.width - (self.right_offset + 0.5 + from_last) * self.bar_spacing
    }

    /// The (fractional) index at pixel `x`: the inverse of [`Viewport::x_of`].
    #[must_use]
    pub fn index_at(&self, x: f64, count: usize) -> f64 {
        let from_last = (self.width - x) / self.bar_spacing - self.right_offset - 0.5;
        count as f64 - 1.0 - from_last
    }

    /// The bar under pixel `x`, if there is one (`x` may fall in the empty space on either side
    /// of the data).
    #[must_use]
    pub fn bar_at(&self, x: f64, count: usize) -> Option<usize> {
        let index = self.index_at(x, count).round();
        (index >= 0.0 && index < count as f64).then_some(index as usize)
    }

    /// The bars that touch the plot, as a half open index range, with one bar of margin on each
    /// side so a bar half off the edge is still drawn.
    #[must_use]
    pub fn visible(&self, count: usize) -> std::ops::Range<usize> {
        if count == 0 {
            return 0..0;
        }
        let first = self.index_at(0.0, count).floor() - 1.0;
        let last = self.index_at(self.width, count).ceil() + 1.0;
        let first = first.clamp(0.0, count as f64) as usize;
        let end = (last + 1.0).clamp(0.0, count as f64) as usize;
        first.min(end)..end
    }

    /// How many bars fit across the plot.
    #[must_use]
    pub fn bars_across(&self) -> f64 {
        self.width / self.bar_spacing
    }

    /// Whether the chart follows the live edge: the newest bar is on screen with room to spare.
    #[must_use]
    pub fn is_following(&self) -> bool {
        self.right_offset >= 0.0
    }

    /// Zooms by `factor` (above 1 stretches the bars) keeping the bar under pixel `anchor_x`
    /// where it is. The spacing is clamped; the result is then re-clamped for panning.
    pub fn zoom_at(&mut self, anchor_x: f64, factor: f64, count: usize) {
        if !(factor.is_finite() && factor > 0.0) || count == 0 {
            return;
        }
        let anchor_index = self.index_at(anchor_x, count);
        self.bar_spacing = (self.bar_spacing * factor).clamp(MIN_SPACING, MAX_SPACING);
        // Solve x_of(anchor_index) == anchor_x for the offset at the new spacing.
        let from_last = count as f64 - 1.0 - anchor_index;
        self.right_offset = (self.width - anchor_x) / self.bar_spacing - 0.5 - from_last;
        self.clamp(count);
    }

    /// Zooms around the right edge, keeping the newest bar where it is. This is what the
    /// keyboard zoom does, and what a zoom does while following the live edge.
    pub fn zoom_at_edge(&mut self, factor: f64, count: usize) {
        self.zoom_at(self.width, factor, count);
    }

    /// Pans by `dx` pixels: a positive value drags the bars to the right (towards older data).
    pub fn pan_by(&mut self, dx: f64, count: usize) {
        if !dx.is_finite() {
            return;
        }
        self.right_offset -= dx / self.bar_spacing;
        self.clamp(count);
    }

    /// Scrolls to the live edge with the default gap on the right, keeping the zoom.
    pub fn scroll_to_latest(&mut self, count: usize) {
        self.right_offset = DEFAULT_RIGHT_OFFSET;
        self.clamp(count);
    }

    /// Back to the default zoom and the live edge.
    pub fn reset(&mut self, count: usize) {
        self.bar_spacing = DEFAULT_SPACING;
        self.scroll_to_latest(count);
    }

    /// Sets the plot width. A chart that follows the live edge keeps its right edge; a scrolled
    /// one keeps the bars it shows, which the right-anchored maths already gives.
    pub fn set_width(&mut self, width: f64, count: usize) {
        self.width = width.max(1.0);
        self.clamp(count);
    }

    /// Accounts for `added` bars appended after the newest one. A chart that follows the live
    /// edge follows; a scrolled back chart keeps showing the same bars.
    pub fn bars_appended(&mut self, added: usize) {
        if !self.is_following() {
            self.right_offset -= added as f64;
        }
    }

    /// Whether the user has scrolled close enough to the oldest loaded bar that older history
    /// should be fetched: fewer than `margin` bars remain to the left of the screen.
    #[must_use]
    pub fn wants_older(&self, count: usize, margin: f64) -> bool {
        count == 0 || self.index_at(0.0, count) < margin
    }

    /// Keeps the chart reachable: some of the data stays on screen at both ends.
    pub fn clamp(&mut self, count: usize) {
        self.bar_spacing = self.bar_spacing.clamp(MIN_SPACING, MAX_SPACING);
        if count == 0 {
            self.right_offset = DEFAULT_RIGHT_OFFSET;
            return;
        }
        let max_offset = self.bars_across() * MAX_EMPTY_RIGHT;
        // Left: the oldest bar may be dragged in until only MIN_BARS_KEPT bars are left between it
        // and the right edge, so the data can never be pushed out of reach.
        let min_offset = (MIN_BARS_KEPT.min(count as f64) - count as f64).min(max_offset);
        self.right_offset = self.right_offset.clamp(min_offset, max_offset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> Viewport {
        Viewport {
            bar_spacing: 10.0,
            right_offset: 0.0,
            width: 1000.0,
        }
    }

    #[test]
    fn the_newest_bar_sits_at_the_right_edge() {
        let v = view();
        assert_eq!(v.x_of(99.0, 100), 995.0);
        assert_eq!(v.x_of(98.0, 100), 985.0);
    }

    #[test]
    fn x_and_index_are_inverses() {
        let v = Viewport {
            bar_spacing: 7.5,
            right_offset: 3.25,
            width: 640.0,
        };
        for i in [0.0, 17.0, 42.5, 99.0] {
            let x = v.x_of(i, 100);
            assert!((v.index_at(x, 100) - i).abs() < 1e-9);
        }
    }

    #[test]
    fn a_bar_is_found_under_the_pointer_only_inside_the_data() {
        let v = view();
        assert_eq!(v.bar_at(995.0, 100), Some(99));
        assert_eq!(v.bar_at(0.0, 5), None, "far left of a short series");
        let empty_right = Viewport {
            right_offset: 5.0,
            ..view()
        };
        assert_eq!(empty_right.bar_at(990.0, 100), None, "the empty gap");
    }

    #[test]
    fn the_visible_range_covers_the_plot_with_a_margin() {
        let v = view();
        let r = v.visible(1000);
        assert_eq!(r.end, 1000);
        assert!(r.start <= 1000 - 100 && r.start >= 1000 - 103);
        assert_eq!(v.visible(0), 0..0);
        assert_eq!(v.visible(10), 0..10, "a short series is entirely visible");
    }

    #[test]
    fn zoom_keeps_the_bar_under_the_pointer_still() {
        let mut v = view();
        let x = 400.0;
        let before = v.index_at(x, 500);
        v.zoom_at(x, 1.5, 500);
        assert!((v.index_at(x, 500) - before).abs() < 1e-6);
        assert!((v.bar_spacing - 15.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut v = view();
        v.zoom_at(500.0, 1e9, 500);
        assert_eq!(v.bar_spacing, MAX_SPACING);
        v.zoom_at(500.0, 1e-9, 500);
        assert_eq!(v.bar_spacing, MIN_SPACING);
        let before = v;
        v.zoom_at(500.0, f64::NAN, 500);
        v.zoom_at(500.0, -2.0, 500);
        assert_eq!(v, before, "nonsense factors do nothing");
    }

    #[test]
    fn dragging_right_shows_older_bars() {
        let mut v = view();
        v.pan_by(100.0, 1000);
        assert!(v.right_offset < 0.0 || v.index_at(1000.0, 1000) < 999.0);
        assert!((v.right_offset - -10.0).abs() < 1e-9);
        assert!(!v.is_following());
    }

    #[test]
    fn panning_cannot_lose_the_data() {
        let mut v = view();
        v.pan_by(-1e9, 1000);
        assert!(v.right_offset <= v.bars_across() * MAX_EMPTY_RIGHT + 1e-9);
        v.pan_by(1e9, 1000);
        assert!(
            v.x_of(0.0, 1000) <= v.width,
            "the oldest bar stays on screen"
        );
        assert!(v.visible(1000).len() >= 2);
    }

    #[test]
    fn a_short_series_stays_visible_when_panned() {
        let mut v = view();
        v.pan_by(1e9, 3);
        assert!(v.x_of(0.0, 3) <= v.width && v.x_of(0.0, 3) >= 0.0);
    }

    #[test]
    fn appending_follows_only_at_the_live_edge() {
        let mut following = view();
        following.bars_appended(3);
        assert_eq!(following.right_offset, 0.0);

        let mut scrolled = view();
        scrolled.right_offset = -20.0;
        let x_before = scrolled.x_of(500.0, 1000);
        scrolled.bars_appended(2);
        assert_eq!(
            scrolled.x_of(500.0, 1002),
            x_before,
            "the same bar, same spot"
        );
    }

    #[test]
    fn prepending_history_moves_nothing() {
        let v = Viewport {
            right_offset: -5.0,
            ..view()
        };
        // Bar 500 of 1000 becomes bar 700 of 1200 after 200 older bars arrive.
        assert_eq!(v.x_of(500.0, 1000), v.x_of(700.0, 1200));
    }

    #[test]
    fn older_history_is_wanted_near_the_left_end() {
        let mut v = view();
        assert!(!v.wants_older(10_000, 50.0));
        v.right_offset = -(10_000.0 - 100.0);
        v.clamp(10_000);
        assert!(v.wants_older(10_000, 150.0));
        assert!(v.wants_older(0, 50.0));
    }

    #[test]
    fn reset_and_latest() {
        let mut v = view();
        v.pan_by(500.0, 1000);
        v.zoom_at(200.0, 3.0, 1000);
        v.scroll_to_latest(1000);
        assert_eq!(v.right_offset, DEFAULT_RIGHT_OFFSET);
        v.reset(1000);
        assert_eq!(v.bar_spacing, DEFAULT_SPACING);
    }
}
