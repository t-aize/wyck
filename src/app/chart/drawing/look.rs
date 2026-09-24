//! The parts of the look of a drawing beyond its line and fill: the words it carries, what ends
//! its lines, what its levels and measures write, and how a volume profile is cut.
//!
//! Each is a small piece of a [`super::model::Style`], with defaults that draw a drawing the way
//! it was drawn before the piece existed. Every piece is left out of the saved form while it is at
//! its default, so a file only says what was changed, and an old file loads unchanged.

use serde::{Deserialize, Serialize};

/// Whether a value is the default one, for leaving it out of the saved form.
pub fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// Where a label sits along the drawing, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HAlign {
    Start,
    #[default]
    Center,
    End,
}

/// Where a label sits across the drawing: above a line and at the top of a shape, on it, or
/// below and at the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// How the label of a drawing is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct TextLayout {
    #[serde(default)]
    pub align: HAlign,
    #[serde(default)]
    pub valign: VAlign,
    /// Whether the words sit on a filled tag.
    #[serde(default)]
    pub background: bool,
    /// The color of that tag; a dark one when unset.
    #[serde(default)]
    pub background_color: Option<u32>,
}

/// What ends a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cap {
    #[default]
    None,
    Arrow,
    Circle,
}

/// What ends the two sides of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Caps {
    #[serde(default)]
    pub start: Cap,
    #[serde(default)]
    pub end: Cap,
}

/// What a level is called on the chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LevelText {
    /// The ratio, with the price where the tool knows one.
    #[default]
    Auto,
    Ratio,
    Percent,
    Price,
}

impl LevelText {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Ratio, Self::Percent, Self::Price];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Ratio and price",
            Self::Ratio => "Ratio",
            Self::Percent => "Percent",
            Self::Price => "Price",
        }
    }
}

/// The side of a drawing its level labels stand on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelSide {
    #[default]
    Left,
    Right,
}

/// What the tools that measure write, and the color of a move down.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MeasureLook {
    #[serde(default = "yes")]
    pub price: bool,
    #[serde(default = "yes")]
    pub percent: bool,
    #[serde(default = "yes")]
    pub bars: bool,
    #[serde(default = "yes")]
    pub time: bool,
    #[serde(default = "yes")]
    pub angle: bool,
    /// The color of a measure that goes down; the drawing's own color is for one that goes up.
    #[serde(default = "default_down")]
    pub down_color: u32,
}

fn yes() -> bool {
    true
}

fn default_down() -> u32 {
    0xff6467
}

impl Default for MeasureLook {
    fn default() -> Self {
        Self {
            price: true,
            percent: true,
            bars: true,
            time: true,
            angle: true,
            down_color: default_down(),
        }
    }
}

/// How a volume profile is cut.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProfileLook {
    /// How many rows the price range is cut into; 0 lets the height on the screen decide.
    #[serde(default)]
    pub rows: u16,
    /// The share of the volume the value area holds, from 0.1 to 1.
    #[serde(default = "default_value_area")]
    pub value_area: f32,
}

fn default_value_area() -> f32 {
    0.7
}

impl Default for ProfileLook {
    fn default() -> Self {
        Self {
            rows: 0,
            value_area: default_value_area(),
        }
    }
}

/// The least and the most rows a profile can be cut into.
pub const MIN_PROFILE_ROWS: u16 = 8;
pub const MAX_PROFILE_ROWS: u16 = 240;

impl ProfileLook {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        if self.rows != 0 {
            self.rows = self.rows.clamp(MIN_PROFILE_ROWS, MAX_PROFILE_ROWS);
        }
        self.value_area = if self.value_area.is_finite() {
            self.value_area.clamp(0.1, 1.0)
        } else {
            default_value_area()
        };
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_piece_starts_where_drawings_always_were() {
        assert!(is_default(&TextLayout::default()));
        assert!(is_default(&Caps::default()));
        assert!(is_default(&MeasureLook::default()));
        assert!(is_default(&ProfileLook::default()));
        assert!(is_default(&LevelText::default()));
        assert!(is_default(&LabelSide::default()));
        assert_eq!(TextLayout::default().align, HAlign::Center);
        assert_eq!(TextLayout::default().valign, VAlign::Top);
    }

    #[test]
    fn a_file_that_says_nothing_of_them_loads_the_defaults() {
        let layout: TextLayout = toml::from_str("").unwrap();
        assert_eq!(layout, TextLayout::default());
        let look: MeasureLook = toml::from_str("bars = false\n").unwrap();
        assert!(!look.bars && look.price && look.time);
        assert_eq!(look.down_color, 0xff6467);
    }

    #[test]
    fn the_profile_rows_and_value_area_stay_in_range() {
        let wild = ProfileLook {
            rows: 3,
            value_area: 7.0,
        }
        .normalized();
        assert_eq!(wild.rows, MIN_PROFILE_ROWS);
        assert_eq!(wild.value_area, 1.0);
        let auto = ProfileLook {
            rows: 0,
            value_area: f32::NAN,
        }
        .normalized();
        assert_eq!(auto.rows, 0, "0 stays automatic");
        assert_eq!(auto.value_area, 0.7);
        assert_eq!(
            ProfileLook {
                rows: 9_000,
                ..ProfileLook::default()
            }
            .normalized()
            .rows,
            MAX_PROFILE_ROWS
        );
    }
}
