//! Small building blocks for settings dialogs and panels, in the app's look: segmented choices,
//! color swatches, number fields, and the subscriptions that read them.
//!
//! They are plain functions returning elements, so a dialog owns its state (which choice, which
//! swatch popover is open) and passes it in; the callbacks say what the user picked.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, ElementId, Entity, SharedString, Subscription, Window, div, px,
};
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{InputEvent, InputState, NumberInput};

use super::{color_picker, menu, theme};

/// The id of child `n` of an element.
pub fn child_id(id: &ElementId, n: usize) -> ElementId {
    ElementId::NamedChild(std::sync::Arc::new(id.clone()), n.to_string().into())
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
        // Above the dialogs of gpui-component, which hold the color fields of the settings.
        .child(menu::below(panel, 30., 100))
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

/// Subscribes to a field: whenever it changes or loses focus and its text reads as a value by
/// `parse` (which sees the view that owns the field), `on_value` gets that value.
pub fn watch_parsed<T: 'static, V: 'static>(
    state: &Entity<InputState>,
    cx: &mut Context<T>,
    parse: impl Fn(&T, &str) -> Option<V> + 'static,
    on_value: impl Fn(&mut T, V, &mut Context<T>) + 'static,
) -> Subscription {
    cx.subscribe(state, move |this, state, event: &InputEvent, cx| {
        if matches!(event, InputEvent::Change | InputEvent::Blur)
            && let Some(value) = parse(this, &state.read(cx).value())
        {
            on_value(this, value, cx);
        }
    })
}

/// [`watch_parsed`] for a number field (see [`number_state`]).
pub fn watch_number<T: 'static>(
    state: &Entity<InputState>,
    cx: &mut Context<T>,
    on_value: impl Fn(&mut T, f64, &mut Context<T>) + 'static,
) -> Subscription {
    watch_parsed(state, cx, |_, text| parse_number(text), on_value)
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
