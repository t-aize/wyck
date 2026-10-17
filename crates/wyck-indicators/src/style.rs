//! How the plots, levels and fills of an indicator look, as the user set them.

use serde::{Deserialize, Serialize};

use super::spec::PlotKind;
use wyck_drawings::model::style::Dash;

/// How one plot of an indicator looks.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlotStyle {
    pub color: u32,
    pub width: f32,
    #[serde(default = "wyck_core::defaults::yes")]
    pub visible: bool,
    /// How opaque the plot is, from 0 to 1.
    #[serde(default = "opaque")]
    pub opacity: f32,
    #[serde(default)]
    pub dash: Dash,
    /// Replace the default presentation of a numeric series.
    #[serde(default)]
    pub kind: Option<PlotKind>,
    #[serde(default)]
    pub up_color: Option<u32>,
    #[serde(default)]
    pub down_color: Option<u32>,
    /// Fraction of a bar occupied by a histogram column.
    #[serde(default = "default_bar_width")]
    pub bar_width: f32,
}

pub(super) fn default_bar_width() -> f32 {
    0.6
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LevelStyle {
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default = "default_level_color")]
    pub color: u32,
    #[serde(default = "opaque")]
    pub opacity: f32,
    #[serde(default = "default_level_width")]
    pub width: f32,
    #[serde(default = "default_level_dash")]
    pub dash: Dash,
    #[serde(default = "wyck_core::defaults::yes")]
    pub visible: bool,
}

fn default_level_color() -> u32 {
    0x78_7b86
}
fn default_level_width() -> f32 {
    1.0
}
fn default_level_dash() -> Dash {
    Dash::Dashed
}

impl Default for LevelStyle {
    fn default() -> Self {
        Self {
            value: None,
            color: default_level_color(),
            opacity: 0.55,
            width: default_level_width(),
            dash: default_level_dash(),
            visible: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FillStyle {
    pub color: u32,
    #[serde(default)]
    pub other: Option<u32>,
    #[serde(default = "default_fill_opacity")]
    pub opacity: f32,
    #[serde(default = "wyck_core::defaults::yes")]
    pub visible: bool,
}

pub(super) fn default_fill_opacity() -> f32 {
    0.08
}

pub(super) fn opaque() -> f32 {
    1.0
}
