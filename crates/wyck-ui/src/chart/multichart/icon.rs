//! The small drawing of a layout, for the picker and for the header button.

use gpui::prelude::*;
use gpui::{div, px};

use super::layouts::Layout;
use crate::kit::theme;
use wyck_domain::base::math::u32_to_f32;

/// The corner of one chart in the icon of a layout.
const CELL_RADIUS: f32 = 1.5;
/// The corner of the icon of a layout.
const ICON_RADIUS: f32 = 3.0;

/// A layout drawn as its cells. A selected one is filled, so it stands out in the picker.
pub fn layout_icon(layout: &Layout, width: f32, height: f32, selected: bool) -> impl IntoElement {
    let (cols, rows) = (u32_to_f32(layout.cols), u32_to_f32(layout.rows));
    let (ink, paper) = if selected {
        (theme::bg(), Some(theme::fg()))
    } else {
        (theme::fg(), None)
    };
    let cells = layout.cells.iter().map(|cell| {
        let cell_w = width / cols;
        let cell_h = height / rows;
        div()
            .absolute()
            .left(px(u32_to_f32(cell.x) * cell_w + 1.0))
            .top(px(u32_to_f32(cell.y) * cell_h + 1.0))
            .w(px(u32_to_f32(cell.w) * cell_w - 2.0))
            .h(px(u32_to_f32(cell.h) * cell_h - 2.0))
            .border_1()
            .border_color(ink)
            .rounded(px(CELL_RADIUS))
    });
    div()
        .relative()
        .flex_none()
        .w(px(width))
        .h(px(height))
        .rounded(px(ICON_RADIUS))
        .when_some(paper, Styled::bg)
        .children(cells)
}
