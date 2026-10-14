//! One indicator on a chart as the user set it, which is also what is saved.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::custom;
use super::kind::StudyKind;
use super::spec::{ATR_SMOOTHING, GRAY, InputKind, PlotKind, SOURCES, Spec, ValueFormat};
use super::style::{FillStyle, LevelStyle, PlotStyle};
use super::style::{default_bar_width, default_fill_opacity, opaque};
use wyck_drawings::model::style::Dash;

/// One indicator on a chart, as the user set it. Also its saved form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudyConfig {
    pub kind: StudyKind,
    /// The id of the script, for [`StudyKind::Custom`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    #[serde(default)]
    pub inputs: BTreeMap<String, f64>,
    /// The inputs that are text: symbols, timeframes, sessions, free text.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub texts: BTreeMap<String, String>,
    #[serde(default)]
    pub plots: BTreeMap<String, PlotStyle>,
    #[serde(default = "wyck_core::defaults::yes")]
    pub visible: bool,
    /// How tall its pane is, relative to the prices (only for an indicator with a pane).
    #[serde(default = "pane_weight")]
    pub weight: f32,
    #[serde(default)]
    pub levels: BTreeMap<usize, LevelStyle>,
    #[serde(default)]
    pub fills: BTreeMap<usize, FillStyle>,
    #[serde(default)]
    pub band: Option<FillStyle>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub precision: Option<u32>,
    #[serde(default = "wyck_core::defaults::yes")]
    pub axis_labels: bool,
    #[serde(default = "wyck_core::defaults::yes")]
    pub legend_values: bool,
}

fn pane_weight() -> f32 {
    1.0
}

impl StudyConfig {
    pub fn default_levels(&self) -> Vec<f64> {
        match self.kind {
            StudyKind::Rsi => vec![self.input("upper"), 50.0, self.input("lower")],
            StudyKind::Stochastic | StudyKind::WilliamsR | StudyKind::StochasticRsi => {
                vec![self.input("upper"), self.input("lower")]
            }
            StudyKind::Cci => vec![self.input("upper"), 0.0, self.input("lower")],
            StudyKind::Dmi => vec![25.0],
            StudyKind::Aroon | StudyKind::UltimateOscillator => vec![70.0, 30.0],
            StudyKind::DeMarker => vec![0.7, 0.3],
            StudyKind::MoneyFlowIndex => vec![80.0, 20.0],
            StudyKind::Macd
            | StudyKind::Momentum
            | StudyKind::Roc
            | StudyKind::AwesomeOscillator
            | StudyKind::AcceleratorOscillator
            | StudyKind::Trix
            | StudyKind::RelativeVigorIndex
            | StudyKind::ChaikinMoneyFlow
            | StudyKind::ForceIndex
            | StudyKind::VolumeOscillator
            | StudyKind::EaseOfMovement => vec![0.0],
            _ => Vec::new(),
        }
    }

    pub fn default_band(&self) -> Option<FillStyle> {
        matches!(
            self.kind,
            StudyKind::Rsi
                | StudyKind::Stochastic
                | StudyKind::WilliamsR
                | StudyKind::Cci
                | StudyKind::StochasticRsi
        )
        .then(|| FillStyle {
            color: self
                .spec()
                .plots
                .first()
                .map_or(GRAY, |p| self.plot_style(p.key).color),
            other: None,
            opacity: 0.06,
            visible: true,
        })
    }

    pub fn default_fill(&self, index: usize) -> Option<FillStyle> {
        let (color, other, opacity) = match self.kind {
            StudyKind::Bollinger
            | StudyKind::Keltner
            | StudyKind::Donchian
            | StudyKind::Envelopes
            | StudyKind::LinearRegression
                if index == 0 =>
            {
                (self.plot_style("upper").color, None, 0.08)
            }
            StudyKind::Ichimoku if index == 0 => (0x43a047, Some(0xf44336), 0.12),
            StudyKind::Vwap if index < 2 => (
                self.plot_style(if index == 0 { "upper_1" } else { "upper_2" })
                    .color,
                None,
                if index == 0 { 0.06 } else { 0.04 },
            ),
            _ => return None,
        };
        Some(FillStyle {
            color,
            other,
            opacity,
            visible: true,
        })
    }

    /// An indicator with every input and plot at its default.
    pub fn new(kind: StudyKind) -> Self {
        Self {
            kind,
            script: None,
            inputs: BTreeMap::new(),
            texts: BTreeMap::new(),
            plots: BTreeMap::new(),
            visible: true,
            weight: pane_weight(),
            levels: BTreeMap::new(),
            fills: BTreeMap::new(),
            band: None,
            name: String::new(),
            precision: None,
            axis_labels: true,
            legend_values: true,
        }
        .normalized()
    }

    /// An indicator written as a script, with every input and plot at its default.
    pub fn for_script(id: &str) -> Self {
        Self {
            kind: StudyKind::Custom,
            script: Some(id.to_owned()),
            inputs: BTreeMap::new(),
            texts: BTreeMap::new(),
            plots: BTreeMap::new(),
            visible: true,
            weight: pane_weight(),
            levels: BTreeMap::new(),
            fills: BTreeMap::new(),
            band: None,
            name: String::new(),
            precision: None,
            axis_labels: true,
            legend_values: true,
        }
        .normalized()
    }

    /// The same indicator with every input and plot at its default.
    #[must_use]
    pub fn fresh(&self) -> Self {
        match (&self.kind, self.script.as_deref()) {
            (StudyKind::Custom, Some(id)) => Self::for_script(id),
            (kind, _) => Self::new(*kind),
        }
    }

    /// Whether this is an indicator written as a script.
    pub fn is_script(&self) -> bool {
        self.kind == StudyKind::Custom
    }

    /// Everything fixed about the indicator: what the app ships, or what the script declared. A
    /// script that is gone gives an empty spec.
    pub fn spec(&self) -> Spec {
        match self.kind {
            StudyKind::Custom => custom::spec_for(self.script.as_deref()),
            kind => kind.spec(),
        }
    }

    /// Whether the script this indicator holds is known and works. True for the indicators the
    /// app ships.
    pub fn is_ready(&self) -> bool {
        self.kind != StudyKind::Custom
            || self
                .script
                .as_deref()
                .and_then(custom::library::registry::get)
                .is_some_and(|entry| entry.is_ready())
    }

    /// The config repaired: every input of the spec present and inside its range (whole where it
    /// must be), unknown keys dropped, every plot styled.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        // A script that is not there (yet, or any more), or that does not work at the moment (a
        // mistake being typed), keeps what was saved for it: the file may come back, and the
        // settings with it.
        if self.kind == StudyKind::Custom
            && !self
                .script
                .as_deref()
                .and_then(custom::library::registry::get)
                .is_some_and(|entry| entry.is_ready())
        {
            if !(self.weight.is_finite() && self.weight > 0.0) {
                self.weight = pane_weight();
            }
            self.weight = self.weight.clamp(0.1, 20.0);
            return self;
        }
        let spec = self.spec();
        let mut inputs = BTreeMap::new();
        for input in spec.inputs {
            let raw = self
                .inputs
                .get(input.key)
                .copied()
                .filter(|v| v.is_finite())
                .unwrap_or(input.default);
            let mut value = raw.clamp(input.min, input.max);
            if !matches!(input.kind, InputKind::Float) {
                value = value.round();
            }
            inputs.insert(input.key.to_owned(), value);
        }
        self.inputs = inputs;
        let mut plots = BTreeMap::new();
        for plot in spec.plots {
            let style = self
                .plots
                .get(plot.key)
                .copied()
                .filter(|s| s.width.is_finite() && s.width > 0.0)
                .unwrap_or(PlotStyle {
                    color: plot.color,
                    width: plot.width,
                    visible: plot.visible,
                    opacity: if plot.kind == PlotKind::Histogram {
                        0.6
                    } else {
                        opaque()
                    },
                    dash: Dash::Solid,
                    kind: None,
                    up_color: None,
                    down_color: None,
                    bar_width: default_bar_width(),
                });
            let style = PlotStyle {
                opacity: if style.opacity.is_finite() {
                    style.opacity.clamp(0.05, 1.0)
                } else {
                    opaque()
                },
                bar_width: if style.bar_width.is_finite() {
                    style.bar_width.clamp(0.1, 1.0)
                } else {
                    default_bar_width()
                },
                width: style.width.clamp(0.5, 20.0),
                ..style
            };
            plots.insert(plot.key.to_owned(), style);
        }
        self.plots = plots;
        if !(self.weight.is_finite() && self.weight > 0.0) {
            self.weight = pane_weight();
        }
        self.weight = self.weight.clamp(0.1, 20.0);
        self.levels
            .retain(|index, style| *index < 32 && style.value.is_none_or(f64::is_finite));
        for style in self.levels.values_mut() {
            style.opacity = if style.opacity.is_finite() {
                style.opacity.clamp(0.0, 1.0)
            } else {
                0.55
            };
            style.width = if style.width.is_finite() {
                style.width.clamp(0.5, 20.0)
            } else {
                1.0
            };
        }
        self.fills.retain(|index, _| *index < 16);
        for style in self.fills.values_mut() {
            style.opacity = if style.opacity.is_finite() {
                style.opacity.clamp(0.0, 1.0)
            } else {
                default_fill_opacity()
            };
        }
        if let Some(style) = &mut self.band {
            style.opacity = if style.opacity.is_finite() {
                style.opacity.clamp(0.0, 1.0)
            } else {
                default_fill_opacity()
            };
        }
        self.precision = self.precision.map(|p| p.min(8));
        self.name = self.name.trim().chars().take(80).collect();
        self
    }

    /// How tall its pane is, relative to the prices.
    pub fn pane_weight(&self) -> f32 {
        self.weight
    }

    pub fn value_format(&self) -> ValueFormat {
        if let Some(precision) = self.precision {
            return match self.spec().format {
                ValueFormat::Percent(_) => ValueFormat::Percent(precision),
                ValueFormat::Count => ValueFormat::Plain(precision),
                _ => ValueFormat::Plain(precision),
            };
        }
        if self.kind == StudyKind::Atr && self.input("unit") == 1.0 {
            ValueFormat::Percent(self.input("percent_decimals").clamp(0.0, 6.0) as u32)
        } else {
            self.spec().format
        }
    }

    /// A text input's value (its default if it was never set).
    pub fn text(&self, key: &str) -> String {
        self.texts.get(key).cloned().unwrap_or_else(|| {
            self.spec()
                .inputs
                .iter()
                .find(|i| i.key == key)
                .map_or_else(String::new, |i| i.text.to_owned())
        })
    }

    /// Sets a text input.
    pub fn set_text(&mut self, key: &str, value: impl Into<String>) {
        self.texts.insert(key.to_owned(), value.into());
    }

    /// An input's value (its default if it is somehow missing).
    pub fn input(&self, key: &str) -> f64 {
        self.inputs.get(key).copied().unwrap_or_else(|| {
            self.kind
                .spec()
                .inputs
                .iter()
                .find(|i| i.key == key)
                .map_or(0.0, |i| i.default)
        })
    }

    pub(super) fn length(&self, key: &str) -> usize {
        self.input(key).max(1.0) as usize
    }

    pub fn plot_style(&self, key: &str) -> PlotStyle {
        self.plots.get(key).copied().unwrap_or_else(|| {
            let spec = self.spec();
            let plot = spec.plots.iter().find(|p| p.key == key);
            PlotStyle {
                color: plot.map_or(GRAY, |p| p.color),
                width: plot.map_or(1.0, |p| p.width),
                visible: plot.is_none_or(|p| p.visible),
                opacity: if plot.is_some_and(|p| p.kind == PlotKind::Histogram) {
                    0.6
                } else {
                    opaque()
                },
                dash: Dash::Solid,
                kind: None,
                up_color: None,
                down_color: None,
                bar_width: default_bar_width(),
            }
        })
    }

    /// The legend's title: the short name and the main inputs, `SMA 20 close`.
    pub fn title(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        if self.kind == StudyKind::Atr {
            let mut title = format!("ATR {}", self.length("length"));
            if self.input("smoothing") != 0.0 {
                title.push_str(&format!(
                    " {}",
                    ATR_SMOOTHING[(self.input("smoothing") as usize).min(ATR_SMOOTHING.len() - 1)]
                ));
            }
            if self.input("unit") == 1.0 {
                title.push_str(" %");
            }
            return title;
        }
        let spec = self.spec();
        let mut parts = vec![spec.short.to_owned()];
        for input in spec.inputs {
            if matches!(
                input.key,
                "area_opacity" | "other_opacity" | "normal_opacity"
            ) {
                continue;
            }
            let value = self.input(input.key);
            match input.kind {
                InputKind::Int => parts.push(format!("{value:.0}")),
                InputKind::Float => parts.push(wyck_core::format::trim(value, 3)),
                InputKind::Source => {
                    parts.push(SOURCES[(value as usize).min(SOURCES.len() - 1)].to_lowercase());
                }
                InputKind::Symbol | InputKind::Timeframe => {
                    let text = self.text(input.key);
                    if !text.is_empty() {
                        parts.push(text);
                    }
                }
                InputKind::Choice(_)
                | InputKind::Toggle
                | InputKind::Color
                | InputKind::Session
                | InputKind::Text => {}
            }
            if input.key == "offset" && value == 0.0 {
                parts.pop();
            }
        }
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::super::compute::*;
    use super::super::config::*;
    use super::super::kind::*;
    use super::super::spec::*;
    use super::super::style::*;

    use serde::{Deserialize, Serialize};
    use wyck_drawings::model::style::Dash;

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
            toml::from_str("[[studies]]\nkind = \"super_trend\"\n[studies.inputs]\nx = 1.0\n")
                .unwrap();
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
}
