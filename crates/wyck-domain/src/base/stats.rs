//! Rolling statistics and small oscillators of the indicators, with the definitions of the Pine
//! Script reference: variance, deviation, median, mode, percentiles, correlation, and the like.
//!
//! The rules of [`super::ta`] hold: every output is as long as its input, a window with a value
//! that is not finite has no value, and the first points of a window have none.

use super::ta::{Series as _, ema, highest, lowest, sma};
use crate::base::math::count_f64;

/// `f` over each window of `period` points, in time order, or `NaN` where a window holds a value
/// that is not finite.
fn over_windows(values: &[f64], period: usize, f: impl Fn(&[f64]) -> f64) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    if period == 0 {
        return out;
    }
    for (slot, window) in out.iter_mut().skip(period - 1).zip(values.windows(period)) {
        if window.iter().all(|v| v.is_finite()) {
            *slot = f(window);
        }
    }
    out
}

fn mean(window: &[f64]) -> f64 {
    window.iter().sum::<f64>() / count_f64(window.len())
}

/// The variance over `period` points: `biased` divides by the number of points, otherwise by one
/// less (the sample variance).
pub fn variance(values: &[f64], period: usize, biased: bool) -> Vec<f64> {
    if !biased && period < 2 {
        return vec![f64::NAN; values.len()];
    }
    over_windows(values, period, |w| {
        let centre = mean(w);
        let squares = w.iter().map(|v| (v - centre).powi(2)).sum::<f64>();
        squares / count_f64(if biased { w.len() } else { w.len() - 1 })
    })
}

/// The mean absolute deviation over `period` points: how far the points are from their mean, on
/// average.
pub fn deviation(values: &[f64], period: usize) -> Vec<f64> {
    over_windows(values, period, |w| {
        let centre = mean(w);
        w.iter().map(|v| (v - centre).abs()).sum::<f64>() / count_f64(w.len())
    })
}

/// The window sorted, smallest first.
fn sorted(window: &[f64]) -> Vec<f64> {
    let mut copy = window.to_vec();
    copy.sort_by(f64::total_cmp);
    copy
}

/// The median over `period` points: the middle one, or the mean of the two in the middle.
pub fn median(values: &[f64], period: usize) -> Vec<f64> {
    over_windows(values, period, |w| {
        let s = sorted(w);
        let mid = s.len() / 2;
        if s.len() % 2 == 1 {
            s.get(mid).copied().unwrap_or(f64::NAN)
        } else {
            f64::midpoint(
                s.get(mid.wrapping_sub(1)).copied().unwrap_or(f64::NAN),
                s.get(mid).copied().unwrap_or(f64::NAN),
            )
        }
    })
}

/// The mode over `period` points: the value that comes most often, the smallest of them when
/// several tie.
pub fn mode(values: &[f64], period: usize) -> Vec<f64> {
    over_windows(values, period, |w| {
        let s = sorted(w);
        let (mut best, mut best_count) = (f64::NAN, 0_usize);
        let mut run_start = 0;
        for (i, value) in s.iter().enumerate() {
            if s.get(run_start)
                .is_some_and(|first| first.to_bits() != value.to_bits())
            {
                run_start = i;
            }
            let count = i + 1 - run_start;
            if count > best_count {
                best_count = count;
                best = *value;
            }
        }
        best
    })
}

/// The value at the percentage `pct` (0 to 100) of the window by the nearest rank: the smallest
/// value such that at least that share of the points is at or below it.
pub fn percentile_nearest_rank(values: &[f64], period: usize, pct: f64) -> Vec<f64> {
    let pct = pct.clamp(0.0, 100.0);
    over_windows(values, period, |w| {
        let s = sorted(w);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a rank is between 0 and the length of a window"
        )]
        let rank = ((pct / 100.0 * count_f64(s.len())).ceil() as usize).max(1);
        s.get(rank - 1).copied().unwrap_or(f64::NAN)
    })
}

/// The value at the percentage `pct` (0 to 100) of the window, interpolated between the two
/// nearest ranks.
pub fn percentile_linear_interpolation(values: &[f64], period: usize, pct: f64) -> Vec<f64> {
    let pct = pct.clamp(0.0, 100.0);
    over_windows(values, period, |w| {
        let s = sorted(w);
        let position = pct / 100.0 * count_f64(s.len().saturating_sub(1));
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a position is between 0 and the length of a window"
        )]
        let below = position.floor() as usize;
        let (low, high) = (
            s.get(below).copied().unwrap_or(f64::NAN),
            s.get(below + 1)
                .copied()
                .unwrap_or_else(|| s.last().copied().unwrap_or(f64::NAN)),
        );
        low + (high - low) * (position - position.floor())
    })
}

/// The share (0 to 100) of the `period` points before each point that are at or below it.
pub fn percent_rank(values: &[f64], period: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    if period == 0 {
        return out;
    }
    for (i, slot) in out.iter_mut().enumerate().skip(period) {
        let now = values.at(i);
        let window = values.get(i - period..i).unwrap_or_default();
        if now.is_finite() && window.iter().all(|v| v.is_finite()) {
            let at_or_below = window.iter().filter(|v| **v <= now).count();
            *slot = 100.0 * count_f64(at_or_below) / count_f64(period);
        }
    }
    out
}

/// The highest value minus the lowest over `period` points.
pub fn range(values: &[f64], period: usize) -> Vec<f64> {
    highest(values, period)
        .iter()
        .zip(&lowest(values, period))
        .map(|(h, l)| h - l)
        .collect()
}

/// The Pearson correlation of two series over `period` points, from -1 to 1. `NaN` where one of
/// the series is flat in the window.
pub fn correlation(a: &[f64], b: &[f64], period: usize) -> Vec<f64> {
    let n = a.len().min(b.len());
    let mut out = vec![f64::NAN; n];
    if period < 2 {
        return out;
    }
    for (i, slot) in out.iter_mut().enumerate().skip(period - 1) {
        let (wa, wb) = (
            a.get(i + 1 - period..=i).unwrap_or_default(),
            b.get(i + 1 - period..=i).unwrap_or_default(),
        );
        if !wa.iter().chain(wb).all(|v| v.is_finite()) {
            continue;
        }
        let (ma, mb) = (mean(wa), mean(wb));
        let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
        for (x, y) in wa.iter().zip(wb) {
            sab += (x - ma) * (y - mb);
            saa += (x - ma).powi(2);
            sbb += (y - mb).powi(2);
        }
        if saa > 0.0 && sbb > 0.0 {
            *slot = sab / (saa * sbb).sqrt();
        }
    }
    out
}

/// The Arnaud Legoux moving average: a window weighted by a gaussian whose peak is at `offset`
/// (0 to 1, the newest end) with the width `period / sigma`.
pub fn alma(values: &[f64], period: usize, offset: f64, sigma: f64) -> Vec<f64> {
    if period == 0 || sigma <= 0.0 {
        return vec![f64::NAN; values.len()];
    }
    let peak = (offset.clamp(0.0, 1.0) * count_f64(period - 1)).floor();
    let width = count_f64(period) / sigma;
    let weights: Vec<f64> = (0..period)
        .map(|i| (-(count_f64(i) - peak).powi(2) / (2.0 * width * width)).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    over_windows(values, period, |w| {
        w.iter().zip(&weights).map(|(v, k)| v * k).sum::<f64>() / total
    })
}

/// The symmetrically weighted moving average of four points: 1, 2, 2, 1 over 6.
pub fn swma(values: &[f64]) -> Vec<f64> {
    over_windows(values, 4, |w| {
        (w.first().copied().unwrap_or(f64::NAN)
            + 2.0 * w.get(1).copied().unwrap_or(f64::NAN)
            + 2.0 * w.get(2).copied().unwrap_or(f64::NAN)
            + w.get(3).copied().unwrap_or(f64::NAN))
            / 6.0
    })
}

/// The Chande momentum oscillator, from -100 to 100: gains minus losses over their sum, for the
/// changes of the last `period` points.
pub fn cmo(values: &[f64], period: usize) -> Vec<f64> {
    let mut gains = vec![f64::NAN; values.len()];
    let mut losses = vec![f64::NAN; values.len()];
    for i in 1..values.len() {
        let change = values.at(i) - values.at(i - 1);
        gains.put(i, change.max(0.0));
        losses.put(i, (-change).max(0.0));
    }
    let (up, down) = (sma(&gains, period), sma(&losses, period));
    up.iter()
        .zip(&down)
        .map(|(u, d)| {
            if u + d > 0.0 {
                100.0 * (u - d) / (u + d)
            } else if u.is_finite() && d.is_finite() {
                0.0
            } else {
                f64::NAN
            }
        })
        .collect()
}

/// The center of gravity oscillator: the weighted center of the last `period` points, newest
/// weighing 1, negated.
pub fn center_of_gravity(values: &[f64], period: usize) -> Vec<f64> {
    over_windows(values, period, |w| {
        let total: f64 = w.iter().sum();
        let weighted: f64 = w
            .iter()
            .rev()
            .enumerate()
            .map(|(i, v)| v * count_f64(i + 1))
            .sum();
        if total == 0.0 {
            f64::NAN
        } else {
            -weighted / total
        }
    })
}

/// The true strength index, from -1 to 1: the momentum smoothed twice over the absolute momentum
/// smoothed twice.
pub fn true_strength_index(values: &[f64], short: usize, long: usize) -> Vec<f64> {
    let change: Vec<f64> = (0..values.len())
        .map(|i| {
            if i == 0 {
                f64::NAN
            } else {
                values.at(i) - values.at(i - 1)
            }
        })
        .collect();
    let magnitude: Vec<f64> = change.iter().map(|c| c.abs()).collect();
    let top = ema(&ema(&change, long), short);
    let bottom = ema(&ema(&magnitude, long), short);
    top.iter()
        .zip(&bottom)
        .map(|(t, b)| if *b > 0.0 { t / b } else { f64::NAN })
        .collect()
}

/// Bars back to the highest value of the last `period` points, 0 when it is this one, negative
/// otherwise. The most recent wins a tie.
pub fn highest_bars(values: &[f64], period: usize) -> Vec<f64> {
    extreme_bars(values, period, |new, best| new >= best)
}

/// Bars back to the lowest value of the last `period` points, 0 when it is this one, negative
/// otherwise. The most recent wins a tie.
pub fn lowest_bars(values: &[f64], period: usize) -> Vec<f64> {
    extreme_bars(values, period, |new, best| new <= best)
}

fn extreme_bars(values: &[f64], period: usize, beats: fn(f64, f64) -> bool) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    if period == 0 {
        return out;
    }
    for (i, slot) in out.iter_mut().enumerate().skip(period - 1) {
        let window = values.get(i + 1 - period..=i).unwrap_or_default();
        if !window.iter().all(|v| v.is_finite()) {
            continue;
        }
        let mut best_at = 0;
        for (k, value) in window.iter().enumerate() {
            if window.get(best_at).is_some_and(|best| beats(*value, *best)) {
                best_at = k;
            }
        }
        *slot = -count_f64(period - 1 - best_at);
    }
    out
}

/// 1 where the series fell on each of the last `period` bars, 0 where it did not.
pub fn falling(values: &[f64], period: usize) -> Vec<f64> {
    run_of_moves(values, period, |now, before| now < before)
}

/// 1 where the series rose on each of the last `period` bars, 0 where it did not.
pub fn rising(values: &[f64], period: usize) -> Vec<f64> {
    run_of_moves(values, period, |now, before| now > before)
}

fn run_of_moves(values: &[f64], period: usize, moved: fn(f64, f64) -> bool) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    if period == 0 {
        return out;
    }
    for (i, slot) in out.iter_mut().enumerate().skip(period) {
        let window = values.get(i - period..=i).unwrap_or_default();
        if window.iter().all(|v| v.is_finite()) {
            let all = window
                .windows(2)
                .all(|pair| matches!(pair, [before, now] if moved(*now, *before)));
            *slot = if all { 1.0 } else { 0.0 };
        }
    }
    out
}

/// The value of `values` on the bar where `condition` was true `occurrence` times ago (0 is the
/// latest, this bar included).
pub fn value_when(condition: &[f64], values: &[f64], occurrence: usize) -> Vec<f64> {
    let mut seen: Vec<f64> = Vec::new();
    (0..condition.len())
        .map(|i| {
            if condition.at(i).is_finite() && condition.at(i) != 0.0 {
                seen.push(values.at(i));
            }
            seen.len()
                .checked_sub(occurrence + 1)
                .and_then(|at| seen.get(at))
                .copied()
                .unwrap_or(f64::NAN)
        })
        .collect()
}

/// The series with each value that is not a number replaced by the last one that was.
pub fn fix_nan(values: &[f64]) -> Vec<f64> {
    let mut last = f64::NAN;
    values
        .iter()
        .map(|v| {
            if !v.is_nan() {
                last = *v;
            }
            last
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: &[f64], b: &[f64]) {
        assert_eq!(a.len(), b.len(), "{a:?} against {b:?}");
        for (x, y) in a.iter().zip(b) {
            assert!(
                (x.is_nan() && y.is_nan()) || (x - y).abs() < 1e-9,
                "{a:?} against {b:?}"
            );
        }
    }

    const NAN: f64 = f64::NAN;

    #[test]
    fn variance_and_deviation_follow_their_definitions() {
        let v = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        // The classic example: mean 5, population variance 4.
        same(&variance(&v, 8, true)[7..], &[4.0]);
        same(&variance(&v, 8, false)[7..], &[32.0 / 7.0]);
        same(&deviation(&v, 8)[7..], &[1.5]);
        assert!(variance(&v, 1, false).iter().all(|x| x.is_nan()));
    }

    #[test]
    fn the_median_the_mode_and_the_range_read_a_window() {
        let v = [5.0, 1.0, 3.0, 3.0, 9.0];
        same(&median(&v, 3), &[NAN, NAN, 3.0, 3.0, 3.0]);
        same(&median(&v, 4), &[NAN, NAN, NAN, 3.0, 3.0]);
        same(&mode(&v, 4), &[NAN, NAN, NAN, 3.0, 3.0]);
        same(&mode(&[1.0, 2.0, 1.0, 2.0], 4), &[NAN, NAN, NAN, 1.0]);
        same(&range(&v, 3), &[NAN, NAN, 4.0, 2.0, 6.0]);
    }

    #[test]
    fn percentiles_use_the_nearest_rank_or_interpolate() {
        let v = [15.0, 20.0, 35.0, 40.0, 50.0];
        same(&percentile_nearest_rank(&v, 5, 30.0)[4..], &[20.0]);
        same(&percentile_nearest_rank(&v, 5, 100.0)[4..], &[50.0]);
        same(&percentile_nearest_rank(&v, 5, 0.0)[4..], &[15.0]);
        same(&percentile_linear_interpolation(&v, 5, 50.0)[4..], &[35.0]);
        same(&percentile_linear_interpolation(&v, 5, 40.0)[4..], &[29.0]);
        same(&percentile_linear_interpolation(&v, 5, 100.0)[4..], &[50.0]);
    }

    #[test]
    fn the_percent_rank_counts_the_points_before_that_are_at_or_below() {
        let v = [1.0, 2.0, 3.0, 4.0, 2.5];
        same(&percent_rank(&v, 4), &[NAN, NAN, NAN, NAN, 50.0]);
        same(&percent_rank(&[1.0, 2.0, 3.0], 2), &[NAN, NAN, 100.0]);
    }

    #[test]
    fn a_correlation_is_one_for_a_line_and_minus_one_for_its_mirror() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [2.0, 4.0, 6.0, 8.0, 10.0];
        let c = [5.0, 4.0, 3.0, 2.0, 1.0];
        same(&correlation(&a, &b, 5)[4..], &[1.0]);
        same(&correlation(&a, &c, 5)[4..], &[-1.0]);
        assert!(
            correlation(&a, &[1.0; 5], 5)[4].is_nan(),
            "a flat series has none"
        );
    }

    #[test]
    fn alma_weights_the_window_by_a_gaussian_and_a_flat_series_stays_flat() {
        same(&alma(&[7.0; 6], 4, 0.85, 6.0)[3..], &[7.0, 7.0, 7.0]);
        // With a peak at the newest end the average sits above the plain one on a rising line.
        let line: Vec<f64> = (0..10).map(f64::from).collect();
        let (a, s) = (alma(&line, 5, 0.85, 6.0), sma(&line, 5));
        assert!(a[9] > s[9]);
    }

    #[test]
    fn swma_weights_four_points_one_two_two_one() {
        same(
            &swma(&[1.0, 2.0, 3.0, 4.0, 5.0]),
            &[NAN, NAN, NAN, 2.5, 3.5],
        );
    }

    #[test]
    fn cmo_is_a_hundred_on_a_rise_and_minus_a_hundred_on_a_fall() {
        let up: Vec<f64> = (0..8).map(f64::from).collect();
        let down: Vec<f64> = up.iter().rev().copied().collect();
        same(&cmo(&up, 5)[5..], &[100.0, 100.0, 100.0]);
        same(&cmo(&down, 5)[5..], &[-100.0, -100.0, -100.0]);
        same(&cmo(&[3.0; 8], 5)[5..], &[0.0, 0.0, 0.0]);
    }

    #[test]
    fn the_center_of_gravity_of_a_flat_window_is_minus_the_middle_weight() {
        // Weights 1..4 over four equal points: -(1+2+3+4)/4 = -2.5.
        same(&center_of_gravity(&[5.0; 6], 4)[3..], &[-2.5, -2.5, -2.5]);
    }

    #[test]
    fn the_true_strength_index_is_one_on_a_straight_rise() {
        let up: Vec<f64> = (0..40).map(|i| f64::from(i) * 2.0).collect();
        let t = true_strength_index(&up, 5, 10);
        assert!((t[39] - 1.0).abs() < 1e-9);
        assert!(t[0].is_nan());
    }

    #[test]
    fn the_bars_back_to_an_extreme_are_zero_or_negative_and_the_latest_wins_a_tie() {
        let v = [3.0, 9.0, 4.0, 9.0, 1.0];
        same(&highest_bars(&v, 4), &[NAN, NAN, NAN, 0.0, -1.0]);
        same(&lowest_bars(&v, 3), &[NAN, NAN, -2.0, -1.0, 0.0]);
    }

    #[test]
    fn rising_and_falling_need_every_bar_of_the_run() {
        let v = [1.0, 2.0, 3.0, 2.0, 1.0, 0.5];
        same(&rising(&v, 2), &[NAN, NAN, 1.0, 0.0, 0.0, 0.0]);
        same(&falling(&v, 2), &[NAN, NAN, 0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn value_when_looks_back_to_the_bars_where_a_condition_held() {
        let cond = [0.0, 1.0, 0.0, 1.0, 0.0];
        let src = [10.0, 20.0, 30.0, 40.0, 50.0];
        same(&value_when(&cond, &src, 0), &[NAN, 20.0, 20.0, 40.0, 40.0]);
        same(&value_when(&cond, &src, 1), &[NAN, NAN, NAN, 20.0, 20.0]);
    }

    #[test]
    fn fix_nan_carries_the_last_value_forward() {
        same(
            &fix_nan(&[NAN, 1.0, NAN, NAN, 4.0, NAN]),
            &[NAN, 1.0, 1.0, 1.0, 4.0, 4.0],
        );
    }
}
