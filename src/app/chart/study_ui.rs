//! The dialogs of a chart: the list of indicators to add, the settings of one indicator (its
//! inputs and the look of each of its lines), and the settings of the chart itself (grid, volume,
//! price scale, time zone, and the sizes of the price based chart types).
//!
//! Every change applies at once, as the user makes it, so the chart behind the dialog shows the
//! result live; closing the dialog keeps it.

use gpui::prelude::*;
use gpui::{App, Context, Entity, SharedString, Subscription, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Sizable, StyledExt as _};
use wyck::openapi::market::PRICE_SCALE;

use super::Chart;
use super::settings::{ChartKind, ScaleMode};
use super::study::{InputKind, Placement, SOURCES, StudyConfig, StudyKind};
use super::transform::BoxSize;
use super::zone::Zone;
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

// ---- one indicator's settings ----

/// Opens the settings of indicator `index` of `chart`.
pub fn open_study_settings(chart: Entity<Chart>, index: usize, window: &mut Window, cx: &mut App) {
    // Opened once the chart that asked is no longer being updated, since the dialog reads it.
    window.defer(cx, move |window, cx| {
        let Some(config) = chart.read(cx).settings.studies.get(index).cloned() else {
            return;
        };
        let editor = cx.new(|cx| StudyEditor::new(chart, index, &config, window, cx));
        let title = config.kind.spec().label;
        window.open_dialog(cx, move |dialog, _window, _cx| {
            dialog.title(title).w(px(480.)).child(editor.clone())
        });
    });
}

/// The indicator a dialog edits: which chart, where in its list, and of what kind (so a dialog
/// left open after the list changed edits nothing).
#[derive(Clone)]
struct Target {
    chart: Entity<Chart>,
    index: usize,
    kind: StudyKind,
}

impl Target {
    fn edit(&self, cx: &mut App, change: impl FnOnce(&mut StudyConfig)) {
        let (index, kind) = (self.index, self.kind);
        self.chart.update(cx, |chart, cx| {
            chart.edit_settings(cx, |settings| {
                if let Some(study) = settings.studies.get_mut(index).filter(|s| s.kind == kind) {
                    change(study);
                }
            });
        });
    }

    fn set_input(&self, key: &'static str, value: f64, cx: &mut App) {
        self.edit(cx, |study| {
            study.inputs.insert(key.to_owned(), value);
        });
    }
}

struct StudyEditor {
    target: Target,
    chart: Entity<Chart>,
    index: usize,
    kind: StudyKind,
    fields: Vec<(&'static str, Entity<InputState>)>,
    color_open: Option<&'static str>,
    _subscriptions: Vec<Subscription>,
}

impl StudyEditor {
    fn new(
        chart: Entity<Chart>,
        index: usize,
        config: &StudyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut fields = Vec::new();
        let mut subscriptions = vec![cx.observe(&chart, |_this, _chart, cx| cx.notify())];
        for input in config.kind.spec().inputs {
            if !matches!(input.kind, InputKind::Int | InputKind::Float) {
                continue;
            }
            let decimals = if input.kind == InputKind::Int { 0 } else { 4 };
            let value = config.input(input.key);
            let state = cx.new(|cx| {
                widgets::number_state(
                    value, input.min, input.max, input.step, decimals, window, cx,
                )
            });
            let key = input.key;
            subscriptions.push(
                cx.subscribe(&state, move |this, state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change | InputEvent::Blur)
                        && let Some(value) = widgets::parse_number(&state.read(cx).value())
                    {
                        this.target.clone().set_input(key, value, cx);
                    }
                }),
            );
            fields.push((key, state));
        }
        Self {
            target: Target {
                chart: chart.clone(),
                index,
                kind: config.kind,
            },
            chart,
            index,
            kind: config.kind,
            fields,
            color_open: None,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for StudyEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(config) = self
            .chart
            .read(cx)
            .settings
            .studies
            .get(self.index)
            .filter(|s| s.kind == self.kind)
            .cloned()
        else {
            return div()
                .py_4()
                .text_color(theme::muted_fg())
                .child("This indicator was removed.")
                .into_any_element();
        };
        let spec = config.kind.spec();
        let this = cx.entity();
        let target = self.target.clone();
        let mut body = div().flex().flex_col();
        if !spec.inputs.is_empty() {
            body = body.child(widgets::section("Inputs"));
        }
        for input in spec.inputs {
            let key = input.key;
            let control = match input.kind {
                InputKind::Int | InputKind::Float => self
                    .fields
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, state)| widgets::number_field(state, 150.).into_any_element()),
                InputKind::Source => {
                    let target = target.clone();
                    Some(
                        widgets::segmented(
                            SharedString::from(format!("source-{key}")),
                            &SOURCES,
                            config.input(key) as usize,
                            move |choice, _window, cx| {
                                target.set_input(key, choice as f64, cx);
                            },
                        )
                        .into_any_element(),
                    )
                }
                InputKind::Choice(options) => {
                    let target = target.clone();
                    Some(
                        widgets::segmented(
                            SharedString::from(format!("choice-{key}")),
                            options,
                            config.input(key) as usize,
                            move |choice, _window, cx| {
                                target.set_input(key, choice as f64, cx);
                            },
                        )
                        .into_any_element(),
                    )
                }
                InputKind::Toggle => {
                    let target = target.clone();
                    Some(
                        Switch::new(SharedString::from(format!("toggle-{key}")))
                            .checked(config.input(key) != 0.0)
                            .on_click(move |checked, _window, cx| {
                                target.set_input(key, if *checked { 1.0 } else { 0.0 }, cx);
                            })
                            .into_any_element(),
                    )
                }
            };
            if let Some(control) = control {
                body = body.child(widgets::row(input.label, control));
            }
        }
        body = body.child(widgets::section("Style"));
        for plot in spec.plots {
            let key = plot.key;
            let style = config.plot_style(key);
            let open = self.color_open == Some(key);
            let (toggle_this, pick_this) = (this.clone(), this.clone());
            let (pick_target, width_target, visible_target) =
                (target.clone(), target.clone(), target.clone());
            let widths = ["1", "1.5", "2", "3"];
            let width_index = [1.0, 1.5, 2.0, 3.0]
                .iter()
                .position(|w| (w - style.width).abs() < 0.01)
                .unwrap_or(0);
            body = body.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .min_h(px(36.))
                    .child(
                        Switch::new(SharedString::from(format!("plot-visible-{key}")))
                            .small()
                            .checked(style.visible)
                            .on_click(move |checked, _window, cx| {
                                let checked = *checked;
                                visible_target.edit(cx, |study| {
                                    if let Some(s) = study.plots.get_mut(key) {
                                        s.visible = checked;
                                    }
                                });
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .text_color(theme::fg())
                            .child(plot.label),
                    )
                    .child(widgets::color_swatch(
                        SharedString::from(format!("plot-color-{key}")),
                        style.color,
                        open,
                        move |_window, cx| {
                            toggle_this.update(cx, |this, cx| {
                                this.color_open = if this.color_open == Some(key) {
                                    None
                                } else {
                                    Some(key)
                                };
                                cx.notify();
                            });
                        },
                        move |color, _window, cx| {
                            pick_this.update(cx, |this, cx| {
                                this.color_open = None;
                                cx.notify();
                            });
                            pick_target.edit(cx, |study| {
                                if let Some(s) = study.plots.get_mut(key) {
                                    s.color = color;
                                }
                            });
                        },
                    ))
                    .child(widgets::segmented(
                        SharedString::from(format!("plot-width-{key}")),
                        &widths,
                        width_index,
                        move |choice, _window, cx| {
                            let width = [1.0, 1.5, 2.0, 3.0][choice];
                            width_target.edit(cx, |study| {
                                if let Some(s) = study.plots.get_mut(key) {
                                    s.width = width;
                                }
                            });
                        },
                    )),
            );
        }
        let reset_this = this.clone();
        let reset_target = target.clone();
        let chart = self.chart.clone();
        let index = self.index;
        body.child(widgets::divider())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .child(
                        Button::new("study-defaults")
                            .ghost()
                            .small()
                            .icon(IconName::RotateCcw)
                            .label("Defaults")
                            .on_click(move |_, window, cx| {
                                let kind = reset_target.kind;
                                reset_target.edit(cx, |study| {
                                    let visible = study.visible;
                                    *study = StudyConfig::new(kind);
                                    study.visible = visible;
                                });
                                // The number fields show the defaults again.
                                let config = StudyConfig::new(kind);
                                let fields = reset_this.read(cx).fields.clone();
                                for (key, state) in fields {
                                    let value = config.input(key);
                                    state.update(cx, |state, cx| {
                                        state.set_value(
                                            widgets::format_number(value, 4),
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new("study-remove")
                            .ghost()
                            .small()
                            .icon(IconName::Trash)
                            .label("Remove")
                            .on_click(move |_, window, cx| {
                                chart.update(cx, |chart, cx| chart.remove_study(index, cx));
                                window.close_dialog(cx);
                            }),
                    ),
            )
            .into_any_element()
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
        Self {
            chart,
            sizes,
            line_break,
            reversal,
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
                Switch::new(id)
                    .checked(on)
                    .on_click(move |checked, _window, cx| {
                        let checked = *checked;
                        edit_chart(&chart, cx, |s| change(s, checked));
                    })
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
            .child(widgets::row(
                "Volume at the bottom of the prices",
                switch("settings-volume", settings.volume, |s, on| s.volume = on),
            ))
            .child(widgets::row(
                "Sell and buy buttons",
                switch("settings-trade", settings.trade_buttons, |s, on| {
                    s.trade_buttons = on;
                }),
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
    }
}
