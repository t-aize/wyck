//! What a tool can do: which properties a drawing of it has and how a click is read.

use super::style::WIDTHS;
use super::tool::{Group, Tool};

impl Tool {
    /// Whether a click places the last point and finishes, without a second click.
    pub fn is_single_click(self) -> bool {
        self.anchors() == 1 || self.is_position()
    }

    pub fn is_position(self) -> bool {
        matches!(self, Self::LongPosition | Self::ShortPosition)
    }

    /// The quick color is useful only when it changes the drawing's visible primary color.
    /// Positions, levels and range measures have separate colors in their settings.
    pub fn has_quick_color(self) -> bool {
        !self.is_position()
            && !self.has_levels()
            && !matches!(
                self,
                Self::Measure | Self::PriceRange | Self::DateRange | Self::DatePriceRange
            )
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

    /// Whether the tool can carry words of its own, as a label on a line or a shape. The tools
    /// that are made of words (a note, a callout) have their own, and the ones that draw a
    /// stroke, a position, a profile or a copy of the bars have nowhere to put any.
    pub fn takes_label(self) -> bool {
        !self.has_text()
            && !self.is_freehand()
            && !self.is_position()
            && !matches!(
                self,
                Self::Icon
                    | Self::Unknown
                    | Self::FixedRangeVolumeProfile
                    | Self::AnchoredVolumeProfile
                    | Self::BarsPattern
                    | Self::GhostFeed
            )
    }

    /// Whether the start of the line can end in an arrow or a dot.
    pub fn has_start_cap(self) -> bool {
        matches!(
            self,
            Self::TrendLine
                | Self::Ray
                | Self::InfoLine
                | Self::TrendAngle
                | Self::Arrow
                | Self::ArrowPath
        )
    }

    /// Whether the far end of the line can end in an arrow or a dot (the others already end in a
    /// head, or go on for ever).
    pub fn has_end_cap(self) -> bool {
        matches!(self, Self::TrendLine | Self::InfoLine | Self::TrendAngle)
    }

    /// Whether the drawing is a marker whose size can be set.
    pub fn has_size(self) -> bool {
        matches!(
            self,
            Self::ArrowMarkUp | Self::ArrowMarkDown | Self::FlagMark | Self::Icon
        )
    }

    /// Whether the tool writes the numbers of a measure (price, percent, bars, time, angle).
    pub fn has_measure_look(self) -> bool {
        matches!(
            self,
            Self::Measure
                | Self::PriceRange
                | Self::DateRange
                | Self::DatePriceRange
                | Self::InfoLine
                | Self::TrendAngle
                | Self::Forecast
        )
    }

    /// Whether the labels of the levels can stand on either side.
    pub fn has_label_side(self) -> bool {
        matches!(self, Self::FibRetracement | Self::FibExtension)
    }

    /// Whether the tool is a volume profile, cut into rows.
    pub fn has_profile(self) -> bool {
        matches!(
            self,
            Self::FixedRangeVolumeProfile | Self::AnchoredVolumeProfile
        )
    }

    /// Whether the drawing labels its points with a wave degree.
    pub fn is_elliott(self) -> bool {
        self.group() == Group::Elliott
    }
}
