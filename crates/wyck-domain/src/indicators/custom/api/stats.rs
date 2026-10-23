//! Statistics over a window, and the other `ta` functions of Pine that work on one series: the
//! functions behind `variance`, `median`, `percentrank`, `correlation`, `alma`, `valuewhen`...

use super::{charge_window, columns, period, scalar};
use crate::base::levels;
use crate::base::stats;
use crate::base::ta::{self, MovingAverage};
use crate::indicators::custom::series::{Fallible, Series};
use rhai::{Dynamic, Engine};

/// A count given by a script (a number of bars, an occurrence), checked.
fn count(name: &str, value: i64) -> Fallible<usize> {
    match usize::try_from(value) {
        Ok(count) if value <= super::MAX_PERIOD => Ok(count),
        _ => Err(format!("{name}: {value} is not between 0 and {}", super::MAX_PERIOD).into()),
    }
}

pub(super) fn statistics(engine: &mut Engine) {
    // `name(series, length) -> series`, announcing the cost of looking back over the window.
    macro_rules! window {
        ($name:literal, $f:expr) => {
            engine.register_fn($name, |s: Series, n: i64| -> Fallible<Series> {
                let p = period($name, n)?;
                charge_window(s.len(), p)?;
                Ok(Series::new(($f)(s.values(), p)))
            });
        };
    }
    window!("dev", stats::deviation);
    window!("median", stats::median);
    window!("mode", stats::mode);
    window!("range", stats::range);
    window!("percentrank", stats::percent_rank);
    window!("cmo", stats::cmo);
    window!("cog", stats::center_of_gravity);
    window!("highestbars", stats::highest_bars);
    window!("lowestbars", stats::lowest_bars);
    window!("rising", stats::rising);
    window!("falling", stats::falling);
    window!("variance", |v: &[f64], p| stats::variance(v, p, true));

    engine.register_fn(
        "variance",
        |s: Series, n: i64, biased: bool| -> Fallible<Series> {
            let p = period("variance", n)?;
            charge_window(s.len(), p)?;
            Ok(Series::new(stats::variance(s.values(), p, biased)))
        },
    );
    engine.register_fn(
        "percentile_nearest_rank",
        |s: Series, n: i64, pct: Dynamic| -> Fallible<Series> {
            let (p, pct) = (
                period("percentile_nearest_rank", n)?,
                scalar("percentile_nearest_rank", &pct)?,
            );
            charge_window(s.len(), p)?;
            Ok(Series::new(stats::percentile_nearest_rank(
                s.values(),
                p,
                pct,
            )))
        },
    );
    engine.register_fn(
        "percentile_linear_interpolation",
        |s: Series, n: i64, pct: Dynamic| -> Fallible<Series> {
            let (p, pct) = (
                period("percentile_linear_interpolation", n)?,
                scalar("percentile_linear_interpolation", &pct)?,
            );
            charge_window(s.len(), p)?;
            Ok(Series::new(stats::percentile_linear_interpolation(
                s.values(),
                p,
                pct,
            )))
        },
    );
    engine.register_fn("alma", |s: Series, n: i64| -> Fallible<Series> {
        let p = period("alma", n)?;
        charge_window(s.len(), p)?;
        Ok(Series::new(stats::alma(s.values(), p, 0.85, 6.0)))
    });
    engine.register_fn(
        "alma",
        |s: Series, n: i64, offset: Dynamic, sigma: Dynamic| -> Fallible<Series> {
            let p = period("alma", n)?;
            let (offset, sigma) = (scalar("alma", &offset)?, scalar("alma", &sigma)?);
            charge_window(s.len(), p)?;
            Ok(Series::new(stats::alma(s.values(), p, offset, sigma)))
        },
    );
    engine.register_fn("swma", |s: Series| Series::new(stats::swma(s.values())));
    engine.register_fn(
        "tsi",
        |s: Series, short: i64, long: i64| -> Fallible<Series> {
            Ok(Series::new(stats::true_strength_index(
                s.values(),
                period("tsi", short)?,
                period("tsi", long)?,
            )))
        },
    );
    engine.register_fn(
        "correlation",
        |a: Series, b: Series, n: i64| -> Fallible<Series> {
            if a.len() != b.len() {
                return Err("correlation: the two series must be as long as each other".into());
            }
            let p = period("correlation", n)?;
            charge_window(a.len(), p)?;
            Ok(Series::new(stats::correlation(a.values(), b.values(), p)))
        },
    );
    engine.register_fn(
        "valuewhen",
        |cond: Series, s: Series, occurrence: i64| -> Fallible<Series> {
            if cond.len() != s.len() {
                return Err(
                    "valuewhen: the condition and the series must be as long as each other".into(),
                );
            }
            Ok(Series::new(stats::value_when(
                cond.values(),
                s.values(),
                count("valuewhen", occurrence)?,
            )))
        },
    );
    engine.register_fn("fixnan", |s: Series| {
        Series::new(stats::fix_nan(s.values()))
    });
    engine.register_fn(
        "linreg",
        |s: Series, n: i64, offset: i64| -> Fallible<Series> {
            let p = period("linreg", n)?;
            charge_window(s.len(), p)?;
            let (level, slope) = ta::linear_regression(s.values(), p);
            let back = crate::base::math::i64_to_f64(offset);
            Ok(Series::new(
                level
                    .iter()
                    .zip(&slope)
                    .map(|(l, k)| l - k * back)
                    .collect(),
            ))
        },
    );
    engine.register_fn(
        "linreg_deviation",
        |s: Series, n: i64| -> Fallible<Series> {
            let p = period("linreg_deviation", n)?;
            charge_window(s.len(), p.saturating_mul(2))?;
            Ok(Series::new(ta::linear_regression_deviation(s.values(), p)))
        },
    );
    engine.register_fn(
        "ma",
        |s: Series, n: i64, method: &str| -> Fallible<Series> {
            let p = period("ma", n)?;
            let kind = MovingAverage::parse(method).ok_or_else(|| {
                format!("ma: \"{method}\" is not an average (use sma, ema, wma, rma or hma)")
            })?;
            charge_window(s.len(), p)?;
            Ok(Series::new(kind.of(s.values(), p)))
        },
    );

    // Pivots: the value of the pivot, on the bar where it is known.
    engine.register_fn(
        "pivothigh",
        |s: Series, left: i64, right: i64| -> Fallible<Series> {
            let (l, r) = (count("pivothigh", left)?, count("pivothigh", right)?);
            charge_window(s.len(), l + r + 1)?;
            Ok(Series::new(levels::pivot_high(s.values(), l, r)))
        },
    );
    engine.register_fn(
        "pivotlow",
        |s: Series, left: i64, right: i64| -> Fallible<Series> {
            let (l, r) = (count("pivotlow", left)?, count("pivotlow", right)?);
            charge_window(s.len(), l + r + 1)?;
            Ok(Series::new(levels::pivot_low(s.values(), l, r)))
        },
    );
    engine.register_fn("pivothigh", |left: i64, right: i64| -> Fallible<Series> {
        let c = columns()?;
        let (l, r) = (count("pivothigh", left)?, count("pivothigh", right)?);
        charge_window(c.len, l + r + 1)?;
        Ok(Series::new(levels::pivot_high(c.high.values(), l, r)))
    });
    engine.register_fn("pivotlow", |left: i64, right: i64| -> Fallible<Series> {
        let c = columns()?;
        let (l, r) = (count("pivotlow", left)?, count("pivotlow", right)?);
        charge_window(c.len, l + r + 1)?;
        Ok(Series::new(levels::pivot_low(c.low.values(), l, r)))
    });
    engine.register_fn("fractals", |wing: i64| -> Fallible<rhai::Map> {
        let c = columns()?;
        let w = period("fractals", wing)?;
        charge_window(c.len, 2 * w + 1)?;
        let (up, down) = levels::fractals(c.high.values(), c.low.values(), w);
        Ok(super::map_of(vec![
            ("high", Series::new(up)),
            ("low", Series::new(down)),
        ]))
    });
    engine.register_fn("aroon", |n: i64| -> Fallible<rhai::Map> {
        let c = columns()?;
        let p = period("aroon", n)?;
        charge_window(c.len, p)?;
        let (up, down) = levels::aroon(c.high.values(), c.low.values(), p);
        Ok(super::map_of(vec![
            ("up", Series::new(up)),
            ("down", Series::new(down)),
        ]))
    });
}
