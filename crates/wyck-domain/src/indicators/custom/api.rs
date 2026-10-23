//! What a script can call: the functions on series, the ready made indicators, and the calls
//! that describe an indicator (`indicator`, `input_*`, `plot`, `hline`, `fill`).
//!
//! The functions on series are thin: each hands the numbers to the arithmetic in [`crate::base::ta`]
//! and wraps the answer. The description of every function, for the editor, is in
//! [`super::docs`], and a test checks that the two lists agree.

mod declare;
pub(crate) mod describe;
pub(crate) mod indicators;
pub(crate) mod inputs;
pub(crate) mod plots;
pub(crate) mod symbols;

use super::context::with_run;
use super::series::{Fallible, Series, finite_or_na, is_true, number_of, truth};
use crate::base::math::{count_i64, i64_to_f64};
use crate::base::ta;
use crate::base::ta::Series as _;
use rhai::{Dynamic, Engine, Map};

/// The longest period a function takes.
pub(crate) const MAX_PERIOD: i64 = 100_000;

pub(crate) fn register(engine: &mut Engine) {
    super::series::register(engine);
    elementwise(engine);
    windows(engine);
    logic(engine);
    indicators::indicators(engine);
    describe::describing(engine);
    indicators::colors(engine);
    symbols::symbols(engine);
    super::draw_api::register(engine);
}

/// A period, checked.
fn period(name: &str, value: i64) -> Fallible<usize> {
    if (1..=MAX_PERIOD).contains(&value)
        && let Ok(period) = usize::try_from(value)
    {
        Ok(period)
    } else {
        Err(format!("{name}: the period must be between 1 and {MAX_PERIOD}, not {value}").into())
    }
}

/// A number given as a whole number or a decimal. Both work wherever a number is taken, so that
/// `bollinger(close, 20, 2)` is as good as `bollinger(close, 20, 2.0)`.
pub(super) fn scalar(name: &str, value: &Dynamic) -> Fallible<f64> {
    number_of(value)
        .ok_or_else(|| format!("{name}: expected a number, found {}", value.type_name()).into())
}

/// A series, or a number made into a series as long as `len`.
fn series_of(len: usize, value: &Dynamic) -> Fallible<Series> {
    if let Some(series) = value.clone().try_cast::<Series>() {
        return Ok(series);
    }
    number_of(value)
        .map(|x| Series::constant(len, x))
        .ok_or_else(|| format!("expected a series or a number, found {}", value.type_name()).into())
}

fn map_of(pairs: Vec<(&str, Series)>) -> Map {
    pairs
        .into_iter()
        .map(|(name, series)| (name.into(), Dynamic::from(series)))
        .collect()
}

/// The bars of the chart the script runs on.
fn columns() -> Fallible<super::decl::Columns> {
    with_run(|run| run.columns.clone())
}

fn elementwise(engine: &mut Engine) {
    macro_rules! unary {
        ($name:literal, $f:expr) => {
            engine.register_fn($name, |s: Series| s.map(|x| finite_or_na($f(x))));
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

    // Smallest and largest of two, with a number (whole or decimal) or a series on either side.
    engine.register_fn("min", |a: Series, b: Series| a.zip(&b, nan_min));
    engine.register_fn("max", |a: Series, b: Series| a.zip(&b, nan_max));
    engine.register_fn("min", |a: Series, b: Dynamic| -> Fallible<Series> {
        let b = scalar("min", &b)?;
        Ok(a.map(|x| nan_min(x, b)))
    });
    engine.register_fn("max", |a: Series, b: Dynamic| -> Fallible<Series> {
        let b = scalar("max", &b)?;
        Ok(a.map(|x| nan_max(x, b)))
    });
    engine.register_fn("min", |a: Dynamic, b: Series| -> Fallible<Series> {
        let a = scalar("min", &a)?;
        Ok(b.map(|x| nan_min(a, x)))
    });
    engine.register_fn("max", |a: Dynamic, b: Series| -> Fallible<Series> {
        let a = scalar("max", &a)?;
        Ok(b.map(|x| nan_max(a, x)))
    });
    engine.register_fn("pow", |a: Series, b: Dynamic| -> Fallible<Series> {
        let b = scalar("pow", &b)?;
        Ok(a.map(|x| finite_or_na(x.powf(b))))
    });
    engine.register_fn(
        "clamp",
        |s: Series, low: Dynamic, high: Dynamic| -> Fallible<Series> {
            let (low, high) = (scalar("clamp", &low)?, scalar("clamp", &high)?);
            Ok(s.map(|x| {
                if x.is_nan() {
                    x
                } else {
                    x.clamp(low.min(high), high.max(low))
                }
            }))
        },
    );

    // A value where there is none, and which bars have none.
    engine.register_fn("nz", |s: Series| {
        s.map(|x| if x.is_nan() { 0.0 } else { x })
    });
    engine.register_fn("nz", |s: Series, fallback: Dynamic| -> Fallible<Series> {
        let fallback = scalar("nz", &fallback)?;
        Ok(s.map(|x| if x.is_nan() { fallback } else { x }))
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

/// Announces the work of a function that looks back over `period` bars for every one of `len`
/// bars. It is one call to the interpreter however long the period, so it says what it costs, and
/// is refused, ending the run, when that is more than the run has left.
pub(super) fn charge_window(len: usize, period: usize) -> Fallible<()> {
    let visits = len.saturating_mul(period.min(len));
    with_run(|run| run.charge(u64::try_from(visits / VISITS_PER_OPERATION).unwrap_or(u64::MAX)))?
}

/// Bars a function goes over in the time of one operation of the interpreter.
const VISITS_PER_OPERATION: usize = 256;

fn windows(engine: &mut Engine) {
    // A window whose cost grows with its period announces it first.
    macro_rules! window {
        ($name:literal, $f:path) => {
            engine.register_fn($name, |s: Series, n: i64| -> Fallible<Series> {
                Ok(Series::new($f(s.values(), period($name, n)?)))
            });
        };
        ($name:literal, $f:path, looks_back) => {
            engine.register_fn($name, |s: Series, n: i64| -> Fallible<Series> {
                let p = period($name, n)?;
                charge_window(s.len(), p)?;
                Ok(Series::new($f(s.values(), p)))
            });
        };
    }
    window!("sma", ta::sma);
    window!("ema", ta::ema);
    window!("wma", ta::wma, looks_back);
    window!("hma", ta::hma, looks_back);
    window!("rma", ta::rma);
    window!("stdev", ta::stdev, looks_back);
    window!("highest", ta::highest);
    window!("lowest", ta::lowest);
    window!("sum", ta::sum);
    window!("rsi", ta::rsi);
    window!("roc", ta::roc);
    window!("momentum", ta::momentum);

    engine.register_fn("vwma", |s: Series, n: i64| -> Fallible<Series> {
        let volume = columns()?.volume;
        Ok(Series::new(ta::vwma(
            s.values(),
            volume.values(),
            period("vwma", n)?,
        )))
    });
    engine.register_fn("linreg", |s: Series, n: i64| -> Fallible<Series> {
        let p = period("linreg", n)?;
        charge_window(s.len(), p)?;
        Ok(Series::new(ta::linear_regression(s.values(), p).0))
    });
    engine.register_fn("slope", |s: Series, n: i64| -> Fallible<Series> {
        let p = period("slope", n)?;
        charge_window(s.len(), p)?;
        Ok(Series::new(ta::linear_regression(s.values(), p).1))
    });
    engine.register_fn("change", |s: Series| s.zip(&s.shifted(1), |a, b| a - b));
    engine.register_fn("change", |s: Series, n: i64| -> Fallible<Series> {
        let p = count_i64(period("change", n)?);
        s.zip(&s.shifted(p), |a, b| a - b)
    });
    engine.register_fn("cum", |s: Series| Series::new(ta::cumulative(s.values())));
    engine.register_fn("barssince", |cond: Series| {
        Series::new(ta::bars_since(cond.values()))
    });
}

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
                        if is_true(cond.values().at(i)) {
                            a.values().at(i)
                        } else {
                            b.values().at(i)
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
                crossing(&a, &Series::constant(a.len(), i64_to_f64(b)), $test)
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
