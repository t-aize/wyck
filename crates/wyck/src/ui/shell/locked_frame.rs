//! The frame of the dashboard drawn as soft blocks, shown behind the sign-in modal.
//!
//! It holds no data and does nothing: it only lets the person see, under the veil, the shape of
//! the app they are about to open.

use gpui::prelude::*;
use gpui::{Div, div, px};

use crate::ui::kit::{theme, tokens};

fn block() -> Div {
    div().rounded_md().bg(theme::border_subtle())
}

fn rows(count: usize) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_3()
        .children((0..count).map(|_| block().h_4().w_full()))
}

/// The header, a rail of tools, a chart area, a watchlist and a panel below, as blocks.
pub fn render() -> impl IntoElement {
    let header = div()
        .flex()
        .flex_none()
        .items_center()
        .gap_4()
        .px_4()
        .h(px(tokens::bar::header()))
        .bg(theme::surface())
        .border_b_1()
        .border_color(theme::border_hairline())
        .child(block().size_6())
        .child(block().h_4().w_24())
        .child(block().h_6().w_40())
        .child(div().flex_1())
        .child(block().h_6().w_32());

    let rail = div()
        .flex()
        .flex_col()
        .flex_none()
        .items_center()
        .gap_3()
        .py_3()
        .w_12()
        .bg(theme::surface())
        .border_r_1()
        .border_color(theme::border_hairline())
        .children((0..8).map(|_| block().size_5()));

    let chart = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(div().flex().items_end().flex_1().gap_2().p_6().children(
            [8, 14, 10, 18, 12, 20, 16, 24, 14, 22, 18, 26].map(|h| {
                div()
                    .flex_1()
                    .h(px(tokens::space::xl() * h as f32 / 4.0))
                    .rounded_sm()
                    .bg(theme::border_subtle())
            }),
        ))
        .child(
            div()
                .h_8()
                .border_t_1()
                .border_color(theme::border_hairline()),
        );

    let watchlist = rows(10)
        .flex_none()
        .w_64()
        .bg(theme::surface())
        .border_l_1()
        .border_color(theme::border_hairline());

    let panel = rows(3)
        .flex_none()
        .h_40()
        .bg(theme::surface())
        .border_t_1()
        .border_color(theme::border_hairline());

    div()
        .flex()
        .flex_col()
        .size_full()
        .bg(theme::bg())
        .child(header)
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(rail)
                .child(chart)
                .child(watchlist),
        )
        .child(panel)
}
