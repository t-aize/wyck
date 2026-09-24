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
    ParallelChannel,
    Pitchfork,
    SchiffPitchfork,
    ModifiedSchiffPitchfork,
    InsidePitchfork,
    FibRetracement,
    FibExtension,
    FibChannel,
    FibTimeZone,
    FibFan,
    GannFan,
    GannBox,
    GannSquare,
    Xabcd,
    Cypher,
    Abcd,
    HeadAndShoulders,
    TrianglePattern,
    ThreeDrives,
    ElliottImpulse,
    ElliottCorrection,
    ElliottTriangle,
    ElliottDoubleCombo,
    ElliottTripleCombo,
    Rectangle,
    Ellipse,
    Triangle,
    Brush,
    Measure,
    LongPosition,
    ShortPosition,
    Text,
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
    Annotations,
}

impl Group {
    pub const ALL: [Self; 9] = [
        Self::Lines,
        Self::Channels,
        Self::Fibonacci,
        Self::Gann,
        Self::Patterns,
        Self::Elliott,
        Self::Shapes,
        Self::Measure,
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
            Self::Annotations => "Annotations",
        }
    }
}

impl Tool {
    /// Every tool a user can pick, in toolbar order.
    pub const ALL: [Self; 41] = [
        Self::TrendLine,
        Self::Ray,
        Self::ExtendedLine,
        Self::HorizontalLine,
        Self::HorizontalRay,
        Self::VerticalLine,
        Self::CrossLine,
        Self::Arrow,
        Self::ParallelChannel,
        Self::Pitchfork,
        Self::SchiffPitchfork,
        Self::ModifiedSchiffPitchfork,
        Self::InsidePitchfork,
        Self::FibRetracement,
        Self::FibExtension,
        Self::FibChannel,
        Self::FibTimeZone,
        Self::FibFan,
        Self::GannFan,
        Self::GannBox,
        Self::GannSquare,
        Self::Xabcd,
        Self::Cypher,
        Self::Abcd,
        Self::HeadAndShoulders,
        Self::TrianglePattern,
        Self::ThreeDrives,
        Self::ElliottImpulse,
        Self::ElliottCorrection,
        Self::ElliottTriangle,
        Self::ElliottDoubleCombo,
        Self::ElliottTripleCombo,
        Self::Rectangle,
        Self::Ellipse,
        Self::Triangle,
        Self::Brush,
        Self::Measure,
        Self::LongPosition,
        Self::ShortPosition,
        Self::Text,
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
            Self::ParallelChannel => "Parallel channel",
            Self::Pitchfork => "Pitchfork",
            Self::SchiffPitchfork => "Schiff pitchfork",
            Self::ModifiedSchiffPitchfork => "Modified Schiff pitchfork",
            Self::InsidePitchfork => "Inside pitchfork",
            Self::FibRetracement => "Fib retracement",
            Self::FibExtension => "Trend-based fib extension",
            Self::FibChannel => "Fib channel",
            Self::FibTimeZone => "Fib time zone",
            Self::FibFan => "Fib speed fan",
            Self::GannFan => "Gann fan",
            Self::GannBox => "Gann box",
            Self::GannSquare => "Gann square",
            Self::Xabcd => "XABCD pattern",
            Self::Cypher => "Cypher pattern",
            Self::Abcd => "ABCD pattern",
            Self::HeadAndShoulders => "Head and shoulders",
            Self::TrianglePattern => "Triangle pattern",
            Self::ThreeDrives => "Three drives pattern",
            Self::ElliottImpulse => "Elliott impulse wave (12345)",
            Self::ElliottCorrection => "Elliott correction wave (ABC)",
            Self::ElliottTriangle => "Elliott triangle wave (ABCDE)",
            Self::ElliottDoubleCombo => "Elliott double combo (WXY)",
            Self::ElliottTripleCombo => "Elliott triple combo (WXYXZ)",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Triangle => "Triangle",
            Self::Brush => "Brush",
            Self::Measure => "Measure",
            Self::LongPosition => "Long position",
            Self::ShortPosition => "Short position",
            Self::Text => "Text",
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
            | Self::Arrow => Group::Lines,
            Self::ParallelChannel
            | Self::Pitchfork
            | Self::SchiffPitchfork
            | Self::ModifiedSchiffPitchfork
            | Self::InsidePitchfork => Group::Channels,
            Self::FibRetracement
            | Self::FibExtension
            | Self::FibChannel
            | Self::FibTimeZone
            | Self::FibFan => Group::Fibonacci,
            Self::GannFan | Self::GannBox | Self::GannSquare => Group::Gann,
            Self::Xabcd
            | Self::Cypher
            | Self::Abcd
            | Self::HeadAndShoulders
            | Self::TrianglePattern
            | Self::ThreeDrives => Group::Patterns,
            Self::ElliottImpulse
            | Self::ElliottCorrection
            | Self::ElliottTriangle
            | Self::ElliottDoubleCombo
            | Self::ElliottTripleCombo => Group::Elliott,
            Self::Rectangle | Self::Ellipse | Self::Triangle | Self::Brush => Group::Shapes,
            Self::Measure | Self::LongPosition | Self::ShortPosition => Group::Measure,
            Self::Text | Self::PriceLabel | Self::Unknown => Group::Annotations,
        }
    }

    /// How many points the tool needs. A brush takes as many as the stroke has, at least two.
    pub fn anchors(self) -> usize {
        match self {
            Self::HorizontalLine
            | Self::HorizontalRay
            | Self::VerticalLine
            | Self::CrossLine
            | Self::Text
            | Self::PriceLabel => 1,
            Self::TrendLine
            | Self::Ray
            | Self::ExtendedLine
            | Self::Arrow
            | Self::FibRetracement
            | Self::FibTimeZone
            | Self::FibFan
            | Self::GannFan
            | Self::GannBox
            | Self::GannSquare
            | Self::Rectangle
            | Self::Ellipse
            | Self::Measure
            | Self::Brush => 2,
            Self::ParallelChannel
            | Self::Pitchfork
            | Self::SchiffPitchfork
            | Self::ModifiedSchiffPitchfork
            | Self::InsidePitchfork
            | Self::FibExtension
            | Self::FibChannel
            | Self::Triangle => 3,
            Self::Abcd
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
        matches!(
            self,
            Self::HorizontalLine
                | Self::HorizontalRay
                | Self::VerticalLine
                | Self::CrossLine
                | Self::Text
                | Self::PriceLabel
                | Self::LongPosition
                | Self::ShortPosition
        )
    }

    pub fn is_position(self) -> bool {
        matches!(self, Self::LongPosition | Self::ShortPosition)
    }

    /// Whether the drawing is laid out in the box between its two points, with a grip at every
    /// corner and side.
    pub fn is_box(self) -> bool {
        matches!(
            self,
            Self::Rectangle | Self::Ellipse | Self::GannBox | Self::GannSquare | Self::Measure
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
        matches!(self, Self::Text)
    }

    /// Whether the drawing writes words on the chart (whose size and color can be set).
    pub fn has_words(self) -> bool {
        matches!(self.group(), Group::Patterns | Group::Elliott)
            || matches!(self, Self::Text | Self::PriceLabel)
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
                | Self::ParallelChannel
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
                | Self::FibRetracement
                | Self::FibExtension
                | Self::FibChannel
                | Self::FibFan
                | Self::GannFan
                | Self::GannBox
                | Self::GannSquare
                | Self::Xabcd
                | Self::Cypher
                | Self::Abcd
                | Self::HeadAndShoulders
                | Self::TrianglePattern
                | Self::Rectangle
                | Self::Ellipse
                | Self::Triangle
        ) || self.is_pitchfork()
    }

    /// Whether the line style (solid, dashed, dotted) applies.
    pub fn has_dash(self) -> bool {
        !matches!(self, Self::Text | Self::PriceLabel | Self::Brush)
    }

    /// The name of the drawing's optional extra line, if it has one: the middle of a channel,
    /// the diagonals of a Gann box.
    pub fn middle_label(self) -> Option<&'static str> {
        match self {
            Self::ParallelChannel => Some("Middle line"),
            Self::GannBox | Self::GannSquare => Some("Angles"),
            Self::Xabcd | Self::Cypher | Self::Abcd | Self::ThreeDrives => Some("Ratios"),
            _ => None,
        }
    }

    /// Whether the levels can be drawn the other way round.
    pub fn has_reverse(self) -> bool {
        matches!(
            self,
            Self::FibRetracement | Self::FibChannel | Self::GannBox
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
        self.width = self.width.min(10.0);
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
            | Self::FibFan => (ORANGE, 1.0),
            Self::Measure => (BLUE, 1.0),
            Self::LongPosition => (GREEN, 1.5),
            Self::ShortPosition => (RED, 1.5),
            Self::Text | Self::PriceLabel => (0xffffff, 1.0),
            Self::HorizontalLine | Self::HorizontalRay | Self::VerticalLine | Self::CrossLine => {
                (ORANGE, 1.0)
            }
            Self::GannFan | Self::GannBox | Self::GannSquare => (ORANGE, 1.0),
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
            _ if self.is_pitchfork() => 0.08,
            _ => default_opacity(),
        };
        Style {
            color,
            width,
            extend_right,
            fill_opacity,
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
            Self::GannBox | Self::GannSquare => vec![
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
            Tool::Brush => (2..=MAX_BRUSH_POINTS).contains(&self.points.len()),
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
