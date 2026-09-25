//! What a script can call: the functions on series, the ready made indicators, and the calls
//! that describe an indicator (`indicator`, `input_*`, `plot`, `hline`, `fill`).
//!
//! The functions on series are thin: each hands the numbers to the arithmetic in [`super::super::math`]
//! and wraps the answer. The description of every function, for the editor, is in
//! [`super::docs`], and a test checks that the two lists agree.

use rhai::{Array, Dynamic, Engine, Map};

use super::super::math;
use super::super::{PlotKind, ValueFormat};
use super::run::{
    FillResult, InputDecl, InputType, MAX_INPUTS, MAX_PLOTS, Meta, Mode, PlotDecl, PlotResult,
    source_index, with_run,
};
use super::series::{Fallible, Series, is_true, truth};
use crate::app::chart::drawing::model::Dash;

/// The longest period a function takes.
pub const MAX_PERIOD: i64 = 100_000;

pub fn register(engine: &mut Engine) {
    super::series::register(engine);
    elementwise(engine);
    windows(engine);
    logic(engine);
    indicators(engine);
    describing(engine);
    colors(engine);
}

/// A period, checked.
fn period(name: &str, value: i64) -> Fallible<usize> {
    if (1..=MAX_PERIOD).contains(&value) {
        Ok(value as usize)
    } else {
        Err(format!("{name}: the period must be between 1 and {MAX_PERIOD}, not {value}").into())
    }
}

/// A series, or a number made into a series as long as `len`.
fn series_of(len: usize, value: &Dynamic) -> Fallible<Series> {
    if let Some(series) = value.clone().try_cast::<Series>() {
        return Ok(series);
    }
    if let Ok(x) = value.as_float() {
        return Ok(Series::constant(len, x));
    }
    if let Ok(x) = value.as_int() {
        return Ok(Series::constant(len, x as f64));
    }
    Err(format!("expected a series or a number, found {}", value.type_name()).into())
}

fn map_of(pairs: Vec<(&str, Series)>) -> Map {
    pairs
        .into_iter()
        .map(|(name, series)| (name.into(), Dynamic::from(series)))
        .collect()
}

/// The bars of the chart the script runs on.
fn columns() -> Fallible<super::run::Columns> {
    with_run(|run| run.columns.clone())
}

// ---- arithmetic on every bar ----

fn elementwise(engine: &mut Engine) {
    macro_rules! unary {
        ($name:literal, $f:expr) => {
            engine.register_fn($name, |s: Series| s.map($f));
        };
    }
    unary!("abs", f64::abs);
    unary!("sqrt", f64::sqrt);
    unary!("ln", f64::ln);
    unary!("log10", f64::log10);
    unary!("exp", f64::exp);
    unary!("sin", f64::sin);
    unary!("cos", f64::cos);
    unary!("floor", f64::floor);
    unary!("ceil", f64::ceil);
    unary!("round", f64::round);
    unary!("sign", |x: f64| {
        if x.is_nan() {
            f64::NAN
        } else if x > 0.0 {
            1.0
        } else if x < 0.0 {
            -1.0
        } else {
            0.0
        }
    });

    // Smallest and largest of two, with a number or a series on either side.
    engine.register_fn("min", |a: Series, b: Series| a.zip(&b, nan_min));
    engine.register_fn("max", |a: Series, b: Series| a.zip(&b, nan_max));
    engine.register_fn("min", |a: Series, b: f64| -> Fallible<Series> {
        Ok(a.map(|x| nan_min(x, b)))
    });
    engine.register_fn("max", |a: Series, b: f64| -> Fallible<Series> {
        Ok(a.map(|x| nan_max(x, b)))
    });
    engine.register_fn("min", |a: f64, b: Series| -> Fallible<Series> {
        Ok(b.map(|x| nan_min(a, x)))
    });
    engine.register_fn("max", |a: f64, b: Series| -> Fallible<Series> {
        Ok(b.map(|x| nan_max(a, x)))
    });
    engine.register_fn("min", |a: Series, b: i64| -> Fallible<Series> {
        Ok(a.map(|x| nan_min(x, b as f64)))
    });
    engine.register_fn("max", |a: Series, b: i64| -> Fallible<Series> {
        Ok(a.map(|x| nan_max(x, b as f64)))
    });
    engine.register_fn("pow", |a: Series, b: f64| a.map(|x| x.powf(b)));
    engine.register_fn("pow", |a: Series, b: i64| a.map(|x| x.powf(b as f64)));
    engine.register_fn("clamp", |s: Series, low: f64, high: f64| {
        s.map(|x| {
            if x.is_nan() {
                x
            } else {
                x.clamp(low.min(high), high.max(low))
            }
        })
    });

    // A value where there is none, and which bars have none.
    engine.register_fn("nz", |s: Series| {
        s.map(|x| if x.is_nan() { 0.0 } else { x })
    });
    engine.register_fn("nz", |s: Series, fallback: f64| {
        s.map(|x| if x.is_nan() { fallback } else { x })
    });
    engine.register_fn("is_na", |s: Series| s.map(|x| truth(x.is_nan())));
}

fn nan_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

fn nan_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

// ---- moving over a window of bars ----

fn windows(engine: &mut Engine) {
    macro_rules! window {
        ($name:literal, $f:path) => {
            engine.register_fn($name, |s: Series, n: i64| -> Fallible<Series> {
                Ok(Series::new($f(s.values(), period($name, n)?)))
            });
        };
    }
    window!("sma", math::sma);
    window!("ema", math::ema);
    window!("wma", math::wma);
    window!("hma", math::hma);
    window!("rma", math::rma);
    window!("stdev", math::stdev);
    window!("highest", math::highest);
    window!("lowest", math::lowest);
    window!("sum", math::sum);
    window!("rsi", math::rsi);
    window!("roc", math::roc);
    window!("momentum", math::momentum);

    engine.register_fn("vwma", |s: Series, n: i64| -> Fallible<Series> {
        let volume = columns()?.volume;
        Ok(Series::new(math::vwma(
            s.values(),
            volume.values(),
            period("vwma", n)?,
        )))
    });
    engine.register_fn("linreg", |s: Series, n: i64| -> Fallible<Series> {
        let p = period("linreg", n)?;
        Ok(Series::new(math::linear_regression(s.values(), p).0))
    });
    engine.register_fn("slope", |s: Series, n: i64| -> Fallible<Series> {
        let p = period("slope", n)?;
        Ok(Series::new(math::linear_regression(s.values(), p).1))
    });
    engine.register_fn("change", |s: Series| s.zip(&s.shifted(1), |a, b| a - b));
    engine.register_fn("change", |s: Series, n: i64| -> Fallible<Series> {
        let p = period("change", n)? as i64;
        s.zip(&s.shifted(p), |a, b| a - b)
    });
    engine.register_fn("cum", |s: Series| Series::new(math::cumulative(s.values())));
    engine.register_fn("barssince", |cond: Series| {
        Series::new(math::bars_since(cond.values()))
    });
}

// ---- conditions ----

fn logic(engine: &mut Engine) {
    // `iff(condition, a, b)`: `a` on the bars where the condition holds, `b` on the others. Each of
    // `a` and `b` is a series or a number.
    engine.register_fn(
        "iff",
        |cond: Series, a: Dynamic, b: Dynamic| -> Fallible<Series> {
            let (a, b) = (series_of(cond.len(), &a)?, series_of(cond.len(), &b)?);
            if a.len() != cond.len() || b.len() != cond.len() {
                return Err(
                    "iff: the condition and both results must be as long as each other".into(),
                );
            }
            Ok(Series::new(
                (0..cond.len())
                    .map(|i| {
                        if is_true(cond.values()[i]) {
                            a.values()[i]
                        } else {
                            b.values()[i]
                        }
                    })
                    .collect(),
            ))
        },
    );

    macro_rules! crossing {
        ($name:literal, $test:expr) => {
            engine.register_fn($name, |a: Series, b: Series| -> Fallible<Series> {
                crossing(&a, &b, $test)
            });
            engine.register_fn($name, |a: Series, b: f64| -> Fallible<Series> {
                crossing(&a, &Series::constant(a.len(), b), $test)
            });
            engine.register_fn($name, |a: Series, b: i64| -> Fallible<Series> {
                crossing(&a, &Series::constant(a.len(), b as f64), $test)
            });
        };
    }
    // `cross_over(a, b)`: `a` went from under `b` to over it on this bar.
    crossing!("cross_over", |before: f64, now: f64| before <= 0.0
        && now > 0.0);
    crossing!("cross_under", |before: f64, now: f64| before >= 0.0
        && now < 0.0);
    crossing!("cross", |before: f64, now: f64| {
        before <= 0.0 && now > 0.0 || before >= 0.0 && now < 0.0
    });
}

/// The bars where the difference of two series changes sign as `test` (given the difference on
/// the bar before and on this one) says.
fn crossing(a: &Series, b: &Series, test: fn(f64, f64) -> bool) -> Fallible<Series> {
    let difference = a.zip(b, |x, y| x - y)?;
    let before = difference.shifted(1);
    Ok(Series::new(
        before
            .values()
            .iter()
            .zip(difference.values())
            .map(|(p, d)| truth(p.is_finite() && d.is_finite() && test(*p, *d)))
            .collect(),
    ))
}

// ---- indicators ready to use ----

fn indicators(engine: &mut Engine) {
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

// ---- colors ----

/// Colors by name, for `color: "orange"` in the options of a plot.
const NAMED: [(&str, u32); 14] = [
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

fn colors(engine: &mut Engine) {
    engine.register_fn("rgb", |r: i64, g: i64, b: i64| -> String {
        let byte = |v: i64| v.clamp(0, 255) as u32;
        format!("#{:06x}", byte(r) << 16 | byte(g) << 8 | byte(b))
    });
}

// ---- describing the indicator ----

/// The text of a color, `#rrggbb`.
pub fn color_text(color: u32) -> String {
    format!("#{color:06x}")
}

fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && key.len() <= 32
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn check_key(what: &str, key: &str) -> Fallible<()> {
    if valid_key(key) {
        Ok(())
    } else {
        Err(format!(
            "{what} \"{key}\": a name is lowercase letters, digits and _, starts with a letter and has at most 32 characters"
        )
        .into())
    }
}

/// The options of a call, read one by one; anything that is not asked for is an error, so a
/// misspelled option does not pass silently.
struct Options<'a> {
    call: &'a str,
    map: Map,
}

impl<'a> Options<'a> {
    fn new(call: &'a str, map: Map, allowed: &'a [&'a str]) -> Fallible<Self> {
        for key in map.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(format!(
                    "{call}: unknown option \"{key}\" (the options are: {})",
                    allowed.join(", ")
                )
                .into());
            }
        }
        Ok(Self { call, map })
    }

    fn number(&self, key: &str) -> Fallible<Option<f64>> {
        match self.map.get(key) {
            None => Ok(None),
            Some(v) => v
                .as_float()
                .ok()
                .or_else(|| v.as_int().ok().map(|i| i as f64))
                .filter(|x| x.is_finite())
                .map(Some)
                .ok_or_else(|| {
                    format!("{}: the option \"{key}\" must be a number", self.call).into()
                }),
        }
    }

    fn text(&self, key: &str) -> Fallible<Option<String>> {
        match self.map.get(key) {
            None => Ok(None),
            Some(v) => v
                .clone()
                .into_string()
                .map(Some)
                .map_err(|_| format!("{}: the option \"{key}\" must be text", self.call).into()),
        }
    }

    fn flag(&self, key: &str) -> Fallible<Option<bool>> {
        match self.map.get(key) {
            None => Ok(None),
            Some(v) => v.as_bool().map(Some).map_err(|_| {
                format!("{}: the option \"{key}\" must be true or false", self.call).into()
            }),
        }
    }

    fn color(&self, key: &str) -> Fallible<Option<u32>> {
        match self.text(key)? {
            None => Ok(None),
            Some(text) => parse_color(&text).map(Some).ok_or_else(|| {
                format!(
                    "{}: \"{text}\" is not a color (write #rrggbb or a name such as orange)",
                    self.call
                )
                .into()
            }),
        }
    }

    fn series(&self, key: &str) -> Fallible<Option<Series>> {
        match self.map.get(key) {
            None => Ok(None),
            Some(v) => v.clone().try_cast::<Series>().map(Some).ok_or_else(|| {
                format!("{}: the option \"{key}\" must be a series", self.call).into()
            }),
        }
    }
}

fn describing(engine: &mut Engine) {
    engine.register_fn("indicator", |opts: Map| -> Fallible<()> { indicator(opts) });

    // Inputs.
    engine.register_fn("input_int", |key: &str, default: i64| {
        input_number(InputType::Int, key, default as f64, Map::new()).map(|value| value as i64)
    });
    engine.register_fn("input_int", |key: &str, default: i64, opts: Map| {
        input_number(InputType::Int, key, default as f64, opts).map(|value| value as i64)
    });
    engine.register_fn("input_float", |key: &str, default: f64| {
        input_number(InputType::Float, key, default, Map::new())
    });
    engine.register_fn("input_float", |key: &str, default: f64, opts: Map| {
        input_number(InputType::Float, key, default, opts)
    });
    engine.register_fn("input_float", |key: &str, default: i64| {
        input_number(InputType::Float, key, default as f64, Map::new())
    });
    engine.register_fn("input_float", |key: &str, default: i64, opts: Map| {
        input_number(InputType::Float, key, default as f64, opts)
    });
    engine.register_fn("input_bool", |key: &str, default: bool| {
        input_number(InputType::Bool, key, truth(default), Map::new()).map(|v| v != 0.0)
    });
    engine.register_fn("input_bool", |key: &str, default: bool, opts: Map| {
        input_number(InputType::Bool, key, truth(default), opts).map(|v| v != 0.0)
    });
    engine.register_fn("input_source", |key: &str, default: &str| {
        input_source(key, default, Map::new())
    });
    engine.register_fn("input_source", |key: &str, default: &str, opts: Map| {
        input_source(key, default, opts)
    });
    engine.register_fn(
        "input_choice",
        |key: &str, default: &str, options: Array| input_choice(key, default, options, Map::new()),
    );
    engine.register_fn(
        "input_choice",
        |key: &str, default: &str, options: Array, opts: Map| {
            input_choice(key, default, options, opts)
        },
    );
    engine.register_fn("input_color", |key: &str, default: &str| {
        input_color(key, default, Map::new())
    });
    engine.register_fn("input_color", |key: &str, default: &str, opts: Map| {
        input_color(key, default, opts)
    });

    // What it draws.
    engine.register_fn("plot", |key: &str, s: Series| plot(key, s, Map::new()));
    engine.register_fn("plot", |key: &str, s: Series, opts: Map| plot(key, s, opts));
    engine.register_fn("plot", |key: &str, value: f64| {
        plot_number(key, value, Map::new())
    });
    engine.register_fn("plot", |key: &str, value: i64| {
        plot_number(key, value as f64, Map::new())
    });
    engine.register_fn("plot", |key: &str, value: f64, opts: Map| {
        plot_number(key, value, opts)
    });
    engine.register_fn("plot", |key: &str, value: i64, opts: Map| {
        plot_number(key, value as f64, opts)
    });
    engine.register_fn("hline", |value: f64| hline(value));
    engine.register_fn("hline", |value: i64| hline(value as f64));
    engine.register_fn("band", |low: f64, high: f64| band(low, high));
    engine.register_fn("band", |low: i64, high: i64| band(low as f64, high as f64));
    engine.register_fn("fill", |a: &str, b: &str| fill(a, b, Map::new()));
    engine.register_fn("fill", |a: &str, b: &str, opts: Map| fill(a, b, opts));
}

fn indicator(opts: Map) -> Fallible<()> {
    let options = Options::new(
        "indicator",
        opts,
        &[
            "name",
            "short",
            "overlay",
            "format",
            "decimals",
            "range",
            "description",
            "author",
            "version",
            "category",
        ],
    )?;
    let mut meta = Meta {
        name: options.text("name")?,
        short: options.text("short")?,
        overlay: options.flag("overlay")?.unwrap_or(false),
        description: options.text("description")?.unwrap_or_default(),
        author: options.text("author")?.unwrap_or_default(),
        version: options.text("version")?.unwrap_or_default(),
        category: options.text("category")?.unwrap_or_default(),
        ..Meta::default()
    };
    let decimals = options
        .number("decimals")?
        .map_or(2, |d| d.clamp(0.0, 8.0) as u32);
    meta.format = match options.text("format")?.as_deref() {
        None | Some("plain") => ValueFormat::Plain(decimals),
        Some("price") => ValueFormat::Price,
        Some("count") => ValueFormat::Count,
        Some(other) => {
            return Err(format!(
                "indicator: the format \"{other}\" is not one of price, plain and count"
            )
            .into());
        }
    };
    if let Some(range) = options.map.get("range") {
        let pair = range.clone().into_array().ok().filter(|a| a.len() == 2);
        let number = |d: &Dynamic| {
            d.as_float()
                .ok()
                .or_else(|| d.as_int().ok().map(|i| i as f64))
        };
        match pair.as_deref() {
            Some([low, high]) => match (number(low), number(high)) {
                (Some(low), Some(high)) if low < high => meta.range = Some((low, high)),
                _ => return Err("indicator: range is [low, high] with low under high".into()),
            },
            _ => return Err("indicator: range is [low, high] with low under high".into()),
        }
    }
    with_run(|run| {
        if run.declared {
            return Err("indicator can only be called once".into());
        }
        run.declared = true;
        run.declaration.meta = meta;
        Ok(())
    })?
}

/// Registers an input, and gives the value the user chose (or the default).
fn declare_input(mut input: InputDecl) -> Fallible<f64> {
    check_key("input", &input.key)?;
    with_run(|run| {
        if run.declaration.inputs.iter().any(|i| i.key == input.key) {
            return Err(format!("the input \"{}\" is declared twice", input.key).into());
        }
        if run.declaration.inputs.len() >= MAX_INPUTS {
            return Err(format!("an indicator has at most {MAX_INPUTS} inputs").into());
        }
        let chosen = match run.mode {
            Mode::Compute => run.values.get(&input.key).copied(),
            Mode::Declare => None,
        }
        .filter(|v| v.is_finite())
        .unwrap_or(input.default);
        let value = match input.kind {
            InputType::Float => chosen.clamp(input.min, input.max),
            _ => chosen.clamp(input.min, input.max).round(),
        };
        input.default = input.default.clamp(input.min, input.max);
        run.declaration.inputs.push(input);
        Ok(value)
    })?
}

fn title_of(key: &str) -> String {
    let mut text = key.replace('_', " ");
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    text
}

fn input_number(kind: InputType, key: &str, default: f64, opts: Map) -> Fallible<f64> {
    let options = Options::new("input", opts, &["label", "min", "max", "step"])?;
    let (low, high) = match kind {
        InputType::Bool => (0.0, 1.0),
        _ => (
            options.number("min")?.unwrap_or(-1_000_000.0),
            options.number("max")?.unwrap_or(1_000_000.0),
        ),
    };
    if low > high {
        return Err(format!("input \"{key}\": min is over max").into());
    }
    declare_input(InputDecl {
        key: key.to_owned(),
        label: options.text("label")?.unwrap_or_else(|| title_of(key)),
        kind,
        default,
        min: low,
        max: high,
        step: options
            .number("step")?
            .filter(|s| *s > 0.0)
            .unwrap_or(match kind {
                InputType::Float => 0.1,
                _ => 1.0,
            }),
        options: Vec::new(),
    })
}

fn input_source(key: &str, default: &str, opts: Map) -> Fallible<Series> {
    let options = Options::new("input_source", opts, &["label"])?;
    let index = source_index(default).ok_or_else(|| {
        format!(
            "input_source: \"{default}\" is not a price (the prices are {})",
            super::super::SOURCES.join(", ")
        )
    })?;
    let chosen = declare_input(InputDecl {
        key: key.to_owned(),
        label: options.text("label")?.unwrap_or_else(|| title_of(key)),
        kind: InputType::Source,
        default: index as f64,
        min: 0.0,
        max: (super::super::SOURCES.len() - 1) as f64,
        step: 1.0,
        options: Vec::new(),
    })?;
    with_run(|run| run.columns.source(chosen as usize))
}

fn input_choice(key: &str, default: &str, choices: Array, opts: Map) -> Fallible<String> {
    let options = Options::new("input_choice", opts, &["label"])?;
    let names: Vec<String> = choices
        .into_iter()
        .map(|c| {
            c.into_string()
                .map_err(|_| "input_choice: the choices are text")
        })
        .collect::<Result<_, _>>()?;
    if names.is_empty() || names.len() > 32 {
        return Err("input_choice: give between 1 and 32 choices".into());
    }
    let index = names.iter().position(|n| n == default).ok_or_else(|| {
        format!("input_choice: the default \"{default}\" is not one of the choices")
    })?;
    let chosen = declare_input(InputDecl {
        key: key.to_owned(),
        label: options.text("label")?.unwrap_or_else(|| title_of(key)),
        kind: InputType::Choice,
        default: index as f64,
        min: 0.0,
        max: (names.len() - 1) as f64,
        step: 1.0,
        options: names.clone(),
    })?;
    Ok(names[chosen as usize].clone())
}

fn input_color(key: &str, default: &str, opts: Map) -> Fallible<String> {
    let options = Options::new("input_color", opts, &["label"])?;
    let color =
        parse_color(default).ok_or_else(|| format!("input_color: \"{default}\" is not a color"))?;
    let chosen = declare_input(InputDecl {
        key: key.to_owned(),
        label: options.text("label")?.unwrap_or_else(|| title_of(key)),
        kind: InputType::Color,
        default: f64::from(color),
        min: 0.0,
        max: f64::from(0x00ff_ffff_u32),
        step: 1.0,
        options: Vec::new(),
    })?;
    Ok(color_text(chosen as u32))
}

fn plot_number(key: &str, value: f64, opts: Map) -> Fallible<()> {
    let len = with_run(|run| run.columns.len)?;
    plot(key, Series::constant(len, value), opts)
}

fn plot(key: &str, series: Series, opts: Map) -> Fallible<()> {
    check_key("plot", key)?;
    let options = Options::new(
        "plot",
        opts,
        &["title", "color", "width", "style", "dash", "offset", "up"],
    )?;
    let kind = match options.text("style")?.as_deref() {
        None | Some("line") => PlotKind::Line,
        Some("histogram") => PlotKind::Histogram,
        Some("dots") => PlotKind::Dots,
        Some(other) => {
            return Err(format!(
                "plot: the style \"{other}\" is not one of line, histogram and dots"
            )
            .into());
        }
    };
    let dash = match options.text("dash")?.as_deref() {
        None | Some("solid") => Dash::Solid,
        Some("dashed") => Dash::Dashed,
        Some("dotted") => Dash::Dotted,
        Some(other) => {
            return Err(format!(
                "plot: the dash \"{other}\" is not one of solid, dashed and dotted"
            )
            .into());
        }
    };
    let width = options.number("width")?.unwrap_or(1.5).clamp(0.5, 20.0) as f32;
    let color = options.color("color")?;
    let label = options.text("title")?.unwrap_or_else(|| title_of(key));
    let offset = options
        .number("offset")?
        .unwrap_or(0.0)
        .clamp(-500.0, 500.0) as i64;
    let up = options.series("up")?;
    with_run(|run| {
        if run.declaration.plots.iter().any(|p| p.key == key) {
            return Err(format!("the plot \"{key}\" is drawn twice").into());
        }
        if run.declaration.plots.len() >= MAX_PLOTS {
            return Err(format!("an indicator draws at most {MAX_PLOTS} plots").into());
        }
        let number = run.declaration.plots.len();
        let color = color.unwrap_or(PALETTE[number % PALETTE.len()]);
        let values = if run.mode == Mode::Compute {
            if series.len() != run.columns.len {
                return Err(format!(
                    "plot \"{key}\" has {} values and the chart has {} bars",
                    series.len(),
                    run.columns.len
                )
                .into());
            }
            series.into_vec()
        } else {
            Vec::new()
        };
        let up = match (&up, run.mode) {
            (Some(up), Mode::Compute) if up.len() == run.columns.len => {
                Some(up.values().iter().map(|v| is_true(*v)).collect())
            }
            (Some(_), Mode::Compute) => {
                return Err("plot: \"up\" must have a value for every bar".into());
            }
            _ => None,
        };
        run.declaration.plots.push(PlotDecl {
            key: key.to_owned(),
            label,
            kind,
            color,
            width,
            dash,
        });
        run.plots.push(PlotResult {
            key: key.to_owned(),
            values,
            offset,
            up,
        });
        Ok(())
    })?
}

/// The colors a plot takes when the script does not choose one, in the order plots are drawn.
const PALETTE: [u32; 8] = [
    0x2962ff, 0xff9800, 0x26a69a, 0xef5350, 0x7e57c2, 0x00bcd4, 0xe91e63, 0x9ccc65,
];

fn hline(value: f64) -> Fallible<()> {
    with_run(|run| {
        if run.levels.len() < 16 && value.is_finite() {
            run.levels.push(value);
        }
    })
}

fn band(low: f64, high: f64) -> Fallible<()> {
    if !(low.is_finite() && high.is_finite() && low < high) {
        return Err("band: give the lower level first, under the higher".into());
    }
    with_run(|run| run.band = Some((low, high)))
}

fn fill(a: &str, b: &str, opts: Map) -> Fallible<()> {
    let options = Options::new("fill", opts, &["color", "alpha", "down_color"])?;
    let color = options.color("color")?;
    let other = options.color("down_color")?;
    let alpha = options.number("alpha")?.unwrap_or(0.1).clamp(0.02, 1.0) as f32;
    with_run(|run| {
        let find = |key: &str| -> Fallible<usize> {
            run.plots.iter().position(|p| p.key == key).ok_or_else(|| {
                format!("fill: the plot \"{key}\" is not drawn yet: draw it before filling").into()
            })
        };
        let (index_a, index_b) = (find(a)?, find(b)?);
        let color = color.unwrap_or_else(|| {
            run.declaration
                .plots
                .iter()
                .find(|p| p.key == a)
                .map_or(0x2962ff, |p| p.color)
        });
        run.fills.push(FillResult {
            a: index_a,
            b: index_b,
            color,
            alpha,
            other,
        });
        Ok(())
    })?
}
