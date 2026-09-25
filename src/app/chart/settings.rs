//! What a chart shows and how, as the user set it: its type, its price scale, its time zone, its
//! indicators and the heights of their panes. It is also the saved form, kept per chart in the
//! workspace, so every field has a default and everything read is repaired by
//! [`ChartSettings::normalized`].

use serde::{Deserialize, Serialize};

use super::footprint::FootprintSettings;
use super::options::{
    ChartColors, CrosshairStyle, PriceLines, ScaleMargin, StatusLine, TradingLines,
};
use super::study::{Placement, StudyConfig, StudyKind};
use super::transform::TransformSettings;
use super::zone::Zone;

/// The chart types. The codes are written in saved files and never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChartKind {
    #[default]
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
    /// Candles that show what traded at each price, split between sellers and buyers.
    Footprint,
}

impl ChartKind {
    pub const ALL: [Self; 14] = [
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
        Self::Footprint,
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
            Self::Footprint => "Footprint",
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
            Self::Footprint => "footprint",
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
                | Self::Footprint
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
/// The most indicators on one chart.
pub const MAX_STUDIES: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartSettings {
    #[serde(default)]
    pub kind: ChartKind,
    #[serde(default)]
    pub transform: TransformSettings,
    #[serde(default)]
    pub footprint: FootprintSettings,
    #[serde(default)]
    pub scale: ScaleMode,
    /// Whether higher prices are drawn lower.
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub zone: Zone,
    /// Whether the grid shows.
    #[serde(default = "yes")]
    pub grid: bool,
    /// Whether the grid has its horizontal lines (at the prices), when it shows.
    #[serde(default = "yes")]
    pub grid_horizontal: bool,
    /// Whether the grid has its vertical lines (at the times), when it shows.
    #[serde(default = "yes")]
    pub grid_vertical: bool,
    #[serde(default)]
    pub crosshair: CrosshairStyle,
    /// The room the automatic price scale leaves around the prices.
    #[serde(default)]
    pub margin: ScaleMargin,
    /// The symbol and timeframe, written large and faint behind the prices.
    #[serde(default)]
    pub watermark: bool,
    #[serde(default)]
    pub colors: ChartColors,
    #[serde(default)]
    pub price_lines: PriceLines,
    #[serde(default)]
    pub status: StatusLine,
    #[serde(default)]
    pub trading: TradingLines,
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
            footprint: FootprintSettings::default(),
            scale: ScaleMode::Linear,
            invert: false,
            zone: Zone::default(),
            grid: true,
            grid_horizontal: true,
            grid_vertical: true,
            crosshair: CrosshairStyle::Dashed,
            margin: ScaleMargin::Normal,
            watermark: false,
            colors: ChartColors::default(),
            price_lines: PriceLines::default(),
            status: StatusLine::default(),
            trading: TradingLines::default(),
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
        self.footprint = self.footprint.normalized();
        self.colors = self.colors.normalized();
        self.studies.retain(|s| {
            s.kind != StudyKind::Unknown && (s.kind != StudyKind::Custom || s.script.is_some())
        });
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

    /// The settings with the looks of the chart put back to their defaults: what it shows (the
    /// type, the indicators and the sizes of the panes) is kept.
    #[must_use]
    pub fn with_default_look(self) -> Self {
        Self {
            kind: self.kind,
            transform: self.transform,
            footprint: self.footprint,
            studies: self.studies,
            main_weight: self.main_weight,
            ..Self::default()
        }
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
    fn the_footprint_settings_are_saved_with_the_chart_and_repaired() {
        let mut settings = ChartSettings {
            kind: ChartKind::Footprint,
            ..ChartSettings::default()
        };
        settings.footprint.mode = super::super::footprint::CellMode::Delta;
        settings.footprint.imbalance_percent = 450;
        let text = toml::to_string_pretty(&settings).unwrap();
        let back: ChartSettings = toml::from_str(&text).unwrap();
        assert_eq!(back.normalized(), settings);
        // A file from before the footprint existed reads with its defaults.
        let old: ChartSettings = toml::from_str("kind = \"candles\"\n").unwrap();
        assert_eq!(old.footprint, FootprintSettings::default());
        let broken: ChartSettings =
            toml::from_str("[footprint]\nimbalance_percent = 1\nstack_rows = 500\n").unwrap();
        let broken = broken.normalized();
        assert_eq!(broken.footprint.imbalance_percent, 110);
        assert_eq!(broken.footprint.stack_rows, 12);
    }

    #[test]
    fn a_file_from_before_the_look_options_reads_with_their_defaults() {
        let old: ChartSettings = toml::from_str("kind = \"line\"\ngrid = false\n").unwrap();
        assert!(!old.grid);
        assert!(old.grid_horizontal && old.grid_vertical);
        assert_eq!(old.crosshair, CrosshairStyle::Dashed);
        assert_eq!(old.price_lines, PriceLines::default());
        assert_eq!(old.status, StatusLine::default());
        assert_eq!(old.trading, TradingLines::default());
        assert!(!old.colors.any());
        assert!(!old.watermark);
    }

    #[test]
    fn the_look_is_saved_and_reset_without_touching_what_the_chart_shows() {
        let mut settings = ChartSettings {
            kind: ChartKind::Area,
            grid_vertical: false,
            crosshair: CrosshairStyle::Solid,
            watermark: true,
            ..ChartSettings::default()
        };
        settings.colors.up = Some(0x00ff00);
        settings.price_lines.previous_close = true;
        settings.studies.push(StudyConfig::new(StudyKind::Rsi));
        let text = toml::to_string_pretty(&settings).unwrap();
        let back: ChartSettings = toml::from_str(&text).unwrap();
        assert_eq!(back.normalized(), settings);
        let reset = settings.clone().with_default_look();
        assert_eq!(reset.kind, ChartKind::Area);
        assert_eq!(reset.studies.len(), 1);
        assert!(reset.grid_vertical && !reset.watermark && !reset.colors.any());
        assert_eq!(reset.crosshair, CrosshairStyle::Dashed);
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
