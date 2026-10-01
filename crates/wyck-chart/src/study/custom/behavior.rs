//! What scripts do, end to end: each function against the arithmetic it stands for, the rules
//! about what a script may declare, and the mistakes it is told about.

use std::collections::BTreeMap;

use super::super::{InputKind, PlotKind, StudyInput, math};
use super::run::{Computed, Limits, Script};

fn bars(n: usize) -> StudyInput {
    let close: Vec<f64> = (0..n)
        .map(|i| 100.0 + (i as f64 * 0.31).sin() * 6.0 + i as f64 * 0.05)
        .collect();
    StudyInput {
        time: (0..n as i64)
            .map(|i| 1_700_000_000_000 + i * 60_000)
            .collect(),
        open: close.iter().map(|c| c - 0.4).collect(),
        high: close.iter().map(|c| c + 1.1).collect(),
        low: close.iter().map(|c| c - 0.9).collect(),
        volume: (0..n).map(|i| 10.0 + (i % 7) as f64).collect(),
        day: (0..n).map(|i| (i / 40) as i64).collect(),
        close,
    }
}

fn run_with(source: &str, input: &StudyInput, values: &[(&str, f64)]) -> Computed {
    let script = Script::compile(source).unwrap_or_else(|p| panic!("{p:?}\n{source}"));
    let values: BTreeMap<String, f64> = values.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect();
    script
        .compute(input, &values, Limits::default(), None)
        .unwrap_or_else(|p| panic!("{p:?}\n{source}"))
}

fn run(source: &str, n: usize) -> Computed {
    run_with(source, &bars(n), &[])
}

fn plot<'a>(done: &'a Computed, key: &str) -> &'a [f64] {
    &done
        .output
        .plots
        .iter()
        .find(|p| p.key == key)
        .unwrap_or_else(|| panic!("no plot {key}"))
        .values
}

fn same(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| (x.is_nan() && y.is_nan()) || (x - y).abs() < 1e-9)
}

/// The messages of the problems a script has, at compile time or over 30 bars.
fn problems(source: &str) -> Vec<String> {
    match Script::compile(source) {
        Ok(script) => script
            .compute(&bars(30), &BTreeMap::new(), Limits::default(), None)
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.message)
            .collect(),
        Err(problems) => problems.into_iter().map(|p| p.message).collect(),
    }
}

#[test]
fn the_windowed_functions_are_the_arithmetic_they_stand_for() {
    let input = bars(120);
    let done = run_with(
        "plot(\"sma\", sma(close, 10));\n\
         plot(\"ema\", ema(close, 10));\n\
         plot(\"rsi\", rsi(close, 14));\n\
         plot(\"hi\", highest(high, 7));\n\
         plot(\"sd\", stdev(close, 9));\n\
         plot(\"wma\", wma(close, 6));\n\
         plot(\"hma\", hma(close, 9));",
        &input,
        &[],
    );
    assert!(same(plot(&done, "sma"), &math::sma(&input.close, 10)));
    assert!(same(plot(&done, "ema"), &math::ema(&input.close, 10)));
    assert!(same(plot(&done, "rsi"), &math::rsi(&input.close, 14)));
    assert!(same(plot(&done, "hi"), &math::highest(&input.high, 7)));
    assert!(same(plot(&done, "sd"), &math::stdev(&input.close, 9)));
    assert!(same(plot(&done, "wma"), &math::wma(&input.close, 6)));
    assert!(same(plot(&done, "hma"), &math::hma(&input.close, 9)));
}

#[test]
fn the_ready_made_indicators_are_the_arithmetic_they_stand_for() {
    let input = bars(150);
    let done = run_with(
        "let m = macd(close, 12, 26, 9);\n\
         plot(\"macd\", m.macd); plot(\"signal\", m.signal); plot(\"hist\", m.hist);\n\
         let b = bollinger(close, 20, 2.0);\n\
         plot(\"upper\", b.upper); plot(\"basis\", b.basis); plot(\"lower\", b.lower);\n\
         plot(\"atr\", atr(14));\n\
         plot(\"vwap\", vwap());\n\
         plot(\"obv\", obv());\n\
         let s = stoch(14, 3, 3);\n\
         plot(\"k\", s.k); plot(\"d\", s.d);\n\
         let d = dmi(14, 14);\n\
         plot(\"adx\", d.adx);\n\
         let st = supertrend(10, 3.0);\n\
         plot(\"st\", st.line);\n\
         plot(\"psar\", psar(0.02, 0.02, 0.2));\n\
         plot(\"mfi\", mfi(14));",
        &input,
        &[],
    );
    let (line, signal, hist) = math::macd(&input.close, 12, 26, 9);
    assert!(same(plot(&done, "macd"), &line));
    assert!(same(plot(&done, "signal"), &signal));
    assert!(same(plot(&done, "hist"), &hist));
    let basis = math::sma(&input.close, 20);
    let dev = math::stdev(&input.close, 20);
    let upper: Vec<f64> = basis.iter().zip(&dev).map(|(b, d)| b + 2.0 * d).collect();
    assert!(same(plot(&done, "upper"), &upper));
    assert!(same(plot(&done, "basis"), &basis));
    assert!(same(
        plot(&done, "atr"),
        &math::atr(&input.high, &input.low, &input.close, 14)
    ));
    let typical: Vec<f64> = (0..150)
        .map(|i| (input.high[i] + input.low[i] + input.close[i]) / 3.0)
        .collect();
    assert!(same(
        plot(&done, "vwap"),
        &math::vwap(&typical, &input.volume, &input.day)
    ));
    assert!(same(
        plot(&done, "obv"),
        &math::obv(&input.close, &input.volume)
    ));
    let (k, d) = math::stochastic(&input.high, &input.low, &input.close, 14, 3, 3);
    assert!(same(plot(&done, "k"), &k) && same(plot(&done, "d"), &d));
    let (_, _, adx) = math::dmi(&input.high, &input.low, &input.close, 14, 14);
    assert!(same(plot(&done, "adx"), &adx));
    assert!(same(
        plot(&done, "st"),
        &math::supertrend(&input.high, &input.low, &input.close, 10, 3.0).0
    ));
    assert!(same(
        plot(&done, "psar"),
        &math::parabolic_sar(&input.high, &input.low, 0.02, 0.02, 0.2)
    ));
    assert!(same(
        plot(&done, "mfi"),
        &math::mfi(&typical, &input.volume, 14)
    ));
}

#[test]
fn arithmetic_works_between_series_and_with_numbers_on_either_side() {
    let input = bars(20);
    let done = run_with(
        "plot(\"a\", close - open);\n\
         plot(\"b\", 2 * close);\n\
         plot(\"c\", 100.0 - close);\n\
         plot(\"d\", close / 2 + 1);\n\
         plot(\"e\", -close);\n\
         plot(\"f\", close ** 2);",
        &input,
        &[],
    );
    for i in 0..20 {
        assert!((plot(&done, "a")[i] - 0.4).abs() < 1e-9);
        assert!((plot(&done, "b")[i] - 2.0 * input.close[i]).abs() < 1e-9);
        assert!((plot(&done, "c")[i] - (100.0 - input.close[i])).abs() < 1e-9);
        assert!((plot(&done, "d")[i] - (input.close[i] / 2.0 + 1.0)).abs() < 1e-9);
        assert!((plot(&done, "e")[i] + input.close[i]).abs() < 1e-9);
        assert!((plot(&done, "f")[i] - input.close[i].powi(2)).abs() < 1e-6);
    }
}

#[test]
fn indexing_reads_the_bars_before_and_the_first_ones_have_no_value() {
    let input = bars(10);
    let done = run_with(
        "plot(\"prev\", close[1]); plot(\"two\", shift(close, 2));",
        &input,
        &[],
    );
    let prev = plot(&done, "prev");
    assert!(prev[0].is_nan());
    assert_eq!(prev[1], input.close[0]);
    assert_eq!(plot(&done, "two")[5], input.close[3]);
    assert!(problems("plot(\"a\", close[-1]);")[0].contains("shift"));
}

#[test]
fn conditions_are_ones_and_zeros_and_combine() {
    let done = run(
        "let up = close > open;\n\
         let big = (high - low) > 2.5;\n\
         plot(\"and\", up & big);\n\
         plot(\"or\", up | big);\n\
         plot(\"not\", !up);\n\
         plot(\"pick\", iff(up, 1, -1));\n\
         plot(\"cross\", cross_over(close, sma(close, 5)));\n\
         plot(\"under\", cross_under(close, sma(close, 5)));",
        60,
    );
    // Every bar of this data closes over its open, and none has a range over 2.5.
    assert!(plot(&done, "and").iter().all(|v| *v == 0.0));
    assert!(plot(&done, "or").iter().all(|v| *v == 1.0));
    assert!(plot(&done, "not").iter().all(|v| *v == 0.0));
    assert!(plot(&done, "pick").iter().all(|v| *v == 1.0));
    let over = plot(&done, "cross");
    let under = plot(&done, "under");
    assert!(over.contains(&1.0) && under.contains(&1.0));
    assert!(
        over.iter()
            .zip(under)
            .all(|(a, b)| !(*a == 1.0 && *b == 1.0))
    );
}

#[test]
fn a_loop_can_build_a_series_bar_by_bar() {
    let done = run(
        "let out = series(n, 0.0);\n\
         for i in 1..n { out.set(i, out.at(i - 1) + 1.0); }\n\
         plot(\"count\", out);",
        25,
    );
    assert_eq!(plot(&done, "count")[24], 24.0);
}

#[test]
fn nz_na_and_the_math_functions_work_on_every_bar() {
    let done = run(
        "plot(\"nz\", nz(sma(close, 5), -1.0));\n\
         plot(\"na\", is_na(sma(close, 5)));\n\
         plot(\"clamp\", clamp(close, 100.0, 101.0));\n\
         plot(\"max\", max(close, 101.0));\n\
         plot(\"abs\", abs(close - 100.0));\n\
         plot(\"round\", round(close));",
        30,
    );
    assert_eq!(plot(&done, "nz")[0], -1.0);
    assert_eq!(plot(&done, "na")[0], 1.0);
    assert_eq!(plot(&done, "na")[10], 0.0);
    assert!(
        plot(&done, "clamp")
            .iter()
            .all(|v| (100.0..=101.0).contains(v))
    );
    assert!(plot(&done, "max").iter().all(|v| *v >= 101.0));
}

#[test]
fn inputs_of_every_kind_are_declared_and_read() {
    let source = "let a = input_int(\"len\", 5, #{ min: 1, max: 9 });\n\
        let b = input_float(\"mult\", 1.5, #{ step: 0.5 });\n\
        let c = input_bool(\"on\", true);\n\
        let d = input_source(\"source\", \"high\");\n\
        let e = input_choice(\"kind\", \"EMA\", [\"SMA\", \"EMA\"]);\n\
        let f = input_color(\"tint\", \"orange\");\n\
        print(`${a} ${b} ${c} ${e} ${f}`);\n\
        plot(\"p\", d * b);";
    let script = Script::compile(source).unwrap();
    let kinds: Vec<_> = script
        .declaration
        .inputs
        .iter()
        .map(|i| i.input_kind())
        .collect();
    assert!(matches!(kinds[0], InputKind::Int));
    assert!(matches!(kinds[1], InputKind::Float));
    assert!(matches!(kinds[2], InputKind::Toggle));
    assert!(matches!(kinds[3], InputKind::Source));
    assert!(matches!(kinds[4], InputKind::Choice(["SMA", "EMA"])));
    assert!(matches!(kinds[5], InputKind::Color));

    let input = bars(8);
    let with_defaults = run_with(source, &input, &[]);
    assert_eq!(with_defaults.log[0], "5 1.5 true EMA #ff9800");
    assert_eq!(plot(&with_defaults, "p")[3], input.high[3] * 1.5);
    let chosen = run_with(
        source,
        &input,
        &[
            ("len", 99.0),
            ("on", 0.0),
            ("kind", 0.0),
            ("source", 2.0),
            ("tint", f64::from(0x0011_2233_u32)),
        ],
    );
    assert_eq!(
        chosen.log[0], "9 1.5 false SMA #112233",
        "a value is kept inside its range"
    );
    assert_eq!(plot(&chosen, "p")[3], input.low[3] * 1.5);
}

#[test]
fn plots_take_their_style_from_the_script() {
    let script = Script::compile(
        "plot(\"line\", close, #{ color: \"#123456\", width: 3, dash: \"dashed\", title: \"My line\" });\n\
         plot(\"bars\", volume, #{ style: \"histogram\" });\n\
         plot(\"dots\", close, #{ style: \"dots\" });",
    )
    .unwrap();
    let plots = &script.declaration.plots;
    assert_eq!(
        (plots[0].color, plots[0].width, plots[0].label.as_str()),
        (0x0012_3456, 3.0, "My line")
    );
    assert_eq!(plots[0].dash, crate::drawing::model::Dash::Dashed);
    assert_eq!(plots[1].kind, PlotKind::Histogram);
    assert_eq!(plots[2].kind, PlotKind::Dots);
    assert_ne!(
        plots[0].color, plots[1].color,
        "plots without a color get different ones"
    );
}

#[test]
fn fills_levels_bands_and_histogram_colors_reach_the_output() {
    let done = run(
        "plot(\"a\", close);\n\
         plot(\"b\", close - 2.0);\n\
         fill(\"a\", \"b\", #{ alpha: 0.2 });\n\
         hline(50); hline(70);\n\
         band(30, 70);\n\
         plot(\"h\", close - open, #{ style: \"histogram\", up: close > open });",
        12,
    );
    assert_eq!(done.output.fills.len(), 1);
    assert_eq!((done.output.fills[0].a, done.output.fills[0].b), (0, 1));
    assert!((done.output.fills[0].alpha - 0.2).abs() < 1e-6);
    assert_eq!(done.output.levels, [50.0, 70.0]);
    assert_eq!(done.output.band, Some((30.0, 70.0)));
    assert!(done.output.plots[2].up.as_ref().unwrap().iter().all(|u| *u));
}

#[test]
fn the_indicator_call_sets_the_words_the_menus_show() {
    let script = Script::compile(
        "indicator(#{ name: \"Momentum X\", short: \"MX\", overlay: false, format: \"count\",\n\
           range: [0, 100], category: \"Momentum\", description: \"Does a thing\", author: \"Me\", version: \"1.2\" });\n\
         plot(\"a\", close);",
    )
    .unwrap();
    let meta = &script.declaration.meta;
    assert_eq!(meta.name.as_deref(), Some("Momentum X"));
    assert_eq!(meta.range, Some((0.0, 100.0)));
    assert_eq!(
        (
            meta.category.as_str(),
            meta.author.as_str(),
            meta.version.as_str()
        ),
        ("Momentum", "Me", "1.2")
    );
    assert!(matches!(meta.format, super::super::ValueFormat::Count));
}

#[test]
fn what_a_script_gets_wrong_is_said_plainly() {
    let has = |source: &str, text: &str| {
        let found = problems(source);
        assert!(
            found.first().is_some_and(|m| m.contains(text)),
            "{source}: expected \"{text}\", got {found:?}"
        );
    };
    has(
        "plot(\"a\", close, #{ colour: \"red\" });",
        "unknown option \"colour\"",
    );
    has("plot(\"A b\", close);", "lowercase");
    has("plot(\"a\", close); plot(\"a\", open);", "twice");
    has(
        "let x = input_int(\"n\", 1); let y = input_int(\"n\", 2);",
        "twice",
    );
    has("plot(\"a\", close, #{ style: \"bars\" });", "style");
    has("plot(\"a\", close, #{ color: \"nope\" });", "not a color");
    has("indicator(#{ overlay: true }); indicator(#{});", "once");
    has("plot(\"a\", close + series(3, 1.0));", "as long as");
    has("fill(\"x\", \"y\");", "not drawn yet");
    has("let m = input_source(\"s\", \"middle\");", "not a price");
    has(
        "let c = input_choice(\"k\", \"C\", [\"A\", \"B\"]);",
        "not one of the choices",
    );
}

#[test]
fn a_constant_can_be_plotted_and_a_script_may_not_declare_too_much() {
    let done = run("plot(\"zero\", 0); plot(\"one\", 1.5);", 4);
    assert_eq!(plot(&done, "zero"), [0.0; 4]);
    assert_eq!(plot(&done, "one"), [1.5; 4]);
    let many: String = (0..17).map(|i| format!("plot(\"p{i}\", close);")).collect();
    assert!(problems(&many)[0].contains("at most"));
}

#[test]
fn a_script_sees_an_empty_chart_and_a_long_one_alike() {
    for n in [0, 1, 2, 500] {
        let done = run(
            "let l = input_int(\"l\", 14, #{ min: 1, max: 100 });\n\
             plot(\"r\", rsi(close, l)); plot(\"a\", atr(l));",
            n,
        );
        assert_eq!(plot(&done, "r").len(), n);
    }
}
