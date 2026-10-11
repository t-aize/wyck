//! The settings of one indicator: its inputs, the look of each of its plots, and how it shows.
//!
//! Every change applies at once, so the chart behind the panel shows the result live. OK keeps
//! the changes, Cancel puts the indicator back as it was, and Escape or the close button keep
//! them.

pub mod inputs;
pub mod style;

use super::Chart;
use super::study::config::StudyConfig;
use super::study::kind::StudyKind;
use super::study::spec::{InputKind, Placement};
use super::study::style::{FillStyle, LevelStyle};
use crate::ui::kit::icon::IconName;
use crate::ui::kit::input::{InputEvent, InputState};
use crate::ui::kit::{button, controls, form, form::Head, form::Tab, modal, number, theme, tokens};
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div, px};

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
        modal::open(
            editor,
            modal::Options::new(780.0, 600.0).label("Indicator settings"),
            window,
            cx,
        );
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

    fn set_text(&self, key: &'static str, value: String, cx: &mut App) {
        self.edit(cx, |study| {
            study.texts.insert(key.to_owned(), value);
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
        change: impl FnOnce(&mut super::study::style::PlotStyle),
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
    /// The text fields of the inputs that are text: symbols, timeframes, sessions, free text.
    texts: Vec<(&'static str, Entity<InputState>)>,
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
        let mut texts = Vec::new();
        for input in config.spec().inputs {
            if !input.kind.is_text() {
                continue;
            }
            let placeholder = match input.kind {
                InputKind::Symbol => "The symbol of the chart",
                InputKind::Timeframe => "The timeframe of the chart, or e.g. 4h",
                InputKind::Session => "e.g. 0930-1600",
                _ => "",
            };
            let value = config.text(input.key);
            let state = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
            });
            let key = input.key;
            subscriptions.push(
                cx.subscribe(&state, move |this, state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value = state.read(cx).value().trim().to_owned();
                        this.target.clone().set_text(key, value, cx);
                    }
                }),
            );
            texts.push((key, state));
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
            texts,
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

/// The heading of a group of inputs.
fn group_heading(name: &str) -> AnyElement {
    div()
        .pt_2()
        .text_size(px(tokens::text::small()))
        .text_color(crate::ui::kit::theme::muted_fg())
        .child(name.to_uppercase())
        .into_any_element()
}

/// The timeframes offered beside a timeframe input; the first one is the chart's own.
const TIMEFRAME_PICKS: [(&str, &str); 8] = [
    ("Chart", ""),
    ("1m", "1m"),
    ("5m", "5m"),
    ("15m", "15m"),
    ("1h", "1h"),
    ("4h", "4h"),
    ("1D", "1D"),
    ("1W", "1W"),
];

/// A row of quick picks above the text field of a timeframe input.
fn timeframe_picks(
    key: &'static str,
    target: Target,
    field: Entity<InputState>,
    current: String,
) -> gpui::Div {
    let labels: Vec<&str> = TIMEFRAME_PICKS.iter().map(|(label, _)| *label).collect();
    let chosen = TIMEFRAME_PICKS
        .iter()
        .position(|(_, code)| *code == current)
        .unwrap_or(usize::MAX);
    div().flex().flex_col().gap_2().child(controls::segmented(
        SharedString::from(format!("timeframe-{key}")),
        &labels,
        chosen,
        move |index, window, cx| {
            let code = TIMEFRAME_PICKS[index].1;
            // Setting a field's value does not announce a change, so the study is told here.
            target.set_text(key, code.to_owned(), cx);
            field.update(cx, |state, cx| state.set_value(code, window, cx));
        },
    ))
}
