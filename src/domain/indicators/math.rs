//! The arithmetic of the indicators: moving averages, deviations, oscillators.
//!
//! Every function takes a series and returns one of the same length, with `NaN` where the value
//! is not defined yet (the first `period - 1` points of a moving average, for example). A `NaN`
//! in the input is skipped the way a gap is: averages restart after it. That keeps every output
//! aligned index for index with the bars it was computed from, which is what the chart plots.
//!
//! The definitions follow the usual references (Wilder for RSI and ATR, Appel for MACD,
//! Bollinger for the bands), and the tests check them against values worked out by hand.

/// Simple moving average over `period` points.
pub fn sma(values: &[f64], period: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    if period == 0 {
        return out;
    }
    let mut sum = 0.0;
    let mut run = 0usize;
    for (i, &v) in values.iter().enumerate() {
        if !v.is_finite() {
            sum = 0.0;
            run = 0;
            continue;
        }
        sum += v;
        run += 1;
        if run > period {
            sum -= values[i - period];
            run = period;
        }
        if run == period {
            out[i] = sum / period as f64;
        }
    }
    out
}

/// Exponential moving average with the smoothing `alpha`, seeded with the simple average of the
/// first `period` points (the convention of TA-Lib and most charting platforms).
fn smoothed(values: &[f64], period: usize, alpha: f64) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    if period == 0 {
        return out;
    }
    let mut prev: Option<f64> = None;
    let mut seed_sum = 0.0;
    let mut seed_run = 0usize;
    for (i, &v) in values.iter().enumerate() {
        if !v.is_finite() {
            prev = None;
            seed_sum = 0.0;
            seed_run = 0;
            continue;
        }
        match prev {
            Some(p) => {
                let next = p + alpha * (v - p);
                out[i] = next;
                prev = Some(next);
            }
            None => {
                seed_sum += v;
                seed_run += 1;
                if seed_run == period {
                    let seed = seed_sum / period as f64;
                    out[i] = seed;
                    prev = Some(seed);
                }
            }
        }
    }
    out
}

/// Exponential moving average: `alpha = 2 / (period + 1)`.
pub fn ema(values: &[f64], period: usize) -> Vec<f64> {
    smoothed(values, period, 2.0 / (period as f64 + 1.0))
}

/// Wilder's smoothing (RMA, SMMA): `alpha = 1 / period`.
pub fn rma(values: &[f64], period: usize) -> Vec<f64> {
    smoothed(values, period, 1.0 / period.max(1) as f64)
}

/// Linearly weighted moving average: the newest point weighs `period`, the oldest 1.
pub fn wma(values: &[f64], period: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    if period == 0 {
        return out;
    }
    let denominator = (period * (period + 1)) as f64 / 2.0;
    for i in period.saturating_sub(1)..values.len() {
        let window = &values[i + 1 - period..=i];
        if window.iter().all(|v| v.is_finite()) {
            let weighted: f64 = window
                .iter()
                .enumerate()
                .map(|(k, v)| v * (k + 1) as f64)
                .sum();
            out[i] = weighted / denominator;
        }
    }
    out
}

/// Hull moving average: `WMA(2 * WMA(n / 2) - WMA(n), sqrt(n))`, a fast average with little lag.
pub fn hma(values: &[f64], period: usize) -> Vec<f64> {
    let half = wma(values, (period / 2).max(1));
    let full = wma(values, period);
    let diff: Vec<f64> = half.iter().zip(&full).map(|(h, f)| 2.0 * h - f).collect();
    wma(&diff, ((period as f64).sqrt().round() as usize).max(1))
}

/// The population standard deviation over `period` points (what Bollinger used).
pub fn stdev(values: &[f64], period: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    if period == 0 {
        return out;
    }
    for i in period.saturating_sub(1)..values.len() {
        let window = &values[i + 1 - period..=i];
        if window.iter().all(|v| v.is_finite()) {
            let mean = window.iter().sum::<f64>() / period as f64;
            let variance = window.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / period as f64;
            out[i] = variance.sqrt();
        }
    }
    out
}

/// The highest value over the last `period` points.
pub fn highest(values: &[f64], period: usize) -> Vec<f64> {
    window_fold(values, period, f64::max)
}

/// The lowest value over the last `period` points.
pub fn lowest(values: &[f64], period: usize) -> Vec<f64> {
    window_fold(values, period, f64::min)
}

fn window_fold(values: &[f64], period: usize, fold: fn(f64, f64) -> f64) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    if period == 0 {
        return out;
    }
    for i in period.saturating_sub(1)..values.len() {
        let window = &values[i + 1 - period..=i];
        if window.iter().all(|v| v.is_finite()) {
            out[i] = window.iter().copied().reduce(fold).unwrap_or(f64::NAN);
        }
    }
    out
}

/// Wilder's relative strength index, from 0 to 100.
pub fn rsi(values: &[f64], period: usize) -> Vec<f64> {
    let n = values.len();
    let mut gains = vec![f64::NAN; n];
    let mut losses = vec![f64::NAN; n];
    for i in 1..n {
        let change = values[i] - values[i - 1];
        if change.is_finite() {
            gains[i] = change.max(0.0);
            losses[i] = (-change).max(0.0);
        }
    }
    let (avg_gain, avg_loss) = (rma(&gains, period), rma(&losses, period));
    avg_gain
        .iter()
        .zip(&avg_loss)
        .map(|(&g, &l)| {
            if !(g.is_finite() && l.is_finite()) {
                f64::NAN
            } else if l == 0.0 {
                if g == 0.0 { 50.0 } else { 100.0 }
            } else {
                100.0 - 100.0 / (1.0 + g / l)
            }
        })
        .collect()
}

/// MACD: the fast EMA minus the slow EMA, its signal line, and the histogram between them.
pub fn macd(
    values: &[f64],
    fast: usize,
    slow: usize,
    signal: usize,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let (f, s) = (ema(values, fast), ema(values, slow));
    let line: Vec<f64> = f.iter().zip(&s).map(|(a, b)| a - b).collect();
    let sig = ema(&line, signal);
    let hist = line.iter().zip(&sig).map(|(a, b)| a - b).collect();
    (line, sig, hist)
}

/// The true range of each bar: the widest of high to low and the gaps from the previous close.
pub fn true_range(high: &[f64], low: &[f64], close: &[f64]) -> Vec<f64> {
    (0..high.len())
        .map(|i| {
            let range = high[i] - low[i];
            if i == 0 || !close[i - 1].is_finite() {
                range
            } else {
                range
                    .max((high[i] - close[i - 1]).abs())
                    .max((low[i] - close[i - 1]).abs())
            }
        })
        .collect()
}

/// Wilder's average true range.
pub fn atr(high: &[f64], low: &[f64], close: &[f64], period: usize) -> Vec<f64> {
    rma(&true_range(high, low, close), period)
}

/// The slow stochastic oscillator: `%K` smoothed over `smooth` points, and its `%D` average.
pub fn stochastic(
    high: &[f64],
    low: &[f64],
    close: &[f64],
    period: usize,
    smooth: usize,
    d: usize,
) -> (Vec<f64>, Vec<f64>) {
    let (hh, ll) = (highest(high, period), lowest(low, period));
    let raw: Vec<f64> = (0..close.len())
        .map(|i| {
            let span = hh[i] - ll[i];
            if span.is_finite() && span > 0.0 {
                (close[i] - ll[i]) / span * 100.0
            } else if span == 0.0 {
                50.0
            } else {
                f64::NAN
            }
        })
        .collect();
    let k = sma(&raw, smooth.max(1));
    let d_line = sma(&k, d.max(1));
    (k, d_line)
}

/// The commodity channel index: the distance of the typical price from its average, in units of
/// its mean absolute deviation scaled by 0.015.
pub fn cci(typical: &[f64], period: usize) -> Vec<f64> {
    let mean = sma(typical, period);
    let mut out = vec![f64::NAN; typical.len()];
    for i in period.saturating_sub(1)..typical.len() {
        if !mean[i].is_finite() || period == 0 {
            continue;
        }
        let window = &typical[i + 1 - period..=i];
        let deviation = window.iter().map(|v| (v - mean[i]).abs()).sum::<f64>() / period as f64;
        out[i] = if deviation > 0.0 {
            (typical[i] - mean[i]) / (0.015 * deviation)
        } else {
            0.0
        };
    }
    out
}

/// Williams %R: where the close sits in the range of the last `period` bars, from -100 to 0.
pub fn williams_r(high: &[f64], low: &[f64], close: &[f64], period: usize) -> Vec<f64> {
    let (hh, ll) = (highest(high, period), lowest(low, period));
    (0..close.len())
        .map(|i| {
            let span = hh[i] - ll[i];
            if span > 0.0 {
                (hh[i] - close[i]) / span * -100.0
            } else if span == 0.0 {
                -50.0
            } else {
                f64::NAN
            }
        })
        .collect()
}

/// On balance volume: volume added on an up close, taken away on a down close.
pub fn obv(close: &[f64], volume: &[f64]) -> Vec<f64> {
    let mut out = vec![f64::NAN; close.len()];
    let mut total = 0.0;
    for i in 0..close.len() {
        if i > 0 && close[i].is_finite() && close[i - 1].is_finite() {
            if close[i] > close[i - 1] {
                total += volume[i];
            } else if close[i] < close[i - 1] {
                total -= volume[i];
            }
        }
        out[i] = total;
    }
    out
}

/// Momentum: the change over `period` points.
pub fn momentum(values: &[f64], period: usize) -> Vec<f64> {
    (0..values.len())
        .map(|i| {
            if i >= period {
                values[i] - values[i - period]
            } else {
                f64::NAN
            }
        })
        .collect()
}

/// Wilder's directional movement: `+DI`, `-DI` and `ADX`.
pub fn dmi(
    high: &[f64],
    low: &[f64],
    close: &[f64],
    period: usize,
    smoothing: usize,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let n = high.len();
    let mut plus = vec![f64::NAN; n];
    let mut minus = vec![f64::NAN; n];
    for i in 1..n {
        let up = high[i] - high[i - 1];
        let down = low[i - 1] - low[i];
        plus[i] = if up > down && up > 0.0 { up } else { 0.0 };
        minus[i] = if down > up && down > 0.0 { down } else { 0.0 };
    }
    let mut tr = true_range(high, low, close);
    if n > 0 {
        // The first bar has no previous close: it only seeds the ranges above.
        tr[0] = f64::NAN;
    }
    let (atr, p, m) = (rma(&tr, period), rma(&plus, period), rma(&minus, period));
    let plus_di: Vec<f64> = (0..n)
        .map(|i| {
            if atr[i] > 0.0 {
                p[i] / atr[i] * 100.0
            } else {
                f64::NAN
            }
        })
        .collect();
    let minus_di: Vec<f64> = (0..n)
        .map(|i| {
            if atr[i] > 0.0 {
                m[i] / atr[i] * 100.0
            } else {
                f64::NAN
            }
        })
        .collect();
    let dx: Vec<f64> = (0..n)
        .map(|i| {
            let sum = plus_di[i] + minus_di[i];
            if sum > 0.0 {
                (plus_di[i] - minus_di[i]).abs() / sum * 100.0
            } else if sum == 0.0 {
                0.0
            } else {
                f64::NAN
            }
        })
        .collect();
    (plus_di, minus_di, rma(&dx, smoothing))
}

/// Parabolic SAR with the usual acceleration start, step and maximum.
pub fn parabolic_sar(high: &[f64], low: &[f64], start: f64, step: f64, max: f64) -> Vec<f64> {
    let n = high.len();
    let mut out = vec![f64::NAN; n];
    if n < 2 {
        return out;
    }
    let mut long = high[1] >= high[0];
    let mut af = start;
    let mut ep = if long {
        high[0].max(high[1])
    } else {
        low[0].min(low[1])
    };
    let mut sar = if long { low[0] } else { high[0] };
    out[1] = sar;
    for i in 2..n {
        sar += af * (ep - sar);
        if long {
            sar = sar.min(low[i - 1]).min(low[i - 2]);
            if low[i] < sar {
                long = false;
                sar = ep;
                ep = low[i];
                af = start;
            } else if high[i] > ep {
                ep = high[i];
                af = (af + step).min(max);
            }
        } else {
            sar = sar.max(high[i - 1]).max(high[i - 2]);
            if high[i] > sar {
                long = true;
                sar = ep;
                ep = high[i];
                af = start;
            } else if low[i] < ep {
                ep = low[i];
                af = (af + step).min(max);
            }
        }
        out[i] = sar;
    }
    out
}

/// The volume weighted average of `values`, started over on every new `day`.
pub fn vwap(values: &[f64], volume: &[f64], day: &[i64]) -> Vec<f64> {
    let mut out = vec![f64::NAN; values.len()];
    let (mut weighted, mut total, mut current) = (0.0, 0.0, None);
    for i in 0..values.len() {
        if current != Some(day[i]) {
            current = Some(day[i]);
            weighted = 0.0;
            total = 0.0;
        }
        let v = volume[i].max(0.0);
        weighted += values[i] * v;
        total += v;
        out[i] = if total > 0.0 {
            weighted / total
        } else {
            values[i]
        };
    }
    out
}

/// The volume weighted moving average over `period` points.
pub fn vwma(values: &[f64], volume: &[f64], period: usize) -> Vec<f64> {
    let weighted: Vec<f64> = values.iter().zip(volume).map(|(v, w)| v * w).collect();
    let (top, bottom) = (sma(&weighted, period), sma(volume, period));
    top.iter()
        .zip(&bottom)
        .map(|(t, b)| if *b > 0.0 { t / b } else { f64::NAN })
        .collect()
}

/// The sum of each window of `period` points.
pub fn sum(values: &[f64], period: usize) -> Vec<f64> {
    sma(values, period)
        .into_iter()
        .map(|mean| mean * period as f64)
        .collect()
}

/// The running total of `values`. A gap adds nothing.
pub fn cumulative(values: &[f64]) -> Vec<f64> {
    let mut total = 0.0;
    values
        .iter()
        .map(|v| {
            if v.is_finite() {
                total += v;
            }
            total
        })
        .collect()
}

/// The rate of change over `period` points, in percent.
pub fn roc(values: &[f64], period: usize) -> Vec<f64> {
    (0..values.len())
        .map(|i| {
            if i >= period && values[i - period] != 0.0 {
                (values[i] / values[i - period] - 1.0) * 100.0
            } else {
                f64::NAN
            }
        })
        .collect()
}

/// The least squares line through each window of `period` points: its value at the newest point
/// and its slope per point.
pub fn linear_regression(values: &[f64], period: usize) -> (Vec<f64>, Vec<f64>) {
    let n = values.len();
    let (mut level, mut slope) = (vec![f64::NAN; n], vec![f64::NAN; n]);
    if period < 2 {
        return (level, slope);
    }
    let p = period as f64;
    let mean_x = (p - 1.0) / 2.0;
    let sxx: f64 = (0..period).map(|k| (k as f64 - mean_x).powi(2)).sum();
    for i in period - 1..n {
        let window = &values[i + 1 - period..=i];
        if !window.iter().all(|v| v.is_finite()) {
            continue;
        }
        let mean_y = window.iter().sum::<f64>() / p;
        let sxy: f64 = window
            .iter()
            .enumerate()
            .map(|(k, y)| (k as f64 - mean_x) * (y - mean_y))
            .sum();
        let b = sxy / sxx;
        slope[i] = b;
        level[i] = mean_y + b * (p - 1.0 - mean_x);
    }
    (level, slope)
}

/// The money flow index: a volume weighted RSI of the typical price, from 0 to 100.
pub fn mfi(typical: &[f64], volume: &[f64], period: usize) -> Vec<f64> {
    let n = typical.len();
    let (mut up, mut down) = (vec![f64::NAN; n], vec![f64::NAN; n]);
    for i in 1..n {
        let flow = typical[i] * volume[i];
        if typical[i] > typical[i - 1] {
            (up[i], down[i]) = (flow, 0.0);
        } else if typical[i] < typical[i - 1] {
            (up[i], down[i]) = (0.0, flow);
        } else {
            (up[i], down[i]) = (0.0, 0.0);
        }
    }
    let (up, down) = (sum(&up, period), sum(&down, period));
    up.iter()
        .zip(&down)
        .map(|(u, d)| {
            if !(u.is_finite() && d.is_finite()) {
                f64::NAN
            } else if *d == 0.0 {
                if *u == 0.0 { 50.0 } else { 100.0 }
            } else {
                100.0 - 100.0 / (1.0 + u / d)
            }
        })
        .collect()
}

/// Supertrend: a line that follows the price at `mult` average true ranges and flips when the
/// close goes through it. Returns the line and the direction (1 for up, -1 for down).
pub fn supertrend(
    high: &[f64],
    low: &[f64],
    close: &[f64],
    period: usize,
    mult: f64,
) -> (Vec<f64>, Vec<f64>) {
    let n = close.len();
    let range = atr(high, low, close, period);
    let (mut line, mut direction) = (vec![f64::NAN; n], vec![f64::NAN; n]);
    let (mut upper, mut lower) = (f64::NAN, f64::NAN);
    let mut trend = 1.0;
    for i in 0..n {
        if !range[i].is_finite() {
            continue;
        }
        let mid = (high[i] + low[i]) / 2.0;
        let (basic_upper, basic_lower) = (mid + mult * range[i], mid - mult * range[i]);
        let first = !upper.is_finite();
        let previous_close = if i > 0 { close[i - 1] } else { close[i] };
        let new_upper = if first || basic_upper < upper || previous_close > upper {
            basic_upper
        } else {
            upper
        };
        let new_lower = if first || basic_lower > lower || previous_close < lower {
            basic_lower
        } else {
            lower
        };
        if !first {
            if trend < 0.0 && close[i] > upper {
                trend = 1.0;
            } else if trend > 0.0 && close[i] < lower {
                trend = -1.0;
            }
        }
        upper = new_upper;
        lower = new_lower;
        direction[i] = trend;
        line[i] = if trend > 0.0 { lower } else { upper };
    }
    (line, direction)
}

/// How many bars ago `condition` was last true (0 on the bar it is true). No value before the
/// first time.
pub fn bars_since(condition: &[f64]) -> Vec<f64> {
    let mut since = f64::NAN;
    condition
        .iter()
        .map(|c| {
            if *c != 0.0 && !c.is_nan() {
                since = 0.0;
            } else if since.is_finite() {
                since += 1.0;
            }
            since
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 || (a.is_nan() && b.is_nan())
    }

    fn all_close(a: &[f64], b: &[f64]) {
        assert_eq!(a.len(), b.len());
        for (i, (x, y)) in a.iter().zip(b).enumerate() {
            assert!(close(*x, *y), "at {i}: {x} vs {y}");
        }
    }

    const NAN: f64 = f64::NAN;

    #[test]
    fn a_simple_average_waits_for_its_period() {
        all_close(
            &sma(&[1.0, 2.0, 3.0, 4.0, 5.0], 3),
            &[NAN, NAN, 2.0, 3.0, 4.0],
        );
        assert!(sma(&[1.0, 2.0], 0).iter().all(|v| v.is_nan()));
    }

    #[test]
    fn a_gap_restarts_an_average() {
        all_close(
            &sma(&[1.0, 2.0, NAN, 4.0, 6.0], 2),
            &[NAN, 1.5, NAN, NAN, 5.0],
        );
    }

    #[test]
    fn an_exponential_average_is_seeded_with_the_simple_one() {
        // Period 3: alpha 0.5, seed (1 + 2 + 3) / 3 = 2.
        all_close(
            &ema(&[1.0, 2.0, 3.0, 4.0, 5.0], 3),
            &[NAN, NAN, 2.0, 3.0, 4.0],
        );
        // Wilder's: alpha 1/3, seed 2, then 2 + (4 - 2) / 3.
        let r = rma(&[1.0, 2.0, 3.0, 4.0], 3);
        assert!(close(r[3], 2.0 + 2.0 / 3.0));
    }

    #[test]
    fn a_weighted_average_favors_the_newest() {
        // (1*1 + 2*2 + 3*3) / 6
        all_close(&wma(&[1.0, 2.0, 3.0], 3), &[NAN, NAN, 14.0 / 6.0]);
        let h = hma(&(1..=40).map(f64::from).collect::<Vec<_>>(), 9);
        // On a straight line the Hull average has no lag at all.
        assert!(close(h[39], 40.0), "{}", h[39]);
    }

    #[test]
    fn the_deviation_is_the_population_one() {
        let d = stdev(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0], 8);
        assert!(close(d[7], 2.0));
    }

    #[test]
    fn rsi_is_bounded_and_saturates_on_a_one_way_move() {
        let up: Vec<f64> = (0..30).map(f64::from).collect();
        let r = rsi(&up, 14);
        assert!(r[13].is_nan());
        assert!(close(r[14], 100.0));
        let flat = vec![5.0; 30];
        assert!(close(rsi(&flat, 14)[20], 50.0));
        let zig: Vec<f64> = (0..60)
            .map(|i| if i % 2 == 0 { 1.0 } else { 2.0 })
            .collect();
        let r = rsi(&zig, 14);
        assert!(r[40] > 40.0 && r[40] < 60.0, "{}", r[40]);
    }

    #[test]
    fn rsi_matches_wilders_worked_example() {
        // Gains and losses of 1 alternating after a rise: first average gain 8/14 over the first
        // 14 changes of the series below, first average loss 6/14.
        let mut prices = vec![10.0];
        for i in 0..14 {
            let last = *prices.last().unwrap();
            prices.push(if i < 8 { last + 1.0 } else { last - 1.0 });
        }
        let r = rsi(&prices, 14);
        let rs: f64 = (8.0 / 14.0) / (6.0 / 14.0);
        assert!(close(r[14], 100.0 - 100.0 / (1.0 + rs)));
    }

    #[test]
    fn macd_is_the_gap_between_two_averages() {
        let prices: Vec<f64> = (0..60).map(|i| 100.0 + f64::from(i)).collect();
        let (line, signal, hist) = macd(&prices, 12, 26, 9);
        // On a straight line both averages lag by a constant: (26 - 12) / 2 = 7.
        assert!(close(line[59], 7.0), "{}", line[59]);
        assert!(close(signal[59], 7.0));
        assert!(close(hist[59], 0.0));
        assert!(line[24].is_nan() && line[25].is_finite());
        assert!(signal[32].is_nan() && signal[33].is_finite());
    }

    #[test]
    fn the_true_range_counts_the_gap_from_the_previous_close() {
        let tr = true_range(&[10.0, 15.0], &[8.0, 13.0], &[9.0, 14.0]);
        assert_eq!(tr, vec![2.0, 6.0]);
    }

    #[test]
    fn stochastic_and_williams_agree() {
        let high = vec![10.0, 12.0, 14.0, 13.0];
        let low = vec![8.0, 9.0, 10.0, 11.0];
        let close_ = vec![9.0, 11.0, 13.0, 12.0];
        let (k, _) = stochastic(&high, &low, &close_, 3, 1, 1);
        let w = williams_r(&high, &low, &close_, 3);
        // Last bar: range 9..14, close 12 -> 60% of the way up, %R -40.
        assert!(close(k[3], 60.0));
        assert!(close(w[3], -40.0));
    }

    #[test]
    fn cci_is_zero_on_the_average() {
        let flat = vec![5.0; 30];
        assert!(close(cci(&flat, 20)[25], 0.0));
    }

    #[test]
    fn obv_adds_and_takes_away() {
        assert_eq!(
            obv(&[1.0, 2.0, 1.0, 1.0], &[5.0, 3.0, 2.0, 7.0]),
            vec![0.0, 3.0, 1.0, 1.0]
        );
    }

    #[test]
    fn highest_and_lowest_follow_the_window() {
        all_close(&highest(&[1.0, 3.0, 2.0, 0.0], 2), &[NAN, 3.0, 3.0, 2.0]);
        all_close(&lowest(&[1.0, 3.0, 2.0, 0.0], 2), &[NAN, 1.0, 2.0, 0.0]);
        all_close(&momentum(&[1.0, 3.0, 6.0], 1), &[NAN, 2.0, 3.0]);
    }

    #[test]
    fn a_steady_rise_has_a_strong_adx_and_positive_di_on_top() {
        let high: Vec<f64> = (0..80).map(|i| 10.0 + f64::from(i)).collect();
        let low: Vec<f64> = high.iter().map(|h| h - 2.0).collect();
        let close_: Vec<f64> = high.iter().map(|h| h - 1.0).collect();
        let (plus, minus, adx) = dmi(&high, &low, &close_, 14, 14);
        assert!(plus[70] > minus[70]);
        assert!(adx[70] > 90.0, "{}", adx[70]);
    }

    #[test]
    fn the_parabolic_sar_trails_below_a_rise() {
        let high: Vec<f64> = (0..30).map(|i| 10.0 + f64::from(i)).collect();
        let low: Vec<f64> = high.iter().map(|h| h - 1.0).collect();
        let sar = parabolic_sar(&high, &low, 0.02, 0.02, 0.2);
        assert!(sar[29] < low[29]);
        assert!(sar[29] > sar[10]);
    }

    #[test]
    fn vwap_weighs_by_volume_and_starts_over_each_day() {
        let out = vwap(
            &[10.0, 20.0, 30.0, 40.0],
            &[1.0, 3.0, 1.0, 1.0],
            &[0, 0, 1, 1],
        );
        assert!(close(out[0], 10.0));
        assert!(close(out[1], 17.5));
        assert!(close(out[2], 30.0), "{out:?}");
        assert!(close(out[3], 35.0));
    }

    #[test]
    fn a_regression_through_a_straight_line_is_that_line() {
        let line: Vec<f64> = (0..10).map(|i| 3.0 + 2.0 * f64::from(i)).collect();
        let (level, slope) = linear_regression(&line, 5);
        assert!(level[3].is_nan());
        for i in 4..10 {
            assert!(close(level[i], line[i]), "{i}");
            assert!(close(slope[i], 2.0));
        }
    }

    #[test]
    fn sums_totals_and_changes_are_worked_out_by_hand() {
        assert_eq!(sum(&[1.0, 2.0, 3.0, 4.0], 2)[1..], [3.0, 5.0, 7.0]);
        assert_eq!(cumulative(&[1.0, f64::NAN, 2.0, 3.0]), [1.0, 1.0, 3.0, 6.0]);
        let r = roc(&[100.0, 110.0, 99.0], 1);
        assert!(r[0].is_nan());
        assert!(close(r[1], 10.0) && close(r[2], -10.0));
        assert!(close(vwma(&[1.0, 3.0], &[1.0, 3.0], 2)[1], 2.5));
    }

    #[test]
    fn bars_since_counts_from_the_last_time_the_condition_held() {
        let out = bars_since(&[0.0, 1.0, 0.0, 0.0, 1.0, 0.0]);
        assert!(out[0].is_nan());
        assert_eq!(out[1..], [0.0, 1.0, 2.0, 0.0, 1.0]);
    }

    #[test]
    fn the_money_flow_index_stays_between_0_and_100() {
        let typical: Vec<f64> = (0..40)
            .map(|i| 100.0 + (f64::from(i) * 0.5).sin() * 4.0)
            .collect();
        let out = mfi(&typical, &vec![10.0; 40], 14);
        assert!(out[..14].iter().all(|v| v.is_nan()));
        assert!(out[14..].iter().all(|v| (0.0..=100.0).contains(v)));
        let rising: Vec<f64> = (0..30).map(f64::from).collect();
        assert!(close(mfi(&rising, &vec![1.0; 30], 5)[29], 100.0));
    }

    #[test]
    fn supertrend_flips_when_the_close_goes_through_the_line() {
        let close: Vec<f64> = (0..60)
            .map(|i| {
                if i < 30 {
                    100.0 + f64::from(i)
                } else {
                    130.0 - 2.0 * f64::from(i - 30)
                }
            })
            .collect();
        let high: Vec<f64> = close.iter().map(|c| c + 1.0).collect();
        let low: Vec<f64> = close.iter().map(|c| c - 1.0).collect();
        let (line, direction) = supertrend(&high, &low, &close, 5, 2.0);
        assert_eq!(direction[25], 1.0);
        assert_eq!(direction[59], -1.0);
        assert!(line[25] < close[25] && line[59] > close[59]);
    }
}
