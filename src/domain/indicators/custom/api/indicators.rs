//! The ready made indicators a script can call, and the colors it can name.

use super::{columns, map_of, period};

use crate::domain::indicators::custom::series::{Fallible, Series};
use crate::domain::indicators::math;
use rhai::{Engine, Map};

pub(super) fn indicators(engine: &mut Engine) {
    engine.register_fn("tr", || -> Fallible<Series> {
        let c = columns()?;
        Ok(Series::new(math::true_range(
            c.high.values(),
            c.low.values(),
            c.close.values(),
        )))
    });
    engine.register_fn("atr", |n: i64| -> Fallible<Series> {
        let (c, p) = (columns()?, period("atr", n)?);
        Ok(Series::new(math::atr(
            c.high.values(),
            c.low.values(),
            c.close.values(),
            p,
        )))
    });
    engine.register_fn("cci", |n: i64| -> Fallible<Series> {
        let (c, p) = (columns()?, period("cci", n)?);
        Ok(Series::new(math::cci(c.hlc3.values(), p)))
    });
    engine.register_fn("williams_r", |n: i64| -> Fallible<Series> {
        let (c, p) = (columns()?, period("williams_r", n)?);
        Ok(Series::new(math::williams_r(
            c.high.values(),
            c.low.values(),
            c.close.values(),
            p,
        )))
    });
    engine.register_fn("obv", || -> Fallible<Series> {
        let c = columns()?;
        Ok(Series::new(math::obv(c.close.values(), c.volume.values())))
    });
    engine.register_fn("mfi", |n: i64| -> Fallible<Series> {
        let (c, p) = (columns()?, period("mfi", n)?);
        Ok(Series::new(math::mfi(
            c.hlc3.values(),
            c.volume.values(),
            p,
        )))
    });
    engine.register_fn("vwap", || -> Fallible<Series> {
        let c = columns()?;
        Ok(Series::new(math::vwap(
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
        Ok(Series::new(math::vwap(
            s.values(),
            c.volume.values(),
            &c.day,
        )))
    });
    engine.register_fn(
        "psar",
        |start: f64, step: f64, max: f64| -> Fallible<Series> {
            let c = columns()?;
            Ok(Series::new(math::parabolic_sar(
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
            let (line, sig, hist) = math::macd(s.values(), f, sl, sg);
            Ok(map_of(vec![
                ("macd", Series::new(line)),
                ("signal", Series::new(sig)),
                ("hist", Series::new(hist)),
            ]))
        },
    );
    engine.register_fn(
        "bollinger",
        |s: Series, n: i64, mult: f64| -> Fallible<Map> {
            let p = period("bollinger", n)?;
            let (basis, dev) = (math::sma(s.values(), p), math::stdev(s.values(), p));
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
        let (k, d_line) = math::stochastic(
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
        let (plus, minus, adx) = math::dmi(
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
    engine.register_fn("supertrend", |n: i64, mult: f64| -> Fallible<Map> {
        let (c, p) = (columns()?, period("supertrend", n)?);
        let (line, direction) =
            math::supertrend(c.high.values(), c.low.values(), c.close.values(), p, mult);
        Ok(map_of(vec![
            ("line", Series::new(line)),
            ("direction", Series::new(direction)),
        ]))
    });
    engine.register_fn("donchian", |n: i64| -> Fallible<Map> {
        let (c, p) = (columns()?, period("donchian", n)?);
        let upper = math::highest(c.high.values(), p);
        let lower = math::lowest(c.low.values(), p);
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
        |n: i64, mult: f64, atr_n: i64| -> Fallible<Map> {
            let c = columns()?;
            let (p, a) = (period("keltner", n)?, period("keltner", atr_n)?);
            let basis = math::ema(c.close.values(), p);
            let range = math::atr(c.high.values(), c.low.values(), c.close.values(), a);
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

/// Colors by name, for `color: "orange"` in the options of a plot.
pub(super) const NAMED: [(&str, u32); 14] = [
    ("white", 0xffffff),
    ("black", 0x000000),
    ("gray", 0x787b86),
    ("red", 0xef5350),
    ("green", 0x26a69a),
    ("blue", 0x2962ff),
    ("orange", 0xff9800),
    ("yellow", 0xffeb3b),
    ("purple", 0x7e57c2),
    ("teal", 0x00bcd4),
    ("pink", 0xe91e63),
    ("lime", 0x9ccc65),
    ("aqua", 0x00e5ff),
    ("silver", 0xb2b5be),
];

/// A color: `"#2962ff"`, `"2962ff"` or a name.
pub fn parse_color(text: &str) -> Option<u32> {
    let text = text.trim();
    if let Some((_, rgb)) = NAMED
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(text))
    {
        return Some(*rgb);
    }
    let hex = text.strip_prefix('#').unwrap_or(text);
    (hex.len() == 6 && hex.is_ascii())
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

pub(super) fn colors(engine: &mut Engine) {
    engine.register_fn("rgb", |r: i64, g: i64, b: i64| -> String {
        let byte = |v: i64| v.clamp(0, 255) as u32;
        format!("#{:06x}", byte(r) << 16 | byte(g) << 8 | byte(b))
    });
}

/// The text of a color, `#rrggbb`.
pub fn color_text(color: u32) -> String {
    format!("#{color:06x}")
}
