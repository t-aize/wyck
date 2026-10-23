//! Scripts written to push on the limits of the engine: they must end with a clear error, never a
//! panic, a hang or a silent wrong answer.

use super::decl::{Limits, Problem, Severity};
use super::run::{Computed, Script};
use crate::base::math::{count_f64, count_i64};
use crate::indicators::data::StudyInput;
use std::collections::BTreeMap;

fn bars(n: usize) -> StudyInput {
    let close: Vec<f64> = (0..n)
        .map(|i| 100.0 + (count_f64(i) * 0.1).sin() * 5.0)
        .collect();
    StudyInput {
        time: (0..count_i64(n)).map(|i| i * 60_000).collect(),
        open: close.iter().map(|c| c - 0.5).collect(),
        high: close.iter().map(|c| c + 1.0).collect(),
        low: close.iter().map(|c| c - 1.0).collect(),
        volume: vec![10.0; n],
        day: vec![0; n],
        close,
    }
}

fn run(source: &str, n: usize, limits: Limits) -> Result<Computed, Vec<Problem>> {
    Script::compile(source)?.compute(&bars(n), &BTreeMap::new(), limits, None)
}

fn message_of(result: Result<Computed, Vec<Problem>>) -> String {
    result
        .err()
        .and_then(|problems| problems.into_iter().next())
        .map(|problem| problem.message)
        .unwrap_or_default()
}

#[test]
fn a_script_that_keeps_too_many_series_is_stopped_on_memory() {
    let limits = Limits {
        memory: 4 * 1024 * 1024,
        ..Limits::default()
    };
    let source = "plot(\"a\", close);\n\
                  let keep = [];\n\
                  for i in 0..200 { keep.push(sma(close, 2)); }";
    let said = message_of(run(source, 50_000, limits));
    assert!(said.contains("MiB of series"), "{said}");
}

#[test]
fn a_window_as_long_as_the_chart_is_refused_by_its_cost_not_computed() {
    let limits = Limits {
        operations: 100_000,
        ..Limits::default()
    };
    let said = message_of(run("plot(\"a\", stdev(close, 100000));", 50_000, limits));
    assert!(said.contains("operations"), "{said}");
}

#[test]
fn a_cheap_window_still_runs_inside_a_small_budget() {
    let limits = Limits {
        operations: 100_000,
        ..Limits::default()
    };
    assert!(run("plot(\"a\", stdev(close, 20));", 5_000, limits).is_ok());
}

#[test]
fn a_shift_of_any_size_gives_no_value_and_does_not_panic() {
    for bars_ahead in [
        "9223372036854775807",
        "-9223372036854775807",
        "1000000000000",
    ] {
        let done = run(
            &format!("plot(\"a\", shift(close, {bars_ahead}));"),
            20,
            Limits::default(),
        )
        .unwrap();
        assert!(done.output.plots[0].values.iter().all(|v| v.is_nan()));
    }
}

#[test]
fn a_division_by_zero_is_no_value_never_an_infinity() {
    let done = run(
        "plot(\"a\", close / 0);\nplot(\"b\", ln(close * 0));\nplot(\"c\", close ** 1000);",
        5,
        Limits::default(),
    )
    .unwrap();
    for plot in &done.output.plots {
        assert!(
            plot.values.iter().all(|v| !v.is_infinite()),
            "{}: {:?}",
            plot.key,
            plot.values
        );
    }
    assert!(done.output.plots[0].values.iter().all(|v| v.is_nan()));
}

#[test]
fn a_plot_that_only_exists_on_bars_is_an_error_not_an_unknown_line() {
    let said = message_of(run(
        "plot(\"a\", close);\nif n > 0 { plot(\"late\", close); }",
        10,
        Limits::default(),
    ));
    assert!(said.contains("late"), "{said}");
}

#[test]
fn an_input_that_only_exists_on_bars_is_an_error() {
    let said = message_of(run(
        "plot(\"a\", close);\nif n > 0 { let x = input_int(\"extra\", 1); }",
        10,
        Limits::default(),
    ));
    assert!(said.contains("extra"), "{said}");
}

#[test]
fn more_levels_than_the_limit_is_an_error_not_a_silent_cut() {
    let said = message_of(run(
        "plot(\"a\", close);\nfor i in 0..17 { hline(i); }",
        10,
        Limits::default(),
    ));
    assert!(said.contains("at most 16 levels"), "{said}");
}

#[test]
fn a_level_that_is_not_a_number_is_an_error() {
    let said = message_of(run(
        "plot(\"a\", close);\nhline(na);",
        10,
        Limits::default(),
    ));
    assert!(said.contains("hline"), "{said}");
}

#[test]
fn a_band_drawn_twice_and_an_inline_option_are_warned_about() {
    let done = run(
        "let a = input_int(\"a\", 1, #{ inline: \"row\" });\n\
         plot(\"p\", close);\nband(1, 2);\nband(3, 4);",
        10,
        Limits::default(),
    )
    .unwrap();
    let warnings: Vec<&str> = done.warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(warnings.iter().any(|w| w.contains("band")), "{warnings:?}");
    assert!(
        warnings.iter().any(|w| w.contains("inline")),
        "{warnings:?}"
    );
    assert!(
        done.warnings
            .iter()
            .all(|w| w.severity == Severity::Warning)
    );
}

#[test]
fn a_call_with_the_wrong_kind_of_argument_says_how_the_function_is_called() {
    let said = message_of(run(
        "plot(\"a\", sma(close, \"x\"));",
        10,
        Limits::default(),
    ));
    assert!(said.contains("Usage: sma("), "{said}");
}

#[test]
fn whole_numbers_and_decimals_both_work_wherever_a_number_is_taken() {
    let done = run(
        "let b = bollinger(close, 20, 2);\n\
         let s = supertrend(10, 3);\n\
         plot(\"a\", clamp(close, 90, 110.5));\n\
         plot(\"b\", nz(close, 0));\n\
         plot(\"c\", min(1, close));\n\
         plot(\"d\", max(close, 1));\n\
         plot(\"e\", pow(close, 2));\n\
         plot(\"f\", psar(1, 1, 1));\n\
         plot(\"g\", b.upper);",
        60,
        Limits::default(),
    );
    assert!(done.is_ok(), "{:?}", done.err());
}

#[test]
fn a_run_ends_with_the_series_it_made_released_so_the_next_one_starts_clean() {
    let limits = Limits {
        memory: 4 * 1024 * 1024,
        ..Limits::default()
    };
    // Two runs in a row on the same thread: the second one must not inherit the first one's count.
    for _ in 0..3 {
        assert!(run("plot(\"a\", sma(close, 5));", 20_000, limits).is_ok());
    }
}
