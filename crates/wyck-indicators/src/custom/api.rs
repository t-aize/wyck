//! What a script can call: the functions on series, the ready made indicators, and the calls
//! that describe an indicator (`indicator`, `input_*`, `plot`, `hline`, `fill`).
//!
//! The functions on series are thin: each hands the numbers to the arithmetic in [`wyck_core::math`]
//! and wraps the answer. The description of every function, for the editor, is in
//! [`super::docs`], and a test checks that the two lists agree.

pub mod describe;
pub mod indicators;
pub mod inputs;
pub mod plots;
pub mod symbols;

use super::run::with_run;
use super::series::{Fallible, Series, is_true, truth};
use rhai::{Dynamic, Engine, Map};
use wyck_core::math;

/// The longest period a function takes.
pub const MAX_PERIOD: i64 = 100_000;

pub fn register(engine: &mut Engine) {
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
fn columns() -> Fallible<super::run::decl::Columns> {
    with_run(|run| run.columns.clone())
}

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
