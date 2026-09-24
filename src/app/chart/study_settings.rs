//! The settings of one indicator: its inputs, the look of each of its plots, and how it shows.
//!
//! Every change applies at once, so the chart behind the panel shows the result live. OK keeps
//! the changes, Cancel puts the indicator back as it was, and Escape or the close button keep
//! them.

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{InputEvent, InputState};

use super::Chart;
use super::drawing::model::Dash;
use super::study::{InputKind, Placement, PlotKind, SOURCES, StudyConfig, StudyKind};
use crate::app::settings_ui::{self as ui, Head, Tab};
use crate::app::{modal, theme, widgets};

/// How tall a pane is, as the choices the panel offers: a name and its weight against the prices.
const PANE_HEIGHTS: [(&str, f32); 4] = [
    ("Compact", 0.6),
    ("Normal", 1.0),
    ("Tall", 1.6),
    ("Huge", 2.4),
];

/// The widths a line can have.
const WIDTHS: [f32; 4] = [1.0, 1.5, 2.0, 3.0];

const DASHES: [Dash; 3] = [Dash::Solid, Dash::Dashed, Dash::Dotted];

/// Opens the settings of indicator `index` of `chart`.
pub fn open(chart: Entity<Chart>, index: usize, window: &mut Window, cx: &mut App) {
    // Opened once the chart that asked is no longer being updated, since the panel reads it.
    window.defer(cx, move |window, cx| {
        let Some(config) = chart.read(cx).settings.studies.get(index).cloned() else {
            return;
        };
        let editor = cx.new(|cx| StudyEditor::new(chart, index, config, window, cx));
        modal::open(editor, modal::Options::new(780.0, 600.0), window, cx);
    });
}

/// The indicator a panel edits: which chart, where in its list, and of what kind (so a panel left
/// open after the list changed edits nothing).
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

    fn plot(
        &self,
        key: &'static str,
        cx: &mut App,
        change: impl FnOnce(&mut super::study::PlotStyle),
    ) {
        self.edit(cx, |study| {
            if let Some(style) = study.plots.get_mut(key) {
                change(style);
            }
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Inputs,
    Style,
    Display,
}

impl Page {
    fn tab(self) -> Tab {
        match self {
            Self::Inputs => Tab {
                label: "Inputs",
                icon: IconName::SlidersHorizontal,
            },
            Self::Style => Tab {
                label: "Style",
                icon: IconName::Palette,
            },
            Self::Display => Tab {
                label: "Display",
                icon: IconName::Eye,
            },
        }
    }
}

struct StudyEditor {
    target: Target,
    chart: Entity<Chart>,
    index: usize,
    kind: StudyKind,
    /// How the indicator was when the panel opened, for Cancel.
    original: StudyConfig,
    page: Page,
    /// The number fields of the inputs.
    fields: Vec<(&'static str, Entity<InputState>)>,
    /// The opacity field of each plot, in percent.
    opacity: Vec<(&'static str, Entity<InputState>)>,
    color_open: Option<&'static str>,
    _subscriptions: Vec<Subscription>,
}

impl StudyEditor {
    fn new(
        chart: Entity<Chart>,
        index: usize,
        config: StudyConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let target = Target {
            chart: chart.clone(),
            index,
            kind: config.kind,
        };
        let mut subscriptions = vec![cx.observe(&chart, |_this, _chart, cx| cx.notify())];
        let mut fields = Vec::new();
        for input in config.kind.spec().inputs {
            if !matches!(input.kind, InputKind::Int | InputKind::Float) {
                continue;
            }
            let decimals = if input.kind == InputKind::Int { 0 } else { 4 };
            let state = cx.new(|cx| {
                widgets::number_state(
                    config.input(input.key),
                    input.min,
                    input.max,
                    input.step,
                    decimals,
                    window,
                    cx,
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
        let mut opacity = Vec::new();
        for plot in config.kind.spec().plots {
            let state = cx.new(|cx| {
                widgets::number_state(
                    f64::from(config.plot_style(plot.key).opacity) * 100.0,
                    5.0,
                    100.0,
                    5.0,
                    0,
                    window,
                    cx,
                )
            });
            let key = plot.key;
            subscriptions.push(
                cx.subscribe(&state, move |this, state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change | InputEvent::Blur)
                        && let Some(value) = widgets::parse_number(&state.read(cx).value())
                    {
                        let value = (value / 100.0).clamp(0.05, 1.0) as f32;
                        this.target
                            .clone()
                            .plot(key, cx, |style| style.opacity = value);
                    }
                }),
            );
            opacity.push((key, state));
        }
        Self {
            target,
            chart,
            index,
            kind: config.kind,
            original: config,
            page: Page::Inputs,
            fields,
            opacity,
            color_open: None,
            _subscriptions: subscriptions,
        }
    }

    fn pages(&self) -> Vec<Page> {
        let mut pages = Vec::new();
        if !self.kind.spec().inputs.is_empty() {
            pages.push(Page::Inputs);
        }
        pages.push(Page::Style);
        pages.push(Page::Display);
        pages
    }

    /// Puts the values of `config` back in the number fields.
    fn set_fields(&self, config: &StudyConfig, window: &mut Window, cx: &mut App) {
        for (key, state) in &self.fields {
            let text = widgets::format_number(config.input(key), 4);
            state.update(cx, |state, cx| state.set_value(text, window, cx));
        }
        for (key, state) in &self.opacity {
            let text = widgets::format_number(f64::from(config.plot_style(key).opacity) * 100.0, 0);
            state.update(cx, |state, cx| state.set_value(text, window, cx));
        }
    }

    fn inputs_page(&self, config: &StudyConfig) -> AnyElement {
        let target = &self.target;
        let mut rows = Vec::new();
        for input in config.spec().inputs {
            let key = input.key;
            let control: Option<AnyElement> = match input.kind {
                InputKind::Int | InputKind::Float => self
                    .fields
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, state)| widgets::number_field(state, 130.).into_any_element()),
                InputKind::Source => {
                    let target = target.clone();
                    Some(
                        widgets::segmented(
                            SharedString::from(format!("source-{key}")),
                            &SOURCES,
                            config.input(key) as usize,
                            move |choice, _window, cx| target.set_input(key, choice as f64, cx),
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
                            move |choice, _window, cx| target.set_input(key, choice as f64, cx),
                        )
                        .into_any_element(),
                    )
                }
                InputKind::Toggle => {
                    let target = target.clone();
                    Some(
                        ui::toggle(
                            SharedString::from(format!("toggle-{key}")),
                            config.input(key) != 0.0,
                            move |on, _window, cx| {
                                target.set_input(key, if on { 1.0 } else { 0.0 }, cx);
                            },
                        )
                        .into_any_element(),
                    )
                }
            };
            if let Some(control) = control {
                rows.push(ui::field(input.label, None, control));
            }
        }
        ui::page()
            .child(ui::group(IconName::SlidersHorizontal, "Parameters", rows))
            .into_any_element()
    }

    fn style_page(&self, config: &StudyConfig, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        let mut page = ui::page();
        for plot in config.spec().plots {
            let key = plot.key;
            let style = config.plot_style(key);
            let open = self.color_open == Some(key);
            let toggle_this = this.clone();
            let (color_target, shown_target) = (self.target.clone(), self.target.clone());
            let icon = match plot.kind {
                PlotKind::Line => IconName::Spline,
                PlotKind::Histogram => IconName::ChartColumn,
                PlotKind::Dots => IconName::CircleDot,
            };
            let shown = ui::toggle(
                SharedString::from(format!("plot-visible-{key}")),
                style.visible,
                move |on, _window, cx| shown_target.plot(key, cx, |s| s.visible = on),
            )
            .into_any_element();

            let swatch = widgets::color_swatch(
                SharedString::from(format!("plot-color-{key}")),
                style.color,
                open,
                cx,
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
                move |color, _window, cx| color_target.plot(key, cx, |s| s.color = color),
            );
            let opacity = self
                .opacity
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, state)| widgets::number_field(state, 96.));
            let mut rows = vec![ui::field(
                "Color",
                Some("Opacity in percent"),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(swatch)
                    .children(opacity),
            )];

            let width_index = WIDTHS.iter().position(|w| (w - style.width).abs() < 0.01);
            let width_target = self.target.clone();
            let width = ui::width_picker(
                // One id per plot: the pickers of several plots must not share state.
                SharedString::from(format!("plot-width-{key}")),
                &WIDTHS,
                width_index,
                move |choice, _window, cx| {
                    width_target.plot(key, cx, |s| s.width = WIDTHS[choice]);
                },
            );
            match plot.kind {
                PlotKind::Line => {
                    let dash_target = self.target.clone();
                    let dash_index = DASHES.iter().position(|d| *d == style.dash).unwrap_or(0);
                    rows.push(ui::field("Width", None, width));
                    rows.push(ui::field(
                        "Line style",
                        None,
                        ui::dash_picker(
                            SharedString::from(format!("plot-dash-{key}")),
                            dash_index,
                            move |choice, _window, cx| {
                                dash_target.plot(key, cx, |s| s.dash = DASHES[choice]);
                            },
                        ),
                    ));
                }
                PlotKind::Dots => rows.push(ui::field("Size", None, width)),
                PlotKind::Histogram => {}
            }
            page = page.child(ui::group_with(icon, plot.label, Some(shown), rows));
        }
        page.into_any_element()
    }

    fn display_page(&self, config: &StudyConfig) -> AnyElement {
        let mut page = ui::page();
        let target = self.target.clone();
        let mut rows = vec![ui::field(
            "Show on the chart",
            Some("Hides the indicator without removing it"),
            ui::toggle("study-visible", config.visible, move |on, _window, cx| {
                target.edit(cx, |s| s.visible = on);
            }),
        )];
        if config.spec().placement == Placement::Pane {
            let names: Vec<&str> = PANE_HEIGHTS.iter().map(|(name, _)| *name).collect();
            let current = PANE_HEIGHTS
                .iter()
                .position(|(_, w)| (w - config.pane_weight()).abs() < 0.05)
                .unwrap_or(usize::MAX);
            let target = self.target.clone();
            rows.push(ui::field(
                "Pane height",
                Some("Next to the price chart. Drag the divider for any other height"),
                widgets::segmented(
                    "study-height",
                    &names,
                    current,
                    move |choice, _window, cx| {
                        target.edit(cx, |s| s.weight = PANE_HEIGHTS[choice].1);
                    },
                ),
            ));
        }
        page = page.child(ui::group(IconName::Eye, "Visibility", rows));
        page.into_any_element()
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
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .rounded_xl()
                .border_1()
                .border_color(theme::border_subtle())
                .bg(theme::bg())
                .child(ui::empty(IconName::Info, "This indicator was removed."))
                .into_any_element();
        };
        let pages = self.pages();
        if !pages.contains(&self.page) {
            self.page = pages[0];
        }
        let active = pages.iter().position(|p| *p == self.page).unwrap_or(0);
        let tabs: Vec<Tab> = pages.iter().map(|p| p.tab()).collect();

        let spec = config.spec();
        let head = Head {
            icon: match spec.placement {
                Placement::Overlay => IconName::ChartLine,
                Placement::Pane => IconName::Activity,
            },
            title: spec.label.into(),
            subtitle: format!(
                "{} - {}",
                config.title(),
                match spec.placement {
                    Placement::Overlay => "drawn on the prices",
                    Placement::Pane => "in a pane of its own",
                }
            )
            .into(),
        };

        let body = match self.page {
            Page::Inputs => self.inputs_page(&config),
            Page::Style => self.style_page(&config, cx),
            Page::Display => self.display_page(&config),
        };

        let this = cx.entity();
        let (defaults, cancel, remove) = (cx.entity(), cx.entity(), cx.entity());
        let footer = ui::footer(
            vec![
                ui::action(
                    "study-defaults",
                    "Defaults",
                    Some(IconName::RotateCcw),
                    false,
                    move |window, cx| {
                        defaults.update(cx, |e, cx| {
                            let kind = e.kind;
                            e.target.edit(cx, |study| {
                                let visible = study.visible;
                                *study = StudyConfig::new(kind);
                                study.visible = visible;
                            });
                            let fresh = StudyConfig::new(kind);
                            e.set_fields(&fresh, window, cx);
                        });
                    },
                )
                .into_any_element(),
                ui::action(
                    "study-remove",
                    "Remove",
                    Some(IconName::Trash),
                    false,
                    move |window, cx| {
                        let (chart, index) = {
                            let e = remove.read(cx);
                            (e.chart.clone(), e.index)
                        };
                        chart.update(cx, |chart, cx| chart.remove_study(index, cx));
                        modal::close(window, cx);
                    },
                )
                .into_any_element(),
            ],
            vec![
                ui::action("study-cancel", "Cancel", None, false, move |window, cx| {
                    cancel.update(cx, |e, cx| {
                        let original = e.original.clone();
                        e.target.edit(cx, |study| *study = original);
                    });
                    modal::close(window, cx);
                })
                .into_any_element(),
                ui::action("study-ok", "OK", None, true, |window, cx| {
                    modal::close(window, cx)
                })
                .into_any_element(),
            ],
        );

        ui::frame(
            head,
            &tabs,
            active,
            move |index, _window, cx| {
                let page = pages[index];
                this.update(cx, |e, cx| {
                    e.page = page;
                    e.color_open = None;
                    cx.notify();
                });
            },
            modal::dismiss,
            body,
            footer,
        )
        .into_any_element()
    }
}
