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

/// A place on the chart: a time in Unix milliseconds and a raw price.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub t: i64,
    pub p: f64,
}

/// The kinds of drawing. The names are written in saved files and never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    FibRetracement,
    FibExtension,
    Rectangle,
    Ellipse,
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
    Shapes,
    Measure,
    Annotations,
}

impl Group {
    pub const ALL: [Self; 6] = [
        Self::Lines,
        Self::Channels,
        Self::Fibonacci,
        Self::Shapes,
        Self::Measure,
        Self::Annotations,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Lines => "Lines",
            Self::Channels => "Channels",
            Self::Fibonacci => "Fibonacci",
            Self::Shapes => "Shapes",
            Self::Measure => "Measure and position",
            Self::Annotations => "Annotations",
        }
    }
}

impl Tool {
    /// Every tool a user can pick, in toolbar order.
    pub const ALL: [Self; 19] = [
        Self::TrendLine,
        Self::Ray,
        Self::ExtendedLine,
        Self::HorizontalLine,
        Self::HorizontalRay,
        Self::VerticalLine,
        Self::CrossLine,
        Self::Arrow,
        Self::ParallelChannel,
        Self::FibRetracement,
        Self::FibExtension,
        Self::Rectangle,
        Self::Ellipse,
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
            Self::FibRetracement => "Fib retracement",
            Self::FibExtension => "Trend-based fib extension",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Brush => "Brush",
            Self::Measure => "Measure",
            Self::LongPosition => "Long position",
            Self::ShortPosition => "Short position",
            Self::Text => "Text",
            Self::PriceLabel => "Price label",
            Self::Unknown => "Unknown",
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
            Self::ParallelChannel => Group::Channels,
            Self::FibRetracement | Self::FibExtension => Group::Fibonacci,
            Self::Rectangle | Self::Ellipse | Self::Brush => Group::Shapes,
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
            | Self::Rectangle
            | Self::Ellipse
            | Self::Measure
            | Self::Brush => 2,
            Self::ParallelChannel | Self::FibExtension => 3,
            Self::LongPosition | Self::ShortPosition => 4,
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

    /// Whether the drawing has text the user can edit.
    pub fn has_text(self) -> bool {
        matches!(self, Self::Text)
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

/// How a drawing looks. The color is `0xRRGGBB`.
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
}

fn default_color() -> u32 {
    0x4f8dff
}

fn default_width() -> f32 {
    1.5
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
        }
    }
}

/// The colors the style bar offers.
pub const PALETTE: [u32; 8] = [
    0xffffff, 0x4f8dff, 0x00d492, 0xff6467, 0xffb900, 0xc27aff, 0x00d3f2, 0x9ca3af,
];

/// The widths the style bar offers.
pub const WIDTHS: [f32; 4] = [1.0, 1.5, 2.5, 4.0];

impl Tool {
    /// The look a new drawing of this tool starts with.
    pub fn default_style(self) -> Style {
        let (color, width) = match self {
            Self::FibRetracement | Self::FibExtension => (0xffb900, 1.0),
            Self::Measure => (0x4f8dff, 1.0),
            Self::LongPosition => (0x00d492, 1.5),
            Self::ShortPosition => (0xff6467, 1.5),
            Self::Text | Self::PriceLabel => (0xffffff, 1.0),
            Self::Rectangle | Self::Ellipse | Self::ParallelChannel => (0x4f8dff, 1.5),
            Self::HorizontalLine | Self::HorizontalRay | Self::VerticalLine | Self::CrossLine => {
                (0xffb900, 1.0)
            }
            _ => (0x4f8dff, 1.5),
        };
        Style {
            color,
            width,
            ..Style::default()
        }
    }
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
}

impl Drawing {
    /// Whether the drawing shows on a chart of this timeframe (by its saved code).
    pub fn shows_on(&self, _timeframe: &str) -> bool {
        !self.hidden
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
            drawings.retain(Drawing::is_valid);
            drawings.truncate(MAX_DRAWINGS_PER_SYMBOL);
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
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawing(id: u64, tool: Tool, count: usize) -> Drawing {
        Drawing {
            id,
            tool,
            points: (0..count)
                .map(|i| Point {
                    t: i as i64 * 60_000,
                    p: 100.0 + i as f64,
                })
                .collect(),
            style: tool.default_style(),
            text: String::new(),
            locked: false,
            hidden: false,
        }
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
            tool = "gann_fan"
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
}
