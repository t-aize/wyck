//! The dialogs of a chart: the list of indicators to add, the settings of one indicator (its
//! inputs and the look of each of its lines), and the settings of the chart itself (grid,
//! price scale, time zone, and the sizes of the price based chart types).
//!
//! Every change applies at once, as the user makes it, so the chart behind the dialog shows the
//! result live; closing the dialog keeps it.

use gpui::prelude::*;
use gpui::{App, Context, Entity, SharedString, Subscription, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::WindowExt;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use wyck::openapi::market::PRICE_SCALE;

use super::settings::{ChartKind, ScaleMode};
use super::study::{Placement, StudyConfig, StudyKind};
use super::transform::BoxSize;
use super::zone::Zone;
use super::{Chart, footprint_ui};
use crate::app::connection::ui;
use crate::app::{theme, widgets};

// ---- the list of indicators ----

/// Opens the list of indicators to add to `chart`.
pub fn open_picker(chart: Entity<Chart>, _this: &mut Chart, window: &mut Window, cx: &mut App) {
    // Opened once the chart that asked is no longer being updated, since the dialog reads it.
    window.defer(cx, move |window, cx| {
        let picker = cx.new(|cx| StudyPicker::new(chart, window, cx));
        let focus = picker.read(cx).search.clone();
        window.open_dialog(cx, move |dialog, _window, _cx| {
            dialog.title("Indicators").w(px(560.)).child(picker.clone())
        });
        focus.update(cx, |state, cx| state.focus(window, cx));
    });
}

struct StudyPicker {
    chart: Entity<Chart>,
    search: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl StudyPicker {
    fn new(chart: Entity<Chart>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search indicators"));
        let subscriptions = vec![
            cx.subscribe(&search, |_this, _input, _event: &InputEvent, cx| {
                cx.notify()
            }),
            cx.observe(&chart, |_this, _chart, cx| cx.notify()),
        ];
        Self {
            chart,
            search,
            _subscriptions: subscriptions,
        }
    }
}

fn matches(kind: StudyKind, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let spec = kind.spec();
    let haystack = format!("{} {}", spec.label, spec.short).to_lowercase();
    query.split_whitespace().all(|word| haystack.contains(word))
}

impl Render for StudyPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).value().to_string();
        let on_chart = self.chart.read(cx).settings.studies.clone();
        let mut sections: Vec<(&str, Vec<StudyKind>)> = vec![
            ("On the prices", Vec::new()),
            ("In a pane of their own", Vec::new()),
        ];
        for kind in StudyKind::ALL {
            if matches(kind, &query) {
                let slot = match kind.spec().placement {
                    Placement::Overlay => 0,
                    Placement::Pane => 1,
                };
                sections[slot].1.push(kind);
            }
        }
        let mut list = div().flex().flex_col().gap_0p5();
        let mut any = false;
        for (title, kinds) in sections {
            if kinds.is_empty() {
                continue;
            }
            any = true;
            list = list.child(widgets::section(title));
            for kind in kinds {
                let spec = kind.spec();
                let count = on_chart.iter().filter(|s| s.kind == kind).count();
                let chart = self.chart.clone();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("add-study-{kind:?}")))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .h(px(34.))
                        .px_2()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::surface_hover()))
                        .on_click(move |_, _window, cx| {
                            chart.update(cx, |chart, cx| {
                                chart.add_study(StudyConfig::new(kind), cx);
                            });
                        })
                        .child(
                            div()
                                .w(px(64.))
                                .text_size(px(11.))
                                .font_semibold()
                                .text_color(theme::accent())
                                .child(spec.short),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(13.))
                                .text_color(theme::fg())
                                .child(spec.label),
                        )
                        .children((count > 0).then(|| {
                            div()
                                .px_1p5()
                                .rounded_sm()
                                .bg(theme::accent_selected())
                                .text_size(px(11.))
                                .text_color(theme::fg())
                                .child(format!("{count} on chart"))
                        }))
                        .child(ui::icon_colored(IconName::Plus, 14., theme::muted_fg())),
                );
            }
        }
        if !any {
            list = list.child(
                div()
                    .py_6()
                    .text_center()
                    .text_size(px(13.))
                    .text_color(theme::muted_fg())
                    .child("No indicator matches this search."),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(Input::new(&self.search).prefix(ui::icon_colored(
                IconName::Search,
                14.,
                theme::muted_fg(),
            )))
            .child(
                div()
                    .id("study-list")
                    .max_h(px(420.))
                    .overflow_y_scroll()
                    .child(list),
            )
    }
}

// ---- the chart's own settings ----

/// Opens the settings of `chart`.
pub fn open_chart_settings(chart: Entity<Chart>, window: &mut Window, cx: &mut App) {
    // Opened once the chart that asked is no longer being updated, since the dialog reads it.
    window.defer(cx, move |window, cx| {
        let editor = cx.new(|cx| ChartSettingsEditor::new(chart, window, cx));
        window.open_dialog(cx, move |dialog, _window, _cx| {
            dialog
                .title("Chart settings")
                .w(px(520.))
                .child(editor.clone())
        });
    });
}

/// Which size of the price based chart types a field sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SizeField {
    Renko,
    Kagi,
    PointFigure,
    Range,
}

impl SizeField {
    const ALL: [Self; 4] = [Self::Renko, Self::Kagi, Self::PointFigure, Self::Range];

    fn label(self) -> &'static str {
        match self {
            Self::Renko => "Renko box",
            Self::Kagi => "Kagi reversal",
            Self::PointFigure => "Point and figure box",
            Self::Range => "Range bar size",
        }
    }

    fn kind(self) -> ChartKind {
        match self {
            Self::Renko => ChartKind::Renko,
            Self::Kagi => ChartKind::Kagi,
            Self::PointFigure => ChartKind::PointFigure,
            Self::Range => ChartKind::Range,
        }
    }

    fn get(self, t: &super::transform::TransformSettings) -> BoxSize {
        match self {
            Self::Renko => t.renko_box,
            Self::Kagi => t.kagi_reversal,
            Self::PointFigure => t.pnf_box,
            Self::Range => t.range,
        }
    }

    fn set(self, t: &mut super::transform::TransformSettings, size: BoxSize) {
        match self {
            Self::Renko => t.renko_box = size,
            Self::Kagi => t.kagi_reversal = size,
            Self::PointFigure => t.pnf_box = size,
            Self::Range => t.range = size,
        }
    }
}

fn size_value(size: BoxSize) -> f64 {
    match size {
        BoxSize::Atr { length } => f64::from(length),
        BoxSize::Fixed { price } => price,
        BoxSize::Percent { percent } => percent,
    }
}

fn size_mode(size: BoxSize) -> usize {
    match size {
        BoxSize::Atr { .. } => 0,
        BoxSize::Fixed { .. } => 1,
        BoxSize::Percent { .. } => 2,
    }
}

struct ChartSettingsEditor {
    chart: Entity<Chart>,
    sizes: Vec<(SizeField, Entity<InputState>)>,
    line_break: Entity<InputState>,
    reversal: Entity<InputState>,
    footprint: Vec<(footprint_ui::Field, Entity<InputState>)>,
    _subscriptions: Vec<Subscription>,
}

impl ChartSettingsEditor {
    fn new(chart: Entity<Chart>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let transform = chart.read(cx).settings.transform;
        let mut subscriptions = vec![cx.observe(&chart, |_this, _chart, cx| cx.notify())];
        let mut sizes = Vec::new();
        for field in SizeField::ALL {
            let size = field.get(&transform);
            let state =
                cx.new(|cx| widgets::number_state(size_value(size), 0.0, 1e9, 1.0, 6, window, cx));
            subscriptions.push(
                cx.subscribe(&state, move |this, state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change | InputEvent::Blur)
                        && let Some(value) = widgets::parse_number(&state.read(cx).value())
                        && value > 0.0
                    {
                        this.chart.update(cx, |chart, cx| {
                            chart.edit_settings(cx, |s| {
                                let size = match field.get(&s.transform) {
                                    BoxSize::Atr { .. } => BoxSize::Atr {
                                        length: value.round().max(1.0) as u32,
                                    },
                                    BoxSize::Fixed { .. } => BoxSize::Fixed { price: value },
                                    BoxSize::Percent { .. } => BoxSize::Percent { percent: value },
                                };
                                field.set(&mut s.transform, size);
                            });
                        });
                    }
                }),
            );
            sizes.push((field, state));
        }
        let line_break = cx.new(|cx| {
            widgets::number_state(
                f64::from(transform.line_break),
                1.0,
                10.0,
                1.0,
                0,
                window,
                cx,
            )
        });
        subscriptions.push(
            cx.subscribe(&line_break, |this, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change | InputEvent::Blur)
                    && let Some(value) = widgets::parse_number(&state.read(cx).value())
                {
                    this.chart.update(cx, |chart, cx| {
                        chart.edit_settings(cx, |s| s.transform.line_break = value.round() as u32);
                    });
                }
            }),
        );
        let reversal = cx.new(|cx| {
            widgets::number_state(
                f64::from(transform.pnf_reversal),
                1.0,
                10.0,
                1.0,
                0,
                window,
                cx,
            )
        });
        subscriptions.push(
            cx.subscribe(&reversal, |this, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change | InputEvent::Blur)
                    && let Some(value) = widgets::parse_number(&state.read(cx).value())
                {
                    this.chart.update(cx, |chart, cx| {
                        chart
                            .edit_settings(cx, |s| s.transform.pnf_reversal = value.round() as u32);
                    });
                }
            }),
        );
        let (footprint, footprint_subscriptions) = footprint_ui::inputs(&chart, window, cx);
        subscriptions.extend(footprint_subscriptions);
        Self {
            chart,
            sizes,
            line_break,
            reversal,
            footprint,
            _subscriptions: subscriptions,
        }
    }
}

fn edit_chart(
    chart: &Entity<Chart>,
    cx: &mut App,
    change: impl FnOnce(&mut super::settings::ChartSettings),
) {
    chart.update(cx, |chart, cx| chart.edit_settings(cx, change));
}

impl Render for ChartSettingsEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (settings, digits, box_size) = {
            let chart = self.chart.read(cx);
            (
                chart.settings.clone(),
                chart.digits(),
                chart.display.box_size,
            )
        };
        let chart = self.chart.clone();
        let switch =
            |id: &'static str, on: bool, change: fn(&mut super::settings::ChartSettings, bool)| {
                let chart = chart.clone();
                Switch::new(id).cursor_pointer().checked(on).on_click(
                    move |checked, _window, cx| {
                        let checked = *checked;
                        edit_chart(&chart, cx, |s| change(s, checked));
                    },
                )
            };
        let scale_labels: Vec<&str> = ScaleMode::ALL.iter().map(|m| m.label()).collect();
        let scale_index = ScaleMode::ALL
            .iter()
            .position(|m| *m == settings.scale)
            .unwrap_or(0);
        let scale_chart = chart.clone();

        let now = super::now_ms();
        let mut zones = div()
            .id("zone-list")
            .flex()
            .flex_col()
            .max_h(px(170.))
            .overflow_y_scroll()
            .rounded_md()
            .border_1()
            .border_color(theme::border_subtle());
        for zone in Zone::menu() {
            let chosen = zone == settings.zone;
            let chart = chart.clone();
            zones = zones.child(
                div()
                    .id(SharedString::from(format!("settings-zone-{}", zone.code())))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(28.))
                    .px_2()
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(if chosen {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    })
                    .when(chosen, |el| el.bg(theme::accent_selected()))
                    .hover(|s| s.bg(theme::surface_hover()))
                    .on_click(move |_, _window, cx| {
                        edit_chart(&chart, cx, |s| s.zone = zone);
                    })
                    .child(zone.name(now))
                    .children(
                        chosen.then(|| ui::icon_colored(IconName::Check, 13., theme::accent())),
                    ),
            );
        }

        let mut sizes = div().flex().flex_col();
        for (field, state) in &self.sizes {
            let field = *field;
            let size = field.get(&settings.transform);
            let mode_chart = chart.clone();
            let state_for_mode = state.clone();
            let unit_real = super::scene::quote_unit(digits) / PRICE_SCALE as f64;
            let resolved = (field.kind() == settings.kind && box_size > 0)
                .then(|| box_size as f64 / PRICE_SCALE as f64);
            sizes = sizes.child(widgets::row(
                field.label(),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(widgets::segmented(
                        SharedString::from(format!("size-mode-{field:?}")),
                        &["ATR", "Price", "%"],
                        size_mode(size),
                        move |choice, window, cx| {
                            let next = match choice {
                                0 => BoxSize::Atr { length: 14 },
                                1 => BoxSize::Fixed {
                                    price: resolved.unwrap_or(unit_real * 10.0),
                                },
                                _ => BoxSize::Percent { percent: 0.1 },
                            };
                            edit_chart(&mode_chart, cx, |s| field.set(&mut s.transform, next));
                            state_for_mode.update(cx, |state, cx| {
                                state.set_value(
                                    widgets::format_number(size_value(next), 6),
                                    window,
                                    cx,
                                );
                            });
                        },
                    ))
                    .child(widgets::number_field(state, 120.)),
            ));
        }

        div()
            .flex()
            .flex_col()
            .child(widgets::section("Appearance"))
            .child(widgets::row(
                "Grid",
                switch("settings-grid", settings.grid, |s, on| s.grid = on),
            ))
            .child(widgets::section("Price scale"))
            .child(widgets::row(
                "Scale",
                widgets::segmented(
                    "settings-scale",
                    &scale_labels,
                    scale_index,
                    move |choice, _window, cx| {
                        edit_chart(&scale_chart, cx, |s| s.scale = ScaleMode::ALL[choice]);
                    },
                ),
            ))
            .child(widgets::row(
                "Invert the scale",
                switch("settings-invert", settings.invert, |s, on| s.invert = on),
            ))
            .child(widgets::section("Time zone"))
            .child(zones)
            .child(widgets::section("Price based chart types"))
            .child(sizes)
            .child(widgets::row(
                "Line break lines",
                widgets::number_field(&self.line_break, 120.),
            ))
            .child(widgets::row(
                "Point and figure reversal (boxes)",
                widgets::number_field(&self.reversal, 120.),
            ))
            .children(
                (settings.kind == ChartKind::Footprint).then(|| {
                    footprint_ui::section(&self.chart, &self.footprint, &settings.footprint)
                }),
            )
    }
}
