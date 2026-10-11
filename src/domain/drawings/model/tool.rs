//! The tools a drawing can be made with, their groups and what each one needs.

use serde::{Deserialize, Serialize};

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

    /// The tool saved under `code`, or `None` when no tool has that name.
    pub fn from_code(code: &str) -> Option<Self> {
        serde_json::from_value::<Self>(serde_json::Value::String(code.to_owned()))
            .ok()
            .filter(|tool| *tool != Self::Unknown)
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

    /// The keys that pick the tool, for the tools drawn most, or `None`.
    pub fn shortcut(self) -> Option<&'static str> {
        match self {
            Self::TrendLine => Some("Alt+T"),
            Self::HorizontalLine => Some("Alt+H"),
            Self::VerticalLine => Some("Alt+V"),
            Self::FibRetracement => Some("Alt+F"),
            Self::Rectangle => Some("Alt+X"),
            Self::Text => Some("Alt+N"),
            Self::Measure => Some("Alt+M"),
            Self::Brush => Some("Alt+P"),
            _ => None,
        }
    }
}
