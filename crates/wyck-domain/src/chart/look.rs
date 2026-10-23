//! What of a chart's settings is kept for the chart, and what is the same for all of them.
//!
//! [`ChartContent`] is what one chart shows: its type, its price scale and its indicators. It is
//! saved per chart. [`ChartLook`] is how every chart looks: the grid, the crosshair, the colors,
//! the legend, the time zone and the lines it draws. It is saved once, and a change to it reaches
//! every chart. [`ChartSettings`] is the two together, which is what a chart works with.

use serde::{Deserialize, Serialize};

use super::options::{
    ChartColors, CrosshairStyle, PriceLines, ScaleMargin, StatusLine, TradingLines,
};
use super::order_flow::footprint::FootprintSettings;
use super::order_flow::tpo::TpoSettings;
use super::order_flow::volume::{VolumeBarSettings, VolumeCandleSettings};
use super::settings::{ChartKind, ChartSettings, ScaleMode};
use super::transform::settings::TransformSettings;
use super::zone::Zone;
use crate::indicators::config::StudyConfig;

/// What one chart shows, as it is saved for that chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChartContent {
    pub kind: ChartKind,
    pub transform: TransformSettings,
    pub footprint: FootprintSettings,
    pub volume_candles: VolumeCandleSettings,
    pub volume_bars: VolumeBarSettings,
    pub tpo: TpoSettings,
    pub scale: ScaleMode,
    pub studies: Vec<StudyConfig>,
    /// How much height the prices take relative to the panes.
    pub main_weight: f32,
}

impl Default for ChartContent {
    fn default() -> Self {
        ChartSettings::default().content()
    }
}

/// How every chart looks, saved once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is its own saved switch; grouping them would change the saved settings"
)]
pub struct ChartLook {
    pub zone: Zone,
    /// Whether higher prices are drawn lower.
    pub invert: bool,
    /// Whether the grid shows, and which of its lines.
    pub grid: bool,
    pub grid_horizontal: bool,
    pub grid_vertical: bool,
    pub crosshair: CrosshairStyle,
    /// The room the automatic price scale leaves around the prices.
    pub margin: ScaleMargin,
    /// The symbol and timeframe, written large and faint behind the prices.
    pub watermark: bool,
    pub colors: ChartColors,
    pub price_lines: PriceLines,
    pub status: StatusLine,
    pub trading: TradingLines,
}

impl Default for ChartLook {
    fn default() -> Self {
        ChartSettings::default().look()
    }
}

impl ChartLook {
    /// The look repaired: the colors in range.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.colors = self.colors.normalized();
        self
    }
}

impl ChartSettings {
    /// What this chart shows, for saving it with the chart.
    pub fn content(&self) -> ChartContent {
        ChartContent {
            kind: self.kind,
            transform: self.transform,
            footprint: self.footprint,
            volume_candles: self.volume_candles,
            volume_bars: self.volume_bars,
            tpo: self.tpo,
            scale: self.scale,
            studies: self.studies.clone(),
            main_weight: self.main_weight,
        }
    }

    pub fn look(&self) -> ChartLook {
        ChartLook {
            zone: self.zone,
            invert: self.invert,
            grid: self.grid,
            grid_horizontal: self.grid_horizontal,
            grid_vertical: self.grid_vertical,
            crosshair: self.crosshair,
            margin: self.margin,
            watermark: self.watermark,
            colors: self.colors,
            price_lines: self.price_lines,
            status: self.status,
            trading: self.trading,
        }
    }

    /// The settings of a chart from what it shows and how every chart looks.
    pub fn from_parts(content: ChartContent, look: &ChartLook) -> Self {
        let mut settings = Self {
            kind: content.kind,
            transform: content.transform,
            footprint: content.footprint,
            volume_candles: content.volume_candles,
            volume_bars: content.volume_bars,
            tpo: content.tpo,
            scale: content.scale,
            studies: content.studies,
            main_weight: content.main_weight,
            ..Self::default()
        };
        settings.set_look(look);
        settings
    }

    /// Puts the look every chart shares on this chart.
    pub fn set_look(&mut self, look: &ChartLook) {
        self.zone = look.zone;
        self.invert = look.invert;
        self.grid = look.grid;
        self.grid_horizontal = look.grid_horizontal;
        self.grid_vertical = look.grid_vertical;
        self.crosshair = look.crosshair;
        self.margin = look.margin;
        self.watermark = look.watermark;
        self.colors = look.colors;
        self.price_lines = look.price_lines;
        self.status = look.status;
        self.trading = look.trading;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicators::kind::StudyKind;

    #[test]
    fn the_content_and_the_look_make_the_settings_back() {
        let mut settings = ChartSettings {
            kind: ChartKind::Renko,
            scale: ScaleMode::Log,
            invert: true,
            grid: false,
            watermark: true,
            zone: Zone::Utc,
            ..ChartSettings::default()
        };
        settings.studies.push(StudyConfig::new(StudyKind::Rsi));
        settings.colors.up = Some(0x00_ff00);
        let back = ChartSettings::from_parts(settings.content(), &settings.look());
        assert_eq!(back, settings);
    }

    #[test]
    fn a_look_goes_on_a_chart_without_touching_what_it_shows() {
        let mut chart = ChartSettings {
            kind: ChartKind::Bars,
            scale: ScaleMode::Percent,
            ..ChartSettings::default()
        };
        chart.studies.push(StudyConfig::new(StudyKind::Sma));
        let look = ChartLook {
            grid: false,
            crosshair: CrosshairStyle::Solid,
            ..ChartLook::default()
        };
        chart.set_look(&look);
        assert_eq!(chart.look(), look);
        assert_eq!(chart.kind, ChartKind::Bars);
        assert_eq!(chart.scale, ScaleMode::Percent);
        assert_eq!(chart.studies.len(), 1);
    }

    #[test]
    fn the_defaults_of_the_parts_are_the_defaults_of_the_whole() {
        let whole = ChartSettings::default();
        assert_eq!(
            ChartSettings::from_parts(ChartContent::default(), &ChartLook::default()),
            whole
        );
    }

    #[test]
    fn what_is_saved_for_a_chart_has_no_look_and_old_files_still_read() {
        let content = ChartContent::default();
        let text = toml::to_string(&content).unwrap();
        assert!(!text.contains("grid"), "{text}");
        assert!(!text.contains("zone"), "{text}");
        // What a chart saved before the look was shared still reads: the look keys are ignored.
        let old: ChartContent =
            toml::from_str("kind = \"line\"\ngrid = false\nzone = \"utc\"\n").unwrap();
        assert_eq!(old.kind, ChartKind::Line);
        assert!(toml::from_str::<ChartLook>("").is_ok());
    }
}
