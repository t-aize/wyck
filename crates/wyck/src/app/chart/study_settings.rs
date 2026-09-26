//! The settings of one indicator: its inputs, the look of each of its plots, and how it shows.
//!
//! Every change applies at once, so the chart behind the panel shows the result live. OK keeps
//! the changes, Cancel puts the indicator back as it was, and Escape or the close button keep
//! them.

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::InputState;

use super::Chart;
use super::drawing::model::{DASHES, WIDTHS};
use super::study::custom::library::registry;
use super::study::custom::{Problem, Severity};
use super::study::{InputKind, Placement, PlotKind, SOURCES, StudyConfig, StudyKind};
use crate::app::connection::ui::icon_colored;
use crate::app::settings_ui::{self as ui, Head, Tab};
use crate::app::{modal, theme, widgets};

/// How tall a pane is, as the choices the panel offers: a name and its weight against the prices.
const PANE_HEIGHTS: &[(&str, f32)] = &[
    ("Compact", 0.6),
    ("Normal", 1.0),
    ("Tall", 1.6),
    ("Huge", 2.4),
];

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
    /// The width field of each plot, in pixels: any width, beside the presets.
    widths: Vec<(&'static str, Entity<InputState>)>,
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
        for input in config.spec().inputs {
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
            subscriptions.push(widgets::watch_number(&state, cx, move |this, value, cx| {
                this.target.clone().set_input(key, value, cx);
            }));
            fields.push((key, state));
        }
        let mut opacity = Vec::new();
        for plot in config.spec().plots {
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
            subscriptions.push(widgets::watch_number(&state, cx, move |this, value, cx| {
                let value = (value / 100.0).clamp(0.05, 1.0) as f32;
                this.target
                    .clone()
                    .plot(key, cx, |style| style.opacity = value);
            }));
            opacity.push((key, state));
        }
        let mut widths = Vec::new();
        for plot in config.spec().plots {
            let state = cx.new(|cx| {
                widgets::number_state(
                    f64::from(config.plot_style(plot.key).width),
                    0.5,
                    20.0,
                    0.5,
                    1,
                    window,
                    cx,
                )
            });
            let key = plot.key;
            subscriptions.push(widgets::watch_number(&state, cx, move |this, value, cx| {
                let value = value.clamp(0.5, 20.0) as f32;
                this.target
                    .clone()
                    .plot(key, cx, |style| style.width = value);
            }));
            widths.push((key, state));
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
            widths,
            color_open: None,
            _subscriptions: subscriptions,
        }
    }

    fn pages(config: &StudyConfig) -> Vec<Page> {
        let mut pages = Vec::new();
        if !config.spec().inputs.is_empty() || config.is_script() {
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
        for (key, state) in &self.widths {
            let text = widgets::format_number(f64::from(config.plot_style(key).width), 1);
            state.update(cx, |state, cx| state.set_value(text, window, cx));
        }
    }

    fn inputs_page(&self, config: &StudyConfig, cx: &mut Context<Self>) -> AnyElement {
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
                InputKind::Color => {
                    let (this, pick) = (cx.entity(), target.clone());
                    let open = self.color_open == Some(key);
                    Some(widgets::color_swatch(
                        SharedString::from(format!("input-color-{key}")),
                        config.input(key) as u32,
                        open,
                        cx,
                        move |_window, cx| {
                            this.update(cx, |e, cx| {
                                e.color_open = if e.color_open == Some(key) {
                                    None
                                } else {
                                    Some(key)
                                };
                                cx.notify();
                            });
                        },
                        move |color, _window, cx| pick.set_input(key, f64::from(color), cx),
                    ))
                }
            };
            if let Some(control) = control {
                rows.push(ui::field(input.label, None, control));
            }
        }
        let mut page = ui::page();
        if let Some(group) = self.script_group(config, cx) {
            page = page.child(group);
        }
        if rows.is_empty() {
            page = page.child(ui::note("This indicator has no parameters to change."));
        } else {
            page = page.child(ui::group(IconName::SlidersHorizontal, "Parameters", rows));
        }
        if config.kind == StudyKind::Atr {
            page = page.child(ui::note(
                "ATR measures volatility, not direction. Price uses the symbol's price scale; % of close compares volatility across price levels. Percent decimals only affects % of close. Enable the Signal average or True range on the Style tab. Signal length and smoothing affect only the Signal average.",
            ));
        }
        page.into_any_element()
    }

    /// For an indicator written as a script: which script it is, the way to its editor, and what
    /// is wrong with it when something is.
    fn script_group(&self, config: &StudyConfig, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = config.script.clone().filter(|_| config.is_script())?;
        let entry = registry::get(&id);
        let mut problems: Vec<Problem> = entry
            .as_ref()
            .map(|e| e.problems.clone())
            .unwrap_or_default();
        if problems.is_empty()
            && let Some(report) = self.chart.read(cx).script_report(&id)
        {
            problems = report.problems;
        }
        let (edit_chart, edit_id) = (self.chart.clone(), id.clone());
        let reveal_path = entry.as_ref().map(|e| e.path.clone());
        let mut rows = Vec::new();
        let mut about = id.clone();
        if let Some(entry) = &entry {
            let info = &entry.info;
            let mut parts = Vec::new();
            if !info.author.is_empty() {
                parts.push(format!("by {}", info.author));
            }
            if !info.version.is_empty() {
                parts.push(format!("version {}", info.version));
            }
            if !parts.is_empty() {
                about = format!("{id} ({})", parts.join(", "));
            }
        }
        rows.push(ui::field(
            "Script",
            entry
                .as_ref()
                .map(|_| "Written in the indicators folder. Save it in the editor and the chart follows")
                .or(Some("The file is not in the indicators folder")),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .max_w(px(200.))
                        .truncate()
                        .text_size(px(12.))
                        .text_color(theme::muted_fg())
                        .child(about),
                )
                .child(ui::action(
                    "study-edit-script",
                    "Edit",
                    Some(IconName::Pencil),
                    false,
                    move |window, cx| {
                        modal::close(window, cx);
                        let id = edit_id.clone();
                        edit_chart.update(cx, |_, cx| {
                            cx.emit(super::ChartEvent::IndicatorEditor(
                                super::EditorRequest::Edit(id),
                            ));
                        });
                    },
                ))
                .children(reveal_path.map(|path| {
                    ui::action(
                        "study-reveal-script",
                        "Show",
                        Some(IconName::FolderOpen),
                        false,
                        move |_window, cx| crate::app::indicators::reveal(cx, &path),
                    )
                })),
        ));
        if let Some(entry) = &entry
            && !entry.info.description.is_empty()
        {
            rows.push(ui::block(ui::note(entry.info.description.clone())));
        }
        for problem in problems.iter().take(4) {
            let error = problem.severity == Severity::Error;
            let place = if problem.line > 0 {
                format!("line {}: ", problem.line)
            } else {
                String::new()
            };
            rows.push(ui::block(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .items_start()
                    .text_size(px(12.))
                    .text_color(if error {
                        theme::destructive()
                    } else {
                        theme::amber()
                    })
                    .child(icon_colored(
                        if error {
                            IconName::CircleAlert
                        } else {
                            IconName::TriangleAlert
                        },
                        13.,
                        if error {
                            theme::destructive()
                        } else {
                            theme::amber()
                        },
                    ))
                    .child(format!("{place}{}", problem.message)),
            ));
        }
        Some(ui::group(IconName::CodeXml, "Script", rows).into_any_element())
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
            let picker = ui::width_picker(
                // One id per plot: the pickers of several plots must not share state.
                SharedString::from(format!("plot-width-{key}")),
                &WIDTHS,
                width_index,
                move |choice, _window, cx| {
                    width_target.plot(key, cx, |s| s.width = WIDTHS[choice]);
                },
            );
            // The presets, and a field for any other width.
            let width = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(picker)
                .children(
                    self.widths
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, state)| widgets::number_field(state, 84.)),
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
            let target = self.target.clone();
            rows.push(ui::field(
                "Pane height",
                Some("Next to the price chart. Drag the divider for any other height"),
                ui::height_picker(
                    "study-height",
                    PANE_HEIGHTS,
                    config.pane_weight(),
                    move |weight, _window, cx| target.edit(cx, |s| s.weight = weight),
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
        let pages = Self::pages(&config);
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
            Page::Inputs => self.inputs_page(&config, cx),
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
                            let Some(fresh) = e
                                .chart
                                .read(cx)
                                .settings
                                .studies
                                .get(e.index)
                                .map(StudyConfig::fresh)
                            else {
                                return;
                            };
                            let again = fresh.clone();
                            e.target.edit(cx, |study| {
                                let visible = study.visible;
                                *study = again;
                                study.visible = visible;
                            });
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
