//! What a drawing is: a tool, the points it hangs on, and how it looks.
//!
//! A point is a time and a price, never a screen position, so a drawing stays where the user put
//! it when the chart is scrolled, zoomed, resized or shown on another timeframe. These types are
//! also what is saved: every field has a default and unknown tools are tolerated, so a file from
//! another version still loads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;

/// The most drawings kept for one symbol, and the most points in one brush stroke. A guard
/// against a runaway file more than a limit anyone meets.
pub const MAX_DRAWINGS_PER_SYMBOL: usize = 2_000;
pub const MAX_BRUSH_POINTS: usize = 2_000;
/// The most points an arrow path can have.
pub const MAX_PATH_POINTS: usize = 64;
/// The most levels a drawing can have.
pub const MAX_LEVELS: usize = 24;

/// A place on the chart: a time in Unix milliseconds and a raw price.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub t: i64,
    pub p: f64,
}

/// The kinds of drawing. The names are written in saved files and never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    TrendLine,
    Ray,
    ExtendedLine,
    HorizontalLine,
    HorizontalRay,
    VerticalLine,
    CrossLine,
    Arrow,
    /// A broken line of as many points as the user places, with an arrow head at the end.
    ArrowPath,
    /// A line with the change, the bars, the time and the angle written beside it.
    InfoLine,
    /// A line with its angle to the horizontal.
    TrendAngle,
    ParallelChannel,
    /// The linear regression of the closes between two times, with its deviation bands.
    RegressionTrend,
    /// A trend line with a flat line under or over it.
    FlatTopBottom,
    /// Two lines that need not be parallel, with the zone between them.
    DisjointChannel,
    Pitchfork,
    SchiffPitchfork,
    ModifiedSchiffPitchfork,
    InsidePitchfork,
    /// Rays from a point through the levels of the segment between two others.
    Pitchfan,
    FibRetracement,
    FibExtension,
    FibChannel,
    FibTimeZone,
    FibFan,
    /// The time zones of a move, repeated from a third point.
    FibTimeExtension,
    FibCircles,
    FibSpiral,
    FibArcs,
    FibWedge,
    GannFan,
    GannBox,
    GannSquare,
    /// A Gann square that is square on the screen, whatever the scale.
    GannSquareFixed,
    Xabcd,
    Cypher,
    Abcd,
    HeadAndShoulders,
    TrianglePattern,
    ThreeDrives,
    CyclicLines,
    TimeCycles,
    SineLine,
    ElliottImpulse,
    ElliottCorrection,
    ElliottTriangle,
    ElliottDoubleCombo,
    ElliottTripleCombo,
    Rectangle,
    RotatedRectangle,
    Circle,
    Ellipse,
    Triangle,
    Arc,
    Curve,
    DoubleCurve,
    Brush,
    Highlighter,
    /// A block arrow between two points.
    ArrowMarker,
    ArrowMarkUp,
    ArrowMarkDown,
    Measure,
    PriceRange,
    DateRange,
    DatePriceRange,
    LongPosition,
    ShortPosition,
    /// An arrow to a target, with the move it means written at its end.
    Forecast,
    /// The bars between two times, copied to start at a third point.
    BarsPattern,
    /// Bars made up from the moves of the bars between two times, from a third point.
    GhostFeed,
    AnchoredVwap,
    FixedRangeVolumeProfile,
    AnchoredVolumeProfile,
    Text,
    Note,
    PriceNote,
    Callout,
    Comment,
    Pin,
    Signpost,
    FlagMark,
    Table,
    Icon,
    PriceLabel,
    /// A tool a newer version wrote. Such drawings are dropped on load.
    #[serde(other)]
    Unknown,
}

/// The families of the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Lines,
    Channels,
    Fibonacci,
    Gann,
    Patterns,
    Elliott,
    Shapes,
    Measure,
    Forecast,
    Annotations,
}

impl Group {
    pub const ALL: [Self; 10] = [
        Self::Lines,
        Self::Channels,
        Self::Fibonacci,
        Self::Gann,
        Self::Patterns,
        Self::Elliott,
        Self::Shapes,
        Self::Measure,
        Self::Forecast,
        Self::Annotations,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Lines => "Lines",
            Self::Channels => "Channels and pitchforks",
            Self::Fibonacci => "Fibonacci",
            Self::Gann => "Gann",
            Self::Patterns => "Patterns",
            Self::Elliott => "Elliott waves",
            Self::Shapes => "Shapes",
            Self::Measure => "Measure and position",
            Self::Forecast => "Forecast and volume",
            Self::Annotations => "Annotations",
        }
    }
}

impl Tool {
    /// Every tool a user can pick, in toolbar order.
    pub const ALL: [Self; 84] = [
        Self::TrendLine,
        Self::Ray,
        Self::ExtendedLine,
        Self::HorizontalLine,
        Self::HorizontalRay,
        Self::VerticalLine,
        Self::CrossLine,
        Self::Arrow,
        Self::ArrowPath,
        Self::InfoLine,
        Self::TrendAngle,
        Self::ParallelChannel,
        Self::RegressionTrend,
        Self::FlatTopBottom,
        Self::DisjointChannel,
        Self::Pitchfork,
        Self::SchiffPitchfork,
        Self::ModifiedSchiffPitchfork,
        Self::InsidePitchfork,
        Self::Pitchfan,
        Self::FibRetracement,
        Self::FibExtension,
        Self::FibChannel,
        Self::FibTimeZone,
        Self::FibFan,
        Self::FibTimeExtension,
        Self::FibCircles,
        Self::FibSpiral,
        Self::FibArcs,
        Self::FibWedge,
        Self::GannFan,
        Self::GannBox,
        Self::GannSquare,
        Self::GannSquareFixed,
        Self::Xabcd,
        Self::Cypher,
        Self::Abcd,
        Self::HeadAndShoulders,
        Self::TrianglePattern,
        Self::ThreeDrives,
        Self::CyclicLines,
        Self::TimeCycles,
        Self::SineLine,
        Self::ElliottImpulse,
        Self::ElliottCorrection,
        Self::ElliottTriangle,
        Self::ElliottDoubleCombo,
        Self::ElliottTripleCombo,
        Self::Rectangle,
        Self::RotatedRectangle,
        Self::Circle,
        Self::Ellipse,
        Self::Triangle,
        Self::Arc,
        Self::Curve,
        Self::DoubleCurve,
        Self::Brush,
        Self::Highlighter,
        Self::ArrowMarker,
        Self::ArrowMarkUp,
        Self::ArrowMarkDown,
        Self::Measure,
        Self::PriceRange,
        Self::DateRange,
        Self::DatePriceRange,
        Self::LongPosition,
        Self::ShortPosition,
        Self::Forecast,
        Self::BarsPattern,
        Self::GhostFeed,
        Self::AnchoredVwap,
        Self::FixedRangeVolumeProfile,
        Self::AnchoredVolumeProfile,
        Self::Text,
        Self::Note,
        Self::PriceNote,
        Self::Callout,
        Self::Comment,
        Self::Pin,
        Self::Signpost,
        Self::FlagMark,
        Self::Table,
        Self::Icon,
        Self::PriceLabel,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::TrendLine => "Trend line",
            Self::Ray => "Ray",
            Self::ExtendedLine => "Extended line",
            Self::HorizontalLine => "Horizontal line",
            Self::HorizontalRay => "Horizontal ray",
            Self::VerticalLine => "Vertical line",
            Self::CrossLine => "Cross line",
            Self::Arrow => "Arrow",
            Self::ArrowPath => "Arrow path",
            Self::InfoLine => "Info line",
            Self::TrendAngle => "Trend angle",
            Self::ParallelChannel => "Parallel channel",
            Self::RegressionTrend => "Regression trend",
            Self::FlatTopBottom => "Flat top/bottom",
            Self::DisjointChannel => "Disjoint channel",
            Self::Pitchfork => "Pitchfork",
            Self::SchiffPitchfork => "Schiff pitchfork",
            Self::ModifiedSchiffPitchfork => "Modified Schiff pitchfork",
            Self::InsidePitchfork => "Inside pitchfork",
            Self::Pitchfan => "Pitchfan",
            Self::FibRetracement => "Fib retracement",
            Self::FibExtension => "Trend-based fib extension",
            Self::FibChannel => "Fib channel",
            Self::FibTimeZone => "Fib time zone",
            Self::FibFan => "Fib speed fan",
            Self::FibTimeExtension => "Trend-based fib time",
            Self::FibCircles => "Fib circles",
            Self::FibSpiral => "Fib spiral",
            Self::FibArcs => "Fib speed arcs",
            Self::FibWedge => "Fib wedge",
            Self::GannFan => "Gann fan",
            Self::GannBox => "Gann box",
            Self::GannSquare => "Gann square",
            Self::GannSquareFixed => "Gann square fixed",
            Self::Xabcd => "XABCD pattern",
            Self::Cypher => "Cypher pattern",
            Self::Abcd => "ABCD pattern",
            Self::HeadAndShoulders => "Head and shoulders",
            Self::TrianglePattern => "Triangle pattern",
            Self::ThreeDrives => "Three drives pattern",
            Self::CyclicLines => "Cyclic lines",
            Self::TimeCycles => "Time cycles",
            Self::SineLine => "Sine line",
            Self::ElliottImpulse => "Elliott impulse wave (12345)",
            Self::ElliottCorrection => "Elliott correction wave (ABC)",
            Self::ElliottTriangle => "Elliott triangle wave (ABCDE)",
            Self::ElliottDoubleCombo => "Elliott double combo (WXY)",
            Self::ElliottTripleCombo => "Elliott triple combo (WXYXZ)",
            Self::Rectangle => "Rectangle",
            Self::RotatedRectangle => "Rotated rectangle",
            Self::Circle => "Circle",
            Self::Ellipse => "Ellipse",
            Self::Triangle => "Triangle",
            Self::Arc => "Arc",
            Self::Curve => "Curve",
            Self::DoubleCurve => "Double curve",
            Self::Brush => "Brush",
            Self::Highlighter => "Highlighter",
            Self::ArrowMarker => "Arrow marker",
            Self::ArrowMarkUp => "Arrow mark up",
            Self::ArrowMarkDown => "Arrow mark down",
            Self::Measure => "Measure",
            Self::PriceRange => "Price range",
            Self::DateRange => "Date range",
            Self::DatePriceRange => "Date and price range",
            Self::LongPosition => "Long position",
            Self::ShortPosition => "Short position",
            Self::Forecast => "Forecast",
            Self::BarsPattern => "Bars pattern",
            Self::GhostFeed => "Ghost feed",
            Self::AnchoredVwap => "Anchored VWAP",
            Self::FixedRangeVolumeProfile => "Fixed range volume profile",
            Self::AnchoredVolumeProfile => "Anchored volume profile",
            Self::Text => "Text",
            Self::Note => "Note",
            Self::PriceNote => "Price note",
            Self::Callout => "Callout",
            Self::Comment => "Comment",
            Self::Pin => "Pin",
            Self::Signpost => "Signpost",
            Self::FlagMark => "Flag mark",
            Self::Table => "Table",
            Self::Icon => "Icon",
            Self::PriceLabel => "Price label",
            Self::Unknown => "Unknown",
        }
    }

    /// The name the tool is saved under.
    pub fn code(self) -> String {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::String(code)) => code,
            _ => "unknown".to_owned(),
        }
    }

    pub fn group(self) -> Group {
        match self {
            Self::TrendLine
            | Self::Ray
            | Self::ExtendedLine
            | Self::HorizontalLine
            | Self::HorizontalRay
            | Self::VerticalLine
            | Self::CrossLine
            | Self::Arrow
            | Self::ArrowPath
            | Self::InfoLine
            | Self::TrendAngle => Group::Lines,
            Self::ParallelChannel
            | Self::RegressionTrend
            | Self::FlatTopBottom
            | Self::DisjointChannel
            | Self::Pitchfork
            | Self::SchiffPitchfork
            | Self::ModifiedSchiffPitchfork
            | Self::InsidePitchfork
            | Self::Pitchfan => Group::Channels,
            Self::FibRetracement
            | Self::FibExtension
            | Self::FibChannel
            | Self::FibTimeZone
            | Self::FibFan
            | Self::FibTimeExtension
            | Self::FibCircles
            | Self::FibSpiral
            | Self::FibArcs
            | Self::FibWedge => Group::Fibonacci,
            Self::GannFan | Self::GannBox | Self::GannSquare | Self::GannSquareFixed => Group::Gann,
            Self::Xabcd
            | Self::Cypher
            | Self::Abcd
            | Self::HeadAndShoulders
            | Self::TrianglePattern
            | Self::ThreeDrives
            | Self::CyclicLines
            | Self::TimeCycles
            | Self::SineLine => Group::Patterns,
            Self::ElliottImpulse
            | Self::ElliottCorrection
            | Self::ElliottTriangle
            | Self::ElliottDoubleCombo
            | Self::ElliottTripleCombo => Group::Elliott,
            Self::Rectangle
            | Self::RotatedRectangle
            | Self::Circle
            | Self::Ellipse
            | Self::Triangle
            | Self::Arc
            | Self::Curve
            | Self::DoubleCurve
            | Self::Brush
            | Self::Highlighter
            | Self::ArrowMarker
            | Self::ArrowMarkUp
            | Self::ArrowMarkDown => Group::Shapes,
            Self::Measure
            | Self::PriceRange
            | Self::DateRange
            | Self::DatePriceRange
            | Self::LongPosition
            | Self::ShortPosition => Group::Measure,
            Self::Forecast
            | Self::BarsPattern
            | Self::GhostFeed
            | Self::AnchoredVwap
            | Self::FixedRangeVolumeProfile
            | Self::AnchoredVolumeProfile => Group::Forecast,
            Self::Text
            | Self::Note
            | Self::PriceNote
            | Self::Callout
            | Self::Comment
            | Self::Pin
            | Self::Signpost
            | Self::FlagMark
            | Self::Table
            | Self::Icon
            | Self::PriceLabel
            | Self::Unknown => Group::Annotations,
        }
    }

    /// How many points the tool needs. A brush, a highlighter or an arrow path takes as many as
    /// the user places, at least two.
    pub fn anchors(self) -> usize {
        match self {
            Self::HorizontalLine
            | Self::HorizontalRay
            | Self::VerticalLine
            | Self::CrossLine
            | Self::Text
            | Self::PriceLabel
            | Self::ArrowMarkUp
            | Self::ArrowMarkDown
            | Self::AnchoredVwap
            | Self::AnchoredVolumeProfile
            | Self::Note
            | Self::Comment
            | Self::Pin
            | Self::Signpost
            | Self::FlagMark
            | Self::Table
            | Self::Icon => 1,
            Self::TrendLine
            | Self::Ray
            | Self::ExtendedLine
            | Self::Arrow
            | Self::ArrowPath
            | Self::InfoLine
            | Self::TrendAngle
            | Self::RegressionTrend
            | Self::FibRetracement
            | Self::FibTimeZone
            | Self::FibFan
            | Self::FibCircles
            | Self::FibSpiral
            | Self::FibArcs
            | Self::GannFan
            | Self::GannBox
            | Self::GannSquare
            | Self::GannSquareFixed
            | Self::CyclicLines
            | Self::TimeCycles
            | Self::SineLine
            | Self::Rectangle
            | Self::Circle
            | Self::Ellipse
            | Self::DoubleCurve
            | Self::Brush
            | Self::Highlighter
            | Self::ArrowMarker
            | Self::Measure
            | Self::PriceRange
            | Self::DateRange
            | Self::DatePriceRange
            | Self::Forecast
            | Self::FixedRangeVolumeProfile
            | Self::PriceNote
            | Self::Callout => 2,
            Self::ParallelChannel
            | Self::FlatTopBottom
            | Self::Pitchfork
            | Self::SchiffPitchfork
            | Self::ModifiedSchiffPitchfork
            | Self::InsidePitchfork
            | Self::Pitchfan
            | Self::FibExtension
            | Self::FibChannel
            | Self::FibTimeExtension
            | Self::FibWedge
            | Self::Triangle
            | Self::RotatedRectangle
            | Self::Arc
            | Self::Curve
            | Self::BarsPattern
            | Self::GhostFeed => 3,
            Self::DisjointChannel
            | Self::Abcd
            | Self::TrianglePattern
            | Self::ElliottCorrection
            | Self::ElliottDoubleCombo
            | Self::LongPosition
            | Self::ShortPosition => 4,
            Self::Xabcd | Self::Cypher => 5,
            Self::ElliottImpulse | Self::ElliottTriangle | Self::ElliottTripleCombo => 6,
            Self::HeadAndShoulders | Self::ThreeDrives => 7,
            Self::Unknown => 0,
        }
    }

    /// Whether a click places the last point and finishes, without a second click.
    pub fn is_single_click(self) -> bool {
        self.anchors() == 1 || self.is_position()
    }

    pub fn is_position(self) -> bool {
        matches!(self, Self::LongPosition | Self::ShortPosition)
    }

    /// Whether the drawing is a stroke that follows the pointer while the button is down.
    pub fn is_freehand(self) -> bool {
        matches!(self, Self::Brush | Self::Highlighter)
    }

    /// Whether the drawing is laid out in the box between its two points, with a grip at every
    /// corner and side.
    pub fn is_box(self) -> bool {
        matches!(
            self,
            Self::Rectangle
                | Self::Ellipse
                | Self::GannBox
                | Self::GannSquare
                | Self::Measure
                | Self::DatePriceRange
        )
    }

    pub fn is_pitchfork(self) -> bool {
        matches!(
            self,
            Self::Pitchfork
                | Self::SchiffPitchfork
                | Self::ModifiedSchiffPitchfork
                | Self::InsidePitchfork
        )
    }

    /// Whether the drawing has text the user can edit.
    pub fn has_text(self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Note
                | Self::PriceNote
                | Self::Callout
                | Self::Comment
                | Self::Pin
                | Self::Signpost
                | Self::Table
        )
    }

    /// Whether the drawing writes words on the chart (whose size and color can be set).
    pub fn has_words(self) -> bool {
        matches!(self.group(), Group::Patterns | Group::Elliott)
            && !matches!(self, Self::CyclicLines | Self::TimeCycles | Self::SineLine)
            || matches!(self, Self::PriceLabel)
            || self.has_text()
    }

    /// The name of the switch that turns the tool's own writing (levels, names, values) on and
    /// off, for a tool that has some.
    pub fn labels_switch(self) -> Option<&'static str> {
        if self.has_levels() {
            return Some("Write the levels");
        }
        match self {
            Self::HorizontalLine => Some("Write the price"),
            Self::InfoLine
            | Self::TrendAngle
            | Self::PriceRange
            | Self::DateRange
            | Self::DatePriceRange
            | Self::Forecast => Some("Write the values"),
            tool if tool.has_words() && !tool.has_text() && tool != Self::PriceLabel => {
                Some("Write the names")
            }
            _ => None,
        }
    }

    /// Whether the drawing has levels (Fibonacci ratios, pitchfork lines, Gann angles).
    pub fn has_levels(self) -> bool {
        !self.default_levels().is_empty()
    }

    /// Whether the drawing's lines can be extended past its points.
    pub fn has_extend(self) -> bool {
        matches!(
            self,
            Self::TrendLine
                | Self::Arrow
                | Self::InfoLine
                | Self::TrendAngle
                | Self::ParallelChannel
                | Self::RegressionTrend
                | Self::FlatTopBottom
                | Self::DisjointChannel
                | Self::SineLine
                | Self::FibRetracement
                | Self::FibExtension
                | Self::FibChannel
                | Self::Rectangle
                | Self::HeadAndShoulders
        ) || self.is_pitchfork()
    }

    /// Whether the drawing has an area that can be filled.
    pub fn has_fill(self) -> bool {
        matches!(
            self,
            Self::ParallelChannel
                | Self::RegressionTrend
                | Self::FlatTopBottom
                | Self::DisjointChannel
                | Self::Pitchfan
                | Self::FibRetracement
                | Self::FibExtension
                | Self::FibChannel
                | Self::FibFan
                | Self::FibTimeExtension
                | Self::FibWedge
                | Self::GannFan
                | Self::GannBox
                | Self::GannSquare
                | Self::GannSquareFixed
                | Self::TimeCycles
                | Self::Xabcd
                | Self::Cypher
                | Self::Abcd
                | Self::HeadAndShoulders
                | Self::TrianglePattern
                | Self::Rectangle
                | Self::RotatedRectangle
                | Self::Circle
                | Self::Ellipse
                | Self::Triangle
                | Self::Arc
                | Self::PriceRange
                | Self::DateRange
                | Self::DatePriceRange
                | Self::FixedRangeVolumeProfile
                | Self::AnchoredVolumeProfile
        ) || self.is_pitchfork()
    }

    /// Whether the line style (solid, dashed, dotted) applies.
    pub fn has_dash(self) -> bool {
        !matches!(
            self,
            Self::Text
                | Self::PriceLabel
                | Self::Brush
                | Self::Highlighter
                | Self::Note
                | Self::Comment
                | Self::Pin
                | Self::Signpost
                | Self::FlagMark
                | Self::Table
                | Self::Icon
                | Self::ArrowMarker
                | Self::ArrowMarkUp
                | Self::ArrowMarkDown
                | Self::BarsPattern
                | Self::GhostFeed
                | Self::FixedRangeVolumeProfile
                | Self::AnchoredVolumeProfile
        )
    }

    /// Whether the width of the line applies.
    pub fn has_width(self) -> bool {
        !matches!(
            self,
            Self::Text
                | Self::PriceLabel
                | Self::Note
                | Self::Comment
                | Self::Pin
                | Self::Signpost
                | Self::FlagMark
                | Self::Table
                | Self::Icon
                | Self::ArrowMarkUp
                | Self::ArrowMarkDown
                | Self::BarsPattern
                | Self::GhostFeed
                | Self::FixedRangeVolumeProfile
                | Self::AnchoredVolumeProfile
        )
    }

    /// The widths the style bar offers for the tool.
    pub fn widths(self) -> [f32; 4] {
        if self == Self::Highlighter {
            [8.0, 12.0, 20.0, 32.0]
        } else {
            WIDTHS
        }
    }

    /// The name of the drawing's optional extra line, if it has one: the middle of a channel,
    /// the diagonals of a Gann box.
    pub fn middle_label(self) -> Option<&'static str> {
        match self {
            Self::ParallelChannel | Self::DisjointChannel => Some("Middle line"),
            Self::GannBox | Self::GannSquare | Self::GannSquareFixed => Some("Angles"),
            Self::Xabcd | Self::Cypher | Self::Abcd | Self::ThreeDrives => Some("Ratios"),
            _ => None,
        }
    }

    /// Whether the levels can be drawn the other way round.
    pub fn has_reverse(self) -> bool {
        matches!(
            self,
            Self::FibRetracement | Self::FibChannel | Self::GannBox | Self::FibSpiral
        )
    }

    /// Whether the drawing labels its points with a wave degree.
    pub fn is_elliott(self) -> bool {
        self.group() == Group::Elliott
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// How a drawing looks. Colors are `0xRRGGBB`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Style {
    #[serde(default = "default_color")]
    pub color: u32,
    #[serde(default = "default_width")]
    pub width: f32,
    #[serde(default)]
    pub dash: Dash,
    /// Whether closed shapes are filled.
    #[serde(default = "yes")]
    pub fill: bool,
    /// The color of the fill; the line color when unset.
    #[serde(default)]
    pub fill_color: Option<u32>,
    /// How opaque the fill is, from 0 to 1.
    #[serde(default = "default_opacity")]
    pub fill_opacity: f32,
    /// Whether the lines go on past the first point, to the edge of the chart.
    #[serde(default)]
    pub extend_left: bool,
    /// Whether the lines go on past the last point.
    #[serde(default)]
    pub extend_right: bool,
    /// The size of the words, in points.
    #[serde(default = "default_text_size")]
    pub text_size: f32,
    /// The color of the words; the line color when unset.
    #[serde(default)]
    pub text_color: Option<u32>,
    #[serde(default)]
    pub bold: bool,
    /// Whether the levels and prices are written.
    #[serde(default = "yes")]
    pub labels: bool,
    /// Whether the extra line of the tool shows (see [`Tool::middle_label`]).
    #[serde(default = "yes")]
    pub middle: bool,
}

fn default_color() -> u32 {
    0x4f8dff
}

fn default_width() -> f32 {
    1.5
}

fn default_opacity() -> f32 {
    0.15
}

fn default_text_size() -> f32 {
    12.0
}

fn yes() -> bool {
    true
}

impl Default for Style {
    fn default() -> Self {
        Self {
            color: default_color(),
            width: default_width(),
            dash: Dash::Solid,
            fill: true,
            fill_color: None,
            fill_opacity: default_opacity(),
            extend_left: false,
            extend_right: false,
            text_size: default_text_size(),
            text_color: None,
            bold: false,
            labels: true,
            middle: true,
        }
    }
}

impl Style {
    pub fn fill_color(&self) -> u32 {
        self.fill_color.unwrap_or(self.color)
    }

    pub fn text_color(&self) -> u32 {
        self.text_color.unwrap_or(self.color)
    }

    /// The style with every number in its range, whatever a file said.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.color &= 0xff_ffff;
        self.fill_color = self.fill_color.map(|c| c & 0xff_ffff);
        self.text_color = self.text_color.map(|c| c & 0xff_ffff);
        if !(self.width.is_finite() && self.width > 0.0) {
            self.width = default_width();
        }
        self.width = self.width.min(40.0);
        if !self.fill_opacity.is_finite() {
            self.fill_opacity = default_opacity();
        }
        self.fill_opacity = self.fill_opacity.clamp(0.0, 1.0);
        if !(self.text_size.is_finite() && self.text_size >= 6.0) {
            self.text_size = default_text_size();
        }
        self.text_size = self.text_size.min(48.0);
        self
    }
}

/// The colors the style bar offers.
pub const PALETTE: [u32; 8] = [
    0xffffff, 0x4f8dff, 0x00d492, 0xff6467, 0xffb900, 0xc27aff, 0x00d3f2, 0x9ca3af,
];

/// The widths the style bar offers.
pub const WIDTHS: [f32; 4] = [1.0, 1.5, 2.5, 4.0];

/// One level of a drawing: a ratio (or a multiple), its color, and whether it shows.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub value: f64,
    pub color: u32,
    #[serde(default = "yes")]
    pub visible: bool,
}

const fn level(value: f64, color: u32, visible: bool) -> Level {
    Level {
        value,
        color,
        visible,
    }
}

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

/// How the points of an Elliott wave are named, from the largest degree to the smallest.
pub const DEGREES: [&str; 7] = [
    "Supercycle",
    "Cycle",
    "Primary",
    "Intermediate",
    "Minor",
    "Minute",
    "Minuette",
];

/// The degree new waves start with: Minor, plain `1 2 3` and `A B C`.
pub const DEFAULT_DEGREE: u8 = 4;

/// The name of wave `name` (a digit or a capital letter) at `degree`.
pub fn wave_label(name: &str, degree: u8) -> String {
    let lower = name.to_ascii_lowercase();
    let roman = |text: &str, upper: bool| -> String {
        let r = match text {
            "1" => "i",
            "2" => "ii",
            "3" => "iii",
            "4" => "iv",
            "5" => "v",
            other => return other.to_owned(),
        };
        if upper {
            r.to_ascii_uppercase()
        } else {
            r.to_owned()
        }
    };
    match degree {
        0 => format!("({})", roman(name, true)),
        1 => roman(name, true),
        2 => format!("(({name}))"),
        3 => format!("({name})"),
        4 => name.to_owned(),
        5 => roman(&lower, false),
        _ => format!("({})", roman(&lower, false)),
    }
}

/// A look (and levels) saved as what a tool starts with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub levels: Vec<Level>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
    pub id: u64,
    pub tool: Tool,
    pub points: Vec<Point>,
    #[serde(default)]
    pub style: Style,
    /// The words of a text drawing.
    #[serde(default)]
    pub text: String,
    /// A locked drawing can be selected but not moved, edited or deleted by mistake.
    #[serde(default)]
    pub locked: bool,
    /// A hidden drawing is kept but not shown.
    #[serde(default)]
    pub hidden: bool,
    /// The levels, when the tool has some; empty means the tool's own.
    #[serde(default)]
    pub levels: Vec<Level>,
    /// The timeframes (by code) the drawing shows on; `None` for every one.
    #[serde(default)]
    pub timeframes: Option<Vec<String>>,
    /// A name the user gave it, for the list of drawings.
    #[serde(default)]
    pub name: String,
    /// Whether the levels run the other way.
    #[serde(default)]
    pub reverse: bool,
    /// The degree of an Elliott wave, an index in [`DEGREES`].
    #[serde(default = "default_degree")]
    pub degree: u8,
}

fn default_degree() -> u8 {
    DEFAULT_DEGREE
}

impl Drawing {
    /// A new drawing of `tool` with its default look.
    pub fn new(id: u64, tool: Tool, points: Vec<Point>) -> Self {
        Self {
            id,
            tool,
            points,
            style: tool.default_style(),
            text: String::new(),
            locked: false,
            hidden: false,
            levels: Vec::new(),
            timeframes: None,
            name: String::new(),
            reverse: false,
            degree: DEFAULT_DEGREE,
        }
    }

    /// Whether the drawing shows on a chart of this timeframe (by its saved code).
    pub fn shows_on(&self, timeframe: &str) -> bool {
        !self.hidden
            && self
                .timeframes
                .as_ref()
                .is_none_or(|list| list.iter().any(|code| code == timeframe))
    }

    /// The levels the drawing draws: its own, or the tool's.
    pub fn levels(&self) -> Vec<Level> {
        if self.levels.is_empty() {
            self.tool.default_levels()
        } else {
            self.levels.clone()
        }
    }

    /// What the list of drawings calls it.
    pub fn title(&self) -> String {
        if !self.name.trim().is_empty() {
            self.name.trim().to_owned()
        } else if self.tool == Tool::Text && !self.text.trim().is_empty() {
            let text: String = self.text.trim().chars().take(24).collect();
            format!("Text: {text}")
        } else {
            self.tool.label().to_owned()
        }
    }

    /// Whether the drawing is well formed for its tool: the right number of finite points.
    pub fn is_valid(&self) -> bool {
        let count_ok = match self.tool {
            Tool::Unknown => false,
            Tool::Brush | Tool::Highlighter => (2..=MAX_BRUSH_POINTS).contains(&self.points.len()),
            Tool::ArrowPath => (2..=MAX_PATH_POINTS).contains(&self.points.len()),
            tool => self.points.len() == tool.anchors(),
        };
        count_ok
            && self.points.iter().all(|p| p.p.is_finite())
            && self.style.width.is_finite()
            && self.style.width > 0.0
    }

    /// The drawing with its style in range and its levels sane.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.style = self.style.normalized();
        self.levels.retain(|l| l.value.is_finite());
        self.levels.truncate(MAX_LEVELS);
        for level in &mut self.levels {
            level.color &= 0xff_ffff;
        }
        if !self.tool.has_levels() {
            self.levels.clear();
        }
        self.degree = self.degree.min(DEGREES.len() as u8 - 1);
        if let Some(list) = &mut self.timeframes {
            list.sort();
            list.dedup();
        }
        self
    }
}

/// The saved document: every symbol's drawings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingsDoc {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    /// The next id to give, so ids stay unique across runs.
    #[serde(default)]
    pub next_id: u64,
    #[serde(default)]
    pub symbols: BTreeMap<String, Vec<Drawing>>,
    /// What each tool starts with, by the tool's code, when the user saved a look as default.
    #[serde(default)]
    pub templates: BTreeMap<String, Template>,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for DrawingsDoc {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            next_id: 1,
            symbols: BTreeMap::new(),
            templates: BTreeMap::new(),
        }
    }
}

impl DrawingsDoc {
    /// The document repaired: malformed drawings and empty symbols dropped, ids made unique, and
    /// `next_id` past every id in use.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.schema_version = SCHEMA_VERSION;
        let mut seen = std::collections::HashSet::new();
        let mut highest = 0;
        for drawings in self.symbols.values_mut() {
            *drawings = std::mem::take(drawings)
                .into_iter()
                .filter(Drawing::is_valid)
                .take(MAX_DRAWINGS_PER_SYMBOL)
                .map(Drawing::normalized)
                .collect();
            for drawing in drawings.iter_mut() {
                if drawing.id == 0 || !seen.insert(drawing.id) {
                    // A repeated or missing id gets a fresh one below.
                    drawing.id = 0;
                }
                highest = highest.max(drawing.id);
            }
        }
        let mut next = self.next_id.max(highest + 1).max(1);
        for drawings in self.symbols.values_mut() {
            for drawing in drawings.iter_mut() {
                if drawing.id == 0 {
                    drawing.id = next;
                    next += 1;
                }
            }
        }
        self.next_id = next;
        self.symbols.retain(|_, drawings| !drawings.is_empty());
        let known: std::collections::HashSet<String> = Tool::ALL.iter().map(|t| t.code()).collect();
        self.templates.retain(|code, _| known.contains(code));
        for template in self.templates.values_mut() {
            template.style = template.style.clone().normalized();
            template.levels.retain(|l| l.value.is_finite());
            template.levels.truncate(MAX_LEVELS);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawing(id: u64, tool: Tool, count: usize) -> Drawing {
        Drawing::new(
            id,
            tool,
            (0..count)
                .map(|i| Point {
                    t: i as i64 * 60_000,
                    p: 100.0 + i as f64,
                })
                .collect(),
        )
    }

    #[test]
    fn every_tool_has_a_group_a_label_and_a_positive_anchor_count() {
        for tool in Tool::ALL {
            assert!(!tool.label().is_empty());
            assert!(tool.anchors() >= 1, "{tool:?}");
            let _ = tool.group();
        }
        let mut labels: Vec<_> = Tool::ALL.iter().map(|t| t.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), Tool::ALL.len());
    }

    #[test]
    fn a_drawing_needs_the_points_its_tool_asks_for() {
        assert!(drawing(1, Tool::TrendLine, 2).is_valid());
        assert!(!drawing(1, Tool::TrendLine, 1).is_valid());
        assert!(drawing(1, Tool::LongPosition, 4).is_valid());
        assert!(!drawing(1, Tool::LongPosition, 3).is_valid());
        assert!(drawing(1, Tool::Brush, 40).is_valid());
        assert!(!drawing(1, Tool::Brush, 1).is_valid());
        assert!(!drawing(1, Tool::Unknown, 2).is_valid());
        let mut nan = drawing(1, Tool::HorizontalLine, 1);
        nan.points[0].p = f64::NAN;
        assert!(!nan.is_valid());
    }

    #[test]
    fn a_saved_document_reads_back_and_unknown_tools_do_not_break_it() {
        let mut doc = DrawingsDoc::default();
        doc.symbols.insert(
            "US100.cash".into(),
            vec![drawing(1, Tool::FibRetracement, 2)],
        );
        doc.next_id = 2;
        let text = toml::to_string_pretty(&doc).unwrap();
        let back: DrawingsDoc = toml::from_str(&text).unwrap();
        assert_eq!(back, doc);

        let with_future_tool = r#"
            [[symbols."EURUSD"]]
            id = 5
            tool = "future_tool"
            points = [{ t = 1, p = 1.0 }, { t = 2, p = 2.0 }]
            [[symbols."EURUSD"]]
            id = 6
            tool = "trend_line"
            points = [{ t = 1, p = 1.0 }, { t = 2, p = 2.0 }]
        "#;
        let parsed: DrawingsDoc = toml::from_str(with_future_tool).unwrap();
        let parsed = parsed.normalized();
        let kept = &parsed.symbols["EURUSD"];
        assert_eq!(
            kept.len(),
            1,
            "the tool this version does not know is dropped"
        );
        assert_eq!(kept[0].tool, Tool::TrendLine);
        assert!(parsed.next_id > 6);
    }

    #[test]
    fn normalizing_repairs_ids_and_drops_the_broken() {
        let mut doc = DrawingsDoc::default();
        doc.symbols.insert(
            "A".into(),
            vec![
                drawing(3, Tool::Rectangle, 2),
                drawing(3, Tool::Ellipse, 2),
                drawing(0, Tool::Ray, 2),
                drawing(9, Tool::Ray, 1),
            ],
        );
        doc.symbols.insert("Empty".into(), vec![]);
        doc.next_id = 1;
        let doc = doc.normalized();
        let ids: Vec<u64> = doc.symbols["A"].iter().map(|d| d.id).collect();
        assert_eq!(ids.len(), 3, "the one with too few points is gone");
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 3, "ids are unique: {ids:?}");
        assert!(doc.next_id > *ids.iter().max().unwrap());
        assert!(!doc.symbols.contains_key("Empty"));
    }

    #[test]
    fn an_empty_or_partial_file_still_loads() {
        let doc: DrawingsDoc = toml::from_str("").unwrap();
        assert!(doc.symbols.is_empty());
        let one: Drawing =
            toml::from_str("id = 1\ntool = \"text\"\npoints = [{ t = 5, p = 2.5 }]\n").unwrap();
        assert_eq!(one.style, Style::default());
        assert!(!one.locked);
        assert!(one.is_valid());
    }

    #[test]
    fn a_drawing_shows_on_the_timeframes_it_is_kept_for() {
        let mut d = drawing(1, Tool::TrendLine, 2);
        assert!(d.shows_on("M5"));
        d.timeframes = Some(vec!["H1".into(), "D1".into()]);
        assert!(!d.shows_on("M5"));
        assert!(d.shows_on("H1"));
        d.hidden = true;
        assert!(!d.shows_on("H1"));
    }

    #[test]
    fn levels_default_to_the_tools_and_are_kept_sane() {
        let mut d = drawing(1, Tool::FibRetracement, 2);
        assert_eq!(d.levels(), Tool::FibRetracement.default_levels());
        d.levels = vec![Level {
            value: f64::NAN,
            color: 0xff00_0000,
            visible: true,
        }];
        let d = d.normalized();
        assert!(d.levels.is_empty(), "the broken level is dropped");
        let mut line = drawing(2, Tool::TrendLine, 2);
        line.levels = vec![Level {
            value: 1.0,
            color: 0,
            visible: true,
        }];
        assert!(line.normalized().levels.is_empty(), "a line has no levels");
    }

    #[test]
    fn every_tool_has_a_code_and_templates_of_unknown_tools_are_dropped() {
        assert_eq!(Tool::HeadAndShoulders.code(), "head_and_shoulders");
        let mut doc = DrawingsDoc::default();
        doc.templates.insert(
            "gann_fan".into(),
            Template {
                style: Style {
                    width: -3.0,
                    ..Style::default()
                },
                levels: Vec::new(),
            },
        );
        doc.templates.insert(
            "nothing".into(),
            Template {
                style: Style::default(),
                levels: Vec::new(),
            },
        );
        let text = toml::to_string_pretty(&doc).unwrap();
        let back: DrawingsDoc = toml::from_str(&text).unwrap();
        let back = back.normalized();
        assert_eq!(back.templates.len(), 1);
        assert_eq!(back.templates["gann_fan"].style.width, 1.5);
    }

    #[test]
    fn waves_are_named_by_their_degree() {
        assert_eq!(wave_label("3", DEFAULT_DEGREE), "3");
        assert_eq!(wave_label("3", 0), "(III)");
        assert_eq!(wave_label("4", 1), "IV");
        assert_eq!(wave_label("B", 2), "((B))");
        assert_eq!(wave_label("5", 5), "v");
        assert_eq!(wave_label("C", 6), "(c)");
    }
}
