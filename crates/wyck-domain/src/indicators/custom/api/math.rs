//! The rest of the arithmetic of a script: trigonometry, constants, several values at once,
//! statistics of an array of numbers, text from a number, and the arithmetic of colors.

use super::{nan_max, nan_min, scalar, series_of};
use crate::base::math::{count_f64, f64_to_usize};
use crate::indicators::custom::color::{color_text, parse_color};
use crate::indicators::custom::series::{Fallible, Series, finite_or_na, number_of};
use rhai::{Array, Dynamic, Engine};

/// The golden ratio and its inverse.
const PHI: f64 = std::f64::consts::GOLDEN_RATIO;

pub(super) fn math(engine: &mut Engine) {
    trigonometry(engine);
    several(engine);
    arrays(engine);
    text(engine);
}

fn trigonometry(engine: &mut Engine) {
    macro_rules! unary {
        ($name:literal, $f:expr) => {
            engine.register_fn($name, |s: Series| s.map(|x| finite_or_na($f(x))));
        };
    }
    unary!("tan", f64::tan);
    unary!("asin", f64::asin);
    unary!("acos", f64::acos);
    unary!("atan", f64::atan);
    unary!("todegrees", f64::to_degrees);
    unary!("toradians", f64::to_radians);
    // The natural logarithm, as Pine names it.
    unary!("log", f64::ln);

    // Each value rounded to some decimals.
    engine.register_fn("round", |s: Series, digits: i64| -> Fallible<Series> {
        let digits = i32::try_from(digits.clamp(0, 12)).unwrap_or(0);
        let scale = 10_f64.powi(digits);
        Ok(s.map(|x| (x * scale).round() / scale))
    });

    engine.register_fn("pi", || std::f64::consts::PI);
    engine.register_fn("e", || std::f64::consts::E);
    engine.register_fn("phi", || PHI);
    engine.register_fn("rphi", || 1.0 / PHI);
}

/// The values of a call that takes several: a series or a number each, as series as long as
/// the longest series among them.
fn all_series(name: &str, items: &[Dynamic]) -> Fallible<Vec<Series>> {
    let len = items
        .iter()
        .find_map(|item| item.clone().try_cast::<Series>().map(|s| s.len()))
        .unwrap_or(1);
    items
        .iter()
        .map(|item| series_of(len, item))
        .collect::<Fallible<Vec<_>>>()
        .and_then(|list| {
            if list.iter().any(|s| s.len() != len) {
                Err(format!("{name}: the series must be as long as each other").into())
            } else {
                Ok(list)
            }
        })
}

/// Bar by bar, `fold` over the values of every series.
fn across(list: &[Series], fold: fn(f64, f64) -> f64) -> Series {
    let len = list.first().map_or(0, Series::len);
    Series::new(
        (0..len)
            .map(|i| {
                list.iter()
                    .map(|s| s.values().get(i).copied().unwrap_or(f64::NAN))
                    .reduce(fold)
                    .unwrap_or(f64::NAN)
            })
            .collect(),
    )
}

fn several(engine: &mut Engine) {
    // Three values at once; any of them may be a number.
    engine.register_fn(
        "min",
        |a: Dynamic, b: Dynamic, c: Dynamic| -> Fallible<Series> {
            Ok(across(&all_series("min", &[a, b, c])?, nan_min))
        },
    );
    engine.register_fn(
        "max",
        |a: Dynamic, b: Dynamic, c: Dynamic| -> Fallible<Series> {
            Ok(across(&all_series("max", &[a, b, c])?, nan_max))
        },
    );
    // The mean of two values bar by bar.
    engine.register_fn("avg", |a: Dynamic, b: Dynamic| -> Fallible<Series> {
        let list = all_series("avg", &[a, b])?;
        Ok(across(&list, f64::midpoint))
    });
}

/// The numbers of an array, or why it is not one.
fn numbers(name: &str, items: &Array) -> Fallible<Vec<f64>> {
    items
        .iter()
        .map(|item| {
            number_of(item).ok_or_else(|| format!("{name}: an array of numbers is expected").into())
        })
        .collect()
}

fn is_series_array(items: &Array) -> bool {
    items.iter().any(Dynamic::is::<Series>)
}

fn arrays(engine: &mut Engine) {
    engine.register_fn("sum", |items: Array| -> Fallible<f64> {
        Ok(numbers("sum", &items)?.iter().sum())
    });
    engine.register_fn("avg", |items: Array| -> Fallible<f64> {
        let list = numbers("avg", &items)?;
        Ok(if list.is_empty() {
            f64::NAN
        } else {
            list.iter().sum::<f64>() / count_f64(list.len())
        })
    });
    // The smallest of an array of numbers, or bar by bar of an array of series.
    engine.register_fn("min", |items: Array| -> Fallible<Dynamic> {
        extreme("min", &items, f64::min, nan_min)
    });
    engine.register_fn("max", |items: Array| -> Fallible<Dynamic> {
        extreme("max", &items, f64::max, nan_max)
    });
    engine.register_fn("stdev", |items: Array| -> Fallible<f64> {
        let list = numbers("stdev", &items)?;
        if list.is_empty() {
            return Ok(f64::NAN);
        }
        let mean = list.iter().sum::<f64>() / count_f64(list.len());
        Ok((list.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count_f64(list.len())).sqrt())
    });
    engine.register_fn("median", |items: Array| -> Fallible<f64> {
        percentile_of("median", &items, 50.0)
    });
    // The value at a percentage (0 to 100) of an array of numbers, interpolated.
    engine.register_fn(
        "percentile",
        |items: Array, pct: Dynamic| -> Fallible<f64> {
            percentile_of("percentile", &items, scalar("percentile", &pct)?)
        },
    );
}

fn extreme(
    name: &str,
    items: &Array,
    scalar_fold: fn(f64, f64) -> f64,
    series_fold: fn(f64, f64) -> f64,
) -> Fallible<Dynamic> {
    if is_series_array(items) {
        let list = all_series(name, items)?;
        return Ok(Dynamic::from(across(&list, series_fold)));
    }
    Ok(Dynamic::from(
        numbers(name, items)?
            .into_iter()
            .reduce(scalar_fold)
            .unwrap_or(f64::NAN),
    ))
}

fn percentile_of(name: &str, items: &Array, pct: f64) -> Fallible<f64> {
    let mut list = numbers(name, items)?;
    if list.is_empty() {
        return Ok(f64::NAN);
    }
    list.sort_by(f64::total_cmp);
    let position = pct.clamp(0.0, 100.0) / 100.0 * count_f64(list.len() - 1);
    let below = f64_to_usize(position.floor());
    let low = list.get(below).copied().unwrap_or(f64::NAN);
    let high = list.get(below + 1).copied().unwrap_or(low);
    Ok(low + (high - low) * (position - position.floor()))
}

fn text(engine: &mut Engine) {
    // A number as text with some decimals; "na" for a number that is not one.
    engine.register_fn("fmt", |x: Dynamic, decimals: i64| -> Fallible<String> {
        let x = scalar("fmt", &x)?;
        let digits = usize::try_from(decimals.clamp(0, 10)).unwrap_or(0);
        Ok(if x.is_finite() {
            format!("{x:.digits$}")
        } else {
            "na".to_owned()
        })
    });
    // Any value as text.
    engine.register_fn("tostring", |x: Dynamic| -> String {
        match number_of(&x) {
            Some(n) if n.is_finite() => format!("{n}"),
            Some(_) => "na".to_owned(),
            None => x.to_string(),
        }
    });

    // The channels of a color, and the color between two colors.
    engine.register_fn("color_r", |c: &str| channel("color_r", c, 16));
    engine.register_fn("color_g", |c: &str| channel("color_g", c, 8));
    engine.register_fn("color_b", |c: &str| channel("color_b", c, 0));
    // `color_mix(a, b, t)`: a at t = 0, b at t = 1, and the colors between for the values between.
    engine.register_fn(
        "color_mix",
        |from: &str, to: &str, share: Dynamic| -> Fallible<String> {
            let share = scalar("color_mix", &share)?;
            mix("color_mix", from, to, share)
        },
    );
    // `from_gradient(value, low, high, from_color, to_color)`: the color for a value between two.
    engine.register_fn(
        "from_gradient",
        |value: Dynamic, low: Dynamic, high: Dynamic, from: &str, to: &str| -> Fallible<String> {
            let (value, low, high) = (
                scalar("from_gradient", &value)?,
                scalar("from_gradient", &low)?,
                scalar("from_gradient", &high)?,
            );
            let share = if (high - low).abs() < f64::EPSILON {
                0.0
            } else {
                (value - low) / (high - low)
            };
            mix("from_gradient", from, to, share)
        },
    );
}

fn channel(name: &str, text: &str, shift: u32) -> Fallible<i64> {
    let rgb = parse_color(text)
        .ok_or_else(|| format!("{name}: \"{text}\" is not a color (write #rrggbb or a name)"))?;
    Ok(i64::from((rgb >> shift) & 0xff))
}

fn mix(name: &str, from: &str, to: &str, share: f64) -> Fallible<String> {
    let parse = |text: &str| {
        parse_color(text)
            .ok_or_else(|| format!("{name}: \"{text}\" is not a color (write #rrggbb or a name)"))
    };
    let (from, to) = (parse(from)?, parse(to)?);
    let share = if share.is_finite() {
        share.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let blend = |shift: u32| -> u32 {
        let (start, end) = (
            f64::from((from >> shift) & 0xff),
            f64::from((to >> shift) & 0xff),
        );
        crate::base::math::f64_to_u32((start + (end - start) * share).round().clamp(0.0, 255.0))
    };
    Ok(color_text(blend(16) << 16 | blend(8) << 8 | blend(0)))
}
