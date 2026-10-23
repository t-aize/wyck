//! The ready made indicators a script can call, and the colors it can name.

use super::{charge_window, columns, map_of, period, scalar};

use crate::base::ta;
use crate::indicators::custom::series::{Fallible, Series};
use rhai::{Dynamic, Engine, Map};

pub(super) fn indicators(engine: &mut Engine) {
    engine.register_fn("tr", || -> Fallible<Series> {
        let c = columns()?;
        Ok(Series::new(ta::true_range(
            c.high.values(),
            c.low.values(),
            c.close.values(),
        )))
    });
    engine.register_fn("atr", |n: i64| -> Fallible<Series> {
        let (c, p) = (columns()?, period("atr", n)?);
        Ok(Series::new(ta::atr(
            c.high.values(),
            c.low.values(),
            c.close.values(),
            p,
        )))
    });
    engine.register_fn("cci", |n: i64| -> Fallible<Series> {
        let (c, p) = (columns()?, period("cci", n)?);
        charge_window(c.len, p)?;
        Ok(Series::new(ta::cci(c.hlc3.values(), p)))
    });
    engine.register_fn("williams_r", |n: i64| -> Fallible<Series> {
        let (c, p) = (columns()?, period("williams_r", n)?);
        Ok(Series::new(ta::williams_r(
            c.high.values(),
            c.low.values(),
            c.close.values(),
            p,
        )))
    });
    engine.register_fn("obv", || -> Fallible<Series> {
        let c = columns()?;
        Ok(Series::new(ta::obv(c.close.values(), c.volume.values())))
    });
    engine.register_fn("mfi", |n: i64| -> Fallible<Series> {
        let (c, p) = (columns()?, period("mfi", n)?);
        Ok(Series::new(ta::mfi(c.hlc3.values(), c.volume.values(), p)))
    });
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
    engine.register_fn(
        "psar",
        |start: Dynamic, step: Dynamic, max: Dynamic| -> Fallible<Series> {
            let c = columns()?;
            let (start, step, max) = (
                scalar("psar", &start)?,
                scalar("psar", &step)?,
                scalar("psar", &max)?,
            );
            Ok(Series::new(ta::parabolic_sar(
                c.high.values(),
                c.low.values(),
                start,
                step,
                max,
            )))
        },
    );

    // These give several series at once, as a map: `let m = macd(close, 12, 26, 9); m.hist`.
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
            let p = period("bollinger", n)?;
            let mult = scalar("bollinger", &mult)?;
            charge_window(s.len(), p)?;
            let (basis, dev) = (ta::sma(s.values(), p), ta::stdev(s.values(), p));
            let band = |sign: f64| -> Series {
                Series::new(
                    basis
                        .iter()
                        .zip(&dev)
                        .map(|(b, d)| b + sign * mult * d)
                        .collect(),
                )
            };
            Ok(map_of(vec![
                ("basis", Series::new(basis.clone())),
                ("upper", band(1.0)),
                ("lower", band(-1.0)),
            ]))
        },
    );
    engine.register_fn("stoch", |n: i64, smooth: i64, d: i64| -> Fallible<Map> {
        let c = columns()?;
        let (n, smooth, d) = (
            period("stoch", n)?,
            period("stoch", smooth)?,
            period("stoch", d)?,
        );
        let (k, d_line) = ta::stochastic(
            c.high.values(),
            c.low.values(),
            c.close.values(),
            n,
            smooth,
            d,
        );
        Ok(map_of(vec![
            ("k", Series::new(k)),
            ("d", Series::new(d_line)),
        ]))
    });
    engine.register_fn("dmi", |n: i64, smoothing: i64| -> Fallible<Map> {
        let c = columns()?;
        let (n, smoothing) = (period("dmi", n)?, period("dmi", smoothing)?);
        let (plus, minus, adx) = ta::dmi(
            c.high.values(),
            c.low.values(),
            c.close.values(),
            n,
            smoothing,
        );
        Ok(map_of(vec![
            ("plus", Series::new(plus)),
            ("minus", Series::new(minus)),
            ("adx", Series::new(adx)),
        ]))
    });
    engine.register_fn("supertrend", |n: i64, mult: Dynamic| -> Fallible<Map> {
        let (c, p) = (columns()?, period("supertrend", n)?);
        let mult = scalar("supertrend", &mult)?;
        let (line, direction) =
            ta::supertrend(c.high.values(), c.low.values(), c.close.values(), p, mult);
        Ok(map_of(vec![
            ("line", Series::new(line)),
            ("direction", Series::new(direction)),
        ]))
    });
    engine.register_fn("donchian", |n: i64| -> Fallible<Map> {
        let (c, p) = (columns()?, period("donchian", n)?);
        let upper = ta::highest(c.high.values(), p);
        let lower = ta::lowest(c.low.values(), p);
        let basis = upper
            .iter()
            .zip(&lower)
            .map(|(u, l)| (u + l) / 2.0)
            .collect();
        Ok(map_of(vec![
            ("upper", Series::new(upper)),
            ("lower", Series::new(lower)),
            ("basis", Series::new(basis)),
        ]))
    });
    engine.register_fn(
        "keltner",
        |n: i64, mult: Dynamic, atr_n: i64| -> Fallible<Map> {
            let c = columns()?;
            let (p, a) = (period("keltner", n)?, period("keltner", atr_n)?);
            let mult = scalar("keltner", &mult)?;
            let basis = ta::ema(c.close.values(), p);
            let range = ta::atr(c.high.values(), c.low.values(), c.close.values(), a);
            let band = |sign: f64| -> Series {
                Series::new(
                    basis
                        .iter()
                        .zip(&range)
                        .map(|(b, r)| b + sign * mult * r)
                        .collect(),
                )
            };
            Ok(map_of(vec![
                ("basis", Series::new(basis.clone())),
                ("upper", band(1.0)),
                ("lower", band(-1.0)),
            ]))
        },
    );
}

pub(super) fn colors(engine: &mut Engine) {
    engine.register_fn("rgb", |r: i64, g: i64, b: i64| -> String {
        let byte = |v: i64| u32::try_from(v.clamp(0, 255)).unwrap_or_default();
        format!("#{:06x}", byte(r) << 16 | byte(g) << 8 | byte(b))
    });
}
