//! Number fields with steppers, and the subscriptions that read them.
//!
//! Every field says what it holds with a [`Kind`]. The kind decides how far one click of a
//! stepper moves the value and how many decimals it shows, so the same quantity steps the same way
//! in every panel: a reward to risk ratio moves by 0.1 in the order ticket and in the position
//! tool alike. A field narrows the range of its kind for its own value with `.min(..)` and
//! `.max(..)` on the state.

use gpui::prelude::*;
use gpui::{Context, Entity, Subscription, Window, div, px};
use gpui_kit::component::input::{InputEvent, InputState, NumberStep};

/// What a number field holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// A whole number: a period, a count, a size in points, a number of decimals.
    Count,
    /// The width of a panel, in pixels.
    PanelWidth,
    /// A reward to risk ratio.
    Ratio,
    /// A multiple of a distance, such as the ATR.
    Multiplier,
    /// A share of an account or of a balance, in percent.
    Percent,
    /// A share of 100 for a look: an opacity, a width against its slot, a value area.
    Share,
    /// A scale, in percent.
    Scale,
    /// A distance in pips.
    Pips,
    /// How far a price may slip, in pips.
    Slippage,
    /// An amount of money.
    Money,
    /// The width of a line, in pixels.
    LineWidth,
    /// A level of a study or of a drawing: a ratio of a Fibonacci tool, the value of a line.
    Level,
    /// A quantity with no natural step: a contract size, the value of a point, a box size.
    Amount,
    /// A price, stepping by `step` (a pip or a tick of the symbol).
    Price { step: f64 },
    /// A volume, stepping by the volume step of the symbol.
    Volume { step: f64 },
    /// A number whose step its source declares, such as an input of an indicator script.
    Declared { step: f64, decimals: usize },
}

/// How a [`Kind`] of number steps and reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spec {
    pub step: f64,
    pub min: f64,
    pub max: f64,
    pub decimals: usize,
}

impl Kind {
    pub fn spec(self) -> Spec {
        let spec = |step, min, max, decimals| Spec {
            step,
            min,
            max,
            decimals,
        };
        match self {
            Self::Count => spec(1.0, 0.0, 1e9, 0),
            Self::PanelWidth => spec(10.0, 0.0, 10_000.0, 0),
            Self::Ratio | Self::Multiplier => spec(0.1, 0.1, 100.0, 2),
            Self::Percent => spec(0.25, 0.0, 100.0, 2),
            Self::Share => spec(5.0, 0.0, 100.0, 0),
            Self::Scale => spec(10.0, 10.0, 1_000.0, 0),
            Self::Pips => spec(1.0, 0.0, 100_000.0, 1),
            Self::Slippage => spec(0.5, 0.0, 10_000.0, 1),
            Self::Money => spec(10.0, 0.0, 1e12, 2),
            Self::LineWidth => spec(0.5, 0.5, 40.0, 1),
            Self::Level => spec(0.1, -1e12, 1e12, 3),
            Self::Amount => spec(1.0, 0.0, 1e12, 4),
            Self::Price { step } | Self::Volume { step } => {
                spec(step, 0.0, 1e12, decimals_of(step))
            }
            Self::Declared { step, decimals } => spec(step, -1e12, 1e12, decimals),
        }
    }
}

/// How many decimals it takes to write `step`: 2 for 0.01, 0 for 5.
fn decimals_of(step: f64) -> usize {
    (0..=10)
        .find(|&d| {
            let scaled = step * 10f64.powi(d);
            (scaled - scaled.round()).abs() < 1e-6
        })
        .unwrap_or(10) as usize
}

/// A number field with steppers, for a state made by [`state`]. `width` is one of
/// [`crate::ui::kit::tokens::field`].
pub fn field(state: &Entity<InputState>, width: f32) -> impl IntoElement {
    div()
        .w(px(width))
        .child(crate::ui::kit::input::number(state))
}

/// The state of a number field of `kind` holding `value`, over the whole range of the kind.
pub fn state(
    kind: Kind,
    value: f64,
    window: &mut Window,
    cx: &mut gpui::Context<InputState>,
) -> InputState {
    let spec = kind.spec();
    InputState::new(window, cx)
        .default_value(format(value, spec.decimals))
        .min(spec.min)
        .max(spec.max)
        .step(NumberStep::Fixed(spec.step))
}

/// The state of an empty number field of `kind`, for a value the user has not typed yet.
pub fn empty(kind: Kind, window: &mut Window, cx: &mut gpui::Context<InputState>) -> InputState {
    let spec = kind.spec();
    InputState::new(window, cx)
        .min(spec.min)
        .max(spec.max)
        .step(NumberStep::Fixed(spec.step))
}

/// Makes a field step as `kind` does, for a field whose unit the user can change.
pub fn set_kind(state: &Entity<InputState>, kind: Kind, window: &mut Window, cx: &mut gpui::App) {
    let spec = kind.spec();
    state.update(cx, |s, cx| {
        s.set_step(NumberStep::Fixed(spec.step), window, cx);
        s.set_min(Some(spec.min), window, cx);
        s.set_max(Some(spec.max), window, cx);
    });
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

    #[test]
    fn the_same_quantity_steps_the_same_way() {
        assert_eq!(Kind::Ratio.spec().step, 0.1);
        assert_eq!(Kind::Multiplier.spec().step, 0.1);
        assert_eq!(Kind::Percent.spec().step, 0.25);
        assert_eq!(Kind::LineWidth.spec().step, 0.5);
        assert_eq!(Kind::LineWidth.spec().min, 0.5);
        assert_eq!(Kind::Share.spec().min, 0.0);
        assert_eq!(Kind::Share.spec().step, 5.0);
    }

    #[test]
    fn a_price_shows_as_many_decimals_as_its_step() {
        assert_eq!(Kind::Price { step: 0.0001 }.spec().decimals, 4);
        assert_eq!(Kind::Price { step: 0.01 }.spec().decimals, 2);
        assert_eq!(Kind::Volume { step: 1000.0 }.spec().decimals, 0);
        assert_eq!(decimals_of(0.5), 1);
    }
}
