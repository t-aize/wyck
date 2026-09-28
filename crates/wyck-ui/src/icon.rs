//! Icons: a Lucide glyph from gpui-kit, sized and tinted.

use gpui::prelude::*;
use gpui::{Rgba, Svg, px, svg};
use gpui_kit::assets::IconName;

use crate::theme;

/// A Lucide icon in the primary text color. An `svg` doesn't inherit color from its parent in
/// this GPUI version, so a different tint is chained on with `.text_color(...)` (or use
/// [`tinted`]).
pub fn plain(name: IconName, size_px: f32) -> Svg {
    tinted(name, size_px, theme::fg())
}

pub fn tinted(name: IconName, size_px: f32, color: Rgba) -> Svg {
    svg()
        .path(name.path())
        .size(px(size_px))
        .flex_shrink_0()
        .text_color(color)
}

/// A small icon, for a mark beside a name.
pub fn small(icon: IconName, color: Rgba) -> gpui::Svg {
    tinted(icon, 13., color)
}
