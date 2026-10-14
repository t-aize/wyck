//! Building the editor of an indicator and filling its fields.

use super::{App, Context, Entity, InputEvent, InputKind, InputState, StudyConfig, Window, number};
use super::{Chart, Page, StudyEditor, Target};
use gpui::prelude::*;

impl StudyEditor {
    pub(super) fn new(
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

    /// Puts the values of `config` back in the number fields.
    pub(super) fn set_fields(&self, config: &StudyConfig, window: &mut Window, cx: &mut App) {
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
