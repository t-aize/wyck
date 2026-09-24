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

use super::{color_picker, theme};

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

/// A swatch showing `color`; clicking it calls `on_toggle`. With `open`, the color panel shows
/// under it (see [`super::color_picker`]), and every change of the color calls `on_pick` while the
/// panel stays open: a click outside it, or on the swatch, calls `on_toggle` to close it.
pub fn color_swatch(
    id: impl Into<ElementId>,
    color: u32,
    open: bool,
    cx: &mut App,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
    on_pick: impl Fn(u32, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let id: ElementId = id.into();
    let on_toggle = Rc::new(on_toggle);
    let close = on_toggle.clone();
    let panel = color_picker::panel(&id, color, open, cx, Rc::new(on_pick), close);
    let hover = panel.clone();
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
        .on_hover(move |hovered, _window, cx| {
            color_picker::set_swatch_hovered(&hover, *hovered, cx);
        })
        .on_click(move |_, window, cx| on_toggle(window, cx))
        .child(div().size_full().rounded_sm().bg(gpui::rgb(color)));
    if !open {
        return swatch.into_any_element();
    }
    div()
        .relative()
        .child(swatch)
        .child(
            gpui::deferred(
                gpui::anchored()
                    .snap_to_window_with_margin(px(8.))
                    .child(div().pt(px(30.)).child(panel)),
            )
            // Above the dialogs of gpui-component, which hold the color fields of the settings.
            .with_priority(100),
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
}
