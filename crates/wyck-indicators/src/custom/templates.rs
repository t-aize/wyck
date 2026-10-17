//! The scripts a new indicator can start from: ten examples that between them use every call and
//! every option of the language, from the settings to drawings and loops.
//! The files are in `assets/starters` of the crate and are compiled into it.

/// A script to start from.
#[derive(Debug, Clone, Copy)]
pub struct Template {
    pub name: &'static str,
    pub description: &'static str,
    pub source: &'static str,
}

pub const TEMPLATES: &[Template] = &[
    Template {
        name: "Settings",
        description: "Every kind of setting (numbers, switch, price, choice, color, text, timeframe, session) with all its options.",
        source: include_str!("../../assets/starters/settings.rhai"),
    },
    Template {
        name: "Plots and levels",
        description: "Lines, histogram with up and down colors, dots, offset, levels, a band and fills in a pane.",
        source: include_str!("../../assets/starters/plots.rhai"),
    },
    Template {
        name: "Series lab",
        description: "Pick any function on series (math, shifts, conditions, operators) and see its result.",
        source: include_str!("../../assets/starters/series_lab.rhai"),
    },
    Template {
        name: "Window lab",
        description: "Pick any average or window function (sma, ema, hma, stdev, rsi, linreg...) and see it.",
        source: include_str!("../../assets/starters/window_lab.rhai"),
    },
    Template {
        name: "Overlay toolbox",
        description: "Bollinger, Keltner, Donchian, Supertrend, parabolic SAR and VWAP behind switches.",
        source: include_str!("../../assets/starters/overlay_toolbox.rhai"),
    },
    Template {
        name: "Oscillator toolbox",
        description: "RSI, stochastic, MACD, CCI, Williams R, MFI, DMI, ATR, OBV and more, one at a time.",
        source: include_str!("../../assets/starters/oscillator_toolbox.rhai"),
    },
    Template {
        name: "Drawings",
        description: "Lines, Fibonacci, channel, rectangle, arrow, text and a position drawn by a script.",
        source: include_str!("../../assets/starters/drawings.rhai"),
    },
    Template {
        name: "Loops and state",
        description: "An ATR trailing stop written with functions, for, while and loop.",
        source: include_str!("../../assets/starters/loops.rhai"),
    },
    Template {
        name: "Two symbols",
        description: "The symbol of the chart against another one, both starting at 100.",
        source: include_str!("../../assets/starters/two_symbols.rhai"),
    },
    Template {
        name: "Session range",
        description: "The high and low of a daily session, from a session setting and the time of the bars.",
        source: include_str!("../../assets/starters/session_range.rhai"),
    },
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::custom::decl::{InputType, Limits};
    use crate::custom::other::{OtherBars, OtherSymbols, key_of};
    use crate::custom::run::Script;
    use crate::data::{StudyInput, StudyOutput};
    use wyck_core::math::{count_f64, count_i64};

    /// 15 minute bars over a few days (2026-01-05 00:00 UTC on), moving enough that every
    /// indicator has something to say.
    fn market() -> StudyInput {
        const START: i64 = 1_767_571_200_000;
        const BARS: usize = 600;
        let close: Vec<f64> = (0..BARS)
            .map(|i| {
                let x = count_f64(i);
                100.0 + 6.0 * (x / 15.0).sin() + 3.0 * (x / 4.0).sin() + x / 100.0
            })
            .collect();
        let open: Vec<f64> = std::iter::once(100.0)
            .chain(close.iter().copied())
            .take(BARS)
            .collect();
        let time: Vec<i64> = (0..BARS).map(|i| START + count_i64(i) * 900_000).collect();
        StudyInput {
            day: time.iter().map(|t| t.div_euclid(86_400_000)).collect(),
            high: open
                .iter()
                .zip(&close)
                .enumerate()
                .map(|(i, (o, c))| o.max(*c) + 0.4 + count_f64(i % 5) * 0.1)
                .collect(),
            low: open
                .iter()
                .zip(&close)
                .enumerate()
                .map(|(i, (o, c))| o.min(*c) - 0.4 - count_f64(i % 3) * 0.1)
                .collect(),
            volume: (0..BARS)
                .map(|i| 100.0 + count_f64(i % 11) * 20.0)
                .collect(),
            time,
            open,
            close,
        }
    }

    /// Another symbol for the scripts that compare two: the same bars, a little different.
    fn others(script: &Script, input: &StudyInput) -> OtherSymbols {
        script
            .symbols_wanted(&BTreeMap::new())
            .into_iter()
            .map(|name| {
                let mut bars = OtherBars::from_input(input);
                for price in bars
                    .open
                    .iter_mut()
                    .chain(&mut bars.high)
                    .chain(&mut bars.low)
                    .chain(&mut bars.close)
                {
                    *price = *price * 1.3 + 7.0;
                }
                (key_of(&name), bars)
            })
            .collect()
    }

    fn run(
        template: &Template,
        script: &Script,
        input: &StudyInput,
        values: &BTreeMap<String, f64>,
    ) -> StudyOutput {
        script
            .compute_with_others(
                input,
                values,
                &BTreeMap::new(),
                &others(script, input),
                Limits::default(),
                None,
            )
            .unwrap_or_else(|problems| panic!("{}: {problems:?}", template.name))
            .output
    }

    fn has_a_value(output: &StudyOutput) -> bool {
        !output.drawings.is_empty()
            || output
                .plots
                .iter()
                .any(|plot| plot.values.iter().any(|v| v.is_finite()))
    }

    fn template(name: &str) -> &'static Template {
        TEMPLATES
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("no starter called {name}"))
    }

    #[test]
    fn starters_compile_and_compute() {
        let input = market();
        let mut names = Vec::new();
        for template in TEMPLATES {
            assert!(template.source.is_ascii(), "{}", template.name);
            assert!(!template.description.is_empty(), "{}", template.name);
            // The name becomes the name of the file of the new script.
            assert!(
                template.name.chars().count() <= 64
                    && template
                        .name
                        .chars()
                        .all(|c| c.is_alphanumeric() || " -_()".contains(c)),
                "{} is not a name a script file can have",
                template.name
            );
            names.push(template.name);
            let script = Script::compile(template.source).unwrap();
            assert_eq!(script.declaration.meta.name.as_deref(), Some(template.name));
            assert!(script.warnings.is_empty(), "{}", template.name);
            let output = run(template, &script, &input, &BTreeMap::new());
            assert!(has_a_value(&output), "{} shows nothing", template.name);
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), TEMPLATES.len(), "two starters share a name");
    }

    #[test]
    fn starters_also_compute_on_no_bars_and_on_one_bar() {
        let full = market();
        for bars in [0, 1] {
            let few = StudyInput {
                time: full.time.iter().copied().take(bars).collect(),
                open: full.open.iter().copied().take(bars).collect(),
                high: full.high.iter().copied().take(bars).collect(),
                low: full.low.iter().copied().take(bars).collect(),
                close: full.close.iter().copied().take(bars).collect(),
                volume: full.volume.iter().copied().take(bars).collect(),
                day: full.day.iter().copied().take(bars).collect(),
            };
            for template in TEMPLATES {
                let script = Script::compile(template.source).unwrap();
                run(template, &script, &few, &BTreeMap::new());
            }
        }
    }

    /// Every choice of every starter, every switch on and off, and all the switches together:
    /// the tours are labs, and a branch that fails would only be found by a user.
    #[test]
    fn every_choice_and_switch_of_a_starter_computes() {
        let input = market();
        for template in TEMPLATES {
            let script = Script::compile(template.source).unwrap();
            let mut all_on = BTreeMap::new();
            for decl in &script.declaration.inputs {
                let tries: Vec<f64> = match decl.kind {
                    InputType::Choice => (0..decl.options.len()).map(count_f64).collect(),
                    InputType::Bool => {
                        all_on.insert(decl.key.clone(), 1.0);
                        vec![0.0, 1.0]
                    }
                    InputType::Int | InputType::Float => {
                        vec![decl.min.max(1.0), decl.max.min(50.0)]
                    }
                    _ => continue,
                };
                for value in tries {
                    let values = BTreeMap::from([(decl.key.clone(), value)]);
                    let output = run(template, &script, &input, &values);
                    assert_eq!(output.plots.len(), script.declaration.plots.len());
                }
            }
            run(template, &script, &input, &all_on);
        }
    }

    #[test]
    fn the_tour_of_plots_declares_its_levels_band_and_fills() {
        let template = template("Plots and levels");
        let script = Script::compile(template.source).unwrap();
        let output = run(template, &script, &market(), &BTreeMap::new());
        assert_eq!(output.levels, [0.0, 2.0, -2.0]);
        assert_eq!(output.band, Some((-1.0, 1.0)));
        assert_eq!(script.declaration.meta.range, Some((-4.0, 4.0)));
        assert_eq!(output.fills.len(), 1);
        let gap = output.plots.iter().find(|p| p.key == "gap").unwrap();
        assert!(gap.up.is_some(), "the histogram has its up condition");
        let later = output.plots.iter().find(|p| p.key == "later").unwrap();
        assert_eq!(later.offset, 5);
    }

    #[test]
    fn the_oscillator_toolbox_changes_its_levels_with_the_choice() {
        let template = template("Oscillator toolbox");
        let script = Script::compile(template.source).unwrap();
        let input = market();
        let choice = |index: f64| {
            let values = BTreeMap::from([("oscillator".to_owned(), index)]);
            run(template, &script, &input, &values)
        };
        assert_eq!(choice(0.0).levels, [30.0, 70.0], "RSI");
        assert_eq!(choice(3.0).levels, [-100.0, 0.0, 100.0], "CCI");
        let macd = choice(2.0);
        let columns = macd.plots.iter().find(|p| p.key == "columns").unwrap();
        assert!(
            columns.values.iter().any(|v| v.is_finite()),
            "MACD histogram"
        );
    }

    #[test]
    fn the_tour_of_drawings_draws_a_dozen_objects_that_follow_the_window() {
        let template = template("Drawings");
        let script = Script::compile(template.source).unwrap();
        let input = market();
        let output = run(template, &script, &input, &BTreeMap::new());
        let keys: Vec<&str> = output.drawings.iter().map(|d| d.key.as_str()).collect();
        for key in [
            "window", "top", "bottom", "swing", "fib", "channel", "title", "now", "plan",
        ] {
            assert!(keys.contains(&key), "{key} is missing from {keys:?}");
        }
        let off = BTreeMap::from([("show_fib".to_owned(), 0.0), ("show_plan".to_owned(), 0.0)]);
        let output = run(template, &script, &input, &off);
        let keys: Vec<&str> = output.drawings.iter().map(|d| d.key.as_str()).collect();
        assert!(
            !keys.contains(&"fib") && !keys.contains(&"plan"),
            "{keys:?}"
        );
    }

    #[test]
    fn the_trailing_stop_flips_and_the_session_range_follows_its_hours() {
        let input = market();

        let template_stop = template("Loops and state");
        let script = Script::compile(template_stop.source).unwrap();
        let output = run(template_stop, &script, &input, &BTreeMap::new());
        let finite = |key: &str| {
            output
                .plots
                .iter()
                .find(|p| p.key == key)
                .unwrap()
                .values
                .iter()
                .filter(|v| v.is_finite())
                .count()
        };
        assert!(finite("stop_long") > 0 && finite("stop_short") > 0);

        let template_session = template("Session range");
        let script = Script::compile(template_session.source).unwrap();
        let output = run(template_session, &script, &input, &BTreeMap::new());
        let line = |key: &str| &output.plots.iter().find(|p| p.key == key).unwrap().values;
        let (top, bottom) = (line("session_high"), line("session_low"));
        // 08:00 UTC is the 33rd bar of the first day: from there the range exists and holds the bar.
        assert!(
            top.get(31).is_some_and(|v| v.is_nan()),
            "before the session"
        );
        let at = top.get(40).copied().unwrap();
        assert!(at >= input.high[40] && bottom[40] <= input.low[40]);
        assert!(top.iter().zip(bottom).all(|(h, l)| h.is_nan() || h >= l));
    }
}
