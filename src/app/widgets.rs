//! Small building blocks for settings dialogs and panels, in the app's look: section titles,
//! labelled rows, segmented choices, color swatches, number fields.
//!
//! They are plain functions returning elements, so a dialog owns its state (which choice, which
//! swatch popover is open) and passes it in; the callbacks say what the user picked.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, Div, ElementId, Entity, SharedString, Window, div, px};
use gpui_kit::component::input::{InputState, NumberInput};
use gpui_kit::component::{Sizable, StyledExt as _};

use super::theme;

/// The colors a swatch popover offers: a row of grays, then eight hues in four shades.
pub const SWATCHES: [u32; 40] = [
    0xffffff, 0xd1d4dc, 0x9598a1, 0x787b86, 0x5d606b, 0x434651, 0x2a2e39, 0x000000, 0xf23645,
    0xff9800, 0xffeb3b, 0x4caf50, 0x089981, 0x00bcd4, 0x2962ff, 0x9c27b0, 0xfccbcd, 0xffe0b2,
    0xfff9c4, 0xc8e6c9, 0xace5dc, 0xb2ebf2, 0xbbd9fb, 0xe1bee7, 0xf7525f, 0xffb74d, 0xfff176,
    0x81c784, 0x22ab94, 0x4dd0e1, 0x5b9cf6, 0xba68c8, 0xb22833, 0xf57c00, 0xfbc02d, 0x388e3c,
    0x056656, 0x0097a7, 0x1848cc, 0x7b1fa2,
];

/// The id of child `n` of an element.
fn child_id(id: &ElementId, n: usize) -> ElementId {
    ElementId::NamedChild(std::sync::Arc::new(id.clone()), n.to_string().into())
}

/// A section title in a dialog.
pub fn section(title: impl Into<SharedString>) -> Div {
    div()
        .pt_3()
        .pb_1()
        .text_size(px(11.))
        .font_semibold()
        .text_color(theme::muted_fg())
        .child(title.into().to_uppercase())
}

/// A row with a label on the left and a control on the right.
pub fn row(label: impl Into<SharedString>, control: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_4()
        .min_h(px(34.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(13.))
                .text_color(theme::fg())
                .child(label.into()),
        )
        .child(div().flex_none().child(control))
}

/// A thin line between groups.
pub fn divider() -> Div {
    div().my_2().h(px(1.)).w_full().bg(theme::border_hairline())
}

/// Buttons side by side, one of them chosen.
pub fn segmented(
    id: impl Into<ElementId>,
    options: &[&str],
    selected: usize,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let on_select = Rc::new(on_select);
    let id: ElementId = id.into();
    let mut strip = div()
        .flex()
        .flex_row()
        .items_center()
        .p_0p5()
        .gap_0p5()
        .rounded_md()
        .bg(theme::bg())
        .border_1()
        .border_color(theme::border_subtle());
    for (index, label) in options.iter().enumerate() {
        let chosen = index == selected;
        let on_select = on_select.clone();
        strip = strip.child(
            div()
                .id(child_id(&id, index))
                .h(px(24.))
                .px_2p5()
                .flex()
                .items_center()
                .rounded_sm()
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(if chosen {
                    theme::fg()
                } else {
                    theme::muted_fg()
                })
                .when(chosen, |el| el.bg(theme::accent_selected()))
                .when(!chosen, |el| el.hover(|s| s.bg(theme::surface_hover())))
                .on_click(move |_, window, cx| on_select(index, window, cx))
                .child(SharedString::from((*label).to_owned())),
        );
    }
    strip
}

/// A swatch showing `color`; clicking it calls `on_toggle`. With `open`, the palette shows under
/// it, and picking a color calls `on_pick`.
pub fn color_swatch(
    id: impl Into<ElementId>,
    color: u32,
    open: bool,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
    on_pick: impl Fn(u32, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let id: ElementId = id.into();
    let on_pick = Rc::new(on_pick);
    let swatch = div()
        .id(id.clone())
        .size(px(26.))
        .p(px(3.))
        .rounded_md()
        .border_1()
        .border_color(if open {
            theme::accent()
        } else {
            theme::border_subtle()
        })
        .cursor_pointer()
        .hover(|s| s.border_color(theme::border_strong()))
        .on_click(move |_, window, cx| on_toggle(window, cx))
        .child(div().size_full().rounded_sm().bg(gpui::rgb(color)));
    if !open {
        return swatch.into_any_element();
    }
    let mut grid = div()
        .w(px(8.0 * 24.0 + 7.0 * 4.0 + 16.0))
        .p_2()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap_1()
        .rounded_lg()
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border_subtle())
        .shadow_lg()
        .occlude();
    for (index, swatch_color) in SWATCHES.iter().copied().enumerate() {
        let on_pick = on_pick.clone();
        let chosen = swatch_color == color;
        grid = grid.child(
            div()
                .id(child_id(&id, index + 1))
                .size(px(24.))
                .rounded_sm()
                .bg(gpui::rgb(swatch_color))
                .border_2()
                .border_color(if chosen {
                    theme::fg()
                } else {
                    gpui::rgba(0x00000000)
                })
                .cursor_pointer()
                .hover(|s| s.border_color(theme::border_strong()))
                .on_click(move |_, window, cx| on_pick(swatch_color, window, cx)),
        );
    }
    div()
        .relative()
        .child(swatch)
        .child(
            gpui::deferred(
                gpui::anchored()
                    .snap_to_window_with_margin(px(8.))
                    .child(div().pt(px(30.)).child(grid)),
            )
            .with_priority(3),
        )
        .into_any_element()
}

/// A number field with steppers, for a state made by [`number_state`].
pub fn number_field(state: &Entity<InputState>, width: f32) -> impl IntoElement {
    div().w(px(width)).child(NumberInput::new(state).small())
}

/// The state of a number field holding `value`.
pub fn number_state(
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    decimals: usize,
    window: &mut Window,
    cx: &mut gpui::Context<InputState>,
) -> InputState {
    InputState::new(window, cx)
        .default_value(format_number(value, decimals))
        .min(min)
        .max(max)
        .step(step)
}

/// A number written with at most `decimals` decimals, trailing zeros dropped.
pub fn format_number(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// Reads a number field, accepting a comma for the decimal point.
pub fn parse_number(text: &str) -> Option<f64> {
    let value: f64 = text.trim().replace(',', ".").parse().ok()?;
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_and_write_plainly() {
        assert_eq!(format_number(20.0, 2), "20");
        assert_eq!(format_number(0.0250, 4), "0.025");
        assert_eq!(parse_number(" 1,5 "), Some(1.5));
        assert_eq!(parse_number("abc"), None);
        assert_eq!(parse_number("inf"), None);
    }

    #[test]
    fn the_palette_has_no_repeats() {
        let mut colors = SWATCHES.to_vec();
        colors.sort_unstable();
        colors.dedup();
        assert_eq!(colors.len(), SWATCHES.len());
    }
}
