//! The rows the chart type panels are made of, shared by them: a choice between a few values, a
//! switch, a typed number. Each one applies its change to the chart at once, like the rest of
//! the chart settings panel.

use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, Subscription, Window, div};
use gpui_kit::component::input::InputState;

use super::Chart;
use super::settings::ChartSettings;
use crate::app::settings_ui as ui;
use crate::app::widgets;

pub(super) fn edit(
    chart: &Entity<Chart>,
    cx: &mut gpui::App,
    change: impl FnOnce(&mut ChartSettings),
) {
    chart.update(cx, |chart, cx| chart.edit_settings(cx, change));
}

/// The values of a choice with their names.
pub(super) fn named<T: Copy>(all: &[T], label_of: fn(T) -> &'static str) -> Vec<(T, &'static str)> {
    all.iter().map(|v| (*v, label_of(*v))).collect()
}

/// A row that picks one of `options`.
pub(super) fn choice<T: Copy + PartialEq + 'static>(
    chart: &Entity<Chart>,
    id: &'static str,
    label: &'static str,
    hint: Option<&'static str>,
    options: Vec<(T, &'static str)>,
    current: T,
    set: fn(&mut ChartSettings, T),
) -> AnyElement {
    let names: Vec<&'static str> = options.iter().map(|(_, name)| *name).collect();
    let index = options.iter().position(|(v, _)| *v == current).unwrap_or(0);
    let chart = chart.clone();
    ui::field(
        label,
        hint,
        widgets::segmented(id, &names, index, move |chosen, _window, cx| {
            edit(&chart, cx, |s| set(s, options[chosen].0));
        }),
    )
}

/// A row with a switch.
pub(super) fn switch(
    chart: &Entity<Chart>,
    id: &'static str,
    label: &'static str,
    hint: Option<&'static str>,
    on: bool,
    set: fn(&mut ChartSettings, bool),
) -> AnyElement {
    let chart = chart.clone();
    ui::field(
        label,
        hint,
        ui::toggle(id, on, move |on, _window, cx| {
            edit(&chart, cx, |s| set(s, on));
        }),
    )
}

/// A row with a number field.
pub(super) fn number(
    label: &'static str,
    hint: Option<&'static str>,
    state: &Entity<InputState>,
) -> AnyElement {
    ui::field(label, hint, div().child(widgets::number_field(state, 120.)))
}

/// Makes the number fields of a panel and keeps the subscriptions that apply what is typed.
pub(super) struct Numbers<'a, 'b, T: 'static> {
    chart: &'a Entity<Chart>,
    window: &'a mut Window,
    cx: &'a mut Context<'b, T>,
    pub subscriptions: Vec<Subscription>,
}

impl<'a, 'b, T: 'static> Numbers<'a, 'b, T> {
    pub fn new(
        chart: &'a Entity<Chart>,
        window: &'a mut Window,
        cx: &'a mut Context<'b, T>,
    ) -> Self {
        Self {
            chart,
            window,
            cx,
            subscriptions: Vec::new(),
        }
    }

    /// A field holding `value`, between `low` and `high`. What is typed at least `low` is
    /// applied with `apply` (which the settings then repair to what they allow).
    pub fn add(
        &mut self,
        value: f64,
        (low, high): (f64, f64),
        decimals: usize,
        apply: fn(&mut ChartSettings, f64),
    ) -> Entity<InputState> {
        let window = &mut *self.window;
        let state = self
            .cx
            .new(|cx| widgets::number_state(value, low, high, 1.0, decimals, window, cx));
        let chart = self.chart.clone();
        self.subscriptions.push(widgets::watch_number(
            &state,
            self.cx,
            move |_this, value, cx| {
                if value >= low {
                    edit(&chart, cx, |s| apply(s, value));
                }
            },
        ));
        state
    }
}
