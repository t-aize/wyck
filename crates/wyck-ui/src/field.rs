//! The inputs a panel is built from, all with the same height, the same corners and the same
//! widths, so a field looks and steps alike wherever it is.
//!
//! * [`SliderField`]: a slider with its number field beside it, for a value with a range that
//!   is felt more than typed (an opacity, a width, a size).
//! * [`unit`]: a number field with its unit written after it ("px", "%", "pt").
//! * [`icon_choice`]: one choice among a few, each an icon with a tooltip (an alignment).
//! * [`check`]: a checkbox with its label.
//! * [`text`]: a text field of the standard width.
//!
//! Number fields themselves are in [`crate::number`], switches and segmented choices in
//! [`crate::controls`].

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Context, Div, ElementId, Entity, EventEmitter, SharedString, Subscription, Window, div, px,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};

use crate::controls::{child_id, ink, tooltip};
use crate::number::{self, Kind};
use crate::{icon, theme, tokens};

/// What a [`SliderField`] says when its value changes.
pub struct ValueChanged(pub f64);

/// The width of the slider itself.
const SLIDER_WIDTH: f32 = 130.0;

/// A slider and a number field that always show the same value. Dragging the slider or typing in
/// the field changes both, and the field says so with a [`ValueChanged`] event.
pub struct SliderField {
    slider: Entity<SliderState>,
    input: Entity<InputState>,
    unit: SharedString,
    decimals: usize,
    value: f64,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ValueChanged> for SliderField {}

impl SliderField {
    /// A field over the range and the step of `kind`, holding `value`, with `unit` written after
    /// the number (`""` for none).
    pub fn new(
        kind: Kind,
        value: f64,
        unit: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::within(
            kind,
            kind.spec().min,
            kind.spec().max,
            value,
            unit,
            window,
            cx,
        )
    }

    /// [`Self::new`] with a narrower range than the kind's, for the slider and the number alike.
    pub fn within(
        kind: Kind,
        min: f64,
        max: f64,
        value: f64,
        unit: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let spec = kind.spec();
        let value = value.clamp(min, max);
        let slider = cx.new(|_| {
            SliderState::new()
                .min(min as f32)
                .max(max as f32)
                .step(spec.step as f32)
                .default_value(value as f32)
        });
        let input = cx.new(|cx| number::state(kind, value, window, cx).min(min).max(max));
        let subscriptions = vec![
            cx.subscribe_in(
                &slider,
                window,
                |this, _slider, event: &SliderEvent, window, cx| {
                    if let SliderEvent::Change(v) = event {
                        let v = f64::from(v.end());
                        this.set(v, true, window, cx);
                    }
                },
            ),
            cx.subscribe_in(
                &input,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change | InputEvent::Blur)
                        && let Some(v) = number::parse(&input.read(cx).value())
                    {
                        let (lo, hi) = (
                            f64::from(this.slider.read(cx).min_value()),
                            f64::from(this.slider.read(cx).max_value()),
                        );
                        this.set(v.clamp(lo, hi), false, window, cx);
                    }
                },
            ),
        ];
        Self {
            slider,
            input,
            unit: unit.into(),
            decimals: spec.decimals,
            value,
            _subscriptions: subscriptions,
        }
    }

    /// The value shown.
    pub fn value(&self) -> f64 {
        self.value
    }

    /// Shows `value` without telling anyone (for a value changed from outside).
    pub fn show(&mut self, value: f64, window: &mut Window, cx: &mut Context<Self>) {
        if (self.value - value).abs() < 1e-9 {
            return;
        }
        self.value = value;
        self.slider
            .update(cx, |s, cx| s.set_value(value as f32, window, cx));
        let text = number::format(value, self.decimals);
        self.input.update(cx, |s, cx| s.set_value(text, window, cx));
        cx.notify();
    }

    fn set(&mut self, value: f64, from_slider: bool, window: &mut Window, cx: &mut Context<Self>) {
        if (self.value - value).abs() < 1e-9 {
            return;
        }
        self.value = value;
        if from_slider {
            let text = number::format(value, self.decimals);
            self.input.update(cx, |s, cx| s.set_value(text, window, cx));
        } else {
            self.slider
                .update(cx, |s, cx| s.set_value(value as f32, window, cx));
        }
        cx.emit(ValueChanged(value));
        cx.notify();
    }
}

impl Render for SliderField {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(div().w(px(SLIDER_WIDTH)).child(Slider::new(&self.slider)))
            .child(unit(&self.input, &self.unit, tokens::field::NUMBER))
    }
}

/// A number field, `width` pixels wide, with `unit` written after it.
pub fn unit(state: &Entity<InputState>, unit: &str, width: f32) -> Div {
    let row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .child(number::field(state, width));
    if unit.is_empty() {
        row
    } else {
        row.child(
            div()
                .w(px(18.))
                .text_size(px(tokens::text::BODY))
                .text_color(theme::muted_fg())
                .child(SharedString::from(unit.to_owned())),
        )
    }
}

/// A text field of the standard width.
pub fn text(state: &Entity<InputState>) -> Div {
    div()
        .w(px(tokens::field::TEXT))
        .child(Input::new(state).small())
}

/// One choice among a few, each an icon with a tooltip saying what it is.
pub fn icon_choice(
    id: impl Into<ElementId>,
    options: &[(IconName, &'static str)],
    selected: usize,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Div {
    let id: ElementId = id.into();
    let on_select = Rc::new(on_select);
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
    for (index, (name, label)) in options.iter().enumerate() {
        let chosen = index == selected;
        let on_select = on_select.clone();
        strip = strip.child(
            div()
                .id(child_id(&id, index))
                .flex()
                .items_center()
                .justify_center()
                .size(px(tokens::height::COMPACT))
                .rounded_sm()
                .cursor_pointer()
                .when(chosen, |el| el.bg(theme::accent_selected()))
                .when(!chosen, |el| el.hover(|s| s.bg(theme::surface_hover())))
                .tooltip(tooltip(*label))
                .on_click(move |_, window, cx| on_select(index, window, cx))
                .child(icon::tinted(*name, 14., ink(chosen))),
        );
    }
    strip
}

/// A checkbox with its label.
pub fn check(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    on_change: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> Checkbox {
    Checkbox::new(id)
        .label(label.into())
        .checked(checked)
        .on_click(move |value, window, cx| on_change(*value, window, cx))
}
