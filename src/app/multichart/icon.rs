//! The small drawing of a layout, for the picker and for the header button.

use gpui::prelude::*;
use gpui::{div, px};

use super::layouts::Layout;
use crate::app::theme;

/// A layout drawn as its cells. A selected one is filled, so it stands out in the picker.
pub fn layout_icon(layout: &Layout, width: f32, height: f32, selected: bool) -> impl IntoElement {
    let (cols, rows) = (layout.cols as f32, layout.rows as f32);
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
            .left(px(cell.x as f32 * cell_w + 1.0))
            .top(px(cell.y as f32 * cell_h + 1.0))
            .w(px(cell.w as f32 * cell_w - 2.0))
            .h(px(cell.h as f32 * cell_h - 2.0))
            .border_1()
            .border_color(ink)
            .rounded(px(1.5))
    });
    div()
        .relative()
        .flex_none()
        .w(px(width))
        .h(px(height))
        .rounded(px(3.))
        .when_some(paper, |el, paper| el.bg(paper))
        .children(cells)
}
