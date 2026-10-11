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
#[path = "../../../tests/unit/domain/indicators/math.rs"]
mod tests;
