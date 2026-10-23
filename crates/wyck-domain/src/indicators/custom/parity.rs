//! Each function of the language computes what the arithmetic of `base` computes: a script that
//! calls it gives the same numbers as calling that arithmetic on the same bars.

use super::decl::Limits;
use super::run::Script;
use crate::base::levels;
use crate::base::math::{count_f64, count_i64};
use crate::base::stats;
use crate::base::ta;
use crate::indicators::data::StudyInput;
use std::collections::BTreeMap;

const N: usize = 120;

fn bars() -> StudyInput {
    let close: Vec<f64> = (0..N)
        .map(|i| {
            // A wandering price with a gap, so that windows over it restart.
            if i == 70 {
                f64::NAN
            } else {
                100.0 + (count_f64(i) * 0.37).sin() * 6.0 + count_f64(i % 7)
            }
        })
        .collect();
    StudyInput {
        time: (0..count_i64(N)).map(|i| i * 3_600_000).collect(),
        open: close.iter().map(|c| c - 0.7).collect(),
        high: close.iter().map(|c| c + 1.3).collect(),
        low: close.iter().map(|c| c - 1.1).collect(),
        volume: (0..N).map(|i| 50.0 + count_f64((i * 13) % 17)).collect(),
        day: (0..count_i64(N)).map(|i| i / 24).collect(),
        close,
    }
}

/// The plots a script draws, by key, on the bars above.
fn plots(source: &str) -> BTreeMap<String, Vec<f64>> {
    let script = Script::compile(source).unwrap_or_else(|p| panic!("{source}: {p:?}"));
    let done = script
        .compute(&bars(), &BTreeMap::new(), Limits::default(), None)
        .unwrap_or_else(|p| panic!("{source}: {p:?}"));
    done.output
        .plots
        .into_iter()
        .map(|p| (p.key.to_owned(), p.values))
        .collect()
}

fn plot(source: &str) -> Vec<f64> {
    plots(source).remove("a").unwrap_or_default()
}

fn same(name: &str, got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len(), "{name}");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(
            (g.is_nan() && w.is_nan()) || (g - w).abs() <= 1e-9 * w.abs().max(1.0),
            "{name} bar {i}: {g} against {w}"
        );
    }
}

#[test]
fn the_window_statistics_match_the_arithmetic() {
    let close = bars().close;
    same(
        "variance",
        &plot("plot(\"a\", variance(close, 14));"),
        &stats::variance(&close, 14, true),
    );
    same(
        "variance sample",
        &plot("plot(\"a\", variance(close, 14, false));"),
        &stats::variance(&close, 14, false),
    );
    same(
        "dev",
        &plot("plot(\"a\", dev(close, 14));"),
        &stats::deviation(&close, 14),
    );
    same(
        "median",
        &plot("plot(\"a\", median(close, 14));"),
        &stats::median(&close, 14),
    );
    same(
        "mode",
        &plot("plot(\"a\", mode(close, 14));"),
        &stats::mode(&close, 14),
    );
    same(
        "range",
        &plot("plot(\"a\", range(close, 14));"),
        &stats::range(&close, 14),
    );
    same(
        "percentrank",
        &plot("plot(\"a\", percentrank(close, 14));"),
        &stats::percent_rank(&close, 14),
    );
    same(
        "nearest rank",
        &plot("plot(\"a\", percentile_nearest_rank(close, 14, 80));"),
        &stats::percentile_nearest_rank(&close, 14, 80.0),
    );
    same(
        "interpolation",
        &plot("plot(\"a\", percentile_linear_interpolation(close, 14, 33.5));"),
        &stats::percentile_linear_interpolation(&close, 14, 33.5),
    );
}

#[test]
fn the_oscillators_match_the_arithmetic() {
    let close = bars().close;
    same(
        "alma",
        &plot("plot(\"a\", alma(close, 9));"),
        &stats::alma(&close, 9, 0.85, 6.0),
    );
    same(
        "alma options",
        &plot("plot(\"a\", alma(close, 9, 0.5, 4));"),
        &stats::alma(&close, 9, 0.5, 4.0),
    );
    same(
        "swma",
        &plot("plot(\"a\", swma(close));"),
        &stats::swma(&close),
    );
    same(
        "cmo",
        &plot("plot(\"a\", cmo(close, 9));"),
        &stats::cmo(&close, 9),
    );
    same(
        "cog",
        &plot("plot(\"a\", cog(close, 10));"),
        &stats::center_of_gravity(&close, 10),
    );
    same(
        "tsi",
        &plot("plot(\"a\", tsi(close, 5, 10));"),
        &stats::true_strength_index(&close, 5, 10),
    );
    same(
        "highestbars",
        &plot("plot(\"a\", highestbars(close, 8));"),
        &stats::highest_bars(&close, 8),
    );
    same(
        "lowestbars",
        &plot("plot(\"a\", lowestbars(close, 8));"),
        &stats::lowest_bars(&close, 8),
    );
    same(
        "rising",
        &plot("plot(\"a\", rising(close, 2));"),
        &stats::rising(&close, 2),
    );
    same(
        "falling",
        &plot("plot(\"a\", falling(close, 2));"),
        &stats::falling(&close, 2),
    );
}

#[test]
fn the_functions_of_two_series_and_of_conditions_match_the_arithmetic() {
    let bars = bars();
    same(
        "correlation",
        &plot("plot(\"a\", correlation(close, volume, 20));"),
        &stats::correlation(&bars.close, &bars.volume, 20),
    );
    let cond: Vec<f64> = bars
        .close
        .iter()
        .zip(ta::sma(&bars.close, 5))
        .map(|(c, m)| if c > &m { 1.0 } else { 0.0 })
        .collect();
    same(
        "valuewhen",
        &plot("plot(\"a\", valuewhen(close > sma(close, 5), high, 1));"),
        &stats::value_when(&cond, &bars.high, 1),
    );
    same(
        "fixnan",
        &plot("plot(\"a\", fixnan(close));"),
        &stats::fix_nan(&bars.close),
    );
}

#[test]
fn regression_and_named_averages_match_the_arithmetic() {
    let close = bars().close;
    let (level, slope) = ta::linear_regression(&close, 20);
    let two_back: Vec<f64> = level.iter().zip(&slope).map(|(l, k)| l - 2.0 * k).collect();
    same(
        "linreg offset",
        &plot("plot(\"a\", linreg(close, 20, 2));"),
        &two_back,
    );
    same(
        "linreg_deviation",
        &plot("plot(\"a\", linreg_deviation(close, 20));"),
        &ta::linear_regression_deviation(&close, 20),
    );
    for (name, kind) in [
        ("sma", ta::MovingAverage::Sma),
        ("ema", ta::MovingAverage::Ema),
        ("wma", ta::MovingAverage::Wma),
        ("rma", ta::MovingAverage::Rma),
        ("hma", ta::MovingAverage::Hma),
    ] {
        same(
            name,
            &plot(&format!("plot(\"a\", ma(close, 12, \"{name}\"));")),
            &kind.of(&close, 12),
        );
    }
}

#[test]
fn pivots_fractals_and_aroon_match_the_arithmetic() {
    let bars = bars();
    same(
        "pivothigh of the highs",
        &plot("plot(\"a\", pivothigh(3, 2));"),
        &levels::pivot_high(&bars.high, 3, 2),
    );
    same(
        "pivotlow of a series",
        &plot("plot(\"a\", pivotlow(close, 2, 3));"),
        &levels::pivot_low(&bars.close, 2, 3),
    );
    let (up, down) = levels::fractals(&bars.high, &bars.low, 2);
    let drawn = plots("let f = fractals(2);\nplot(\"a\", f.high);\nplot(\"b\", f.low);");
    same("fractal highs", &drawn["a"], &up);
    same("fractal lows", &drawn["b"], &down);
    let (aroon_up, aroon_down) = levels::aroon(&bars.high, &bars.low, 14);
    let drawn = plots("let r = aroon(14);\nplot(\"a\", r.up);\nplot(\"b\", r.down);");
    same("aroon up", &drawn["a"], &aroon_up);
    same("aroon down", &drawn["b"], &aroon_down);
}
