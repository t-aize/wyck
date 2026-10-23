//! Turning points and levels: pivots, fractals, Aroon, and the pivot point levels of a period.

use super::calendar::Anchor;
use super::ta::Series as _;
use crate::base::math::count_f64;

/// The pivots high of `values`, as Pine's `ta.pivothigh` gives them: on the bar where a pivot is
/// confirmed (`right` bars after it), the value of the pivot; `NaN` elsewhere. A point is a
/// pivot when the `left` points before it are not higher and the `right` points after it are
/// lower: a tie on the left is allowed, one on the right is not.
pub fn pivot_high(values: &[f64], left: usize, right: usize) -> Vec<f64> {
    pivots(values, left, right, |centre, other, before| {
        if before {
            other <= centre
        } else {
            other < centre
        }
    })
}

/// The pivots low of `values`, the mirror of [`pivot_high`].
pub fn pivot_low(values: &[f64], left: usize, right: usize) -> Vec<f64> {
    pivots(values, left, right, |centre, other, before| {
        if before {
            other >= centre
        } else {
            other > centre
        }
    })
}

fn pivots(
    values: &[f64],
    left: usize,
    right: usize,
    keeps: fn(f64, f64, bool) -> bool,
) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    for (confirmed, slot) in out.iter_mut().enumerate().skip(left + right) {
        let at = confirmed - right;
        let centre = values.at(at);
        let window = values.get(at - left..=confirmed).unwrap_or_default();
        let ok = centre.is_finite()
            && window.iter().all(|v| v.is_finite())
            && window.iter().enumerate().all(|(k, v)| {
                let position = at - left + k;
                position == at || keeps(centre, *v, position < at)
            });
        if ok {
            *slot = centre;
        }
    }
    out
}

/// The Williams fractals: a high that is above the `wing` bars on each side, and a low that is
/// under them, each marked on the bar itself. The mark is only known `wing` bars later.
pub fn fractals(high: &[f64], low: &[f64], wing: usize) -> (Vec<f64>, Vec<f64>) {
    let n = high.len().min(low.len());
    let (mut up, mut down) = (vec![f64::NAN; n], vec![f64::NAN; n]);
    for i in wing..n.saturating_sub(wing) {
        let range = (i - wing)..=(i + wing);
        if range.clone().all(|j| j == i || high.at(i) > high.at(j)) {
            up.put(i, high.at(i));
        }
        if range.clone().all(|j| j == i || low.at(i) < low.at(j)) {
            down.put(i, low.at(i));
        }
    }
    (up, down)
}

/// Aroon: how recent the highest high and the lowest low of the last `period + 1` bars are, from
/// 0 to 100. A tie goes to the latest high and to the earliest low.
pub fn aroon(high: &[f64], low: &[f64], period: usize) -> (Vec<f64>, Vec<f64>) {
    let n = high.len().min(low.len());
    let (mut up, mut down) = (vec![f64::NAN; n], vec![f64::NAN; n]);
    if period == 0 {
        return (up, down);
    }
    for i in period..n {
        let start = i - period;
        let h = (start..=i)
            .max_by(|&a, &b| high.at(a).total_cmp(&high.at(b)))
            .unwrap_or(i);
        let l = (start..=i)
            .min_by(|&a, &b| low.at(a).total_cmp(&low.at(b)))
            .unwrap_or(i);
        up.put(i, 100.0 * count_f64(period - (i - h)) / count_f64(period));
        down.put(i, 100.0 * count_f64(period - (i - l)) / count_f64(period));
    }
    (up, down)
}

/// The way pivot point levels are worked out from the previous period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PivotKind {
    Traditional,
    Fibonacci,
    Woodie,
    Classic,
    Camarilla,
    DeMark,
}

impl PivotKind {
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "traditional" => Some(Self::Traditional),
            "fibonacci" => Some(Self::Fibonacci),
            "woodie" => Some(Self::Woodie),
            "classic" => Some(Self::Classic),
            "camarilla" => Some(Self::Camarilla),
            "dm" | "demark" => Some(Self::DeMark),
            _ => None,
        }
    }

    /// The seven levels `[p, r1, s1, r2, s2, r3, s3]` from the high, low, open and close of the
    /// previous period. Levels a kind does not have are `NaN`.
    #[expect(
        clippy::many_single_char_names,
        reason = "the pivot formulas are written with H, L, O, C and P, as everywhere"
    )]
    fn levels(self, h: f64, l: f64, o: f64, c: f64) -> [f64; 7] {
        let range = h - l;
        let p = (h + l + c) / 3.0;
        let n = f64::NAN;
        match self {
            Self::Traditional => [
                p,
                2.0 * p - l,
                2.0 * p - h,
                p + h - l,
                p - h + l,
                h + 2.0 * (p - l),
                l - 2.0 * (h - p),
            ],
            Self::Fibonacci => [
                p,
                p + 0.382 * range,
                p - 0.382 * range,
                p + 0.618 * range,
                p - 0.618 * range,
                p + range,
                p - range,
            ],
            Self::Woodie => {
                let p = (h + l + 2.0 * c) / 4.0;
                [
                    p,
                    2.0 * p - l,
                    2.0 * p - h,
                    p + range,
                    p - range,
                    h + 2.0 * (p - l),
                    l - 2.0 * (h - p),
                ]
            }
            Self::Classic => [
                p,
                2.0 * p - l,
                2.0 * p - h,
                p + range,
                p - range,
                p + 2.0 * range,
                p - 2.0 * range,
            ],
            Self::Camarilla => [
                p,
                c + range * 1.1 / 12.0,
                c - range * 1.1 / 12.0,
                c + range * 1.1 / 6.0,
                c - range * 1.1 / 6.0,
                c + range * 1.1 / 4.0,
                c - range * 1.1 / 4.0,
            ],
            Self::DeMark => {
                let x = if c < o {
                    h + 2.0 * l + c
                } else if c > o {
                    2.0 * h + l + c
                } else {
                    h + l + 2.0 * c
                };
                [x / 4.0, x / 2.0 - l, x / 2.0 - h, n, n, n, n]
            }
        }
    }
}

/// The pivot point levels of every bar, from the period before the bar's own: seven series
/// `[p, r1, s1, r2, s2, r3, s3]`. There are none until the second period begins, and a period
/// whose high, low or close is not a number is passed over.
pub fn pivot_levels(
    open: &[f64],
    high: &[f64],
    low: &[f64],
    close: &[f64],
    day: &[i64],
    anchor: Anchor,
    kind: PivotKind,
) -> [Vec<f64>; 7] {
    let n = close.len();
    let mut columns: [Vec<f64>; 7] = std::array::from_fn(|_| vec![f64::NAN; n]);
    let mut current = None;
    let (mut hi, mut lo, mut period_open, mut last) =
        (f64::NEG_INFINITY, f64::INFINITY, f64::NAN, f64::NAN);
    let mut previous = None;
    for (i, day) in day.iter().enumerate().take(n) {
        let period = anchor.key(*day);
        if current != Some(period) {
            if current.is_some() && hi.is_finite() && lo.is_finite() && last.is_finite() {
                previous = Some((hi, lo, period_open, last));
            }
            current = Some(period);
            hi = f64::NEG_INFINITY;
            lo = f64::INFINITY;
            period_open = open.at(i);
        }
        if let Some((h, l, o, c)) = previous {
            for (column, level) in columns.iter_mut().zip(kind.levels(h, l, o, c)) {
                column.put(i, level);
            }
        }
        hi = hi.max(high.at(i));
        lo = lo.min(low.at(i));
        last = close.at(i);
    }
    columns
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAN: f64 = f64::NAN;

    fn same(a: &[f64], b: &[f64]) {
        assert_eq!(a.len(), b.len(), "{a:?} against {b:?}");
        for (x, y) in a.iter().zip(b) {
            assert!(
                (x.is_nan() && y.is_nan()) || (x - y).abs() < 1e-9,
                "{a:?} against {b:?}"
            );
        }
    }

    #[test]
    fn a_pivot_is_known_when_the_bars_after_it_are_in() {
        let v = [1.0, 3.0, 2.0, 1.0, 2.0, 5.0, 2.0, 1.0];
        same(
            &pivot_high(&v, 2, 2),
            &[NAN, NAN, NAN, NAN, NAN, NAN, NAN, 5.0],
        );
        same(
            &pivot_low(&v, 1, 1),
            &[NAN, NAN, NAN, NAN, 1.0, NAN, NAN, NAN],
        );
    }

    #[test]
    fn a_tie_on_the_left_is_allowed_and_a_tie_on_the_right_is_not() {
        // The two 5s: the first has an equal bar after it, so it is not a pivot high; the second
        // has an equal bar before it, which is allowed.
        let v = [1.0, 5.0, 5.0, 1.0, 1.0];
        same(&pivot_high(&v, 1, 1), &[NAN, NAN, NAN, 5.0, NAN]);
        // The same for the lows.
        let w = [9.0, 2.0, 2.0, 9.0, 9.0];
        same(&pivot_low(&w, 1, 1), &[NAN, NAN, NAN, 2.0, NAN]);
    }

    #[test]
    fn a_fractal_is_strictly_above_or_under_the_bars_on_each_side() {
        let high = [1.0, 2.0, 5.0, 2.0, 1.0, 2.0, 2.0];
        let low = [1.0, 2.0, 0.5, 2.0, 1.0, 2.0, 2.0];
        let (up, down) = fractals(&high, &low, 2);
        same(&up, &[NAN, NAN, 5.0, NAN, NAN, NAN, NAN]);
        same(&down, &[NAN, NAN, 0.5, NAN, NAN, NAN, NAN]);
    }

    #[test]
    fn aroon_is_a_hundred_at_a_new_high_and_counts_the_bars_since() {
        let high = [1.0, 2.0, 3.0, 4.0, 3.0, 2.0];
        let low = [0.5, 0.6, 0.7, 0.8, 0.4, 0.9];
        let (up, down) = aroon(&high, &low, 3);
        same(&up[3..], &[100.0, 100.0 * 2.0 / 3.0, 100.0 / 3.0]);
        same(&down[3..], &[0.0, 100.0, 100.0 * 2.0 / 3.0]);
    }

    #[test]
    fn pivot_levels_start_with_the_second_period_and_follow_the_kind() {
        // Two bars a day: day 0 has high 12, low 8, open 9, close 11; then day 1.
        let (open, high) = ([9.0, 10.0, 11.0, 11.0], [11.0, 12.0, 13.0, 12.0]);
        let (low, close) = ([8.0, 8.5, 10.0, 9.0], [10.0, 11.0, 12.0, 11.5]);
        let day = [0, 0, 1, 1];
        let levels = pivot_levels(
            &open,
            &high,
            &low,
            &close,
            &day,
            Anchor::Day,
            PivotKind::Traditional,
        );
        assert!(levels[0][0].is_nan() && levels[0][1].is_nan());
        // High 12, low 8, close 11: P = 31/3.
        let p = 31.0 / 3.0;
        same(&levels[0][2..], &[p, p]);
        same(&levels[1][2..3], &[2.0 * p - 8.0]);
        same(&levels[2][2..3], &[2.0 * p - 12.0]);
        let fib = pivot_levels(
            &open,
            &high,
            &low,
            &close,
            &day,
            Anchor::Day,
            PivotKind::Fibonacci,
        );
        same(&fib[1][2..3], &[p + 0.382 * 4.0]);
        let dm = pivot_levels(
            &open,
            &high,
            &low,
            &close,
            &day,
            Anchor::Day,
            PivotKind::DeMark,
        );
        // The close (11) is above the open (9): X = 2H + L + C = 24 + 8 + 11 = 43.
        same(&dm[0][2..3], &[43.0 / 4.0]);
        assert!(dm[3][2].is_nan());
    }

    #[test]
    fn a_period_without_a_close_is_passed_over() {
        let open = [1.0, 1.0, 1.0];
        let high = [2.0, 2.0, 2.0];
        let low = [1.0, 1.0, 1.0];
        let close = [1.5, NAN, 1.5];
        let day = [0, 1, 2];
        let levels = pivot_levels(
            &open,
            &high,
            &low,
            &close,
            &day,
            Anchor::Day,
            PivotKind::Traditional,
        );
        // Day 1 ends without a close, so day 2 still shows day 0's levels.
        assert!(levels[0][1].is_finite() && levels[0][2].is_finite());
        assert_eq!(levels[0][1], levels[0][2]);
    }
}
