//! How the color panel is drawn: the square, the hue bar, the presets, the fields and the saved colors.

use super::math::{PRESETS, hsv_to_rgb};
use super::{
    AREA_H, AREA_W, CELL, ColorPanel, Drag, GAP, HUE_H, MAX_SAVED, Saved, marker, small_label,
    surface,
};
use crate::kit::focus::Keyboard;
use crate::kit::theme;
use gpui::prelude::*;
use gpui::{
    Context, Entity, MouseButton, SharedString, Window, div, hsla, linear_color_stop,
    linear_gradient, px, rgb,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::Disableable;
use gpui_kit::component::input::InputState;
use std::rc::Rc;
use wyck_domain::base::math::f32_of;

impl Render for ColorPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.fields.is_none() {
            self.fields = Some(self.make_fields(window, cx));
        }
        let color = self.rgb();
        if self.synced != Some(color) {
            self.sync_fields(window, cx);
            self.synced = Some(color);
        }
        let (hue, saturation, value) = self.hsv;
        let this = cx.entity();
        let pure = hsv_to_rgb(hue, 1.0, 1.0);
        let clear_white = hsla(0.0, 0.0, 1.0, 0.0);
        let clear_black = hsla(0.0, 0.0, 0.0, 0.0);

        // The square: the hue, whitened to the left and darkened downwards.
        let area = div()
            .relative()
            .w(px(AREA_W))
            .h(px(AREA_H))
            .rounded_md()
            .overflow_hidden()
            .bg(rgb(pure))
            .child(div().absolute().size_full().bg(linear_gradient(
                90.,
                linear_color_stop(hsla(0.0, 0.0, 1.0, 1.0), 0.),
                linear_color_stop(clear_white, 1.),
            )))
            .child(div().absolute().size_full().bg(linear_gradient(
                180.,
                linear_color_stop(clear_black, 0.),
                linear_color_stop(hsla(0.0, 0.0, 0.0, 1.0), 1.),
            )))
            .child(
                marker(14.0, color)
                    .left(px(saturation * AREA_W - 7.0))
                    .top(px((1.0 - value) * AREA_H - 7.0)),
            )
            .child(surface("color-area", Drag::Area, Rc::clone(&self.area), cx).cursor_crosshair());

        // The bar: every hue, in six steps of two colors each.
        let strip = (0..6).map(|i| {
            let (from, to) = (f32_of(f64::from(i)) * 60.0, f32_of(f64::from(i + 1)) * 60.0);
            div().flex_1().h_full().bg(linear_gradient(
                90.,
                linear_color_stop(rgb(hsv_to_rgb(from, 1.0, 1.0)), 0.),
                linear_color_stop(rgb(hsv_to_rgb(to, 1.0, 1.0)), 1.),
            ))
        });
        let bar = div()
            .relative()
            .w(px(AREA_W))
            .h(px(HUE_H))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_row()
                    .rounded_full()
                    .overflow_hidden()
                    .children(strip),
            )
            .child(
                marker(16.0, pure)
                    .left(px(hue / 360.0 * AREA_W - 8.0))
                    .top(px(-2.0)),
            )
            .child(surface("color-hue", Drag::Hue, Rc::clone(&self.bar), cx).cursor_pointer());

        let fields = self.fields.as_ref();
        let preview = div()
            .flex_none()
            .flex()
            .flex_row()
            .w(px(44.))
            .h(px(crate::kit::tokens::height::control()))
            .rounded_md()
            .overflow_hidden()
            .border_1()
            .border_color(theme::border_subtle())
            .child(div().flex_1().h_full().bg(rgb(self.original)))
            .child(div().flex_1().h_full().bg(rgb(color)));
        let typed = fields.map(|fields| {
            let channel = |label: &'static str, state: &Entity<InputState>| {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(small_label(label))
                    .child(div().w(px(56.)).child(crate::kit::input::text(state)))
            };
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(preview)
                        .child(small_label("HEX"))
                        .child(div().flex_1().child(crate::kit::input::text(&fields.hex))),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .child(channel("R", &fields.red))
                        .child(channel("G", &fields.green))
                        .child(channel("B", &fields.blue)),
                )
        });

        let swatch = |id: String, shade: u32, panel: &Entity<Self>| {
            let panel = panel.clone();
            div()
                .id(SharedString::from(id))
                .keyboard()
                .role(gpui::Role::Button)
                .aria_label(SharedString::from(format!("Color {shade:06x}")))
                .size(px(CELL))
                .rounded_sm()
                .bg(rgb(shade))
                .border_2()
                .border_color(if shade == color {
                    theme::fg()
                } else {
                    theme::border_subtle()
                })
                .cursor_pointer()
                .hover(|style| style.border_color(theme::border_strong()))
                .on_click(move |_, window, cx| {
                    panel.update(cx, |panel, cx| panel.pick_preset(shade, window, cx));
                })
        };
        let presets = div().flex().flex_row().flex_wrap().gap(px(GAP)).children(
            PRESETS
                .iter()
                .enumerate()
                .map(|(i, shade)| swatch(format!("color-preset-{i}"), *shade, &this)),
        );
        let saved: Vec<u32> = cx.default_global::<Saved>().0.clone();
        let can_save = !saved.contains(&color) && saved.len() < MAX_SAVED;
        let saved_row = div().flex().flex_row().flex_wrap().gap(px(GAP)).children(
            saved.iter().enumerate().map(|(i, shade)| {
                let forget = this.clone();
                let shade = *shade;
                swatch(format!("color-saved-{i}"), shade, &this).on_mouse_down(
                    MouseButton::Right,
                    move |_, _window, cx| {
                        forget.update(cx, |_, cx| Self::forget(shade, cx));
                    },
                )
            }),
        );
        crate::kit::menu::parts::panel(AREA_W + 24.0)
            .on_mouse_down_out(cx.listener(|this, _event, window, cx| {
                // A click on the swatch itself closes the panel through the swatch.
                if !this.swatch_hovered
                    && let Some(close) = this.on_close.clone()
                {
                    close(window, cx);
                }
            }))
            .child(area)
            .child(bar)
            .children(self.opacity_row(cx))
            .children(typed)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(small_label("PRESETS"))
                    .child(presets),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(small_label("SAVED"))
                            .child(
                                crate::kit::button::dense("color-save")
                                    .icon(IconName::Plus)
                                    .label("Save this color")
                                    .disabled(!can_save)
                                    .cursor_pointer()
                                    .when(!can_save, Styled::cursor_not_allowed)
                                    .on_click(move |_, _window, cx| {
                                        this.update(cx, |panel, cx| panel.save_current(cx));
                                    }),
                            ),
                    )
                    .child(if saved.is_empty() {
                        div()
                            .h(px(CELL))
                            .flex()
                            .items_center()
                            .text_size(px(crate::kit::tokens::text::body()))
                            .text_color(theme::muted_fg())
                            .child("Colors you save show here")
                    } else {
                        saved_row
                    })
                    .children(
                        (!saved.is_empty())
                            .then(|| small_label("Right click a saved color to remove it")),
                    ),
            )
    }
}
