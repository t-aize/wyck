//! Each function of the language computes what the arithmetic of `base` computes: a script that
//! calls it gives the same numbers as calling that arithmetic on the same bars.

#![expect(
    clippy::many_single_char_names,
    clippy::float_cmp,
    reason = "the tests name the columns o, h, l, c, v and compare exact small numbers"
)]

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

#[test]
fn the_volume_indicators_match_the_arithmetic() {
    let b = bars();
    let (o, h, l, c, v) = (&b.open, &b.high, &b.low, &b.close, &b.volume);
    same(
        "money_flow",
        &plot("plot(\"a\", money_flow());"),
        &ta::money_flow_volume(h, l, c, v),
    );
    same(
        "accdist",
        &plot("plot(\"a\", accdist());"),
        &ta::accumulation_distribution(h, l, c, v),
    );
    same(
        "pvt",
        &plot("plot(\"a\", pvt());"),
        &ta::price_volume_trend(c, v),
    );
    same(
        "nvi",
        &plot("plot(\"a\", nvi());"),
        &ta::negative_volume_index(c, v),
    );
    same(
        "pvi",
        &plot("plot(\"a\", pvi());"),
        &ta::positive_volume_index(c, v),
    );
    same(
        "wad",
        &plot("plot(\"a\", wad());"),
        &ta::williams_accumulation_distribution(h, l, c),
    );
    same(
        "wvad",
        &plot("plot(\"a\", wvad());"),
        &ta::williams_variable_accumulation_distribution(o, h, l, c, v),
    );
    same(
        "iii",
        &plot("plot(\"a\", iii());"),
        &ta::intraday_intensity(h, l, c, v),
    );
}

#[test]
fn the_channels_and_the_anchored_indicators_match_the_arithmetic() {
    let b = bars();
    let drawn = plots("let k = bb(close, 20, 2);\nplot(\"a\", k.upper);\nplot(\"b\", k.basis);");
    let (basis, dev) = (ta::sma(&b.close, 20), ta::stdev(&b.close, 20));
    let upper: Vec<f64> = basis.iter().zip(&dev).map(|(m, d)| m + 2.0 * d).collect();
    same("bb upper", &drawn["a"], &upper);
    same("bb basis", &drawn["b"], &basis);
    let width: Vec<f64> = basis
        .iter()
        .zip(&dev)
        .map(|(m, d)| 2.0 * 2.0 * d / m)
        .collect();
    same("bbw", &plot("plot(\"a\", bbw(close, 20, 2));"), &width);

    let range = ta::ema(&ta::true_range(&b.high, &b.low, &b.close), 10);
    let mid = ta::ema(&b.close, 10);
    let kc_upper: Vec<f64> = mid.iter().zip(&range).map(|(m, r)| m + 1.5 * r).collect();
    same(
        "kc upper",
        &plot("let k = kc(close, 10, 1.5);\nplot(\"a\", k.upper);"),
        &kc_upper,
    );
    let kc_width: Vec<f64> = range
        .iter()
        .zip(&mid)
        .map(|(r, m)| 2.0 * 1.5 * r / m)
        .collect();
    same("kcw", &plot("plot(\"a\", kcw(close, 10, 1.5));"), &kc_width);

    let (line, spread) = ta::vwap_anchored(
        &b.close,
        &b.volume,
        &b.day,
        crate::base::calendar::Anchor::Week,
    );
    let drawn =
        plots("let v = vwap(close, \"week\", 2);\nplot(\"a\", v.vwap);\nplot(\"b\", v.upper);");
    same("anchored vwap", &drawn["a"], &line);
    let band: Vec<f64> = line.iter().zip(&spread).map(|(v, d)| v + 2.0 * d).collect();
    same("anchored band", &drawn["b"], &band);

    let levels = levels::pivot_levels(
        &b.open,
        &b.high,
        &b.low,
        &b.close,
        &b.day,
        crate::base::calendar::Anchor::Day,
        levels::PivotKind::Camarilla,
    );
    let drawn = plots(
        "let l = pivot_point_levels(\"camarilla\", \"day\");\nplot(\"a\", l.r1);\nplot(\"b\", l.s3);",
    );
    same("pivot r1", &drawn["a"], &levels[1]);
    same("pivot s3", &drawn["b"], &levels[6]);
}

#[test]
fn the_variants_of_an_indicator_match_each_other_and_the_arithmetic() {
    let b = bars();
    let drawn = plots(
        "let own = bars_of(\"\");\n\
         plot(\"a\", atr(14));\n\
         plot(\"b\", atr(own, 14));\n\
         plot(\"c\", atr(14, \"sma\"));\n\
         plot(\"d\", cci(hlc3, 20));\n\
         plot(\"e\", cci(20));",
    );
    same("atr from a map of bars", &drawn["a"], &drawn["b"]);
    let smoothed = ta::sma(&ta::true_range(&b.high, &b.low, &b.close), 14);
    same("atr with a method", &drawn["c"], &smoothed);
    same("cci of a series", &drawn["d"], &drawn["e"]);
    let raw = plot("plot(\"a\", stoch(close, high, low, 14));");
    let (hh, ll) = (ta::highest(&b.high, 14), ta::lowest(&b.low, 14));
    let want: Vec<f64> = (0..N)
        .map(|i| 100.0 * (b.close[i] - ll[i]) / (hh[i] - ll[i]))
        .collect();
    same("raw stochastic", &raw, &want);
}

/// The plots of a script run in the context of a chart: two hours east of UTC, hourly bars, a
/// symbol of 5 digits, the last bar still forming.
fn in_chart(source: &str, digits: u32) -> BTreeMap<String, Vec<f64>> {
    use super::run::Setup;
    use crate::indicators::data::StudyContext;
    use crate::market::{Period, Timeframe};
    let context = StudyContext {
        offsets: std::sync::Arc::new(vec![7_200_000; N]),
        timeframe: Some(Timeframe::Bars(Period::H1)),
        symbol: "EURUSD".to_owned(),
        digits,
        live: true,
    };
    let (values, texts, others) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let setup = Setup {
        values: &values,
        texts: &texts,
        others: &others,
        context: &context,
    };
    let script = Script::compile(source).unwrap_or_else(|p| panic!("{source}: {p:?}"));
    let done = script
        .compute_in(&bars(), &setup, Limits::default(), None)
        .unwrap_or_else(|p| panic!("{source}: {p:?}"));
    done.output
        .plots
        .into_iter()
        .map(|p| (p.key.to_owned(), p.values))
        .collect()
}

#[test]
fn the_calendar_is_read_in_the_zone_of_the_chart() {
    let drawn = in_chart(
        "plot(\"a\", hour());\nplot(\"b\", dayofweek());\nplot(\"c\", year());\nplot(\"d\", month());\n\
         plot(\"e\", in_session(\"0200-0400\"));\nplot(\"f\", utc_offset());\nplot(\"g\", is_new_period(\"day\"));",
        5,
    );
    // Bar 0 is 1970-01-01 00:00 UTC, so 02:00 there, and a Thursday.
    assert_eq!(&drawn["a"][..4], &[2.0, 3.0, 4.0, 5.0]);
    assert_eq!(drawn["b"][0], 5.0);
    assert_eq!((drawn["c"][0], drawn["d"][0]), (1970.0, 1.0));
    assert_eq!(&drawn["e"][..4], &[1.0, 1.0, 0.0, 0.0]);
    assert!(drawn["f"].iter().all(|m| *m == 120.0));
    let first: Vec<usize> = (0..N).filter(|i| drawn["g"][*i] == 1.0).collect();
    assert_eq!(first, vec![0, 24, 48, 72, 96]);
}

#[test]
fn the_timeframe_the_symbol_and_the_bar_are_known_to_the_script() {
    let drawn = in_chart(
        "plot(\"a\", timeframe_seconds());\nplot(\"b\", timeframe_multiplier());\n\
         plot(\"c\", if timeframe_is_intraday() && !timeframe_is_daily() { 1 } else { 0 });\n\
         plot(\"d\", syminfo_mintick());\nplot(\"e\", price_scale());\nplot(\"f\", to_raw(1.1));\n\
         plot(\"g\", barstate_islast());\nplot(\"h\", barstate_isrealtime());\nplot(\"i\", barstate_isconfirmed());\n\
         plot(\"j\", last_bar_index());",
        5,
    );
    let first = |key: &str| drawn[key][0];
    assert_eq!((first("a"), first("b"), first("c")), (3600.0, 60.0, 1.0));
    assert_eq!((first("d"), first("e")), (1.0, 100_000.0));
    assert!((first("f") - 110_000.0).abs() < 1e-6);
    assert_eq!((drawn["g"][N - 1], drawn["g"][0]), (1.0, 0.0));
    assert_eq!(
        (drawn["h"][N - 1], drawn["i"][N - 1], drawn["i"][0]),
        (1.0, 0.0, 1.0)
    );
    assert_eq!(first("j"), count_f64(N - 1));
    // Three digits: a step of 100.
    assert_eq!(
        in_chart("plot(\"a\", syminfo_mintick());", 3)["a"][0],
        100.0
    );
}

fn logged(source: &str) -> Vec<String> {
    Script::compile(source)
        .unwrap_or_else(|p| panic!("{source}: {p:?}"))
        .compute(&bars(), &BTreeMap::new(), Limits::default(), None)
        .unwrap_or_else(|p| panic!("{source}: {p:?}"))
        .log
}

#[test]
fn the_trigonometry_and_rounding_match_the_standard_functions() {
    let ratio: Vec<f64> = bars().close.iter().map(|c| c / 200.0).collect();
    same(
        "asin",
        &plot("plot(\"a\", asin(close / 200));"),
        &ratio.iter().map(|x| x.asin()).collect::<Vec<_>>(),
    );
    same(
        "atan",
        &plot("plot(\"a\", atan(close / 200));"),
        &ratio.iter().map(|x| x.atan()).collect::<Vec<_>>(),
    );
    same(
        "tan",
        &plot("plot(\"a\", tan(close / 200));"),
        &ratio.iter().map(|x| x.tan()).collect::<Vec<_>>(),
    );
    same(
        "todegrees",
        &plot("plot(\"a\", todegrees(close / 200));"),
        &ratio.iter().map(|x| x.to_degrees()).collect::<Vec<_>>(),
    );
    // acos is not a number outside -1 to 1: no value, not an error.
    assert!(plot("plot(\"a\", acos(close));").iter().all(|v| v.is_nan()));
    let rounded = plot("plot(\"a\", round(close / 7, 2));");
    assert!(
        rounded
            .iter()
            .filter(|v| v.is_finite())
            .all(|v| (v * 100.0 - (v * 100.0).round()).abs() < 1e-9)
    );
    same(
        "log is ln",
        &plot("plot(\"a\", log(close));"),
        &plot("plot(\"a\", ln(close));"),
    );
}

#[test]
fn several_values_at_once_and_arrays_of_numbers() {
    let b = bars();
    let three: Vec<f64> = (0..N)
        .map(|i| b.close[i].min(b.open[i]).min(100.0))
        .collect();
    same(
        "min of three",
        &plot("plot(\"a\", min(close, open, 100));"),
        &three
            .iter()
            .enumerate()
            .map(|(i, v)| if b.close[i].is_nan() { f64::NAN } else { *v })
            .collect::<Vec<_>>(),
    );
    let mid: Vec<f64> = (0..N).map(|i| f64::midpoint(b.high[i], b.low[i])).collect();
    same("avg of two", &plot("plot(\"a\", avg(high, low));"), &mid);
    let log = logged(
        "let a = [4, 8, 6, 2];\n\
         print(`${sum(a)} ${avg(a)} ${min(a)} ${max(a)} ${median(a)} ${percentile(a, 50)}`);\n\
         plot(\"p\", close);",
    );
    assert_eq!(log, vec!["20.0 5.0 2.0 8.0 5.0 5.0".to_owned()]);
    // An array of series gives the largest bar by bar.
    same(
        "max of an array of series",
        &plot("plot(\"a\", max([close, open, low]));"),
        &(0..N)
            .map(|i| b.close[i].max(b.open[i]).max(b.low[i]))
            .collect::<Vec<_>>(),
    );
}

#[test]
fn text_and_colors_are_made_from_numbers() {
    let log = logged(
        "print(fmt(3.14159, 2));\nprint(fmt(7, 0));\nprint(tostring(2.5));\n\
         print(color_mix(\"black\", \"white\", 0.5));\n\
         print(from_gradient(50, 0, 100, \"black\", \"white\"));\n\
         print(`${color_r(\"#102030\")} ${color_g(\"#102030\")} ${color_b(\"#102030\")}`);\n\
         plot(\"p\", close);",
    );
    assert_eq!(log, ["3.14", "7", "2.5", "#808080", "#808080", "16 32 48"]);
}

#[test]
fn the_names_of_pine_are_the_same_functions() {
    same(
        "crossover",
        &plot("plot(\"a\", crossover(close, sma(close, 8)));"),
        &plot("plot(\"a\", cross_over(close, sma(close, 8)));"),
    );
    same(
        "crossunder",
        &plot("plot(\"a\", crossunder(close, sma(close, 8)));"),
        &plot("plot(\"a\", cross_under(close, sma(close, 8)));"),
    );
    same(
        "mom",
        &plot("plot(\"a\", mom(close, 6));"),
        &plot("plot(\"a\", momentum(close, 6));"),
    );
    same(
        "wpr",
        &plot("plot(\"a\", wpr(14));"),
        &plot("plot(\"a\", williams_r(14));"),
    );
    same(
        "sar",
        &plot("plot(\"a\", sar(0.02, 0.02, 0.2));"),
        &plot("plot(\"a\", psar(0.02, 0.02, 0.2));"),
    );
}
