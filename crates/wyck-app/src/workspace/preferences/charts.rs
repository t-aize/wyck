//! The layout of the charts and what each one keeps: its timeframe, its type and indicators, its
//! own symbol, and the links that tie the charts together.

use super::Preferences;
use crate::workspace::layouts::{self, LayoutKey};
use serde::{Deserialize, Serialize};
use wyck_chart::{ChartContent, ChartKind, ChartLook, ChartSettings};
use wyck_market::Timeframe;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LayoutPref {
    pub(crate) count: usize,
    pub(crate) variant: usize,
}

impl Default for LayoutPref {
    fn default() -> Self {
        Self {
            count: 1,
            variant: 0,
        }
    }
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "saved document: six independent links of the charts, one switch each, named in the file"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinksPref {
    /// Every chart shows the same symbol.
    #[serde(default = "wyck_core::defaults::yes")]
    pub symbol: bool,
    #[serde(default)]
    pub interval: bool,
    #[serde(default = "wyck_core::defaults::yes")]
    pub crosshair: bool,
    #[serde(default = "wyck_core::defaults::yes")]
    pub time: bool,
    #[serde(default)]
    pub range: bool,
    #[serde(default)]
    pub studies: bool,
}

impl Default for LinksPref {
    fn default() -> Self {
        Self {
            symbol: true,
            interval: false,
            crosshair: true,
            time: true,
            range: false,
            studies: false,
        }
    }
}

/// One chart of the layout, by the stable codes written in the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ChartPref {
    #[serde(default = "default_timeframe_code")]
    pub(crate) timeframe: String,
    /// The chart type, as older files wrote it. [`ChartPref::settings`] wins when present.
    #[serde(default = "default_kind_code")]
    pub(crate) kind: String,
    /// The symbol the chart shows when the symbol is not linked, by the broker's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) symbol: Option<String>,
    /// What the chart shows: type, scale, indicators. How it looks is in `Preferences::chart`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) settings: Option<ChartContent>,
}

fn default_timeframe_code() -> String {
    Timeframe::DEFAULT.code()
}

fn default_kind_code() -> String {
    ChartKind::Candles.code().to_owned()
}

impl Default for ChartPref {
    fn default() -> Self {
        Self {
            timeframe: default_timeframe_code(),
            kind: default_kind_code(),
            symbol: None,
            settings: None,
        }
    }
}

impl ChartPref {
    /// The settings of the chart: what it saved, or the defaults with the saved type, with the look
    /// every chart shares.
    pub(crate) fn chart_settings(&self, look: &ChartLook) -> ChartSettings {
        let content = match &self.settings {
            Some(content) => content.clone(),
            None => ChartContent {
                kind: ChartKind::from_code(&self.kind).unwrap_or(ChartKind::Candles),
                ..ChartContent::default()
            },
        };
        ChartSettings::from_parts(content, look).normalized()
    }
}

/// What one chart of a layout keeps: its timeframe, its settings and its own symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartState {
    pub timeframe: Timeframe,
    pub settings: ChartSettings,
    pub symbol: Option<String>,
}

pub(super) fn default_charts() -> Vec<ChartPref> {
    vec![ChartPref::default()]
}

/// Timeframes given to the charts a layout adds, when nothing is saved for them. They differ, so
/// a new chart shows something else than the first one.
pub const NEW_CHART_TIMEFRAMES: [&str; 8] = ["M5", "M15", "H1", "H4", "D1", "M1", "M30", "W1"];

impl Preferences {
    /// Repairs the layout, the entry of each chart and the active chart: a layout that does not
    /// exist becomes the single chart, and there is one valid entry per chart of the layout.
    pub(super) fn normalize_charts(&mut self) {
        // A layout that does not exist becomes the single chart.
        let key = LayoutKey {
            count: self.layout.count,
            variant: self.layout.variant,
        };
        let count = layouts::layout(key).count();
        if count != self.layout.count || !layouts::exists(key) {
            self.layout = LayoutPref::default();
        }
        let count = layouts::layout(self.layout_key()).count();

        // One valid entry per chart of the layout.
        self.charts.truncate(count);
        for chart in &mut self.charts {
            if Timeframe::from_code(&chart.timeframe).is_none() {
                chart.timeframe = default_timeframe_code();
            }
            if ChartKind::from_code(&chart.kind).is_none() {
                chart.kind = default_kind_code();
            }
        }
        while self.charts.len() < count {
            let timeframe = NEW_CHART_TIMEFRAMES
                .iter()
                .cycle()
                .nth(self.charts.len())
                .copied()
                .unwrap_or_default();
            self.charts.push(ChartPref {
                timeframe: timeframe.to_owned(),
                ..ChartPref::default()
            });
        }
        for chart in &mut self.charts {
            if let Some(content) = chart.settings.take() {
                content.kind.code().clone_into(&mut chart.kind);
                // What a chart shows is repaired as a whole, with the look it will have.
                let settings = ChartSettings::from_parts(content, &self.chart).normalized();
                chart.settings = Some(settings.content());
            }
        }
        self.active_chart = self.active_chart.min(count - 1);
    }

    pub fn layout_key(&self) -> LayoutKey {
        LayoutKey {
            count: self.layout.count,
            variant: self.layout.variant,
        }
    }

    /// What chart `index` keeps, as the file says (repaired on load).
    pub fn chart(&self, index: usize) -> ChartState {
        let pref = self.charts.get(index).cloned().unwrap_or_default();
        ChartState {
            timeframe: Timeframe::from_code(&pref.timeframe).unwrap_or(Timeframe::DEFAULT),
            settings: pref.chart_settings(&self.chart),
            symbol: pref.symbol.clone(),
        }
    }

    /// Records the layout and what each chart keeps, and which chart is active.
    pub fn set_arrangement(&mut self, key: LayoutKey, charts: &[ChartState], active: usize) {
        self.layout = LayoutPref {
            count: key.count,
            variant: key.variant,
        };
        self.charts = charts
            .iter()
            .map(|chart| ChartPref {
                timeframe: chart.timeframe.code(),
                kind: chart.settings.kind.code().to_owned(),
                symbol: chart.symbol.clone(),
                settings: Some(chart.settings.content()),
            })
            .collect();
        self.active_chart = active;
        *self = std::mem::take(self).normalized();
    }

    /// The key a layout's split weights are saved under.
    pub fn split_key(key: LayoutKey) -> String {
        format!("{}-{}", key.count, key.variant)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyck_chart::Zone;
    use wyck_market::Period;

    #[test]
    fn a_saved_arrangement_is_read_back_exactly() {
        let mut prefs = Preferences {
            chart: ChartLook {
                zone: Zone::Utc,
                ..ChartLook::default()
            },
            magnet: true,
            ..Preferences::default()
        };
        prefs.links.interval = true;
        let look = prefs.chart.clone();
        let state = |timeframe, kind, symbol: Option<&str>| ChartState {
            timeframe,
            settings: ChartSettings::from_parts(
                ChartContent {
                    kind,
                    ..ChartContent::default()
                },
                &look,
            ),
            symbol: symbol.map(str::to_owned),
        };
        prefs.set_arrangement(
            LayoutKey {
                count: 4,
                variant: 0,
            },
            &[
                state(Timeframe::Bars(Period::M5), ChartKind::Candles, None),
                state(
                    Timeframe::Seconds(15),
                    ChartKind::HeikinAshi,
                    Some("XAUUSD"),
                ),
                state(Timeframe::Ticks, ChartKind::Line, None),
                state(Timeframe::Bars(Period::D1), ChartKind::Renko, None),
            ],
            2,
        );
        let text = toml::to_string_pretty(&prefs).unwrap();
        let back: Preferences = toml::from_str(&text).unwrap();
        assert_eq!(back.clone().normalized(), prefs);
        let second = back.chart(1);
        assert_eq!(second.timeframe, Timeframe::Seconds(15));
        assert_eq!(second.settings.kind, ChartKind::HeikinAshi);
        assert_eq!(second.symbol.as_deref(), Some("XAUUSD"));
        assert_eq!(back.chart(3).settings.kind, ChartKind::Renko);
        assert_eq!(back.active_chart, 2);
    }

    #[test]
    fn unknown_values_are_repaired_not_refused() {
        let text = r#"
            favorite_timeframes = ["M5", "M5", "X9", "H1"]
            active_chart = 40
            [layout]
            count = 3
            variant = 99
            [[charts]]
            timeframe = "M9999"
            kind = "spiral"
        "#;
        let prefs: Preferences = toml::from_str(text).unwrap();
        let prefs = prefs.normalized();
        assert_eq!(prefs.favorite_timeframes, vec!["M5", "H1"]);
        assert_eq!(prefs.layout, LayoutPref::default(), "a layout that is gone");
        assert_eq!(prefs.charts.len(), 1);
        assert_eq!(prefs.chart(0).timeframe, Timeframe::DEFAULT);
        assert_eq!(prefs.chart(0).settings.kind, ChartKind::Candles);
        assert_eq!(prefs.active_chart, 0);
    }

    #[test]
    fn a_file_from_before_chart_settings_keeps_its_types_and_links() {
        let text = r#"
            [links]
            interval = true
            crosshair = false
            time = false
            range = true
            [[charts]]
            timeframe = "H1"
            kind = "hollow"
        "#;
        let prefs: Preferences = toml::from_str(text).unwrap();
        let prefs = prefs.normalized();
        assert!(prefs.links.symbol, "the symbol link is new and on");
        assert!(prefs.links.interval && prefs.links.range && !prefs.links.time);
        assert_eq!(prefs.chart(0).settings.kind, ChartKind::Hollow);
        assert!(prefs.panel_open);
    }

    #[test]
    fn a_0_1_preferences_fixture_keeps_its_layout_and_chart_state() {
        let text = include_str!("../../fixtures/preferences-0.1.toml");
        let prefs: Preferences = toml::from_str(text).unwrap();
        let prefs = prefs.normalized();

        assert_eq!(prefs.schema_version, 4);
        assert_eq!(
            prefs.layout,
            LayoutPref {
                count: 4,
                variant: 0
            }
        );
        assert_eq!(prefs.active_chart, 2);
        assert_eq!(prefs.favorite_timeframes, vec!["M5", "H1"]);
        assert_eq!(prefs.custom_timeframes, vec!["M7"]);
        assert_eq!(prefs.charts.len(), 4);
        assert_eq!(prefs.chart(1).symbol.as_deref(), Some("XAUUSD"));
        assert_eq!(prefs.chart(1).settings.kind, ChartKind::Hollow);
        assert!(prefs.magnet && prefs.ticket_open);
        assert!(!prefs.keep_drawing);
        assert_eq!(prefs.panel_height, 280.0);
        assert_eq!(prefs.saved_colors, vec![16_711_680, 65280]);
    }

    #[test]
    fn the_charts_always_match_the_layout() {
        let prefs = Preferences {
            layout: LayoutPref {
                count: 6,
                variant: 0,
            },
            ..Preferences::default()
        }
        .normalized();
        assert_eq!(prefs.charts.len(), 6);
        // Added charts do not all start on the same timeframe.
        assert_ne!(prefs.charts[1].timeframe, prefs.charts[2].timeframe);

        let mut prefs = prefs;
        prefs.layout = LayoutPref {
            count: 2,
            variant: 0,
        };
        prefs.active_chart = 5;
        let prefs = prefs.normalized();
        assert_eq!(prefs.charts.len(), 2);
        assert_eq!(prefs.active_chart, 1);
    }
}
