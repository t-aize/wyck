//! What a chart shows and how, as the user set it: its type, its price scale, its time zone, its
//! indicators and the heights of their panes. It is also the saved form, kept per chart in the
//! workspace, so every field has a default and everything read is repaired by
//! [`ChartSettings::normalized`].

use serde::{Deserialize, Serialize};

use super::study::{Placement, StudyConfig, StudyKind};
use super::transform::TransformSettings;
use super::zone::Zone;

/// The chart types. The codes are written in saved files and never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChartKind {
    Candles,
    /// Candles whose body is empty when the price closed above its open, colored by the change
    /// from the previous close.
    Hollow,
    /// Candles of averaged prices, which smooth out the noise of the trend.
    HeikinAshi,
    Bars,
    Line,
    /// A line that holds each price until the next one, like the price itself does.
    Step,
    Area,
    /// A line colored above and below a base price, with the area between them filled.
    Baseline,
    /// Bricks of a fixed size, one each time the price moves a full brick.
    Renko,
    /// Lines that only turn after breaking the extreme of the last few.
    LineBreak,
    /// A line that turns after a reversal, thick above the last shoulder and thin below the waist.
    Kagi,
    /// Columns of X and O boxes.
    PointFigure,
    /// Bars that all span the same range.
    Range,
}

impl ChartKind {
    pub const ALL: [Self; 13] = [
        Self::Candles,
        Self::Hollow,
        Self::HeikinAshi,
        Self::Bars,
        Self::Line,
        Self::Step,
        Self::Area,
        Self::Baseline,
        Self::Renko,
        Self::LineBreak,
        Self::Kagi,
        Self::PointFigure,
        Self::Range,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Candles => "Candles",
            Self::Hollow => "Hollow candles",
            Self::HeikinAshi => "Heikin Ashi",
            Self::Bars => "Bars",
            Self::Line => "Line",
            Self::Step => "Step line",
            Self::Area => "Area",
            Self::Baseline => "Baseline",
            Self::Renko => "Renko",
            Self::LineBreak => "Line break",
            Self::Kagi => "Kagi",
            Self::PointFigure => "Point and figure",
            Self::Range => "Range",
        }
    }

    /// The name written in saved files. It never changes.
    pub fn code(self) -> &'static str {
        match self {
            Self::Candles => "candles",
            Self::Hollow => "hollow",
            Self::HeikinAshi => "heikin_ashi",
            Self::Bars => "bars",
            Self::Line => "line",
            Self::Step => "step",
            Self::Area => "area",
            Self::Baseline => "baseline",
            Self::Renko => "renko",
            Self::LineBreak => "line_break",
            Self::Kagi => "kagi",
            Self::PointFigure => "point_figure",
            Self::Range => "range",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.code() == code)
    }

    /// Whether the kind is drawn from open, high, low and close.
    pub fn uses_ohlc(self) -> bool {
        matches!(
            self,
            Self::Candles
                | Self::Hollow
                | Self::HeikinAshi
                | Self::Bars
                | Self::Renko
                | Self::LineBreak
                | Self::PointFigure
                | Self::Range
        )
    }

    /// Whether the kind lays out its own elements instead of one per period, so the time axis
    /// is not regular.
    pub fn is_derived(self) -> bool {
        matches!(
            self,
            Self::HeikinAshi
                | Self::Renko
                | Self::LineBreak
                | Self::Kagi
                | Self::PointFigure
                | Self::Range
        )
    }

    /// Whether the elements of the kind stand for time periods (so the time axis is regular).
    pub fn keeps_time(self) -> bool {
        !self.is_derived() || self == Self::HeikinAshi
    }
}

impl Serialize for ChartKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for ChartKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Ok(Self::from_code(&code).unwrap_or(Self::Candles))
    }
}

impl Default for ChartKind {
    fn default() -> Self {
        Self::Candles
    }
}

/// How prices map to heights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleMode {
    /// Equal price steps are equal heights.
    #[default]
    Linear,
    /// Equal percentage moves are equal heights, for long histories.
    Log,
    /// Linear, labelled as the percentage change from the first bar on screen.
    Percent,
    /// Linear, labelled as the price indexed to 100 at the first bar on screen.
    Indexed,
}

impl ScaleMode {
    pub const ALL: [Self; 4] = [Self::Linear, Self::Log, Self::Percent, Self::Indexed];

    pub fn label(self) -> &'static str {
        match self {
            Self::Linear => "Regular",
            Self::Log => "Logarithmic",
            Self::Percent => "Percent",
            Self::Indexed => "Indexed to 100",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Self::Linear => "",
            Self::Log => "log",
            Self::Percent => "%",
            Self::Indexed => "100",
        }
    }
}

/// The heights of the panes: the prices, then one weight per indicator pane.
pub const MAIN_WEIGHT: f32 = 3.0;
pub const PANE_WEIGHT: f32 = 1.0;
/// The most indicators on one chart.
pub const MAX_STUDIES: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartSettings {
    #[serde(default)]
    pub kind: ChartKind,
    #[serde(default)]
    pub transform: TransformSettings,
    #[serde(default)]
    pub scale: ScaleMode,
    /// Whether higher prices are drawn lower.
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub zone: Zone,
    /// Whether the tick volume shows at the bottom of the prices.
    #[serde(default = "yes")]
    pub volume: bool,
    /// Whether the grid shows.
    #[serde(default = "yes")]
    pub grid: bool,
    #[serde(default)]
    pub studies: Vec<StudyConfig>,
    /// How much height the prices take relative to the panes.
    #[serde(default = "main_weight")]
    pub main_weight: f32,
}

fn yes() -> bool {
    true
}

fn main_weight() -> f32 {
    MAIN_WEIGHT
}

impl Default for ChartSettings {
    fn default() -> Self {
        Self {
            kind: ChartKind::Candles,
            transform: TransformSettings::default(),
            scale: ScaleMode::Linear,
            invert: false,
            zone: Zone::default(),
            volume: true,
            grid: true,
            studies: Vec::new(),
            main_weight: MAIN_WEIGHT,
        }
    }
}

/// A pane under the prices: which indicator it shows and how tall it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneRef {
    /// The index of the indicator in [`ChartSettings::studies`].
    pub study: usize,
    pub weight: f32,
}

impl ChartSettings {
    /// The settings repaired: unknown indicators dropped, inputs in range, weights sane.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.transform = self.transform.normalized();
        self.studies.retain(|s| s.kind != StudyKind::Unknown);
        self.studies.truncate(MAX_STUDIES);
        self.studies = self
            .studies
            .into_iter()
            .map(StudyConfig::normalized)
            .collect();
        if !(self.main_weight.is_finite() && self.main_weight > 0.0) {
            self.main_weight = MAIN_WEIGHT;
        }
        self.main_weight = self.main_weight.clamp(0.2, 50.0);
        self
    }

    /// The indicator panes, in the order they are stacked.
    pub fn panes(&self) -> Vec<PaneRef> {
        self.studies
            .iter()
            .enumerate()
            .filter(|(_, s)| s.spec().placement == Placement::Pane && s.visible)
            .map(|(index, s)| PaneRef {
                study: index,
                weight: s.pane_weight(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_survives_being_saved() {
        for kind in ChartKind::ALL {
            assert_eq!(ChartKind::from_code(kind.code()), Some(kind));
        }
        let mut labels: Vec<_> = ChartKind::ALL.iter().map(|k| k.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), ChartKind::ALL.len());
    }

    #[test]
    fn the_codes_of_older_files_still_read() {
        // These are a file format.
        for code in [
            "candles",
            "hollow",
            "heikin_ashi",
            "bars",
            "line",
            "step",
            "area",
        ] {
            assert!(ChartKind::from_code(code).is_some(), "{code}");
        }
    }

    #[test]
    fn settings_round_trip_and_repair() {
        let mut settings = ChartSettings {
            kind: ChartKind::Renko,
            scale: ScaleMode::Log,
            invert: true,
            ..ChartSettings::default()
        };
        settings.studies.push(StudyConfig::new(StudyKind::Rsi));
        settings.studies.push(StudyConfig::new(StudyKind::Sma));
        let text = toml::to_string_pretty(&settings).unwrap();
        let back: ChartSettings = toml::from_str(&text).unwrap();
        assert_eq!(back.normalized(), settings);

        let broken: ChartSettings =
            toml::from_str("kind = \"spiral\"\nmain_weight = -3.0\n[[studies]]\nkind = \"nope\"\n")
                .unwrap();
        let broken = broken.normalized();
        assert_eq!(broken.kind, ChartKind::Candles);
        assert!(broken.studies.is_empty());
        assert_eq!(broken.main_weight, MAIN_WEIGHT);
        assert_eq!(
            toml::from_str::<ChartSettings>("").unwrap(),
            ChartSettings::default()
        );
    }

    #[test]
    fn only_pane_indicators_get_a_pane() {
        let mut settings = ChartSettings::default();
        settings.studies.push(StudyConfig::new(StudyKind::Sma));
        settings.studies.push(StudyConfig::new(StudyKind::Macd));
        let mut hidden = StudyConfig::new(StudyKind::Rsi);
        hidden.visible = false;
        settings.studies.push(hidden);
        let panes = settings.panes();
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].study, 1);
    }
}
