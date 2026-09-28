//! The settings of one indicator: its inputs, the look of each of its plots, and how it shows.
//!
//! Every change applies at once, so the chart behind the panel shows the result live. OK keeps
//! the changes, Cancel puts the indicator back as it was, and Escape or the close button keep
//! them.

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Input, InputEvent, InputState};

use super::Chart;
use super::drawing::model::{DASHES, WIDTHS};
use super::study::custom::library::registry;
use super::study::custom::{Problem, Severity};
use super::study::{
    FillStyle, InputKind, LevelStyle, Placement, PlotKind, SOURCES, StudyConfig, StudyKind,
};
use wyck_ui::{button, controls, form, form::Head, form::Tab, icon, modal, number, theme, tokens};

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

    fn level(&self, index: usize, cx: &mut App, change: impl FnOnce(&mut LevelStyle)) {
        self.edit(cx, |study| change(study.levels.entry(index).or_default()));
    }

    fn fill(
        &self,
        index: usize,
        default: FillStyle,
        cx: &mut App,
        change: impl FnOnce(&mut FillStyle),
    ) {
        self.edit(cx, |study| {
            change(study.fills.entry(index).or_insert(default))
        });
    }

    fn band(&self, default: FillStyle, cx: &mut App, change: impl FnOnce(&mut FillStyle)) {
        self.edit(cx, |study| change(study.band.get_or_insert(default)));
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
    bar_widths: Vec<(&'static str, Entity<InputState>)>,
    level_values: Vec<(usize, Entity<InputState>)>,
    level_widths: Vec<(usize, Entity<InputState>)>,
    level_opacity: Vec<(usize, Entity<InputState>)>,
    fill_opacity: Vec<(usize, Entity<InputState>)>,
    band_opacity: Option<Entity<InputState>>,
    name: Entity<InputState>,
    precision: Entity<InputState>,
    color_open: Option<&'static str>,
    up_color_open: Option<&'static str>,
    down_color_open: Option<&'static str>,
    level_color_open: Option<usize>,
    fill_color_open: Option<usize>,
    fill_other_open: Option<usize>,
    band_color_open: bool,
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
                number::state(
                    number::Kind::Declared {
                        step: input.step,
                        decimals,
                    },
                    config.input(input.key),
                    window,
                    cx,
                )
                .min(input.min)
                .max(input.max)
            });
            let key = input.key;
            subscriptions.push(number::watch(&state, cx, move |this, value, cx| {
                this.target.clone().set_input(key, value, cx);
            }));
            fields.push((key, state));
        }
        let mut opacity = Vec::new();
        for plot in config.spec().plots {
            let state = cx.new(|cx| {
                number::state(
                    number::Kind::Share,
                    f64::from(config.plot_style(plot.key).opacity) * 100.0,
                    window,
                    cx,
                )
            });
            let key = plot.key;
            subscriptions.push(number::watch(&state, cx, move |this, value, cx| {
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
                number::state(
                    number::Kind::LineWidth,
                    f64::from(config.plot_style(plot.key).width),
                    window,
                    cx,
                )
            });
            let key = plot.key;
            subscriptions.push(number::watch(&state, cx, move |this, value, cx| {
                let value = value.clamp(0.5, 20.0) as f32;
                this.target
                    .clone()
                    .plot(key, cx, |style| style.width = value);
            }));
            widths.push((key, state));
        }
        let mut bar_widths = Vec::new();
        for plot in config.spec().plots {
            let key = plot.key;
            let state = cx.new(|cx| {
                number::state(
                    number::Kind::Share,
                    f64::from(config.plot_style(key).bar_width) * 100.0,
                    window,
                    cx,
                )
                .min(10.0)
            });
            subscriptions.push(number::watch(&state, cx, move |this, value, cx| {
                this.target
                    .clone()
                    .plot(key, cx, |style| style.bar_width = (value / 100.0) as f32);
            }));
            bar_widths.push((key, state));
        }
        let mut level_values = Vec::new();
        let mut level_widths = Vec::new();
        let mut level_opacity = Vec::new();
        for (index, value) in config.default_levels().into_iter().enumerate() {
            let style = config.levels.get(&index).copied().unwrap_or_default();
            let value = style.value.unwrap_or(value);
            let state = cx.new(|cx| number::state(number::Kind::Level, value, window, cx));
            subscriptions.push(number::watch(&state, cx, move |this, value, cx| {
                this.target
                    .clone()
                    .level(index, cx, |style| style.value = Some(value));
            }));
            level_values.push((index, state));
            let width = cx.new(|cx| {
                number::state(number::Kind::LineWidth, f64::from(style.width), window, cx)
            });
            subscriptions.push(number::watch(&width, cx, move |this, value, cx| {
                this.target
                    .clone()
                    .level(index, cx, |style| style.width = value as f32);
            }));
            level_widths.push((index, width));
            let opacity = cx.new(|cx| {
                number::state(
                    number::Kind::Share,
                    f64::from(style.opacity) * 100.0,
                    window,
                    cx,
                )
            });
            subscriptions.push(number::watch(&opacity, cx, move |this, value, cx| {
                this.target
                    .clone()
                    .level(index, cx, |style| style.opacity = (value / 100.0) as f32);
            }));
            level_opacity.push((index, opacity));
        }
        let mut fill_opacity = Vec::new();
        for index in 0..2 {
            let Some(default) = config.default_fill(index) else {
                continue;
            };
            let style = config.fills.get(&index).copied().unwrap_or(default);
            let state = cx.new(|cx| {
                number::state(
                    number::Kind::Share,
                    f64::from(style.opacity) * 100.0,
                    window,
                    cx,
                )
            });
            subscriptions.push(number::watch(&state, cx, move |this, value, cx| {
                this.target.clone().fill(index, default, cx, |style| {
                    style.opacity = (value / 100.0) as f32
                });
            }));
            fill_opacity.push((index, state));
        }
        let band_opacity = config.default_band().map(|default| {
            let style = config.band.unwrap_or(default);
            let state = cx.new(|cx| {
                number::state(
                    number::Kind::Share,
                    f64::from(style.opacity) * 100.0,
                    window,
                    cx,
                )
            });
            subscriptions.push(number::watch(&state, cx, move |this, value, cx| {
                this.target
                    .clone()
                    .band(default, cx, |style| style.opacity = (value / 100.0) as f32);
            }));
            state
        });
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(config.name.clone())
                .placeholder(config.spec().label)
        });
        subscriptions.push(cx.subscribe(&name, |this, state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let value = state.read(cx).value().to_string();
                this.target.edit(cx, |study| study.name = value);
            }
        }));
        let precision = cx.new(|cx| {
            number::state(
                number::Kind::Count,
                config.precision.unwrap_or(2) as f64,
                window,
                cx,
            )
            .max(8.0)
        });
        subscriptions.push(number::watch(&precision, cx, |this, value, cx| {
            this.target
                .edit(cx, |study| study.precision = Some(value as u32));
        }));
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
            bar_widths,
            level_values,
            level_widths,
            level_opacity,
            fill_opacity,
            band_opacity,
            name,
            precision,
            color_open: None,
            up_color_open: None,
            down_color_open: None,
            level_color_open: None,
            fill_color_open: None,
            fill_other_open: None,
            band_color_open: false,
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
            let text = number::format(config.input(key), 4);
            state.update(cx, |state, cx| state.set_value(text, window, cx));
        }
        for (key, state) in &self.opacity {
            let text = number::format(f64::from(config.plot_style(key).opacity) * 100.0, 0);
            state.update(cx, |state, cx| state.set_value(text, window, cx));
        }
        for (key, state) in &self.widths {
            let text = number::format(f64::from(config.plot_style(key).width), 1);
            state.update(cx, |state, cx| state.set_value(text, window, cx));
        }
        for (key, state) in &self.bar_widths {
            let text = number::format(f64::from(config.plot_style(key).bar_width) * 100.0, 0);
            state.update(cx, |state, cx| state.set_value(text, window, cx));
        }
        for (index, state) in &self.level_values {
            if let Some(value) = config.default_levels().get(*index) {
                let value = config
                    .levels
                    .get(index)
                    .and_then(|s| s.value)
                    .unwrap_or(*value);
                state.update(cx, |state, cx| {
                    state.set_value(number::format(value, 3), window, cx)
                });
            }
        }
        for (index, state) in &self.level_widths {
            let style = config.levels.get(index).copied().unwrap_or_default();
            state.update(cx, |state, cx| {
                state.set_value(number::format(f64::from(style.width), 1), window, cx)
            });
        }
        for (index, state) in &self.level_opacity {
            let style = config.levels.get(index).copied().unwrap_or_default();
            state.update(cx, |state, cx| {
                state.set_value(
                    number::format(f64::from(style.opacity) * 100.0, 0),
                    window,
                    cx,
                )
            });
        }
        for (index, state) in &self.fill_opacity {
            if let Some(default) = config.default_fill(*index) {
                let style = config.fills.get(index).copied().unwrap_or(default);
                state.update(cx, |state, cx| {
                    state.set_value(
                        number::format(f64::from(style.opacity) * 100.0, 0),
                        window,
                        cx,
                    )
                });
            }
        }
        if let (Some(default), Some(state)) = (config.default_band(), &self.band_opacity) {
            let style = config.band.unwrap_or(default);
            state.update(cx, |state, cx| {
                state.set_value(
                    number::format(f64::from(style.opacity) * 100.0, 0),
                    window,
                    cx,
                )
            });
        }
        self.name.update(cx, |state, cx| {
            state.set_value(config.name.clone(), window, cx)
        });
        self.precision.update(cx, |state, cx| {
            state.set_value(config.precision.unwrap_or(2).to_string(), window, cx)
        });
    }

    fn inputs_page(&self, config: &StudyConfig, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.input_rows(config, false, cx);
        let mut page = form::page();
        if let Some(group) = self.script_group(config, cx) {
            page = page.child(group);
        }
        if rows.is_empty() {
            page = page.child(form::note("This indicator has no parameters to change."));
        } else {
            page = page.child(form::group(IconName::SlidersHorizontal, "Parameters", rows));
        }
        if config.kind == StudyKind::Atr {
            page = page.child(form::note(
                "ATR measures volatility, not direction. Price uses the symbol's price scale; % of close compares volatility across price levels. Percent decimals only affects % of close. Enable the Signal average or True range on the Style tab. Signal length and smoothing affect only the Signal average.",
            ));
        }
        page.into_any_element()
    }

    fn input_rows(
        &self,
        config: &StudyConfig,
        style: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let target = &self.target;
        let mut rows = Vec::new();
        let script = config.script.as_deref().and_then(registry::get);
        for input in config.spec().inputs {
            let script_style = script
                .as_ref()
                .and_then(|entry| entry.script.as_ref())
                .and_then(|script| {
                    script
                        .declaration
                        .inputs
                        .iter()
                        .find(|decl| decl.key == input.key)
                })
                .is_some_and(|decl| decl.in_style());
            let in_style = script_style
                || match config.kind {
                    StudyKind::VolumeProfile => matches!(
                        input.key,
                        "area_opacity" | "other_opacity" | "normal_opacity"
                    ),
                    _ => false,
                };
            if in_style != style {
                continue;
            }
            let key = input.key;
            let control: Option<AnyElement> = match input.kind {
                InputKind::Int | InputKind::Float => self
                    .fields
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, state)| number::field(state, tokens::field::WIDE).into_any_element()),
                InputKind::Source => {
                    let target = target.clone();
                    Some(
                        controls::segmented(
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
                        controls::segmented(
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
                        controls::toggle(
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
                    Some(controls::color_swatch(
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
                rows.push(form::field(input.label, None, control));
            }
        }
        rows
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
        rows.push(form::field(
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
                        .text_size(px(tokens::text::BODY))
                        .text_color(theme::muted_fg())
                        .child(about),
                )
                .child(button::action(
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
                    button::action(
                        "study-reveal-script",
                        "Show",
                        Some(IconName::FolderOpen),
                        false,
                        move |_window, cx| crate::indicators::reveal(cx, &path),
                    )
                })),
        ));
        if let Some(entry) = &entry
            && !entry.info.description.is_empty()
        {
            rows.push(form::block(form::note(entry.info.description.clone())));
        }
        for problem in problems.iter().take(4) {
            let error = problem.severity == Severity::Error;
            let place = if problem.line > 0 {
                format!("line {}: ", problem.line)
            } else {
                String::new()
            };
            rows.push(form::block(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .items_start()
                    .text_size(px(tokens::text::BODY))
                    .text_color(if error {
                        theme::destructive()
                    } else {
                        theme::amber()
                    })
                    .child(icon::tinted(
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
        Some(form::group(IconName::CodeXml, "Script", rows).into_any_element())
    }

    fn style_page(&self, config: &StudyConfig, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        let mut page = form::page();
        let style_rows = self.input_rows(config, true, cx);
        if !style_rows.is_empty() {
            page = page.child(form::group(
                IconName::Palette,
                "Indicator style",
                style_rows,
            ));
        } else if config.is_script() && config.spec().plots.is_empty() {
            page = page.child(form::note("This script has no style settings. Declare a color, opacity, width or text size input, or use section: \"style\" on an input."));
        }
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
            let shown = controls::toggle(
                SharedString::from(format!("plot-visible-{key}")),
                style.visible,
                move |on, _window, cx| shown_target.plot(key, cx, |s| s.visible = on),
            )
            .into_any_element();

            let swatch = controls::color_swatch(
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
                .map(|(_, state)| number::field(state, tokens::field::NUMBER));
            let mut rows = vec![form::field(
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

            let kind_target = self.target.clone();
            let displayed_kind = style.kind.unwrap_or(plot.kind);
            if config.kind != StudyKind::VolumeProfile {
                rows.push(form::field(
                    "Plot type",
                    None,
                    controls::segmented(
                        SharedString::from(format!("plot-type-{key}")),
                        &["Line", "Dots", "Columns"],
                        match displayed_kind {
                            PlotKind::Line => 0,
                            PlotKind::Dots => 1,
                            PlotKind::Histogram => 2,
                        },
                        move |choice, _window, cx| {
                            let kind = match choice {
                                1 => PlotKind::Dots,
                                2 => PlotKind::Histogram,
                                _ => PlotKind::Line,
                            };
                            kind_target.plot(key, cx, |s| s.kind = Some(kind));
                        },
                    ),
                ));
            }

            let width_index = WIDTHS.iter().position(|w| (w - style.width).abs() < 0.01);
            let width_target = self.target.clone();
            let picker = controls::width_picker(
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
                        .map(|(_, state)| number::field(state, tokens::field::NARROW)),
                );
            if config.kind == StudyKind::VolumeProfile {
                if key == "poc" {
                    let dash_target = self.target.clone();
                    let dash_index = DASHES.iter().position(|d| *d == style.dash).unwrap_or(0);
                    rows.push(form::field("Width", None, width));
                    rows.push(form::field(
                        "Line style",
                        None,
                        controls::dash_picker(
                            SharedString::from("profile-poc-dash"),
                            dash_index,
                            move |choice, _window, cx| {
                                dash_target.plot(key, cx, |s| s.dash = DASHES[choice])
                            },
                        ),
                    ));
                }
            } else {
                match displayed_kind {
                    PlotKind::Line => {
                        let dash_target = self.target.clone();
                        let dash_index = DASHES.iter().position(|d| *d == style.dash).unwrap_or(0);
                        rows.push(form::field("Width", None, width));
                        rows.push(form::field(
                            "Line style",
                            None,
                            controls::dash_picker(
                                SharedString::from(format!("plot-dash-{key}")),
                                dash_index,
                                move |choice, _window, cx| {
                                    dash_target.plot(key, cx, |s| s.dash = DASHES[choice]);
                                },
                            ),
                        ));
                    }
                    PlotKind::Dots => rows.push(form::field("Size", None, width)),
                    PlotKind::Histogram => {
                        let up_target = self.target.clone();
                        let down_target = self.target.clone();
                        let up_open = self.up_color_open == Some(key);
                        let down_open = self.down_color_open == Some(key);
                        let this_up = this.clone();
                        let this_down = this.clone();
                        rows.push(form::field(
                            "Up color",
                            None,
                            controls::color_swatch(
                                SharedString::from(format!("plot-up-{key}")),
                                style.up_color.unwrap_or(super::study::UP_COLOR),
                                up_open,
                                cx,
                                move |_window, cx| {
                                    this_up.update(cx, |e, cx| {
                                        e.up_color_open = if e.up_color_open == Some(key) {
                                            None
                                        } else {
                                            Some(key)
                                        };
                                        cx.notify();
                                    })
                                },
                                move |color, _window, cx| {
                                    up_target.plot(key, cx, |s| s.up_color = Some(color))
                                },
                            ),
                        ));
                        rows.push(form::field(
                            "Down color",
                            None,
                            controls::color_swatch(
                                SharedString::from(format!("plot-down-{key}")),
                                style.down_color.unwrap_or(super::study::DOWN_COLOR),
                                down_open,
                                cx,
                                move |_window, cx| {
                                    this_down.update(cx, |e, cx| {
                                        e.down_color_open = if e.down_color_open == Some(key) {
                                            None
                                        } else {
                                            Some(key)
                                        };
                                        cx.notify();
                                    })
                                },
                                move |color, _window, cx| {
                                    down_target.plot(key, cx, |s| s.down_color = Some(color))
                                },
                            ),
                        ));
                        if let Some((_, state)) = self.bar_widths.iter().find(|(k, _)| *k == key) {
                            rows.push(form::field(
                                "Column width (%)",
                                None,
                                number::field(state, tokens::field::NUMBER),
                            ));
                        }
                    }
                }
            }
            page = page.child(form::group_with(icon, plot.label, Some(shown), rows));
        }
        for (index, value) in config.default_levels().into_iter().enumerate() {
            let style = config.levels.get(&index).copied().unwrap_or_default();
            let target = self.target.clone();
            let shown = controls::toggle(
                SharedString::from(format!("level-visible-{index}")),
                style.visible,
                move |on, _window, cx| target.level(index, cx, |s| s.visible = on),
            );
            let pick = self.target.clone();
            let open = self.level_color_open == Some(index);
            let owner = this.clone();
            let color = controls::color_swatch(
                SharedString::from(format!("level-color-{index}")),
                style.color,
                open,
                cx,
                move |_window, cx| {
                    owner.update(cx, |e, cx| {
                        e.level_color_open = if e.level_color_open == Some(index) {
                            None
                        } else {
                            Some(index)
                        };
                        cx.notify();
                    })
                },
                move |color, _window, cx| pick.level(index, cx, |s| s.color = color),
            );
            let dash_target = self.target.clone();
            let dash_index = DASHES.iter().position(|d| *d == style.dash).unwrap_or(0);
            let mut rows = vec![form::field("Color", None, color).into_any_element()];
            if let Some((_, state)) = self.level_values.iter().find(|(i, _)| *i == index) {
                rows.push(
                    form::field(
                        "Value",
                        Some("Overrides the calculated level"),
                        number::field(state, tokens::field::WIDE),
                    )
                    .into_any_element(),
                );
            }
            if let Some((_, state)) = self.level_widths.iter().find(|(i, _)| *i == index) {
                rows.push(
                    form::field("Width", None, number::field(state, tokens::field::NUMBER))
                        .into_any_element(),
                );
            }
            if let Some((_, state)) = self.level_opacity.iter().find(|(i, _)| *i == index) {
                rows.push(
                    form::field(
                        "Opacity (%)",
                        None,
                        number::field(state, tokens::field::NUMBER),
                    )
                    .into_any_element(),
                );
            }
            rows.push(
                form::field(
                    "Line style",
                    None,
                    controls::dash_picker(
                        SharedString::from(format!("level-dash-{index}")),
                        dash_index,
                        move |choice, _window, cx| {
                            dash_target.level(index, cx, |s| s.dash = DASHES[choice])
                        },
                    ),
                )
                .into_any_element(),
            );
            page = page.child(form::group_with(
                IconName::Minus,
                format!("Level {} ({})", index + 1, number::format(value, 2)),
                Some(shown.into_any_element()),
                rows,
            ));
        }
        for index in 0..2 {
            let Some(default) = config.default_fill(index) else {
                continue;
            };
            let style = config.fills.get(&index).copied().unwrap_or(default);
            let shown_target = self.target.clone();
            let shown = controls::toggle(
                SharedString::from(format!("fill-visible-{index}")),
                style.visible,
                move |on, _window, cx| shown_target.fill(index, default, cx, |s| s.visible = on),
            );
            let color_target = self.target.clone();
            let owner = this.clone();
            let color = controls::color_swatch(
                SharedString::from(format!("fill-color-{index}")),
                style.color,
                self.fill_color_open == Some(index),
                cx,
                move |_window, cx| {
                    owner.update(cx, |e, cx| {
                        e.fill_color_open = if e.fill_color_open == Some(index) {
                            None
                        } else {
                            Some(index)
                        };
                        cx.notify();
                    })
                },
                move |color, _window, cx| {
                    color_target.fill(index, default, cx, |s| s.color = color)
                },
            );
            let mut rows = vec![form::field("Color", None, color).into_any_element()];
            if let Some(other) = style.other {
                let other_target = self.target.clone();
                let owner = this.clone();
                let other = controls::color_swatch(
                    SharedString::from(format!("fill-other-{index}")),
                    other,
                    self.fill_other_open == Some(index),
                    cx,
                    move |_window, cx| {
                        owner.update(cx, |e, cx| {
                            e.fill_other_open = if e.fill_other_open == Some(index) {
                                None
                            } else {
                                Some(index)
                            };
                            cx.notify();
                        })
                    },
                    move |color, _window, cx| {
                        other_target.fill(index, default, cx, |s| s.other = Some(color))
                    },
                );
                rows.push(form::field("Other color", None, other).into_any_element());
            }
            if let Some((_, state)) = self.fill_opacity.iter().find(|(i, _)| *i == index) {
                rows.push(
                    form::field(
                        "Opacity (%)",
                        None,
                        number::field(state, tokens::field::NUMBER),
                    )
                    .into_any_element(),
                );
            }
            page = page.child(form::group_with(
                IconName::Palette,
                format!("Fill {}", index + 1),
                Some(shown.into_any_element()),
                rows,
            ));
        }
        if let Some(default) = config.default_band() {
            let style = config.band.unwrap_or(default);
            let shown_target = self.target.clone();
            let shown = controls::toggle("band-visible", style.visible, move |on, _window, cx| {
                shown_target.band(default, cx, |s| s.visible = on)
            });
            let color_target = self.target.clone();
            let owner = this.clone();
            let color = controls::color_swatch(
                "band-color",
                style.color,
                self.band_color_open,
                cx,
                move |_window, cx| {
                    owner.update(cx, |e, cx| {
                        e.band_color_open = !e.band_color_open;
                        cx.notify();
                    })
                },
                move |color, _window, cx| color_target.band(default, cx, |s| s.color = color),
            );
            let mut rows = vec![form::field("Color", None, color).into_any_element()];
            if let Some(state) = &self.band_opacity {
                rows.push(
                    form::field(
                        "Opacity (%)",
                        None,
                        number::field(state, tokens::field::NUMBER),
                    )
                    .into_any_element(),
                );
            }
            page = page.child(form::group_with(
                IconName::Palette,
                "Threshold zone",
                Some(shown.into_any_element()),
                rows,
            ));
        }
        page.into_any_element()
    }

    fn display_page(&self, config: &StudyConfig) -> AnyElement {
        let mut page = form::page();
        let target = self.target.clone();
        let mut rows = vec![form::field(
            "Show on the chart",
            Some("Hides the indicator without removing it"),
            controls::toggle("study-visible", config.visible, move |on, _window, cx| {
                target.edit(cx, |s| s.visible = on);
            }),
        )];
        rows.push(form::field(
            "Name",
            Some("Leave empty to use the indicator name"),
            div().w(px(220.)).child(Input::new(&self.name).small()),
        ));
        let axis = self.target.clone();
        rows.push(form::field(
            "Axis value labels",
            None,
            controls::toggle(
                "study-axis-labels",
                config.axis_labels,
                move |on, _window, cx| {
                    axis.edit(cx, |s| s.axis_labels = on);
                },
            ),
        ));
        let legend = self.target.clone();
        rows.push(form::field(
            "Values in legend",
            None,
            controls::toggle(
                "study-legend-values",
                config.legend_values,
                move |on, _window, cx| {
                    legend.edit(cx, |s| s.legend_values = on);
                },
            ),
        ));
        let auto = self.target.clone();
        rows.push(form::field(
            "Automatic precision",
            None,
            controls::toggle(
                "study-auto-precision",
                config.precision.is_none(),
                move |on, _window, cx| {
                    auto.edit(cx, |s| s.precision = if on { None } else { Some(2) });
                },
            ),
        ));
        if config.precision.is_some() {
            rows.push(form::field(
                "Decimal places",
                None,
                number::field(&self.precision, tokens::field::NUMBER),
            ));
        }
        if config.spec().placement == Placement::Pane {
            let target = self.target.clone();
            rows.push(form::field(
                "Pane height",
                Some("Next to the price chart. Drag the divider for any other height"),
                controls::height_picker(
                    "study-height",
                    PANE_HEIGHTS,
                    config.pane_weight(),
                    move |weight, _window, cx| target.edit(cx, |s| s.weight = weight),
                ),
            ));
        }
        page = page.child(form::group(IconName::Eye, "Visibility", rows));
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
                .child(form::empty(IconName::Info, "This indicator was removed."))
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
        let footer = form::footer(
            vec![
                button::action(
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
                button::action(
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
                button::action("study-cancel", "Cancel", None, false, move |window, cx| {
                    cancel.update(cx, |e, cx| {
                        let original = e.original.clone();
                        e.target.edit(cx, |study| *study = original);
                    });
                    modal::close(window, cx);
                })
                .into_any_element(),
                button::action("study-ok", "OK", None, true, |window, cx| {
                    modal::close(window, cx)
                })
                .into_any_element(),
            ],
        );

        form::frame(
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
