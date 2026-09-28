//! Number fields with steppers, and the subscriptions that read them.

use gpui::prelude::*;
use gpui::{Context, Entity, Subscription, Window, div, px};
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{InputEvent, InputState, NumberInput};

/// A number field with steppers, for a state made by [`state`].
pub fn field(state: &Entity<InputState>, width: f32) -> impl IntoElement {
    div().w(px(width)).child(NumberInput::new(state).small())
}

/// The state of a number field holding `value`.
pub fn state(
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    decimals: usize,
    window: &mut Window,
    cx: &mut gpui::Context<InputState>,
) -> InputState {
    InputState::new(window, cx)
        .default_value(format(value, decimals))
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

/// [`watch_parsed`] for a number field (see [`state`]).
pub fn watch<T: 'static>(
    state: &Entity<InputState>,
    cx: &mut Context<T>,
    on_value: impl Fn(&mut T, f64, &mut Context<T>) + 'static,
) -> Subscription {
    watch_parsed(state, cx, |_, text| parse(text), on_value)
}

/// A number written with at most `decimals` decimals, trailing zeros dropped.
pub fn format(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// Reads a number field, accepting a comma for the decimal point.
pub fn parse(text: &str) -> Option<f64> {
    let value: f64 = text.trim().replace(',', ".").parse().ok()?;
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_and_write_plainly() {
        assert_eq!(format(20.0, 2), "20");
        assert_eq!(format(0.0250, 4), "0.025");
        assert_eq!(parse(" 1,5 "), Some(1.5));
        assert_eq!(parse("abc"), None);
        assert_eq!(parse("inf"), None);
    }
}
