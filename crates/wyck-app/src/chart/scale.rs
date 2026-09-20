//! The price scale: the vertical range of the chart and the grid lines that go with it.
//!
//! A [`PriceScale`] is either **auto** or **manual**:
//!
//! - In auto mode the range follows the bars on screen ([`PriceScale::fit`]): it is the lowest
//!   low to the highest high of the visible bars, with some air above and below. Scrolling or
//!   zooming horizontally moves the range by itself, which is what a trader expects.
//! - In manual mode the range stays where the user put it. Dragging the chart up or down, or
//!   zooming on the price axis, switches to manual; [`PriceScale::reset_auto`] (a double click on
//!   the axis) goes back.
//!
//! [`nice_ticks`] chooses the grid lines: a step of 1, 2 or 5 times a power of ten, so the labels
//! read as round numbers whatever the range is. [`format_price`] writes a price with the
//! decimals the symbol quotes.
//!
//! The range is stored as prices and mapped to pixels with [`PriceScale::y_of`] and
//! [`PriceScale::price_at`]. Height is passed in rather than stored, because the plot is resized
//! far more often than the range changes.

use wyck_engine::domain::Candle;

/// Air kept above the highest high and below the lowest low in auto mode, as a fraction of the
/// range.
const MARGIN: f64 = 0.08;

/// The smallest range the scale will show, as a fraction of the mid price. It stops a flat
/// market (or a single bar) from zooming in to nothing.
const MIN_RANGE_FRACTION: f64 = 1e-6;

/// The smallest span, in pixels, for which a grid line is worth drawing.
const TARGET_LINE_GAP: f64 = 56.0;

/// Whether the range follows the data or the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScaleMode {
    /// The range fits the visible bars.
    #[default]
    Auto,
    /// The range stays where the user put it.
    Manual,
}

/// The vertical range of the chart. See the [module docs](self).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriceScale {
    /// Auto or manual.
    pub mode: ScaleMode,
    /// The price at the bottom of the plot.
    pub low: f64,
    /// The price at the top of the plot.
    pub high: f64,
    /// Whether prices are laid out on a logarithmic axis: equal percentage moves take equal
    /// heights. It only applies while the whole range is positive (see [`PriceScale::is_log`]).
    pub log: bool,
}

impl Default for PriceScale {
    fn default() -> Self {
        Self {
            mode: ScaleMode::Auto,
            low: 0.0,
            high: 1.0,
            log: false,
        }
    }
}

impl PriceScale {
    /// The range, always positive.
    #[must_use]
    pub fn span(&self) -> f64 {
        (self.high - self.low).max(f64::MIN_POSITIVE)
    }

    /// Whether the axis is logarithmic right now: asked for, and every price in the range is
    /// positive (a logarithm of zero or less has no meaning).
    #[must_use]
    pub fn is_log(&self) -> bool {
        self.log && self.low > 0.0 && self.high > 0.0
    }

    /// Turns logarithmic layout on or off. Turning it on is refused (and `false` returned) when
    /// the range reaches zero or below.
    pub fn set_log(&mut self, on: bool) -> bool {
        if on && !(self.low > 0.0 && self.high > 0.0) {
            return false;
        }
        self.log = on;
        true
    }

    /// A price on the axis: itself, or its logarithm.
    fn forward(&self, price: f64) -> f64 {
        if self.is_log() {
            price.max(f64::MIN_POSITIVE).ln()
        } else {
            price
        }
    }

    /// The price at a position on the axis: the inverse of `forward`.
    fn inverse(&self, value: f64) -> f64 {
        if self.is_log() { value.exp() } else { value }
    }

    /// The length of the range on the axis.
    fn axis_span(&self) -> f64 {
        (self.forward(self.high) - self.forward(self.low)).max(f64::MIN_POSITIVE)
    }

    /// The y (from the top of the plot, in pixels) of `price` in a plot `height` pixels tall.
    #[must_use]
    pub fn y_of(&self, price: f64, height: f64) -> f64 {
        (self.forward(self.high) - self.forward(price)) / self.axis_span() * height
    }

    /// The price at pixel `y`: the inverse of [`PriceScale::y_of`].
    #[must_use]
    pub fn price_at(&self, y: f64, height: f64) -> f64 {
        self.inverse(self.forward(self.high) - y / height.max(1.0) * self.axis_span())
    }

    /// In auto mode, fits the range to `bars` (the visible ones); in manual mode, does nothing.
    /// With no bars the range is left as it is. Returns whether it changed.
    pub fn fit(&mut self, bars: &[Candle]) -> bool {
        if self.mode == ScaleMode::Manual {
            return false;
        }
        let Some((lo, hi)) = extremes(bars) else {
            return false;
        };
        let (low, high) = if self.log && lo > 0.0 {
            // Air is added in log space so the margins are the same share of the height.
            let (a, b) = padded(lo.ln(), hi.ln());
            (a.exp(), b.exp())
        } else {
            padded(lo, hi)
        };
        let changed = (low, high) != (self.low, self.high);
        self.low = low;
        self.high = high;
        changed
    }

    /// Scales the range by `factor` around the price at pixel `anchor_y` (above 1 zooms in, so
    /// the range gets smaller), which stays under the pointer. Switches to manual.
    pub fn zoom_at(&mut self, anchor_y: f64, factor: f64, height: f64) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let anchor = self.forward(self.price_at(anchor_y, height));
        let below = (anchor - self.forward(self.low)) / self.axis_span();
        let floor = MIN_RANGE_FRACTION * anchor.abs().max(1.0);
        let span = (self.axis_span() / factor).max(floor);
        let low = anchor - below * span;
        // Both ends are computed from the axis values before either is stored, because
        // `forward` and `inverse` depend on the range.
        let (new_low, new_high) = (self.inverse(low), self.inverse(low + span));
        self.low = new_low;
        self.high = new_high;
        self.mode = ScaleMode::Manual;
    }

    /// Moves the range by `dy` pixels: a positive value drags the chart down, so the range
    /// moves up. Switches to manual.
    pub fn pan_by(&mut self, dy: f64, height: f64) {
        if !dy.is_finite() {
            return;
        }
        let shift = dy / height.max(1.0) * self.axis_span();
        let (low, high) = (
            self.forward(self.low) + shift,
            self.forward(self.high) + shift,
        );
        let (new_low, new_high) = (self.inverse(low), self.inverse(high));
        self.low = new_low;
        self.high = new_high;
        self.mode = ScaleMode::Manual;
    }

    /// Goes back to auto mode. The range is refitted by the next [`PriceScale::fit`].
    pub fn reset_auto(&mut self) {
        self.mode = ScaleMode::Auto;
    }

    /// The grid prices for a plot `height` pixels tall and the decimals their labels need, given
    /// the `symbol_digits` the symbol quotes.
    ///
    /// A linear axis gets evenly spaced round steps ([`nice_ticks`]). A logarithmic one gets round
    /// values that thin out as prices rise (1, 2, 5, 10, 20, 50 and so on), which is how such an
    /// axis is read; when the range is too narrow for that to give a grid (a few percent), the
    /// linear grid is used, since the two look alike there.
    #[must_use]
    pub fn grid(&self, height: f64, symbol_digits: u32) -> (Vec<f64>, u32) {
        if self.is_log() {
            let ticks = log_ticks(self.low, self.high, height);
            if ticks.len() >= 3 {
                let smallest = ticks.iter().copied().fold(f64::MAX, f64::min);
                return (ticks, label_digits(smallest, symbol_digits));
            }
        }
        let step = nice_step(self.span(), height);
        (
            nice_ticks(self.low, self.high, height),
            label_digits(step, symbol_digits),
        )
    }
}

/// Round grid prices for a logarithmic axis: 1, 2 and 5 times a power of ten, with every digit
/// added when there is room, and none closer than 26 pixels to the one before.
#[must_use]
pub fn log_ticks(low: f64, high: f64, height: f64) -> Vec<f64> {
    if !(low > 0.0 && high > low && high.is_finite()) {
        return Vec::new();
    }
    let decades = (high / low).log10();
    let per_decade = height / decades.max(1e-9);
    let digits: &[f64] = if per_decade > 320.0 {
        &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]
    } else if per_decade > 130.0 {
        &[1.0, 2.0, 5.0]
    } else {
        &[1.0]
    };
    let first = low.log10().floor() as i32;
    let last = high.log10().ceil() as i32;
    if last - first > 60 {
        return Vec::new();
    }
    let mut ticks: Vec<f64> = Vec::new();
    for power in first..=last {
        for digit in digits {
            let price = digit * 10f64.powi(power);
            if price >= low && price <= high {
                ticks.push(price);
            }
        }
    }
    // Drop a tick that crowds the previous one, keeping the rounder (smaller digit) value.
    let mut kept: Vec<f64> = Vec::new();
    for price in ticks {
        let crowded = kept
            .last()
            .is_some_and(|last| (price / last).ln().abs() / (high / low).ln() * height < 26.0);
        if !crowded {
            kept.push(price);
        }
    }
    kept
}

/// The lowest low and highest high of `bars`, or `None` when there are none.
#[must_use]
pub fn extremes(bars: &[Candle]) -> Option<(f64, f64)> {
    let mut it = bars.iter();
    let first = it.next()?;
    Some(it.fold((first.low, first.high), |(lo, hi), bar| {
        (lo.min(bar.low), hi.max(bar.high))
    }))
}

/// `lo` to `hi` with air on both sides, and never a zero-height range.
fn padded(lo: f64, hi: f64) -> (f64, f64) {
    let mid = (lo + hi) / 2.0;
    let floor = MIN_RANGE_FRACTION * mid.abs().max(1.0);
    let span = (hi - lo).max(floor);
    let pad = span * MARGIN;
    (mid - span / 2.0 - pad, mid + span / 2.0 + pad)
}

/// The grid step for a range: 1, 2 or 5 times a power of ten, chosen so about one line falls
/// every 56 pixels of a plot `height` pixels tall.
#[must_use]
pub fn nice_step(span: f64, height: f64) -> f64 {
    let lines = (height / TARGET_LINE_GAP).max(2.0);
    let raw = span / lines;
    if !(raw.is_finite() && raw > 0.0) {
        return 1.0;
    }
    let power = 10f64.powf(raw.log10().floor());
    let unit = raw / power;
    let nice = if unit <= 1.0 {
        1.0
    } else if unit <= 2.0 {
        2.0
    } else if unit <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * power
}

/// The grid prices inside `[low, high]`, on multiples of the step from [`nice_step`].
#[must_use]
pub fn nice_ticks(low: f64, high: f64, height: f64) -> Vec<f64> {
    let step = nice_step(high - low, height);
    let first = (low / step).ceil();
    let last = (high / step).floor();
    if !(first.is_finite() && last.is_finite()) || last < first || last - first > 200.0 {
        return Vec::new();
    }
    (first as i64..=last as i64)
        .map(|i| i as f64 * step)
        .collect()
}

/// A price with `digits` decimals, the way the symbol quotes it. Grid labels use fewer when the
/// step is coarse, so a grid of whole numbers is not written `1.08000`; pass the decimals the
/// label needs from [`label_digits`].
#[must_use]
pub fn format_price(price: f64, digits: u32) -> String {
    format!("{price:.*}", digits as usize)
}

/// The decimals a grid label needs: enough to show the step exactly, never more than the symbol
/// quotes.
#[must_use]
pub fn label_digits(step: f64, symbol_digits: u32) -> u32 {
    if !(step.is_finite() && step > 0.0) {
        return symbol_digits;
    }
    let needed = (-step.log10().floor()).max(0.0) as u32;
    // A step of 2.5e-4 style never occurs (steps are 1, 2, 5), so one decimal covers the mantissa
    // only when the step is below one; whole steps need none.
    needed.min(symbol_digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(low: f64, high: f64) -> Candle {
        Candle {
            time: 0,
            open: low,
            high,
            low,
            close: high,
            volume: 0.0,
        }
    }

    #[test]
    fn auto_fits_the_visible_bars_with_air() {
        let mut scale = PriceScale::default();
        assert!(scale.fit(&[bar(1.0, 2.0), bar(1.2, 3.0)]));
        assert!(scale.low < 1.0 && scale.high > 3.0);
        assert!((scale.high - scale.low - 2.0 * 1.16).abs() < 1e-9);
        assert!(
            !scale.fit(&[bar(1.0, 2.0), bar(1.2, 3.0)]),
            "same data, no change"
        );
    }

    #[test]
    fn manual_ignores_the_data_until_reset() {
        let mut scale = PriceScale::default();
        scale.fit(&[bar(1.0, 2.0)]);
        scale.pan_by(10.0, 400.0);
        assert_eq!(scale.mode, ScaleMode::Manual);
        let kept = scale;
        assert!(!scale.fit(&[bar(50.0, 60.0)]));
        assert_eq!(scale, kept);
        scale.reset_auto();
        assert!(scale.fit(&[bar(50.0, 60.0)]));
        assert!(scale.low < 50.0 && scale.high > 60.0);
    }

    #[test]
    fn a_flat_market_still_has_a_range() {
        let mut scale = PriceScale::default();
        scale.fit(&[bar(1.5, 1.5)]);
        assert!(scale.high > scale.low);
    }

    #[test]
    fn no_bars_leave_the_range_alone() {
        let mut scale = PriceScale::default();
        assert!(!scale.fit(&[]));
        assert_eq!((scale.low, scale.high), (0.0, 1.0));
    }

    #[test]
    fn price_and_pixel_are_inverses() {
        let scale = PriceScale {
            mode: ScaleMode::Auto,
            low: 10.0,
            high: 20.0,
            log: false,
        };
        assert_eq!(scale.y_of(20.0, 500.0), 0.0);
        assert_eq!(scale.y_of(10.0, 500.0), 500.0);
        let p = scale.price_at(125.0, 500.0);
        assert!((scale.y_of(p, 500.0) - 125.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_keeps_the_anchor_price_under_the_pointer() {
        let mut scale = PriceScale {
            mode: ScaleMode::Auto,
            low: 10.0,
            high: 20.0,
            log: false,
        };
        let before = scale.price_at(100.0, 500.0);
        scale.zoom_at(100.0, 2.0, 500.0);
        assert!((scale.price_at(100.0, 500.0) - before).abs() < 1e-9);
        assert!((scale.span() - 5.0).abs() < 1e-9);
        assert_eq!(scale.mode, ScaleMode::Manual);
    }

    #[test]
    fn zoom_cannot_collapse_the_range() {
        let mut scale = PriceScale {
            mode: ScaleMode::Auto,
            low: 1.0,
            high: 2.0,
            log: false,
        };
        for _ in 0..200 {
            scale.zoom_at(250.0, 10.0, 500.0);
        }
        assert!(scale.high > scale.low);
        scale.zoom_at(250.0, f64::NAN, 500.0);
        assert!(scale.high.is_finite());
    }

    #[test]
    fn dragging_down_moves_the_range_up() {
        let mut scale = PriceScale {
            mode: ScaleMode::Auto,
            low: 0.0,
            high: 10.0,
            log: false,
        };
        scale.pan_by(50.0, 500.0);
        assert!((scale.low - 1.0).abs() < 1e-9 && (scale.high - 11.0).abs() < 1e-9);
    }

    #[test]
    fn steps_are_one_two_or_five_times_a_power_of_ten() {
        for span in [0.0007, 0.013, 0.4, 3.0, 87.0, 12_345.0] {
            let step = nice_step(span, 500.0);
            let mantissa = step / 10f64.powf(step.log10().floor());
            assert!(
                [1.0, 2.0, 5.0, 10.0]
                    .iter()
                    .any(|m| (mantissa - m).abs() < 1e-9),
                "{step} for {span}"
            );
        }
    }

    #[test]
    fn ticks_lie_inside_the_range_on_round_numbers() {
        let ticks = nice_ticks(1.0834, 1.0871, 500.0);
        assert!(!ticks.is_empty());
        assert!(ticks.iter().all(|t| (1.0834..=1.0871).contains(t)));
        let step = ticks[1] - ticks[0];
        assert!(
            ticks
                .windows(2)
                .all(|w| ((w[1] - w[0]) - step).abs() < 1e-9)
        );
    }

    #[test]
    fn a_bad_range_gives_no_ticks() {
        assert!(nice_ticks(f64::NAN, 1.0, 500.0).is_empty());
        assert!(nice_ticks(2.0, 1.0, 500.0).is_empty());
    }

    #[test]
    fn prices_are_written_with_the_symbol_digits() {
        assert_eq!(format_price(1.08, 5), "1.08000");
        assert_eq!(format_price(156.7684, 3), "156.768");
        assert_eq!(format_price(2650.0, 0), "2650");
    }

    #[test]
    fn labels_use_as_many_decimals_as_the_step_needs() {
        assert_eq!(label_digits(0.0005, 5), 4);
        assert_eq!(label_digits(0.00002, 5), 5);
        assert_eq!(label_digits(0.0000001, 5), 5, "never more than the symbol");
        assert_eq!(label_digits(10.0, 5), 0);
        assert_eq!(label_digits(0.5, 5), 1);
    }
}

#[cfg(test)]
mod log_tests {
    use super::*;

    fn log_scale(low: f64, high: f64) -> PriceScale {
        PriceScale {
            mode: ScaleMode::Auto,
            low,
            high,
            log: true,
        }
    }

    #[test]
    fn equal_percentages_take_equal_heights() {
        let scale = log_scale(1.0, 100.0);
        let a = scale.y_of(1.0, 400.0) - scale.y_of(10.0, 400.0);
        let b = scale.y_of(10.0, 400.0) - scale.y_of(100.0, 400.0);
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
        assert!((scale.y_of(10.0, 400.0) - 200.0).abs() < 1e-9, "half way");
    }

    #[test]
    fn price_and_pixel_are_inverses_on_a_log_axis() {
        let scale = log_scale(50.0, 5000.0);
        let p = scale.price_at(123.0, 500.0);
        assert!((scale.y_of(p, 500.0) - 123.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_and_pan_keep_working_in_log_space() {
        let mut scale = log_scale(10.0, 1000.0);
        let before = scale.price_at(100.0, 500.0);
        scale.zoom_at(100.0, 2.0, 500.0);
        assert!((scale.price_at(100.0, 500.0) - before).abs() < 1e-6 * before);
        assert!(scale.low > 0.0 && scale.high > scale.low);
        let ratio = scale.high / scale.low;
        scale.pan_by(50.0, 500.0);
        assert!(
            (scale.high / scale.low - ratio).abs() < 1e-9 * ratio,
            "the shape holds"
        );
    }

    #[test]
    fn a_log_fit_pads_in_log_space() {
        let mut scale = PriceScale {
            log: true,
            ..PriceScale::default()
        };
        let bar = |low: f64, high: f64| Candle {
            time: 0,
            open: low,
            high,
            low,
            close: high,
            volume: 0.0,
        };
        scale.fit(&[bar(10.0, 1000.0)]);
        let below = (10.0 / scale.low).ln();
        let above = (scale.high / 1000.0).ln();
        assert!((below - above).abs() < 1e-9);
    }

    #[test]
    fn log_is_refused_and_ignored_for_a_range_that_reaches_zero() {
        let mut scale = PriceScale {
            low: -5.0,
            high: 5.0,
            ..PriceScale::default()
        };
        assert!(!scale.set_log(true));
        scale.log = true;
        assert!(!scale.is_log(), "asked for, but not meaningful");
        assert!((scale.y_of(0.0, 100.0) - 50.0).abs() < 1e-9, "drawn linear");
    }

    #[test]
    fn a_wide_log_range_gets_decade_grid_lines() {
        let ticks = log_ticks(1.0, 10_000.0, 400.0);
        assert!(ticks.contains(&1.0) && ticks.contains(&10.0) && ticks.contains(&1000.0));
        assert!(ticks.iter().all(|t| (1.0..=10_000.0).contains(t)));
    }

    #[test]
    fn a_narrow_log_range_falls_back_to_the_linear_grid() {
        let scale = log_scale(29_000.0, 29_700.0);
        let (ticks, digits) = scale.grid(500.0, 1);
        assert!(ticks.len() >= 3);
        assert!(ticks.windows(2).all(|w| w[1] > w[0]));
        assert!(digits <= 1);
    }

    #[test]
    fn grid_digits_follow_the_step() {
        let scale = PriceScale {
            mode: ScaleMode::Auto,
            low: 1.0834,
            high: 1.0871,
            log: false,
        };
        let (ticks, digits) = scale.grid(500.0, 5);
        assert!(!ticks.is_empty());
        assert!(digits >= 4);
    }
}
