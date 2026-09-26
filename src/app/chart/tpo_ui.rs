//! The TPO chart type's part of the chart settings panel: how the chart is cut into sessions and
//! periods, how the marks look, and the levels and structure that are drawn on each profile.
//! Every change applies at once.

use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, Subscription, Window};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::InputState;

use super::Chart;
use super::settings_rows::{Numbers, choice, edit, named, number, switch};
use super::tpo::{SessionKind, TpoColor, TpoDisplay, TpoSettings, format_clock, parse_clock};
use crate::app::settings_ui as ui;
use crate::app::widgets;

/// The fields that are typed in.
pub(super) struct Inputs {
    session_hours: Entity<InputState>,
    session_start: Entity<InputState>,
    period_minutes: Entity<InputState>,
    row_units: Entity<InputState>,
    value_area_percent: Entity<InputState>,
    ib_periods: Entity<InputState>,
    single_min_rows: Entity<InputState>,
}

/// The inputs, with the subscriptions that apply what is typed to `chart`.
pub(super) fn inputs<T: 'static>(
    chart: &Entity<Chart>,
    window: &mut Window,
    cx: &mut Context<T>,
) -> (Inputs, Vec<Subscription>) {
    let t = chart.read(cx).settings.tpo;
    let mut numbers = Numbers::new(chart, window, cx);
    let session_hours = numbers.add(f64::from(t.session_hours), (1.0, 168.0), 0, |s, v| {
        s.tpo.session_hours = v.round() as u32;
    });
    let period_minutes = numbers.add(f64::from(t.period_minutes), (1.0, 1_440.0), 0, |s, v| {
        s.tpo.period_minutes = v.round() as u32;
    });
    let row_units = numbers.add(f64::from(t.row_units), (0.0, 1e6), 0, |s, v| {
        s.tpo.row_units = v.round() as u32;
    });
    let value_area_percent =
        numbers.add(f64::from(t.value_area_percent), (50.0, 95.0), 0, |s, v| {
            s.tpo.value_area_percent = v.round() as u32;
        });
    let ib_periods = numbers.add(f64::from(t.ib_periods), (1.0, 24.0), 0, |s, v| {
        s.tpo.ib_periods = v.round() as u32;
    });
    let single_min_rows = numbers.add(f64::from(t.single_min_rows), (1.0, 20.0), 0, |s, v| {
        s.tpo.single_min_rows = v.round() as u32;
    });
    let mut subscriptions = numbers.subscriptions;
    // The start of a session is a time of day, typed as 09:30.
    let session_start =
        cx.new(|cx| InputState::new(window, cx).default_value(format_clock(t.session_start)));
    let target = chart.clone();
    subscriptions.push(widgets::watch_parsed(
        &session_start,
        cx,
        |_, text| parse_clock(text),
        move |_this, minutes, cx| edit(&target, cx, |s| s.tpo.session_start = minutes),
    ));
    (
        Inputs {
            session_hours,
            session_start,
            period_minutes,
            row_units,
            value_area_percent,
            ib_periods,
            single_min_rows,
        },
        subscriptions,
    )
}

/// The groups of the settings of the TPO chart type.
pub(super) fn groups(chart: &Entity<Chart>, inputs: &Inputs, t: &TpoSettings) -> Vec<AnyElement> {
    let mut profile = vec![choice(
        chart,
        "tpo-session",
        "Session",
        Some("How the chart is cut into profiles"),
        named(&SessionKind::ALL, SessionKind::label),
        t.session,
        |s, v| s.tpo.session = v,
    )];
    if t.session == SessionKind::Hours {
        profile.push(number("Hours in a session", None, &inputs.session_hours));
    }
    profile.push(ui::field(
        "Session starts at",
        Some("In the time zone of the chart, as 09:30"),
        ui::text_field(&inputs.session_start, 110.),
    ));
    profile.push(number(
        "Period of a letter (minutes)",
        Some("Never shorter than a bar of the chart"),
        &inputs.period_minutes,
    ));
    profile.push(number(
        "Row height (quote units)",
        Some("0 picks one that gives about forty rows a session"),
        &inputs.row_units,
    ));

    let marks = vec![
        choice(
            chart,
            "tpo-display",
            "Marks",
            Some("Automatic writes letters when the columns are wide enough to read"),
            named(&TpoDisplay::ALL, TpoDisplay::label),
            t.display,
            |s, v| s.tpo.display = v,
        ),
        choice(
            chart,
            "tpo-color",
            "Colored",
            None,
            named(&TpoColor::ALL, TpoColor::label),
            t.color,
            |s, v| s.tpo.color = v,
        ),
        switch(
            chart,
            "tpo-open-close",
            "Open and close",
            Some("A dot where the session opened and a ring where it closed"),
            t.open_close,
            |s, on| s.tpo.open_close = on,
        ),
    ];

    let levels = vec![
        switch(
            chart,
            "tpo-poc",
            "Point of control",
            Some("The row with the most marks"),
            t.poc,
            |s, on| s.tpo.poc = on,
        ),
        switch(
            chart,
            "tpo-poc-line",
            "Line at the point of control",
            None,
            t.poc_line,
            |s, on| s.tpo.poc_line = on,
        ),
        switch(
            chart,
            "tpo-va",
            "Value area",
            Some("The rows around the point of control that hold most of the marks"),
            t.value_area,
            |s, on| s.tpo.value_area = on,
        ),
        number(
            "Value area (% of the marks)",
            Some("70 is the usual"),
            &inputs.value_area_percent,
        ),
        switch(
            chart,
            "tpo-midpoint",
            "Midpoint of the range",
            None,
            t.midpoint,
            |s, on| s.tpo.midpoint = on,
        ),
        switch(
            chart,
            "tpo-labels",
            "Prices of the levels",
            Some("Written beside a profile that is wide enough"),
            t.labels,
            |s, on| s.tpo.labels = on,
        ),
    ];

    let structure = vec![
        switch(
            chart,
            "tpo-ib",
            "Initial balance",
            Some("The range of the first periods of the session"),
            t.initial_balance,
            |s, on| s.tpo.initial_balance = on,
        ),
        number(
            "Periods in the initial balance",
            Some("Two, an hour with half hour periods, is the usual"),
            &inputs.ib_periods,
        ),
        switch(
            chart,
            "tpo-singles",
            "Single prints",
            Some("Rows only one period visited: a gap, or a tail at an end"),
            t.single_prints,
            |s, on| s.tpo.single_prints = on,
        ),
        number(
            "Fewest rows in single prints",
            None,
            &inputs.single_min_rows,
        ),
        switch(
            chart,
            "tpo-poor",
            "Poor highs and lows",
            Some("An end more than one period touched, where the price was not rejected"),
            t.poor_extremes,
            |s, on| s.tpo.poor_extremes = on,
        ),
    ];

    vec![
        ui::group(IconName::ChartNoAxesGantt, "Sessions and periods", profile).into_any_element(),
        ui::group(IconName::Palette, "Marks", marks).into_any_element(),
        ui::group(IconName::Target, "Point of control and value area", levels).into_any_element(),
        ui::group(IconName::Layers, "Structure", structure).into_any_element(),
    ]
}
