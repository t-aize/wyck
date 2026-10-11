use super::*;
use crate::domain::drawings::model::Dash;
use serde::{Deserialize, Serialize};

fn input(n: usize) -> StudyInput {
    let close: Vec<f64> = (0..n)
        .map(|i| 100_000.0 + (i as f64 * 0.7).sin() * 500.0 + i as f64 * 10.0)
        .collect();
    StudyInput {
        time: (0..n as i64).map(|i| i * 60_000).collect(),
        open: close.iter().map(|c| c - 20.0).collect(),
        high: close.iter().map(|c| c + 80.0).collect(),
        low: close.iter().map(|c| c - 90.0).collect(),
        volume: (0..n).map(|i| 1.0 + (i % 5) as f64).collect(),
        day: (0..n as i64).map(|i| i / 100).collect(),
        close,
    }
}

#[test]
fn every_indicator_has_a_unique_label_and_computes_aligned_output() {
    let data = input(300);
    let mut labels = Vec::new();
    for kind in StudyKind::ALL {
        let spec = kind.spec();
        labels.push(spec.label);
        let config = StudyConfig::new(kind);
        let out = compute(&config, &data);
        for plot in &out.plots {
            assert_eq!(plot.values.len(), data.len(), "{kind:?} {}", plot.key);
            assert!(
                spec.plots.iter().any(|p| p.key == plot.key),
                "{kind:?}: plot {} has no spec",
                plot.key
            );
            // The average on a timeframe has nothing to show until that timeframe has history.
            if !matches!(
                kind,
                StudyKind::Supertrend | StudyKind::Fractals | StudyKind::MtfMa
            ) {
                assert!(
                    plot.values.iter().rev().take(5).all(|v| v.is_finite()),
                    "{kind:?} {}: the newest values are defined",
                    plot.key
                );
            }
        }
        if kind != StudyKind::VolumeProfile {
            assert!(!out.plots.is_empty(), "{kind:?}");
        }
        for fill in &out.fills {
            assert!(fill.a < out.plots.len() && fill.b < out.plots.len());
        }
    }
    let total = labels.len();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(labels.len(), total);
}

#[test]
fn a_config_is_repaired_to_its_spec() {
    let mut config = StudyConfig::new(StudyKind::Rsi);
    config.inputs.insert("length".into(), 1e9);
    config.inputs.insert("bogus".into(), 3.0);
    config.inputs.insert("upper".into(), f64::NAN);
    config.inputs.insert("lower".into(), 29.6);
    let config = config.normalized();
    assert_eq!(config.input("length"), 1000.0);
    assert_eq!(config.input("upper"), 70.0);
    assert_eq!(config.input("lower"), 29.6, "a float keeps its decimals");
    assert!(!config.inputs.contains_key("bogus"));
    assert_eq!(config.plots.len(), 1);
}

#[test]
fn a_saved_indicator_reads_back_and_a_future_one_is_recognized() {
    let mut config = StudyConfig::new(StudyKind::Macd);
    config.inputs.insert("fast".into(), 8.0);
    config.plots.get_mut("macd").unwrap().color = 0x123456;
    #[derive(Serialize, Deserialize)]
    struct Holder {
        studies: Vec<StudyConfig>,
    }
    let text = toml::to_string(&Holder {
        studies: vec![config.clone()],
    })
    .unwrap();
    let back: Holder = toml::from_str(&text).unwrap();
    assert_eq!(back.studies[0], config);
    let future: Holder =
        toml::from_str("[[studies]]\nkind = \"super_trend\"\n[studies.inputs]\nx = 1.0\n").unwrap();
    assert_eq!(future.studies[0].kind, StudyKind::Unknown);
}

#[test]
fn old_and_new_style_fields_survive_storage() {
    let old: StudyConfig = toml::from_str("kind = 'macd'\n[plots.macd]\ncolor = 1193046\nwidth = 2.0\nvisible = true\nopacity = 1.0\ndash = 'solid'\n").unwrap();
    let old = old.normalized();
    assert_eq!(old.plot_style("macd").color, 0x123456);
    assert!(old.axis_labels);
    assert!(old.legend_values);
    assert!(old.fills.is_empty());

    let mut current = StudyConfig::new(StudyKind::Vwap);
    current.name = "Session VWAP".into();
    current.precision = Some(4);
    current.axis_labels = false;
    current.legend_values = false;
    current.plots.get_mut("vwap").unwrap().kind = Some(PlotKind::Dots);
    current.fills.insert(
        1,
        FillStyle {
            color: 0x123456,
            other: None,
            opacity: 0.2,
            visible: false,
        },
    );
    let saved = toml::to_string(&current).unwrap();
    let loaded: StudyConfig = toml::from_str(&saved).unwrap();
    assert_eq!(loaded.normalized(), current);
}

#[test]
fn extended_formulas_use_documented_prices_and_warmup() {
    let mut data = input(60);
    data.open = (0..60).map(|i| 10.0 + i as f64).collect();
    data.high = (0..60).map(|i| 14.0 + i as f64).collect();
    data.low = (0..60).map(|i| 8.0 + i as f64).collect();
    data.close = (0..60).map(|i| 12.0 + i as f64).collect();
    data.volume = vec![2.0; 60];

    let mut rma = StudyConfig::new(StudyKind::Rma);
    rma.inputs.insert("length".into(), 3.0);
    let rma_out = compute(&rma, &data);
    assert!(rma_out.plots[0].values[1].is_nan());
    assert_eq!(rma_out.plots[0].values[2], 13.0);
    assert!((rma_out.plots[0].values[3] - (13.0 + 2.0 / 3.0)).abs() < 1e-10);

    let mut ao = StudyConfig::new(StudyKind::AwesomeOscillator);
    ao.inputs.insert("fast".into(), 2.0);
    ao.inputs.insert("slow".into(), 3.0);
    let ao_out = compute(&ao, &data);
    assert!(ao_out.plots[0].values[1].is_nan());
    assert!((ao_out.plots[0].values[2] - 0.5).abs() < 1e-10);

    let mut dpo = StudyConfig::new(StudyKind::Dpo);
    dpo.inputs.insert("length".into(), 4.0);
    let dpo_out = compute(&dpo, &data);
    assert!(dpo_out.plots[0].values[2].is_nan());
    assert!((dpo_out.plots[0].values[6] - (data.close[6] - 13.5)).abs() < 1e-10);

    let mut aroon = StudyConfig::new(StudyKind::Aroon);
    aroon.inputs.insert("length".into(), 3.0);
    let aroon_out = compute(&aroon, &data);
    assert!(aroon_out.plots[0].values[2].is_nan());
    assert_eq!(aroon_out.plots[0].values[3], 100.0);
    assert_eq!(aroon_out.plots[1].values[3], 0.0);

    let mut regression = StudyConfig::new(StudyKind::LinearRegression);
    regression.inputs.insert("length".into(), 5.0);
    let regression_out = compute(&regression, &data);
    assert!((regression_out.plots[0].values[10] - data.close[10]).abs() < 1e-10);
    assert!(
        (regression_out.plots[1].values[10] - regression_out.plots[2].values[10]).abs() < 1e-10
    );
}

#[test]
fn all_indicators_tolerate_a_gap_and_a_large_bar() {
    let mut data = input(240);
    for column in [
        &mut data.open,
        &mut data.high,
        &mut data.low,
        &mut data.close,
        &mut data.volume,
    ] {
        column[100] = f64::NAN;
    }
    data.open[120] = 1e12;
    data.high[120] = 1e12 + 100.0;
    data.low[120] = 1e12 - 100.0;
    data.close[120] = 1e12 + 10.0;
    for kind in StudyKind::ALL {
        let output = compute(&StudyConfig::new(kind), &data);
        for plot in output.plots {
            assert_eq!(plot.values.len(), data.len(), "{kind:?}");
            assert!(
                !plot.values.iter().any(|v| v.is_infinite()),
                "{kind:?} {}",
                plot.key
            );
        }
    }
}

#[test]
fn titles_name_the_main_inputs() {
    assert_eq!(StudyConfig::new(StudyKind::Sma).title(), "SMA 20 close");
    assert_eq!(
        StudyConfig::new(StudyKind::Bollinger).title(),
        "BB 20 2 close"
    );
    assert_eq!(
        StudyConfig::new(StudyKind::Macd).title(),
        "MACD 12 26 9 close"
    );
    assert_eq!(StudyConfig::new(StudyKind::Obv).title(), "OBV");
}

#[test]
fn vwap_restarts_every_day() {
    let data = input(250);
    let out = compute(&StudyConfig::new(StudyKind::Vwap), &data);
    let values = &out.plots[0].values;
    let typical = data.source(5.0);
    // The first bar of a day is its own VWAP.
    assert!((values[100] - typical[100]).abs() < 1e-9);
    assert!((values[200] - typical[200]).abs() < 1e-9);
}

#[test]
fn ichimoku_leads_and_lags() {
    let out = compute(&StudyConfig::new(StudyKind::Ichimoku), &input(200));
    let offsets: Vec<i64> = out.plots.iter().map(|p| p.offset).collect();
    assert_eq!(offsets, vec![0, 0, -25, 25, 25]);
}

#[test]
fn atr_methods_optional_plots_and_percent_use_the_same_bars() {
    let data = StudyInput {
        high: vec![12.0, 13.0, 17.0, 16.0],
        low: vec![8.0, 9.0, 11.0, 10.0],
        close: vec![10.0, 12.0, 12.0, 15.0],
        ..StudyInput::default()
    };
    let mut config = StudyConfig::new(StudyKind::Atr);
    config.inputs.insert("length".into(), 2.0);
    config.inputs.insert("signal_length".into(), 2.0);
    assert!(!config.plot_style("signal").visible);
    assert!(!config.plot_style("tr").visible);
    let out = compute(&config, &data);
    assert_eq!(
        out.plots.iter().map(|p| p.key).collect::<Vec<_>>(),
        ["atr", "signal", "tr"]
    );
    assert_eq!(out.plots[2].values, [4.0, 4.0, 6.0, 6.0]);
    assert!(out.plots[0].values[0].is_nan());
    assert_eq!(&out.plots[0].values[1..], &[4.0, 5.0, 5.5]);
    assert!(out.plots[1].values[1].is_nan());
    assert_eq!(&out.plots[1].values[2..], &[4.5, 5.25]);

    for (method, expected) in [(1.0, 6.0), (2.0, 5.777777777777778), (3.0, 6.0)] {
        config.inputs.insert("smoothing".into(), method);
        let out = compute(&config, &data);
        assert!((out.plots[0].values[3] - expected).abs() < 1e-10);
    }

    config.inputs.insert("smoothing".into(), 0.0);
    config.inputs.insert("unit".into(), 1.0);
    let out = compute(&config, &data);
    assert_eq!(config.value_format(), ValueFormat::Percent(2));
    config.inputs.insert("percent_decimals".into(), 4.0);
    assert_eq!(config.value_format(), ValueFormat::Percent(4));
    assert!((out.plots[0].values[3] - 5.5 / 15.0 * 100.0).abs() < 1e-10);
    assert!((out.plots[2].values[3] - 6.0 / 15.0 * 100.0).abs() < 1e-10);
}

#[test]
fn old_atr_settings_keep_wilder_and_one_visible_plot() {
    let config: StudyConfig = toml::from_str("kind = 'atr'\n[inputs]\nlength = 21\n").unwrap();
    let config = config.normalized();
    assert_eq!(config.input("smoothing"), 0.0);
    assert_eq!(config.input("unit"), 0.0);
    assert_eq!(config.value_format(), ValueFormat::Price);
    assert!(config.plot_style("atr").visible);
    assert!(!config.plot_style("signal").visible);
    assert!(!config.plot_style("tr").visible);
}

#[test]
fn a_plot_saved_before_opacity_and_line_style_loads_opaque_and_solid() {
    let old: PlotStyle = toml::from_str(
        "color = 255
width = 2.0
",
    )
    .unwrap();
    assert_eq!(old.opacity, 1.0);
    assert_eq!(old.dash, Dash::Solid);
    assert!(old.visible);

    let mut config = StudyConfig::new(StudyKind::Sma);
    let key = config.spec().plots[0].key;
    config.plots.insert(
        key.to_owned(),
        PlotStyle {
            opacity: 9.0,
            ..old
        },
    );
    assert_eq!(config.normalized().plot_style(key).opacity, 1.0);
}

#[test]
fn the_average_on_a_timeframe_reads_the_closed_bars_of_that_timeframe() {
    let bars = input(60);
    let mut config = StudyConfig::new(StudyKind::MtfMa);
    config.set_text("timeframe", "5m");
    config.inputs.insert("length".to_owned(), 2.0);
    config.inputs.insert("method".to_owned(), 0.0);
    let out = compute(&config, &bars);
    let values = &out.plots[0].values;
    // The two five minute bars before minute 15 are minutes 5..10 and 10..15.
    let closes = [bars.close[9], bars.close[14]];
    let expected = (closes[0] + closes[1]) / 2.0;
    assert!(
        (values[15] - expected).abs() < 1e-9,
        "{} vs {expected}",
        values[15]
    );
    // Until two bars have closed there is nothing to show.
    assert!(values[..10].iter().all(|v| v.is_nan()));
    // The value holds for the whole of the bar that follows.
    assert_eq!(values[15], values[19]);
}

#[test]
fn without_waiting_the_average_follows_the_bar_still_forming() {
    let bars = input(30);
    let mut config = StudyConfig::new(StudyKind::MtfMa);
    config.set_text("timeframe", "5m");
    config.inputs.insert("length".to_owned(), 1.0);
    config.inputs.insert("confirmed".to_owned(), 0.0);
    let out = compute(&config, &bars);
    // A one bar average of the forming bar is its close so far.
    assert!((out.plots[0].values[7] - bars.close[7]).abs() < 1e-9);
}

#[test]
fn the_chart_s_own_timeframe_gives_the_plain_average() {
    let bars = input(40);
    let mut config = StudyConfig::new(StudyKind::MtfMa);
    config.set_text("timeframe", "");
    config.inputs.insert("length".to_owned(), 5.0);
    let plain = math::sma(&bars.source(3.0), 5);
    let out = compute(&config, &bars);
    for (a, b) in out.plots[0].values.iter().zip(&plain) {
        assert!((a.is_nan() && b.is_nan()) || (a - b).abs() < 1e-9);
    }
}

#[test]
fn text_inputs_survive_saving_and_default_when_absent() {
    let mut config = StudyConfig::new(StudyKind::MtfMa);
    assert_eq!(config.text("timeframe"), "4h");
    config.set_text("timeframe", "1D");
    let saved = serde_json::to_string(&config).unwrap();
    let back: StudyConfig = serde_json::from_str(&saved).unwrap();
    assert_eq!(back.text("timeframe"), "1D");
    // An older save has no texts at all.
    let older: StudyConfig =
        serde_json::from_str(r#"{"kind":"sma","inputs":{"length":9.0}}"#).unwrap();
    assert!(older.texts.is_empty());
}
