//! The footprint's part of the chart settings panel: how the cells look, the imbalances, the
//! point of control and the value area. Every change applies at once.

use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, Subscription, Window};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::InputState;

use super::Chart;
use super::footprint::{CellMode, FootprintSettings, HeatScope};
use super::settings::ChartSettings;
use crate::app::settings_ui as ui;
use crate::app::widgets;

/// The numbers of the footprint that are typed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    RowSteps,
    ImbalancePercent,
    ImbalanceMin,
    StackRows,
    ValueAreaPercent,
}

impl Field {
    const ALL: [Self; 5] = [
        Self::RowSteps,
        Self::ImbalancePercent,
        Self::ImbalanceMin,
        Self::StackRows,
        Self::ValueAreaPercent,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::RowSteps => "Row height",
            Self::ImbalancePercent => "Imbalance ratio (%)",
            Self::ImbalanceMin => "Imbalance smallest volume",
            Self::StackRows => "Rows in a stack",
            Self::ValueAreaPercent => "Value area (% of the volume)",
        }
    }

    fn hint(self) -> Option<&'static str> {
        match self {
            Self::RowSteps => Some("In price steps. 0 is automatic"),
            Self::ImbalancePercent => Some("How much one side must outweigh the other"),
            Self::ImbalanceMin => Some("Smaller volumes are never marked"),
            Self::StackRows => Some("Imbalances in a row that make a stack"),
            Self::ValueAreaPercent => None,
        }
    }

    fn get(self, f: &FootprintSettings) -> u32 {
        match self {
            Self::RowSteps => f.row_steps,
            Self::ImbalancePercent => f.imbalance_percent,
            Self::ImbalanceMin => f.imbalance_min,
            Self::StackRows => f.stack_rows,
            Self::ValueAreaPercent => f.value_area_percent,
        }
    }

    fn set(self, f: &mut FootprintSettings, value: u32) {
        match self {
            Self::RowSteps => f.row_steps = value,
            Self::ImbalancePercent => f.imbalance_percent = value,
            Self::ImbalanceMin => f.imbalance_min = value,
            Self::StackRows => f.stack_rows = value,
            Self::ValueAreaPercent => f.value_area_percent = value,
        }
    }

    /// The least and the most the field takes, as the settings repair them.
    fn range(self) -> (f64, f64) {
        match self {
            Self::RowSteps => (0.0, 10_000.0),
            Self::ImbalancePercent => (110.0, 2_000.0),
            Self::ImbalanceMin => (1.0, 100_000.0),
            Self::StackRows => (2.0, 12.0),
            Self::ValueAreaPercent => (50.0, 95.0),
        }
    }
}

/// The inputs of the numbers, with the subscriptions that apply what is typed to `chart`.
pub(super) fn inputs<T: 'static>(
    chart: &Entity<Chart>,
    window: &mut Window,
    cx: &mut Context<T>,
) -> (Vec<(Field, Entity<InputState>)>, Vec<Subscription>) {
    let settings = chart.read(cx).settings.footprint;
    let mut inputs = Vec::new();
    let mut subscriptions = Vec::new();
    for field in Field::ALL {
        let (low, high) = field.range();
        let value = f64::from(field.get(&settings));
        let state = cx.new(|cx| widgets::number_state(value, low, high, 1.0, 0, window, cx));
        let chart = chart.clone();
        subscriptions.push(widgets::watch_number(
            &state,
            cx,
            move |_this, value, cx| {
                if value >= 0.0 {
                    chart.update(cx, |chart, cx| {
                        chart.edit_settings(cx, |s| {
                            field.set(&mut s.footprint, value.round() as u32)
                        });
                    });
                }
            },
        ));
        inputs.push((field, state));
    }
    (inputs, subscriptions)
}

fn edit(chart: &Entity<Chart>, cx: &mut gpui::App, change: impl FnOnce(&mut ChartSettings)) {
    chart.update(cx, |chart, cx| chart.edit_settings(cx, change));
}

/// The groups of the settings of the footprint chart type.
pub(super) fn groups(
    chart: &Entity<Chart>,
    inputs: &[(Field, Entity<InputState>)],
    f: &FootprintSettings,
) -> Vec<AnyElement> {
    let switch = |id: &'static str, on: bool, change: fn(&mut FootprintSettings, bool)| {
        let chart = chart.clone();
        ui::toggle(id, on, move |on, _window, cx| {
            edit(&chart, cx, |s| change(&mut s.footprint, on));
        })
    };
    let number = |field: Field| {
        let state = inputs
            .iter()
            .find(|(candidate, _)| *candidate == field)
            .map(|(_, state)| state);
        ui::field(
            field.label(),
            field.hint(),
            gpui::div().children(state.map(|state| widgets::number_field(state, 120.))),
        )
    };
    let mode_labels: Vec<&str> = CellMode::ALL.iter().map(|m| m.label()).collect();
    let mode_index = CellMode::ALL.iter().position(|m| *m == f.mode).unwrap_or(0);
    let mode_chart = chart.clone();
    let scope_labels: Vec<&str> = HeatScope::ALL.iter().map(|m| m.label()).collect();
    let scope_index = HeatScope::ALL
        .iter()
        .position(|m| *m == f.heat_scope)
        .unwrap_or(0);
    let scope_chart = chart.clone();

    let cells = vec![
        ui::field(
            "Cells show",
            None,
            widgets::segmented(
                "footprint-mode",
                &mode_labels,
                mode_index,
                move |choice, _window, cx| {
                    edit(&mode_chart, cx, |s| {
                        s.footprint.mode = CellMode::ALL[choice]
                    });
                },
            ),
        ),
        ui::field(
            "Numbers",
            None,
            switch("footprint-numbers", f.numbers, |f, on| f.numbers = on),
        ),
        ui::field(
            "Heat colors",
            None,
            switch("footprint-heat", f.heat, |f, on| f.heat = on),
        ),
        ui::field(
            "Heat compared with",
            None,
            widgets::segmented(
                "footprint-scope",
                &scope_labels,
                scope_index,
                move |choice, _window, cx| {
                    edit(&scope_chart, cx, |s| {
                        s.footprint.heat_scope = HeatScope::ALL[choice];
                    });
                },
            ),
        ),
        number(Field::RowSteps),
        ui::field(
            "Delta and volume under each bar",
            None,
            switch("footprint-summary", f.summary, |f, on| f.summary = on),
        ),
    ];
    let imbalances = vec![
        ui::field(
            "Highlight imbalances",
            None,
            switch("footprint-imbalance", f.imbalance, |f, on| f.imbalance = on),
        ),
        ui::field(
            "Ask against the bid below",
            Some("The diagonal comparison"),
            switch("footprint-diagonal", f.diagonal, |f, on| f.diagonal = on),
        ),
        number(Field::ImbalancePercent),
        number(Field::ImbalanceMin),
        ui::field(
            "Stacked imbalances",
            None,
            switch("footprint-stacked", f.stacked, |f, on| f.stacked = on),
        ),
        number(Field::StackRows),
        ui::field(
            "Extend stacks until touched",
            None,
            switch("footprint-project", f.project_stacks, |f, on| {
                f.project_stacks = on;
            }),
        ),
    ];
    let control = vec![
        ui::field(
            "Point of control",
            None,
            switch("footprint-poc", f.poc, |f, on| f.poc = on),
        ),
        ui::field(
            "Extend the point of control until touched",
            None,
            switch("footprint-extend-poc", f.extend_poc, |f, on| {
                f.extend_poc = on;
            }),
        ),
        ui::field(
            "Value area",
            None,
            switch("footprint-va", f.value_area, |f, on| f.value_area = on),
        ),
        number(Field::ValueAreaPercent),
    ];
    vec![
        ui::group(IconName::Rows3, "Footprint cells", cells).into_any_element(),
        ui::group(IconName::Scale, "Imbalances", imbalances).into_any_element(),
        ui::group(IconName::Target, "Point of control and value area", control).into_any_element(),
    ]
}
