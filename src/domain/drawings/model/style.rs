//! How a drawing is drawn: its line, fill and text, and the levels of the tools that have some.

use crate::domain::drawings::look::{
    Caps, Face, LabelSide, LevelText, MeasureLook, ProfileLook, TextLayout, is_default,
};
use crate::domain::drawings::position::PositionSettings;
use serde::{Deserialize, Serialize};

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
    /// How opaque the lines are, from 0 to 1. The fill and the words have their own.
    #[serde(default = "default_line_opacity")]
    pub opacity: f32,
    /// Whether closed shapes are filled.
    #[serde(default = "crate::domain::chart::defaults::yes")]
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
    /// Whether the words lean.
    #[serde(default, skip_serializing_if = "is_default")]
    pub italic: bool,
    /// The font of the words, by the name the system knows it under; the font of the interface
    /// when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    /// Whether the levels and prices are written.
    #[serde(default = "crate::domain::chart::defaults::yes")]
    pub labels: bool,
    /// Whether the extra line of the tool shows (see [`super::tool::Tool::middle_label`]).
    #[serde(default = "crate::domain::chart::defaults::yes")]
    pub middle: bool,
    /// What a long or short position is sized and told with. Kept in one piece, and left out of
    /// the saved form when it is the default, so the other tools carry nothing of it.
    #[serde(default, skip_serializing_if = "PositionSettings::is_default")]
    pub position: PositionSettings,
    /// Where the label of a line or a shape sits, and what it sits on.
    #[serde(default, skip_serializing_if = "is_default")]
    pub text_layout: TextLayout,
    /// What ends the two sides of a line.
    #[serde(default, skip_serializing_if = "is_default")]
    pub caps: Caps,
    /// What the measuring tools write.
    #[serde(default, skip_serializing_if = "is_default")]
    pub measure: MeasureLook,
    /// How a volume profile is cut.
    #[serde(default, skip_serializing_if = "is_default")]
    pub profile: ProfileLook,
    /// What a level is called.
    #[serde(default, skip_serializing_if = "is_default")]
    pub level_text: LevelText,
    /// The side the labels of the levels stand on.
    #[serde(default, skip_serializing_if = "is_default")]
    pub label_side: LabelSide,
    /// How big a marker is against its usual size.
    #[serde(default = "default_scale", skip_serializing_if = "is_unit_scale")]
    pub scale: f32,
}

/// The least opacity a line can have: below it a drawing is there but cannot be seen or picked.
pub const MIN_LINE_OPACITY: f32 = 0.05;

fn default_color() -> u32 {
    0x4f8dff
}

fn default_width() -> f32 {
    1.5
}

pub(super) fn default_opacity() -> f32 {
    0.15
}

fn default_line_opacity() -> f32 {
    1.0
}

fn default_scale() -> f32 {
    1.0
}

fn is_unit_scale(scale: &f32) -> bool {
    (*scale - 1.0).abs() < 1e-6
}

fn default_text_size() -> f32 {
    12.0
}

impl Style {
    /// The letterforms of the words.
    pub fn face(&self) -> Face {
        Face {
            italic: self.italic,
            family: self
                .font
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .map(|name| crate::domain::indicators::intern::name(name.trim())),
        }
    }
}

impl Default for Style {
    fn default() -> Self {
        Self {
            color: default_color(),
            width: default_width(),
            dash: Dash::Solid,
            opacity: default_line_opacity(),
            fill: true,
            fill_color: None,
            fill_opacity: default_opacity(),
            extend_left: false,
            extend_right: false,
            text_size: default_text_size(),
            text_color: None,
            bold: false,
            italic: false,
            font: None,
            labels: true,
            middle: true,
            position: PositionSettings::default(),
            text_layout: TextLayout::default(),
            caps: Caps::default(),
            measure: MeasureLook::default(),
            profile: ProfileLook::default(),
            level_text: LevelText::default(),
            label_side: LabelSide::default(),
            scale: default_scale(),
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
        if !self.opacity.is_finite() {
            self.opacity = default_line_opacity();
        }
        self.opacity = self.opacity.clamp(MIN_LINE_OPACITY, 1.0);
        if !self.fill_opacity.is_finite() {
            self.fill_opacity = default_opacity();
        }
        self.fill_opacity = self.fill_opacity.clamp(0.0, 1.0);
        if !(self.text_size.is_finite() && self.text_size >= 6.0) {
            self.text_size = default_text_size();
        }
        self.text_size = self.text_size.min(48.0);
        self.position = std::mem::take(&mut self.position).normalized();
        self.profile = self.profile.normalized();
        self.scale = if self.scale.is_finite() {
            self.scale.clamp(0.3, 5.0)
        } else {
            default_scale()
        };
        self
    }
}

/// The colors the style bar offers.
pub const PALETTE: [u32; 8] = [
    0xffffff, 0x4f8dff, 0x00d492, 0xff6467, 0xffb900, 0xc27aff, 0x00d3f2, 0x9ca3af,
];

/// The widths the style bar and the settings panels offer.
pub const WIDTHS: [f32; 4] = [1.0, 1.5, 2.5, 4.0];

/// The line styles the pickers offer, in the order of their samples: solid, dashed, dotted.
pub const DASHES: [Dash; 3] = [Dash::Solid, Dash::Dashed, Dash::Dotted];

/// One level of a drawing: a ratio (or a multiple), its color, and whether it shows.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub value: f64,
    pub color: u32,
    #[serde(default = "crate::domain::chart::defaults::yes")]
    pub visible: bool,
    /// The width of its line; 0 is the width of the drawing.
    #[serde(default, skip_serializing_if = "is_default")]
    pub width: f32,
    /// The style of its line; `None` is the style of the drawing.
    #[serde(default, skip_serializing_if = "is_default")]
    pub dash: Option<Dash>,
}

impl Default for Level {
    fn default() -> Self {
        level(0.0, 0x9ca3af, true)
    }
}

impl Level {
    /// The width of the line of this level, given the width of the drawing.
    pub fn line_width(&self, base: f32) -> f32 {
        if self.width > 0.0 { self.width } else { base }
    }

    /// The style of the line of this level, given the style of the drawing.
    pub fn line_dash(&self, base: Dash) -> Dash {
        self.dash.unwrap_or(base)
    }
}

pub(super) const fn level(value: f64, color: u32, visible: bool) -> Level {
    Level {
        value,
        color,
        visible,
        width: 0.0,
        dash: None,
    }
}
