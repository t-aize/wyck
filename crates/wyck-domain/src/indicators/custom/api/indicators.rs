//! The ready made indicators a script can call, and the colors it can name.
//!
//! An indicator that reads prices takes them from the bars of the chart (`atr(14)`), or from the
//! bars a script got with `bars_of` when they come first (`atr(bars, 14)`).

use super::{charge_window, columns, map_of, period, scalar};

use crate::base::calendar::Anchor;
use crate::base::levels::{self, PivotKind};
use crate::base::ta::{self, MovingAverage, Series as _};
use crate::indicators::custom::series::{Fallible, Series};
use rhai::{Dynamic, Engine, Map};

/// The bars an indicator reads.
struct Bars {
    open: Series,
    high: Series,
    low: Series,
    close: Series,
    volume: Series,
    len: usize,
}

impl Bars {
    /// The bars of the chart the script runs on.
    fn chart() -> Fallible<Self> {
        let c = columns()?;
        Ok(Self {
            open: c.open,
            high: c.high,
            low: c.low,
            close: c.close,
            volume: c.volume,
            len: c.len,
        })
    }

    /// The bars of a map such as `bars_of` gives: `open`, `high`, `low`, `close`, `volume`.
    fn from_map(map: &Map) -> Fallible<Self> {
        let series = |key: &str| -> Fallible<Series> {
            map.get(key)
                .and_then(|value| value.clone().try_cast::<Series>())
                .ok_or_else(|| format!("bars: the map has no series called \"{key}\"").into())
        };
        let bars = Self {
            open: series("open")?,
            high: series("high")?,
            low: series("low")?,
            close: series("close")?,
            volume: series("volume")?,
            len: 0,
        };
        let len = bars.close.len();
        let all = [&bars.open, &bars.high, &bars.low, &bars.volume];
        if all.iter().any(|s| s.len() != len) {
            return Err("bars: the series of a map must be as long as each other".into());
        }
        Ok(Self { len, ..bars })
    }

    /// The typical price: the mean of the high, the low and the close.
    fn typical(&self) -> Vec<f64> {
        (0..self.len)
            .map(|i| {
                (self.high.values().at(i) + self.low.values().at(i) + self.close.values().at(i))
                    / 3.0
            })
            .collect()
    }
}

/// Registers `name` twice: with the arguments given, reading the bars of the chart, and with a
/// map of bars first. `$body` sees the bars as `$bars`.
macro_rules! indicator {
    ($engine:expr, $name:literal, ($($arg:ident: $ty:ty),*) -> $ret:ty, |$bars:ident| $body:expr) => {{
        $engine.register_fn($name, |$($arg: $ty),*| -> Fallible<$ret> {
            let $bars = Bars::chart()?;
            $body
        });
        $engine.register_fn($name, |map: Map, $($arg: $ty),*| -> Fallible<$ret> {
            let $bars = Bars::from_map(&map)?;
            $body
        });
    }};
}

pub(super) fn indicators(engine: &mut Engine) {
    ranges(engine);
    volume_flows(engine);
    families(engine);
    anchored(engine);
}

/// Indicators of the range and of the momentum of a bar.
fn ranges(engine: &mut Engine) {
    indicator!(engine, "tr", () -> Series, |b| {
        Ok(Series::new(ta::true_range(b.high.values(), b.low.values(), b.close.values())))
    });
    indicator!(engine, "atr", (n: i64) -> Series, |b| {
        let p = period("atr", n)?;
        Ok(Series::new(ta::atr(b.high.values(), b.low.values(), b.close.values(), p)))
    });
    // `atr(length, "sma")`: the average true range smoothed by another average.
    engine.register_fn("atr", |n: i64, method: &str| -> Fallible<Series> {
        let c = Bars::chart()?;
        let p = period("atr", n)?;
        let kind = MovingAverage::parse(method).ok_or_else(|| {
            format!("atr: \"{method}\" is not an average (use sma, ema, wma, rma or hma)")
        })?;
        let range = ta::true_range(c.high.values(), c.low.values(), c.close.values());
        Ok(Series::new(kind.of(&range, p)))
    });
    indicator!(engine, "cci", (n: i64) -> Series, |b| {
        let p = period("cci", n)?;
        charge_window(b.len, p)?;
        Ok(Series::new(ta::cci(&b.typical(), p)))
    });
    // `cci(series, length)`: the commodity channel index of any series.
    engine.register_fn("cci", |s: Series, n: i64| -> Fallible<Series> {
        let p = period("cci", n)?;
        charge_window(s.len(), p)?;
        Ok(Series::new(ta::cci(s.values(), p)))
    });
    indicator!(engine, "williams_r", (n: i64) -> Series, |b| {
        let p = period("williams_r", n)?;
        Ok(Series::new(ta::williams_r(b.high.values(), b.low.values(), b.close.values(), p)))
    });
    indicator!(engine, "psar", (start: Dynamic, step: Dynamic, max: Dynamic) -> Series, |b| {
        let (start, step, max) = (
            scalar("psar", &start)?,
            scalar("psar", &step)?,
            scalar("psar", &max)?,
        );
        Ok(Series::new(ta::parabolic_sar(b.high.values(), b.low.values(), start, step, max)))
    });
    // `wpr(length)` and `sar(start, step, max)`: the names of Pine for williams_r and psar.
    indicator!(engine, "wpr", (n: i64) -> Series, |b| {
        let p = period("wpr", n)?;
        Ok(Series::new(ta::williams_r(b.high.values(), b.low.values(), b.close.values(), p)))
    });
    indicator!(engine, "sar", (start: Dynamic, step: Dynamic, max: Dynamic) -> Series, |b| {
        let (start, step, max) = (
            scalar("sar", &start)?,
            scalar("sar", &step)?,
            scalar("sar", &max)?,
        );
        Ok(Series::new(ta::parabolic_sar(b.high.values(), b.low.values(), start, step, max)))
    });
    indicator!(engine, "stoch", (n: i64, smooth: i64, d: i64) -> Map, |b| {
        let (n, smooth, d) = (
            period("stoch", n)?,
            period("stoch", smooth)?,
            period("stoch", d)?,
        );
        let (k, d_line) =
            ta::stochastic(b.high.values(), b.low.values(), b.close.values(), n, smooth, d);
        Ok(map_of(vec![("k", Series::new(k)), ("d", Series::new(d_line))]))
    });
    // `stoch(close, high, low, length)`: the raw %K of Pine, with no smoothing.
    engine.register_fn(
        "stoch",
        |source: Series, high: Series, low: Series, n: i64| -> Fallible<Series> {
            let p = period("stoch", n)?;
            if source.len() != high.len() || source.len() != low.len() {
                return Err("stoch: the three series must be as long as each other".into());
            }
            let (hh, ll) = (ta::highest(high.values(), p), ta::lowest(low.values(), p));
            Ok(Series::new(
                (0..source.len())
                    .map(|i| {
                        let span = hh.at(i) - ll.at(i);
                        if span > 0.0 {
                            100.0 * (source.values().at(i) - ll.at(i)) / span
                        } else if span == 0.0 {
                            50.0
                        } else {
                            f64::NAN
                        }
                    })
                    .collect(),
            ))
        },
    );
    indicator!(engine, "dmi", (n: i64, smoothing: i64) -> Map, |b| {
        let (n, smoothing) = (period("dmi", n)?, period("dmi", smoothing)?);
        let (plus, minus, adx) =
            ta::dmi(b.high.values(), b.low.values(), b.close.values(), n, smoothing);
        Ok(map_of(vec![
            ("plus", Series::new(plus)),
            ("minus", Series::new(minus)),
            ("adx", Series::new(adx)),
        ]))
    });
    indicator!(engine, "supertrend", (n: i64, mult: Dynamic) -> Map, |b| {
        let p = period("supertrend", n)?;
        let mult = scalar("supertrend", &mult)?;
        let (line, direction) =
            ta::supertrend(b.high.values(), b.low.values(), b.close.values(), p, mult);
        Ok(map_of(vec![
            ("line", Series::new(line)),
            ("direction", Series::new(direction)),
        ]))
    });
}

/// The money flow and the volume indices, each as a series.
fn volume_flows(engine: &mut Engine) {
    indicator!(engine, "obv", () -> Series, |b| {
        Ok(Series::new(ta::obv(b.close.values(), b.volume.values())))
    });
    indicator!(engine, "mfi", (n: i64) -> Series, |b| {
        let p = period("mfi", n)?;
        Ok(Series::new(ta::mfi(&b.typical(), b.volume.values(), p)))
    });
    // `mfi(series, length)`: the money flow index of any series, weighted by the chart's volume.
    engine.register_fn("mfi", |s: Series, n: i64| -> Fallible<Series> {
        let c = Bars::chart()?;
        let p = period("mfi", n)?;
        if s.len() != c.len {
            return Err("mfi: the series must have a value for every bar".into());
        }
        Ok(Series::new(ta::mfi(s.values(), c.volume.values(), p)))
    });
    indicator!(engine, "money_flow", () -> Series, |b| {
        Ok(Series::new(ta::money_flow_volume(
            b.high.values(), b.low.values(), b.close.values(), b.volume.values(),
        )))
    });
    indicator!(engine, "accdist", () -> Series, |b| {
        Ok(Series::new(ta::accumulation_distribution(
            b.high.values(), b.low.values(), b.close.values(), b.volume.values(),
        )))
    });
    indicator!(engine, "pvt", () -> Series, |b| {
        Ok(Series::new(ta::price_volume_trend(b.close.values(), b.volume.values())))
    });
    indicator!(engine, "nvi", () -> Series, |b| {
        Ok(Series::new(ta::negative_volume_index(b.close.values(), b.volume.values())))
    });
    indicator!(engine, "pvi", () -> Series, |b| {
        Ok(Series::new(ta::positive_volume_index(b.close.values(), b.volume.values())))
    });
    indicator!(engine, "wad", () -> Series, |b| {
        Ok(Series::new(ta::williams_accumulation_distribution(
            b.high.values(), b.low.values(), b.close.values(),
        )))
    });
    indicator!(engine, "wvad", () -> Series, |b| {
        Ok(Series::new(ta::williams_variable_accumulation_distribution(
            b.open.values(), b.high.values(), b.low.values(), b.close.values(), b.volume.values(),
        )))
    });
    indicator!(engine, "iii", () -> Series, |b| {
        Ok(Series::new(ta::intraday_intensity(
            b.high.values(), b.low.values(), b.close.values(), b.volume.values(),
        )))
    });
}

/// The indicators that give several series at once, as a map: `let m = macd(close, 12, 26, 9);
/// m.hist`.
fn families(engine: &mut Engine) {
    engine.register_fn(
        "macd",
        |s: Series, fast: i64, slow: i64, signal: i64| -> Fallible<Map> {
            let (f, sl, sg) = (
                period("macd", fast)?,
                period("macd", slow)?,
                period("macd", signal)?,
            );
            let (line, sig, hist) = ta::macd(s.values(), f, sl, sg);
            Ok(map_of(vec![
                ("macd", Series::new(line)),
                ("signal", Series::new(sig)),
                ("hist", Series::new(hist)),
            ]))
        },
    );
    engine.register_fn(
        "bollinger",
        |s: Series, n: i64, mult: Dynamic| -> Fallible<Map> {
            bollinger(&s, n, &mult, "bollinger")
        },
    );
    // `bb(series, length, mult)`: the name of Pine for `bollinger`.
    engine.register_fn("bb", |s: Series, n: i64, mult: Dynamic| -> Fallible<Map> {
        bollinger(&s, n, &mult, "bb")
    });
    // `bbw(series, length, mult)`: the width of the Bollinger bands over their basis.
    engine.register_fn(
        "bbw",
        |s: Series, n: i64, mult: Dynamic| -> Fallible<Series> {
            let p = period("bbw", n)?;
            let mult = scalar("bbw", &mult)?;
            charge_window(s.len(), p)?;
            let (basis, dev) = (ta::sma(s.values(), p), ta::stdev(s.values(), p));
            Ok(Series::new(
                basis
                    .iter()
                    .zip(&dev)
                    .map(|(b, d)| {
                        if *b == 0.0 {
                            f64::NAN
                        } else {
                            2.0 * mult * d / b
                        }
                    })
                    .collect(),
            ))
        },
    );
    indicator!(engine, "donchian", (n: i64) -> Map, |b| {
        let p = period("donchian", n)?;
        let upper = ta::highest(b.high.values(), p);
        let lower = ta::lowest(b.low.values(), p);
        let basis = upper.iter().zip(&lower).map(|(u, l)| (u + l) / 2.0).collect();
        Ok(map_of(vec![
            ("upper", Series::new(upper)),
            ("lower", Series::new(lower)),
            ("basis", Series::new(basis)),
        ]))
    });
    indicator!(engine, "keltner", (n: i64, mult: Dynamic, atr_n: i64) -> Map, |b| {
        let (p, a) = (period("keltner", n)?, period("keltner", atr_n)?);
        let mult = scalar("keltner", &mult)?;
        let basis = ta::ema(b.close.values(), p);
        let range = ta::atr(b.high.values(), b.low.values(), b.close.values(), a);
        Ok(channel(&basis, &range, mult))
    });
    // `kc(series, length, mult, use_true_range?)`: the Keltner channel of Pine: an exponential
    // average with bands at mult times the average of the true range (or of the high minus the
    // low). Gives .basis, .upper and .lower.
    engine.register_fn("kc", |s: Series, n: i64, mult: Dynamic| -> Fallible<Map> {
        kc(&s, n, &mult, true)
    });
    engine.register_fn(
        "kc",
        |s: Series, n: i64, mult: Dynamic, use_true_range: bool| -> Fallible<Map> {
            kc(&s, n, &mult, use_true_range)
        },
    );
    // `kcw(series, length, mult)`: the width of the Keltner channel over its basis.
    engine.register_fn(
        "kcw",
        |s: Series, n: i64, mult: Dynamic| -> Fallible<Series> {
            let bands = kc(&s, n, &mult, true)?;
            let take = |key: &str| {
                bands
                    .get(key)
                    .and_then(|v| v.clone().try_cast::<Series>())
                    .unwrap_or_default()
            };
            let (upper, lower, basis) = (take("upper"), take("lower"), take("basis"));
            Ok(Series::new(
                (0..basis.len())
                    .map(|i| {
                        let b = basis.values().at(i);
                        if b == 0.0 {
                            f64::NAN
                        } else {
                            (upper.values().at(i) - lower.values().at(i)) / b
                        }
                    })
                    .collect(),
            ))
        },
    );
}

/// The indicators that start over with each day, week, month or year.
fn anchored(engine: &mut Engine) {
    engine.register_fn("vwap", || -> Fallible<Series> {
        let c = columns()?;
        Ok(Series::new(ta::vwap(
            c.hlc3.values(),
            c.volume.values(),
            &c.day,
        )))
    });
    engine.register_fn("vwap", |s: Series| -> Fallible<Series> {
        let c = columns()?;
        if s.len() != c.len {
            return Err("vwap: the series must have a value for every bar".into());
        }
        Ok(Series::new(ta::vwap(s.values(), c.volume.values(), &c.day)))
    });
    // `vwap(series, "week", 2)`: the price over the volume since the start of the day, week,
    // month or year, with bands at 2 deviations. Gives .vwap, .upper and .lower.
    engine.register_fn(
        "vwap",
        |s: Series, anchor: &str, mult: Dynamic| -> Fallible<Map> {
            let c = columns()?;
            if s.len() != c.len {
                return Err("vwap: the series must have a value for every bar".into());
            }
            let anchor = anchor_of("vwap", anchor)?;
            let mult = scalar("vwap", &mult)?;
            let (line, spread) = ta::vwap_anchored(s.values(), c.volume.values(), &c.day, anchor);
            let band = |sign: f64| {
                Series::new(
                    line.iter()
                        .zip(&spread)
                        .map(|(v, d)| v + sign * mult * d)
                        .collect(),
                )
            };
            let (upper, lower) = (band(1.0), band(-1.0));
            Ok(map_of(vec![
                ("vwap", Series::new(line)),
                ("upper", upper),
                ("lower", lower),
            ]))
        },
    );
    engine.register_fn("vwap", |s: Series, anchor: &str| -> Fallible<Series> {
        let c = columns()?;
        if s.len() != c.len {
            return Err("vwap: the series must have a value for every bar".into());
        }
        let anchor = anchor_of("vwap", anchor)?;
        Ok(Series::new(
            ta::vwap_anchored(s.values(), c.volume.values(), &c.day, anchor).0,
        ))
    });
    // The levels of the previous day, week, month or year: `pivot_point_levels("traditional",
    // "day")`. Gives .p, .r1, .s1, .r2, .s2, .r3 and .s3 (a kind may have fewer).
    engine.register_fn(
        "pivot_point_levels",
        |kind: &str, anchor: &str| -> Fallible<Map> {
            let (b, c) = (Bars::chart()?, columns()?);
            let kind = PivotKind::parse(kind).ok_or_else(|| {
                format!(
                    "pivot_point_levels: \"{kind}\" is not traditional, fibonacci, woodie, classic, camarilla or dm"
                )
            })?;
            let anchor = anchor_of("pivot_point_levels", anchor)?;
            let columns = levels::pivot_levels(
                b.open.values(),
                b.high.values(),
                b.low.values(),
                b.close.values(),
                &c.day,
                anchor,
                kind,
            );
            let keys = ["p", "r1", "s1", "r2", "s2", "r3", "s3"];
            Ok(map_of(
                keys.into_iter()
                    .zip(columns)
                    .map(|(key, values)| (key, Series::new(values)))
                    .collect(),
            ))
        },
    );
}

fn anchor_of(name: &str, text: &str) -> Fallible<Anchor> {
    Anchor::parse(text)
        .ok_or_else(|| format!("{name}: \"{text}\" is not day, week, month or year").into())
}

/// The bands of Bollinger around a basis: `mult` deviations either way.
fn bollinger(s: &Series, n: i64, mult: &Dynamic, name: &str) -> Fallible<Map> {
    let p = period(name, n)?;
    let mult = scalar(name, mult)?;
    charge_window(s.len(), p)?;
    let (basis, dev) = (ta::sma(s.values(), p), ta::stdev(s.values(), p));
    Ok(channel(&basis, &dev, mult))
}

/// A basis with a line `mult` ranges over it and one under it, as a map.
fn channel(basis: &[f64], range: &[f64], mult: f64) -> Map {
    let band = |sign: f64| -> Series {
        Series::new(
            basis
                .iter()
                .zip(range)
                .map(|(b, r)| b + sign * mult * r)
                .collect(),
        )
    };
    map_of(vec![
        ("basis", Series::new(basis.to_vec())),
        ("upper", band(1.0)),
        ("lower", band(-1.0)),
    ])
}

/// The Keltner channel of Pine over `s`.
fn kc(s: &Series, n: i64, mult: &Dynamic, use_true_range: bool) -> Fallible<Map> {
    let c = Bars::chart()?;
    let p = period("kc", n)?;
    let mult = scalar("kc", mult)?;
    if s.len() != c.len {
        return Err("kc: the series must have a value for every bar".into());
    }
    let spread = if use_true_range {
        ta::true_range(c.high.values(), c.low.values(), c.close.values())
    } else {
        (0..c.len)
            .map(|i| c.high.values().at(i) - c.low.values().at(i))
            .collect()
    };
    let basis = ta::ema(s.values(), p);
    Ok(channel(&basis, &ta::ema(&spread, p), mult))
}

pub(super) fn colors(engine: &mut Engine) {
    engine.register_fn("rgb", |r: i64, g: i64, b: i64| -> String {
        let byte = |v: i64| u32::try_from(v.clamp(0, 255)).unwrap_or_default();
        format!("#{:06x}", byte(r) << 16 | byte(g) << 8 | byte(b))
    });
}
