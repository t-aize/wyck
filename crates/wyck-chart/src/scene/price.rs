//! The price scale of a band: which values are on screen, how they map to heights, where the
//! labels go and what they say.
//!
//! The prices can be spaced evenly (regular), by ratio (logarithmic, so a 10 percent move is the
//! same height at any price), or evenly but labelled as a percentage change or an index of 100
//! from the first bar on screen. The scale can also be turned upside down.

use wyck_openapi_model::market::format_price;

use super::super::axis;
use super::super::settings::ScaleMode;
use super::super::study::ValueFormat;

/// Prices under this are clamped before taking their logarithm.
const LOG_FLOOR: f64 = 1e-9;

/// How a band maps values to heights (from the top of the chart).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriceMap {
    pub lo: f64,
    pub hi: f64,
    pub top: f64,
    pub bottom: f64,
    pub mode: ScaleMode,
    pub invert: bool,
    /// The price that reads 0% (or 100): the close of the first bar on screen.
    pub base: f64,
}

impl PriceMap {
    /// A plain linear map, for indicator panes.
    pub fn linear(lo: f64, hi: f64, top: f64, bottom: f64) -> Self {
        Self {
            lo,
            hi,
            top,
            bottom,
            mode: ScaleMode::Linear,
            invert: false,
            base: 0.0,
        }
    }

    fn is_log(&self) -> bool {
        self.mode == ScaleMode::Log
    }

    /// A value in the space the scale is even in.
    fn t(&self, value: f64) -> f64 {
        if self.is_log() {
            value.max(LOG_FLOOR).ln()
        } else {
            value
        }
    }

    fn untransform(&self, t: f64) -> f64 {
        if self.is_log() { t.exp() } else { t }
    }

    /// The share of the way from the bottom of the range to its top.
    fn fraction(&self, value: f64) -> f64 {
        let (a, b) = (self.t(self.lo), self.t(self.hi));
        (self.t(value) - a) / (b - a)
    }

    pub fn y(&self, value: f64) -> f64 {
        let f = self.fraction(value);
        let span = self.bottom - self.top;
        if self.invert {
            self.top + f * span
        } else {
            self.bottom - f * span
        }
    }

    pub fn price(&self, y: f64) -> f64 {
        let span = self.bottom - self.top;
        let f = if self.invert {
            (y - self.top) / span
        } else {
            (self.bottom - y) / span
        };
        let (a, b) = (self.t(self.lo), self.t(self.hi));
        self.untransform(a + f * (b - a))
    }

    /// The range after zooming by `factor` (above 1 magnifies) about the value at `anchor_y`, or
    /// about the middle without one. Zooms in the scale's own space, so a log scale stays even.
    pub fn zoomed(&self, factor: f64, anchor_y: Option<f64>) -> (f64, f64) {
        let factor = factor.clamp(0.05, 20.0);
        let (a, b) = (self.t(self.lo), self.t(self.hi));
        let anchor = anchor_y.map_or((a + b) / 2.0, |y| self.t(self.price(y)));
        (
            self.untransform(anchor - (anchor - a) / factor),
            self.untransform(anchor + (b - anchor) / factor),
        )
    }

    /// The range after the prices were dragged down by `dy` pixels (so higher ones show).
    pub fn panned(&self, dy: f64) -> (f64, f64) {
        let (a, b) = (self.t(self.lo), self.t(self.hi));
        let mut shift = dy / (self.bottom - self.top) * (b - a);
        if self.invert {
            shift = -shift;
        }
        (self.untransform(a + shift), self.untransform(b + shift))
    }

    /// The values to label, about `target` of them. `min_step` is the smallest step that still
    /// means something (one unit of the symbol's last decimal).
    pub fn ticks(&self, target: usize, min_step: f64) -> Vec<f64> {
        let (lo, hi) = (self.lo, self.hi);
        if !(lo.is_finite() && hi.is_finite() && hi > lo) {
            return Vec::new();
        }
        match self.mode {
            ScaleMode::Linear => axis::price_ticks(lo, hi, target, min_step),
            ScaleMode::Percent | ScaleMode::Indexed if self.base > 0.0 => {
                // Round steps of the percentage (or index), mapped back to prices.
                let to_unit = |price: f64| price / self.base * 100.0;
                let from_unit = |unit: f64| unit / 100.0 * self.base;
                axis::price_ticks(
                    to_unit(lo),
                    to_unit(hi),
                    target,
                    to_unit(min_step).abs().max(f64::MIN_POSITIVE),
                )
                .into_iter()
                .map(from_unit)
                .collect()
            }
            ScaleMode::Percent | ScaleMode::Indexed => axis::price_ticks(lo, hi, target, min_step),
            ScaleMode::Log => log_ticks(lo.max(LOG_FLOOR), hi, target, min_step),
        }
    }

    /// The label of a price on the axis.
    pub fn label(&self, price: f64, format: ValueFormat, digits: u32) -> String {
        match self.mode {
            ScaleMode::Percent if self.base > 0.0 => {
                let pct = (price / self.base - 1.0) * 100.0;
                format!("{}%", trim(pct, 2, true))
            }
            ScaleMode::Indexed if self.base > 0.0 => trim(price / self.base * 100.0, 2, false),
            _ => format_value(price, format, digits),
        }
    }
}

/// Labels for a logarithmic scale: evenly spaced by ratio, each rounded to a number that is
/// round for the prices around it.
fn log_ticks(lo: f64, hi: f64, target: usize, min_step: f64) -> Vec<f64> {
    let (a, b) = (lo.ln(), hi.ln());
    let target = target.max(2);
    let step = (b - a) / target as f64;
    let mut out: Vec<f64> = Vec::new();
    for i in 0..=target {
        let raw = (a + step * i as f64).exp();
        // Round to a fifth of the distance to the next label at this price: round enough to
        // read, close enough to keep the labels evenly spread.
        let local = raw * (step.exp() - 1.0);
        let unit = axis::nice_step(local / 5.0, 1).max(min_step.max(f64::MIN_POSITIVE));
        let value = (raw / unit).round() * unit;
        let spread = out
            .last()
            .is_none_or(|last: &f64| value.ln() - last.ln() >= step * 0.6);
        if value >= lo && value <= hi && spread {
            out.push(value);
        }
    }
    out
}

/// A value as a pane's axis writes it.
pub fn format_value(value: f64, format: ValueFormat, digits: u32) -> String {
    if !value.is_finite() {
        return String::new();
    }
    match format {
        ValueFormat::Price => format_price(value.round() as i64, digits),
        ValueFormat::Plain(decimals) => trim(value, decimals as usize, false),
        ValueFormat::Count => count(value),
    }
}

/// `1.2K`, `3.45M`: a large count, short.
pub fn count(value: f64) -> String {
    let abs = value.abs();
    let (scaled, suffix) = if abs >= 1e9 {
        (value / 1e9, "B")
    } else if abs >= 1e6 {
        (value / 1e6, "M")
    } else if abs >= 1e3 {
        (value / 1e3, "K")
    } else {
        return format!("{}", value.round() as i64);
    };
    format!("{}{suffix}", trim(scaled, 2, false))
}

/// A number with at most `decimals` decimals, trailing zeros dropped, optionally signed.
fn trim(value: f64, decimals: usize, signed: bool) -> String {
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    if text == "-0" {
        text = "0".to_owned();
    }
    if signed && value > 0.0 && text != "0" {
        text.insert(0, '+');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(mode: ScaleMode, invert: bool) -> PriceMap {
        PriceMap {
            lo: 100.0,
            hi: 400.0,
            top: 0.0,
            bottom: 300.0,
            mode,
            invert,
            base: 200.0,
        }
    }

    #[test]
    fn every_scale_maps_heights_both_ways() {
        for mode in ScaleMode::ALL {
            for invert in [false, true] {
                let m = map(mode, invert);
                for price in [100.0, 150.0, 333.0, 400.0] {
                    let back = m.price(m.y(price));
                    assert!(
                        (back - price).abs() < 1e-6,
                        "{mode:?} {invert} {price} {back}"
                    );
                }
                let (hi_y, lo_y) = (m.y(400.0), m.y(100.0));
                if invert {
                    assert!(hi_y > lo_y);
                } else {
                    assert!(hi_y < lo_y);
                }
            }
        }
    }

    #[test]
    fn a_log_scale_spaces_equal_ratios_equally() {
        let m = map(ScaleMode::Log, false);
        let (y100, y200, y400) = (m.y(100.0), m.y(200.0), m.y(400.0));
        assert!(((y100 - y200) - (y200 - y400)).abs() < 1e-9);
        let lin = map(ScaleMode::Linear, false);
        assert!((lin.y(250.0) - 150.0).abs() < 1e-9);
    }

    #[test]
    fn zooming_keeps_the_anchor_where_it_is() {
        for mode in [ScaleMode::Linear, ScaleMode::Log] {
            let m = map(mode, false);
            let anchor_y = 120.0;
            let before = m.price(anchor_y);
            let (lo, hi) = m.zoomed(2.0, Some(anchor_y));
            let zoomed = PriceMap { lo, hi, ..m };
            assert!((zoomed.price(anchor_y) - before).abs() < 1e-6, "{mode:?}");
            assert!(hi - lo < 300.0);
        }
    }

    #[test]
    fn dragging_down_shows_higher_prices() {
        let m = map(ScaleMode::Linear, false);
        let (lo, hi) = m.panned(30.0);
        assert!((lo - 130.0).abs() < 1e-9 && (hi - 430.0).abs() < 1e-9);
        let inverted = map(ScaleMode::Linear, true);
        let (lo, _) = inverted.panned(30.0);
        assert!(lo < 100.0);
    }

    #[test]
    fn percent_labels_are_round_percentages_from_the_base() {
        let m = map(ScaleMode::Percent, false);
        let ticks = m.ticks(6, 0.01);
        assert!(!ticks.is_empty());
        for tick in &ticks {
            let label = m.label(*tick, ValueFormat::Price, 5);
            assert!(label.ends_with('%'), "{label}");
            let pct = (tick / 200.0 - 1.0) * 100.0;
            assert!((pct / 10.0 - (pct / 10.0).round()).abs() < 1e-6, "{pct}");
        }
        assert_eq!(m.label(200.0, ValueFormat::Price, 5), "0%");
        assert_eq!(m.label(300.0, ValueFormat::Price, 5), "+50%");
        assert_eq!(m.label(150.0, ValueFormat::Price, 5), "-25%");
        let idx = map(ScaleMode::Indexed, false);
        assert_eq!(idx.label(300.0, ValueFormat::Price, 5), "150");
    }

    #[test]
    fn log_labels_stay_inside_and_increase() {
        let m = PriceMap {
            lo: 1_000.0,
            hi: 1_000_000.0,
            ..map(ScaleMode::Log, false)
        };
        let ticks = m.ticks(8, 1.0);
        assert!(ticks.len() >= 5, "{ticks:?}");
        assert!(ticks.windows(2).all(|w| w[1] > w[0]));
        assert!(ticks.iter().all(|t| *t >= 1_000.0 && *t <= 1_000_000.0));
    }

    #[test]
    fn values_read_the_way_their_pane_says() {
        assert_eq!(format_value(70.123, ValueFormat::Plain(2), 5), "70.12");
        assert_eq!(format_value(70.0, ValueFormat::Plain(2), 5), "70");
        assert_eq!(format_value(1_234.0, ValueFormat::Count, 5), "1.23K");
        assert_eq!(format_value(12.0, ValueFormat::Count, 5), "12");
        assert_eq!(format_value(108_412.0, ValueFormat::Price, 5), "1.08412");
        assert_eq!(count(2_500_000.0), "2.5M");
        assert_eq!(format_value(f64::NAN, ValueFormat::Count, 5), "");
    }
}
