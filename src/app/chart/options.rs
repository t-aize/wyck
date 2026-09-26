//! What a chart shows around its prices, as the user set it for that chart: the colors it
//! overrides, the crosshair, the price lines and tags, the status line, and which trading lines
//! show. Part of the saved [`super::settings::ChartSettings`], so every field has a default and
//! an older file reads with the defaults.

use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

/// Colors that replace the ones of the theme on one chart. `None` follows the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChartColors {
    /// Rising candles, bars and volume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up: Option<u32>,
    /// Falling candles, bars and volume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub down: Option<u32>,
    /// The line of the line, step and area types.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crosshair: Option<u32>,
    /// The numbers and labels of the axes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<u32>,
    /// The thick (yang) lines of Kagi. Follows the rising color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kagi_yang: Option<u32>,
    /// The thin (yin) lines of Kagi. Follows the falling color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kagi_yin: Option<u32>,
    /// The X of point and figure. Follows the rising color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pnf_up: Option<u32>,
    /// The O of point and figure. Follows the falling color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pnf_down: Option<u32>,
    /// The marks of a TPO profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpo: Option<u32>,
    /// The point of control of a TPO profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpo_poc: Option<u32>,
    /// The value area of a TPO profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpo_value_area: Option<u32>,
    /// The initial balance of a TPO profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpo_ib: Option<u32>,
    /// The single prints of a TPO profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpo_single: Option<u32>,
}

impl ChartColors {
    /// Whether any color is overridden.
    pub fn any(&self) -> bool {
        *self != Self::default()
    }

    /// The colors with everything past `0xFFFFFF` cut off.
    #[must_use]
    pub fn normalized(self) -> Self {
        let cut = |c: Option<u32>| c.map(|c| c & 0x00ff_ffff);
        Self {
            up: cut(self.up),
            down: cut(self.down),
            line: cut(self.line),
            background: cut(self.background),
            grid: cut(self.grid),
            crosshair: cut(self.crosshair),
            text: cut(self.text),
            kagi_yang: cut(self.kagi_yang),
            kagi_yin: cut(self.kagi_yin),
            pnf_up: cut(self.pnf_up),
            pnf_down: cut(self.pnf_down),
            tpo: cut(self.tpo),
            tpo_poc: cut(self.tpo_poc),
            tpo_value_area: cut(self.tpo_value_area),
            tpo_ib: cut(self.tpo_ib),
            tpo_single: cut(self.tpo_single),
        }
    }
}

/// The lines that follow the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrosshairStyle {
    #[default]
    Dashed,
    Dotted,
    Solid,
    /// No lines and no tags on the axes.
    Off,
}

impl CrosshairStyle {
    pub const ALL: [Self; 4] = [Self::Dashed, Self::Dotted, Self::Solid, Self::Off];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dashed => "Dashed",
            Self::Dotted => "Dotted",
            Self::Solid => "Solid",
            Self::Off => "Off",
        }
    }

    /// The dash pattern of the lines, `None` for solid lines.
    pub fn dash(self) -> Option<[f32; 2]> {
        match self {
            Self::Dashed | Self::Off => Some([4.0, 4.0]),
            Self::Dotted => Some([1.0, 3.0]),
            Self::Solid => None,
        }
    }
}

/// How much room the automatic price scale leaves above the highest price and below the lowest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleMargin {
    Tight,
    #[default]
    Normal,
    Loose,
}

impl ScaleMargin {
    pub const ALL: [Self; 3] = [Self::Tight, Self::Normal, Self::Loose];

    pub fn label(self) -> &'static str {
        match self {
            Self::Tight => "Tight",
            Self::Normal => "Normal",
            Self::Loose => "Loose",
        }
    }

    /// The room, as a share of the price range.
    pub fn share(self) -> f64 {
        match self {
            Self::Tight => 0.02,
            Self::Normal => 0.07,
            Self::Loose => 0.16,
        }
    }
}

/// The lines and tags the chart draws for the price itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceLines {
    /// The dotted line across the chart at the newest price.
    #[serde(default = "yes")]
    pub last_line: bool,
    /// The tag with the newest price on the axis.
    #[serde(default = "yes")]
    pub last_tag: bool,
    /// The time left in the bar, under the tag of the newest price.
    #[serde(default = "yes")]
    pub countdown: bool,
    /// The faint line at the ask.
    #[serde(default = "yes")]
    pub ask_line: bool,
    /// The line at the close of the day before the newest bar.
    #[serde(default)]
    pub previous_close: bool,
    /// Vertical lines where the day changes.
    #[serde(default)]
    pub day_breaks: bool,
}

impl Default for PriceLines {
    fn default() -> Self {
        Self {
            last_line: true,
            last_tag: true,
            countdown: true,
            ask_line: true,
            previous_close: false,
            day_breaks: false,
        }
    }
}

/// What the legend shows: its first line (the symbol always shows, since it is the button that
/// changes it) and the lines of the indicators under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusLine {
    /// The open, high, low and close of the bar under the pointer.
    #[serde(default = "yes")]
    pub prices: bool,
    /// The change from the bar before, in price and percent.
    #[serde(default = "yes")]
    pub change: bool,
    /// The dot that says whether the market is open.
    #[serde(default = "yes")]
    pub market: bool,
    /// The lines of the indicators drawn on the prices.
    #[serde(default = "yes")]
    pub indicators: bool,
    /// The values of the indicators at the pointer.
    #[serde(default = "yes")]
    pub indicator_values: bool,
}

impl Default for StatusLine {
    fn default() -> Self {
        Self {
            prices: true,
            change: true,
            market: true,
            indicators: true,
            indicator_values: true,
        }
    }
}

/// Which of the lines handed in by trading show on the prices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradingLines {
    #[serde(default = "yes")]
    pub orders: bool,
    #[serde(default = "yes")]
    pub positions: bool,
    /// The stop loss and take profit of positions and orders.
    #[serde(default = "yes")]
    pub protection: bool,
    #[serde(default = "yes")]
    pub alerts: bool,
}

impl Default for TradingLines {
    fn default() -> Self {
        Self {
            orders: true,
            positions: true,
            protection: true,
            alerts: true,
        }
    }
}

impl TradingLines {
    /// Whether the line `id` shows.
    pub fn shows(&self, id: super::lines::LineId) -> bool {
        use super::lines::LineId;
        match id {
            LineId::Order(_) => self.orders,
            LineId::Position(_) => self.positions,
            LineId::OrderStopLoss(_)
            | LineId::OrderTakeProfit(_)
            | LineId::StopLoss(_)
            | LineId::TakeProfit(_) => self.protection,
            LineId::Alert(_) => self.alerts,
            // A price being picked for a new order always shows.
            LineId::Pending(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::chart::lines::LineId;

    #[test]
    fn nothing_is_overridden_by_default_and_nothing_is_written() {
        let colors = ChartColors::default();
        assert!(!colors.any());
        assert_eq!(toml::to_string(&colors).unwrap(), "");
        let set = ChartColors {
            up: Some(0x112233),
            ..ChartColors::default()
        };
        assert!(set.any());
        let back: ChartColors = toml::from_str(&toml::to_string(&set).unwrap()).unwrap();
        assert_eq!(back, set);
    }

    #[test]
    fn colors_are_cut_to_24_bits() {
        let colors = ChartColors {
            up: Some(0xff11_2233),
            ..ChartColors::default()
        }
        .normalized();
        assert_eq!(colors.up, Some(0x11_2233));
    }

    #[test]
    fn a_line_shows_when_its_kind_does() {
        let lines = TradingLines {
            protection: false,
            ..TradingLines::default()
        };
        assert!(lines.shows(LineId::Order(1)));
        assert!(lines.shows(LineId::Position(1)));
        assert!(!lines.shows(LineId::StopLoss(1)));
        assert!(!lines.shows(LineId::OrderTakeProfit(1)));
        assert!(lines.shows(LineId::Pending(0)));
    }

    #[test]
    fn every_margin_leaves_more_room_than_the_one_before() {
        let shares: Vec<f64> = ScaleMargin::ALL.iter().map(|m| m.share()).collect();
        assert!(shares.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(ScaleMargin::default().share(), 0.07);
    }
}
