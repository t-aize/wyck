//! The look and the levels a new drawing of each tool starts with.

use super::style::{Dash, Level, Style};
use super::style::{default_opacity, level};
use super::tool::{Group, Tool};

const GRAY: u32 = 0x9ca3af;
const RED: u32 = 0xff6467;
const ORANGE: u32 = 0xffb900;
const GREEN: u32 = 0x00d492;
const CYAN: u32 = 0x00d3f2;
const BLUE: u32 = 0x4f8dff;
const PURPLE: u32 = 0xc27aff;
const PINK: u32 = 0xf6339a;

impl Tool {
    /// The look a new drawing of this tool starts with.
    pub fn default_style(self) -> Style {
        let (color, width) = match self {
            Self::FibRetracement
            | Self::FibExtension
            | Self::FibChannel
            | Self::FibTimeZone
            | Self::FibFan
            | Self::FibTimeExtension
            | Self::FibCircles
            | Self::FibSpiral
            | Self::FibArcs
            | Self::FibWedge => (ORANGE, 1.0),
            Self::Measure | Self::PriceRange | Self::DateRange | Self::DatePriceRange => {
                (BLUE, 1.0)
            }
            Self::LongPosition | Self::ArrowMarkUp => (GREEN, 1.5),
            Self::ShortPosition | Self::ArrowMarkDown | Self::Pin => (RED, 1.5),
            Self::Text | Self::PriceLabel => (0xffffff, 1.0),
            Self::Forecast | Self::BarsPattern => (CYAN, 1.5),
            Self::GhostFeed => (PURPLE, 1.5),
            Self::AnchoredVwap | Self::Note | Self::FlagMark | Self::Icon => (ORANGE, 1.5),
            Self::PriceNote => (PURPLE, 1.5),
            Self::Highlighter => (ORANGE, 12.0),
            Self::CyclicLines | Self::TimeCycles | Self::SineLine => (CYAN, 1.5),
            Self::Pitchfan => (BLUE, 1.0),
            Self::HorizontalLine | Self::HorizontalRay | Self::VerticalLine | Self::CrossLine => {
                (ORANGE, 1.0)
            }
            Self::GannFan | Self::GannBox | Self::GannSquare | Self::GannSquareFixed => {
                (ORANGE, 1.0)
            }
            Self::Xabcd | Self::Cypher | Self::Abcd | Self::ThreeDrives => (PURPLE, 1.5),
            Self::HeadAndShoulders | Self::TrianglePattern => (CYAN, 1.5),
            Self::ElliottImpulse
            | Self::ElliottCorrection
            | Self::ElliottTriangle
            | Self::ElliottDoubleCombo
            | Self::ElliottTripleCombo => (BLUE, 1.5),
            _ => (BLUE, 1.5),
        };
        let extend_right = self.is_pitchfork();
        let fill_opacity = match self.group() {
            Group::Fibonacci | Group::Gann => 0.08,
            Group::Patterns => 0.18,
            _ if self.is_pitchfork() || self == Self::Pitchfan => 0.08,
            _ if matches!(
                self,
                Self::FixedRangeVolumeProfile | Self::AnchoredVolumeProfile
            ) =>
            {
                0.4
            }
            _ => default_opacity(),
        };
        // The boxes of the notes are filled with the color, so their words are their own.
        let text_color = match self {
            Self::Note => Some(0x0a0a0a),
            Self::PriceNote
            | Self::Callout
            | Self::Comment
            | Self::Signpost
            | Self::Table
            | Self::Pin => Some(0xffffff),
            _ => None,
        };
        let (color, dash) = match self {
            Self::Callout | Self::Comment | Self::Signpost | Self::Table => (BLUE, Dash::Solid),
            Self::Forecast => (color, Dash::Dashed),
            _ => (color, Dash::Solid),
        };
        Style {
            color,
            width,
            dash,
            extend_left: self == Self::SineLine,
            extend_right: extend_right || self == Self::SineLine,
            fill_opacity,
            text_color,
            ..Style::default()
        }
    }

    /// The levels a new drawing of this tool starts with; empty for a tool without levels.
    pub fn default_levels(self) -> Vec<Level> {
        match self {
            Self::FibRetracement | Self::FibChannel => vec![
                level(0.0, GRAY, true),
                level(0.236, RED, true),
                level(0.382, ORANGE, true),
                level(0.5, GREEN, true),
                level(0.618, CYAN, true),
                level(0.786, BLUE, true),
                level(1.0, GRAY, true),
                level(1.272, PURPLE, false),
                level(1.618, RED, false),
                level(2.618, PINK, false),
                level(3.618, PURPLE, false),
                level(4.236, PINK, false),
            ],
            Self::FibExtension => vec![
                level(0.0, GRAY, true),
                level(0.236, RED, false),
                level(0.382, ORANGE, false),
                level(0.5, GREEN, false),
                level(0.618, CYAN, true),
                level(0.786, BLUE, false),
                level(1.0, GRAY, true),
                level(1.272, PURPLE, false),
                level(1.618, ORANGE, true),
                level(2.0, GREEN, false),
                level(2.618, RED, true),
                level(3.618, PURPLE, false),
                level(4.236, PINK, false),
            ],
            Self::FibTimeZone => [0.0, 1.0, 2.0, 3.0, 5.0, 8.0, 13.0, 21.0, 34.0, 55.0, 89.0]
                .iter()
                .map(|&v| level(v, BLUE, true))
                .collect(),
            Self::FibFan => vec![
                level(0.236, RED, true),
                level(0.382, ORANGE, true),
                level(0.5, GREEN, true),
                level(0.618, CYAN, true),
                level(0.786, BLUE, true),
            ],
            Self::Pitchfork
            | Self::SchiffPitchfork
            | Self::ModifiedSchiffPitchfork
            | Self::InsidePitchfork => vec![
                level(0.25, ORANGE, false),
                level(0.382, RED, false),
                level(0.5, GREEN, true),
                level(0.618, CYAN, false),
                level(0.75, BLUE, false),
                level(1.0, BLUE, true),
                level(1.5, PURPLE, false),
                level(1.75, PINK, false),
                level(2.0, RED, false),
            ],
            Self::GannFan => vec![
                level(8.0, PURPLE, true),
                level(4.0, RED, true),
                level(3.0, ORANGE, true),
                level(2.0, GREEN, true),
                level(1.0, CYAN, true),
                level(0.5, GREEN, true),
                level(1.0 / 3.0, ORANGE, true),
                level(0.25, RED, true),
                level(0.125, PURPLE, true),
            ],
            Self::FibTimeExtension => vec![
                level(0.0, GRAY, true),
                level(0.382, ORANGE, true),
                level(0.5, GREEN, false),
                level(0.618, CYAN, true),
                level(0.786, BLUE, false),
                level(1.0, GRAY, true),
                level(1.382, ORANGE, true),
                level(1.618, CYAN, true),
                level(2.0, GREEN, false),
                level(2.618, RED, true),
                level(4.236, PINK, false),
            ],
            Self::FibCircles => vec![
                level(0.236, RED, false),
                level(0.382, ORANGE, true),
                level(0.5, GREEN, true),
                level(0.618, CYAN, true),
                level(0.786, BLUE, false),
                level(1.0, GRAY, true),
                level(1.272, PURPLE, false),
                level(1.618, ORANGE, true),
                level(2.618, RED, false),
            ],
            Self::FibArcs => vec![
                level(0.236, RED, false),
                level(0.382, ORANGE, true),
                level(0.5, GREEN, true),
                level(0.618, CYAN, true),
                level(0.786, BLUE, false),
                level(1.0, GRAY, true),
            ],
            Self::FibWedge => vec![
                level(0.236, RED, false),
                level(0.382, ORANGE, true),
                level(0.5, GREEN, true),
                level(0.618, CYAN, true),
                level(0.786, BLUE, false),
                level(1.0, GRAY, true),
            ],
            Self::Pitchfan => vec![
                level(0.0, GRAY, true),
                level(0.25, ORANGE, false),
                level(0.382, RED, true),
                level(0.5, GREEN, true),
                level(0.618, CYAN, true),
                level(0.75, BLUE, false),
                level(1.0, GRAY, true),
            ],
            Self::RegressionTrend => vec![level(1.0, CYAN, false), level(2.0, BLUE, true)],
            Self::AnchoredVwap => vec![
                level(1.0, GREEN, true),
                level(2.0, CYAN, false),
                level(3.0, PURPLE, false),
            ],
            Self::GannBox | Self::GannSquare | Self::GannSquareFixed => vec![
                level(0.0, GRAY, true),
                level(0.25, ORANGE, true),
                level(0.382, GREEN, true),
                level(0.5, CYAN, true),
                level(0.618, GREEN, true),
                level(0.75, ORANGE, true),
                level(1.0, GRAY, true),
            ],
            _ => Vec::new(),
        }
    }
}
