//! The settings of one drawing: its look (lines, fill, levels, extension), its words, the exact
//! place of its points, and the timeframes it shows on.
//!
//! Every change shows on the charts at once. Closing the dialog keeps the changes as one undo
//! step; Cancel puts the drawing back as it was.

use gpui::prelude::*;
use gpui::{App, Context, Entity, SharedString, Subscription, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable, Sizable, WindowExt};
use wyck::openapi::market::PRICE_SCALE;

use super::drawing::Drawings;
use super::drawing::extras::{ICONS, icon_key};
use super::drawing::figures::wave_names;
use super::drawing::model::{DEGREES, Dash, Drawing, Level, MAX_LEVELS, Point, Tool, wave_label};
use super::timeframe::GROUPS;
use super::zone::Zone;
use crate::app::{theme, widgets};

/// Opens the settings of drawing `id` of `symbol`. Prices are shown with `digits` decimals and
/// times in `zone`.
pub fn open(
    drawings: Entity<Drawings>,
    symbol: String,
    id: u64,
    zone: Zone,
    digits: u32,
    window: &mut Window,
    cx: &mut App,
) {
    // Opened once whatever asked is done updating, since the dialog reads the drawings.
    window.defer(cx, move |window, cx| {
        let Some(drawing) = drawings.read(cx).book().get(&symbol, id).cloned() else {
            return;
        };
        let editor =
            cx.new(|cx| DrawingProps::new(drawings, symbol, &drawing, zone, digits, window, cx));
        let title = drawing.title();
        let (cancel, ok, closed) = (editor.clone(), editor.clone(), editor.clone());
        let template = editor.clone();
        let reset = editor.clone();
        window.open_dialog(cx, move |dialog, _window, cx| {
            let has_template = template.read(cx).has_template(cx);
            let (cancel, ok, closed) = (cancel.clone(), ok.clone(), closed.clone());
            let (template, reset) = (template.clone(), reset.clone());
            dialog
                .title(title.clone())
                .w(px(500.))
                .overlay_closable(false)
                .child(editor.clone())
                .on_close(move |_, _window, cx| closed.update(cx, |e, cx| e.finish(true, cx)))
                .footer(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("drawing-template")
                                .cursor_pointer()
                                .ghost()
                                .small()
                                .icon(IconName::Save)
                                .label("Save as default")
                                .tooltip("New drawings of this tool start with this look")
                                .on_click(move |_, _window, cx| {
                                    template.update(cx, |e, cx| e.save_template(cx));
                                }),
                        )
                        .child(
                            Button::new("drawing-reset")
                                .cursor_pointer()
                                .ghost()
                                .small()
                                .icon(IconName::RotateCcw)
                                .label(if has_template { "Reset" } else { "Reset look" })
                                .tooltip("Back to the look the tool starts with")
                                .on_click(move |_, window, cx| {
                                    reset.update(cx, |e, cx| e.reset_style(window, cx));
                                }),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new("drawing-cancel")
                                .cursor_pointer()
                                .ghost()
                                .small()
                                .label("Cancel")
                                .on_click(move |_, window, cx| {
                                    cancel.update(cx, |e, cx| e.finish(false, cx));
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("drawing-ok")
                                .cursor_pointer()
                                .primary()
                                .small()
                                .label("OK")
                                .on_click(move |_, window, cx| {
                                    ok.update(cx, |e, cx| e.finish(true, cx));
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Style,
    Text,
    Coordinates,
    Visibility,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Self::Style => "Style",
            Self::Text => "Text",
            Self::Coordinates => "Coordinates",
            Self::Visibility => "Visibility",
        }
    }
}

/// Which color's palette is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Swatch {
    Line,
    Fill,
    Text,
    Level(usize),
}

struct DrawingProps {
    drawings: Entity<Drawings>,
    symbol: String,
    id: u64,
    tool: Tool,
    /// The drawings of the symbol when the dialog opened, for Cancel and for undo.
    before: Vec<Drawing>,
    finished: bool,
    tab: Tab,
    zone: Zone,
    digits: u32,
    swatch: Option<Swatch>,
    opacity: Entity<InputState>,
    text_size: Entity<InputState>,
    text: Entity<TextareaState>,
    name: Entity<InputState>,
    prices: Vec<Entity<InputState>>,
    times: Vec<Entity<InputState>>,
    levels: Vec<Entity<InputState>>,
    /// Set when levels were added or removed: their fields are made again at the next render.
    levels_changed: bool,
    _subscriptions: Vec<Subscription>,
    _level_subscriptions: Vec<Subscription>,
}

/// A field that calls `on_value` with its number whenever it holds a valid one.
fn watch_number(
    state: &Entity<InputState>,
    cx: &mut Context<DrawingProps>,
    on_value: impl Fn(&mut DrawingProps, f64, &mut Context<DrawingProps>) + 'static,
) -> Subscription {
    cx.subscribe(state, move |this, state, event: &InputEvent, cx| {
        if matches!(event, InputEvent::Change | InputEvent::Blur)
            && let Some(value) = widgets::parse_number(&state.read(cx).value())
        {
            on_value(this, value, cx);
        }
    })
}

impl DrawingProps {
    fn new(
        drawings: Entity<Drawings>,
        symbol: String,
        drawing: &Drawing,
        zone: Zone,
        digits: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let before = drawings.read(cx).book().drawings(&symbol).to_vec();
        let style = &drawing.style;
        let opacity = cx.new(|cx| {
            widgets::number_state(
                f64::from(style.fill_opacity) * 100.0,
                0.0,
                100.0,
                5.0,
                0,
                window,
                cx,
            )
        });
        let text_size = cx.new(|cx| {
            widgets::number_state(f64::from(style.text_size), 6.0, 48.0, 1.0, 0, window, cx)
        });
        let text = cx.new(|cx| TextareaState::new(window, cx).default_value(drawing.text.clone()));
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(drawing.name.clone())
                .placeholder(drawing.tool.label())
        });
        let mut subscriptions = vec![
            cx.observe(&drawings, |_this, _drawings, cx| cx.notify()),
            watch_number(&opacity, cx, |this, value, cx| {
                this.change(cx, |d| d.style.fill_opacity = (value / 100.0) as f32);
            }),
            watch_number(&text_size, cx, |this, value, cx| {
                this.change(cx, |d| d.style.text_size = value as f32);
            }),
            cx.subscribe(&text, |this, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = state.read(cx).value().to_string();
                    this.change(cx, |d| d.text = value);
                }
            }),
            cx.subscribe(&name, |this, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = state.read(cx).value().to_string();
                    this.change(cx, |d| d.name = value);
                }
            }),
        ];
        let mut prices = Vec::new();
        let mut times = Vec::new();
        if !drawing.tool.is_freehand() {
            for (index, point) in drawing.points.iter().enumerate() {
                let price = cx.new(|cx| {
                    InputState::new(window, cx).default_value(format_real(point.p, digits))
                });
                subscriptions.push(watch_number(&price, cx, move |this, value, cx| {
                    let raw = value * PRICE_SCALE as f64;
                    this.change(cx, |d| set_point(d, index, None, Some(raw)));
                }));
                let time = cx.new(|cx| {
                    InputState::new(window, cx)
                        .default_value(format_time(zone, point.t))
                        .placeholder("YYYY-MM-DD HH:MM")
                });
                subscriptions.push(cx.subscribe(
                    &time,
                    move |this, state, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change | InputEvent::Blur)
                            && let Some(t) = parse_time(this.zone, &state.read(cx).value())
                        {
                            this.change(cx, |d| set_point(d, index, Some(t), None));
                        }
                    },
                ));
                prices.push(price);
                times.push(time);
            }
        }
        let mut this = Self {
            drawings,
            symbol,
            id: drawing.id,
            tool: drawing.tool,
            before,
            finished: false,
            tab: Tab::Style,
            zone,
            digits,
            swatch: None,
            opacity,
            text_size,
            text,
            name,
            prices,
            times,
            levels: Vec::new(),
            levels_changed: false,
            _subscriptions: subscriptions,
            _level_subscriptions: Vec::new(),
        };
        this.make_level_fields(drawing, window, cx);
        this
    }

    fn make_level_fields(
        &mut self,
        drawing: &Drawing,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.levels.clear();
        self._level_subscriptions.clear();
        for (index, level) in drawing.levels().iter().enumerate() {
            let state =
                cx.new(|cx| widgets::number_state(level.value, -100.0, 100.0, 0.1, 4, window, cx));
            self._level_subscriptions
                .push(watch_number(&state, cx, move |this, value, cx| {
                    this.change(cx, |d| {
                        d.levels = d.levels();
                        if let Some(level) = d.levels.get_mut(index) {
                            level.value = value;
                        }
                    });
                }));
            self.levels.push(state);
        }
        self.levels_changed = false;
    }

    fn current(&self, cx: &App) -> Option<Drawing> {
        self.drawings
            .read(cx)
            .book()
            .get(&self.symbol, self.id)
            .cloned()
    }

    fn has_template(&self, cx: &App) -> bool {
        self.drawings.read(cx).book().has_template(self.tool)
    }

    /// Changes the drawing and shows it at once.
    fn change(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Drawing)) {
        let Some(mut drawing) = self.current(cx) else {
            return;
        };
        change(&mut drawing);
        let symbol = self.symbol.clone();
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.preview(&symbol, drawing))
        });
        cx.notify();
    }

    /// Ends the dialog: keeps the changes as one undo step, or puts the drawing back.
    fn finish(&mut self, keep: bool, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.finished, true) {
            return;
        }
        let (symbol, before) = (self.symbol.clone(), std::mem::take(&mut self.before));
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| {
                if keep {
                    book.settle(&symbol, before)
                } else {
                    book.revert(&symbol, before)
                }
            })
        });
    }

    fn save_template(&mut self, cx: &mut Context<Self>) {
        let (symbol, id) = (self.symbol.clone(), self.id);
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.save_template(&symbol, id))
        });
        crate::app::toast::show(
            cx,
            crate::app::toast::Kind::Success,
            "Saved as default",
            format!(
                "New {} drawings start with this look.",
                self.tool.label().to_lowercase()
            ),
        );
    }

    /// Back to the look new drawings of the tool start with; pressed again with a saved default,
    /// back to the built-in look (and the saved default is forgotten).
    fn reset_style(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tool = self.tool;
        let (style, levels) = self.drawings.read(cx).book().starting_style(tool);
        let same = self
            .current(cx)
            .is_some_and(|d| d.style == style && d.levels == levels);
        let (style, levels) = if same && self.has_template(cx) {
            self.drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| book.forget_template(tool))
            });
            (tool.default_style(), Vec::new())
        } else {
            (style, levels)
        };
        self.change(cx, |d| {
            d.style = style;
            d.levels = levels;
        });
        self.set_fields(window, cx);
    }

    /// Puts the values of the drawing back in the number fields.
    fn set_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drawing) = self.current(cx) else {
            return;
        };
        let opacity = widgets::format_number(f64::from(drawing.style.fill_opacity) * 100.0, 0);
        let size = widgets::format_number(f64::from(drawing.style.text_size), 0);
        self.opacity
            .update(cx, |s, cx| s.set_value(opacity, window, cx));
        self.text_size
            .update(cx, |s, cx| s.set_value(size, window, cx));
        self.make_level_fields(&drawing, window, cx);
    }

    fn toggle_swatch(&mut self, swatch: Swatch, cx: &mut Context<Self>) {
        self.swatch = if self.swatch == Some(swatch) {
            None
        } else {
            Some(swatch)
        };
        cx.notify();
    }

    fn swatch(
        &self,
        swatch: Swatch,
        color: u32,
        id: &str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (toggle, pick) = (cx.entity(), cx.entity());
        widgets::color_swatch(
            SharedString::from(id.to_owned()),
            color,
            self.swatch == Some(swatch),
            cx,
            move |_window, cx| toggle.update(cx, |e, cx| e.toggle_swatch(swatch, cx)),
            move |color, _window, cx| {
                pick.update(cx, |e, cx| {
                    e.change(cx, |d| match swatch {
                        Swatch::Line => d.style.color = color,
                        Swatch::Fill => d.style.fill_color = Some(color),
                        Swatch::Text => d.style.text_color = Some(color),
                        Swatch::Level(index) => {
                            d.levels = d.levels();
                            if let Some(level) = d.levels.get_mut(index) {
                                level.color = color;
                            }
                        }
                    });
                });
            },
        )
    }

    /// A switch that sets one flag of the drawing.
    fn switch(
        &self,
        id: &str,
        on: bool,
        cx: &mut Context<Self>,
        set: impl Fn(&mut Drawing, bool) + 'static,
    ) -> Switch {
        let this = cx.entity();
        Switch::new(SharedString::from(id.to_owned()))
            .cursor_pointer()
            .small()
            .checked(on)
            .on_click(move |checked, _window, cx| {
                let checked = *checked;
                this.update(cx, |e, cx| e.change(cx, |d| set(d, checked)));
            })
    }

    fn tabs(&self) -> Vec<Tab> {
        let mut tabs = vec![Tab::Style];
        if self.tool.has_words() || self.tool.has_text() {
            tabs.push(Tab::Text);
        }
        tabs.push(Tab::Coordinates);
        tabs.push(Tab::Visibility);
        tabs
    }

    fn style_tab(&self, drawing: &Drawing, cx: &mut Context<Self>) -> gpui::Div {
        let tool = drawing.tool;
        let style = &drawing.style;
        let this = cx.entity();
        let mut body = div().flex().flex_col();

        // Lines.
        body = body.child(widgets::section("Line"));
        let widths: Vec<String> = tool.widths().iter().map(|w| format!("{w}")).collect();
        let width_labels: Vec<&str> = widths.iter().map(String::as_str).collect();
        let width_index = tool
            .widths()
            .iter()
            .position(|w| (w - style.width).abs() < 0.01)
            .unwrap_or(usize::MAX);
        let width_this = this.clone();
        body = body.child(widgets::row(
            "Color and width",
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(self.swatch(Swatch::Line, style.color, "props-line-color", cx))
                .when(tool.has_width(), |el| {
                    el.child(widgets::segmented(
                        "props-width",
                        &width_labels,
                        width_index,
                        move |choice, _window, cx| {
                            width_this.update(cx, |e, cx| {
                                e.change(cx, |d| d.style.width = tool.widths()[choice]);
                            });
                        },
                    ))
                }),
        ));
        if tool.has_dash() {
            let dashes = [Dash::Solid, Dash::Dashed, Dash::Dotted];
            let index = dashes.iter().position(|d| *d == style.dash).unwrap_or(0);
            let dash_this = this.clone();
            body = body.child(widgets::row(
                "Style",
                widgets::segmented(
                    "props-dash",
                    &["Solid", "Dashed", "Dotted"],
                    index,
                    move |choice, _window, cx| {
                        dash_this
                            .update(cx, |e, cx| e.change(cx, |d| d.style.dash = dashes[choice]));
                    },
                ),
            ));
        }
        if tool.has_extend() {
            body = body
                .child(widgets::row(
                    "Extend left",
                    self.switch("props-extend-left", style.extend_left, cx, |d, on| {
                        d.style.extend_left = on;
                    }),
                ))
                .child(widgets::row(
                    "Extend right",
                    self.switch("props-extend-right", style.extend_right, cx, |d, on| {
                        d.style.extend_right = on;
                    }),
                ));
        }
        if let Some(label) = tool.middle_label() {
            body = body.child(widgets::row(
                label,
                self.switch("props-middle", style.middle, cx, |d, on| {
                    d.style.middle = on
                }),
            ));
        }
        if tool == Tool::Icon {
            let keys: Vec<&str> = ICONS.iter().map(|(key, _)| *key).collect();
            let labels: Vec<&str> = ICONS.iter().map(|(_, label)| *label).collect();
            let chosen = keys
                .iter()
                .position(|key| *key == icon_key(&drawing.text))
                .unwrap_or(0);
            let icon_this = this.clone();
            body = body.child(widgets::section("Icon")).child(chips(
                "props-icon",
                &labels,
                &[chosen],
                move |index, _window, cx| {
                    let key = keys[index].to_owned();
                    icon_this.update(cx, |e, cx| e.change(cx, |d| d.text = key));
                },
            ));
        }
        if let Some(label) = tool.labels_switch() {
            body = body.child(widgets::row(
                label,
                self.switch("props-labels", style.labels, cx, |d, on| {
                    d.style.labels = on
                }),
            ));
        }
        if tool.has_reverse() {
            body = body.child(widgets::row(
                "Reverse",
                self.switch("props-reverse", drawing.reverse, cx, |d, on| d.reverse = on),
            ));
        }
        if tool.is_elliott() {
            let degree_this = this.clone();
            let names = wave_names(tool);
            let example: Vec<String> = names
                .iter()
                .skip(1)
                .take(3)
                .map(|n| wave_label(n, drawing.degree))
                .collect();
            body = body
                .child(widgets::section("Wave degree"))
                .child(chips(
                    "props-degree",
                    &DEGREES,
                    &[usize::from(drawing.degree)],
                    move |index, _window, cx| {
                        degree_this.update(cx, |e, cx| e.change(cx, |d| d.degree = index as u8));
                    },
                ))
                .child(
                    div()
                        .pt_1()
                        .text_size(px(12.))
                        .text_color(theme::muted_fg())
                        .child(format!("Points read {}", example.join(" "))),
                );
        }

        // Fill.
        if tool.has_fill() {
            body = body
                .child(widgets::section("Background"))
                .child(widgets::row(
                    "Fill",
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .when(!tool.has_levels(), |el| {
                            el.child(self.swatch(
                                Swatch::Fill,
                                style.fill_color(),
                                "props-fill-color",
                                cx,
                            ))
                        })
                        .child(
                            self.switch("props-fill", style.fill, cx, |d, on| d.style.fill = on),
                        ),
                ))
                .child(widgets::row(
                    "Opacity (%)",
                    widgets::number_field(&self.opacity, 110.),
                ));
        }

        // Levels.
        if tool.has_levels() {
            let levels = drawing.levels();
            let add_this = this.clone();
            let reset_this = this.clone();
            body = body.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(div().flex_1().child(widgets::section("Levels")))
                    .child(
                        Button::new("props-level-add")
                            .cursor_pointer()
                            .when(levels.len() >= MAX_LEVELS, |button| {
                                button.cursor_not_allowed()
                            })
                            .ghost()
                            .xsmall()
                            .icon(IconName::Plus)
                            .label("Add")
                            .disabled(levels.len() >= MAX_LEVELS)
                            .on_click(move |_, window, cx| {
                                add_this.update(cx, |e, cx| e.add_level(window, cx));
                            }),
                    )
                    .child(
                        Button::new("props-level-reset")
                            .cursor_pointer()
                            .ghost()
                            .xsmall()
                            .icon(IconName::RotateCcw)
                            .label("Default")
                            .on_click(move |_, window, cx| {
                                reset_this.update(cx, |e, cx| {
                                    e.change(cx, |d| d.levels = Vec::new());
                                    e.levels_changed = true;
                                    e.set_fields(window, cx);
                                });
                            }),
                    ),
            );
            let mut grid = div().flex().flex_row().flex_wrap().gap_x_4();
            for (index, level) in levels.iter().enumerate() {
                let Some(field) = self.levels.get(index) else {
                    continue;
                };
                let remove_this = this.clone();
                grid = grid.child(
                    div()
                        .w(px(220.))
                        .h(px(34.))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .child(self.switch(
                            &format!("props-level-on-{index}"),
                            level.visible,
                            cx,
                            move |d, on| {
                                d.levels = d.levels();
                                if let Some(level) = d.levels.get_mut(index) {
                                    level.visible = on;
                                }
                            },
                        ))
                        .child(widgets::number_field(field, 106.))
                        .child(self.swatch(
                            Swatch::Level(index),
                            level.color,
                            &format!("props-level-color-{index}"),
                            cx,
                        ))
                        .child(
                            Button::new(SharedString::from(format!("props-level-remove-{index}")))
                                .cursor_pointer()
                                .ghost()
                                .xsmall()
                                .icon(IconName::X)
                                .on_click(move |_, window, cx| {
                                    remove_this
                                        .update(cx, |e, cx| e.remove_level(index, window, cx));
                                }),
                        ),
                );
            }
            body = body.child(grid);
        }
        body
    }

    fn add_level(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.change(cx, |d| {
            let mut levels = d.levels();
            let next = levels.iter().map(|l| l.value).fold(0.0, f64::max) + 0.5;
            let color = levels.last().map_or(d.style.color, |l| l.color);
            levels.push(Level {
                value: next,
                color,
                visible: true,
            });
            d.levels = levels;
        });
        self.set_fields(window, cx);
    }

    fn remove_level(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.change(cx, |d| {
            let mut levels = d.levels();
            if index < levels.len() && levels.len() > 1 {
                levels.remove(index);
            }
            d.levels = levels;
        });
        self.set_fields(window, cx);
    }

    fn text_tab(&self, drawing: &Drawing, cx: &mut Context<Self>) -> gpui::Div {
        let style = &drawing.style;
        let mut body = div().flex().flex_col();
        if drawing.tool.has_text() {
            body = body
                .child(widgets::section("Words"))
                .child(Textarea::new(&self.text).h(px(90.)));
        }
        body.child(widgets::section("Look"))
            .child(widgets::row(
                "Color",
                self.swatch(Swatch::Text, style.text_color(), "props-text-color", cx),
            ))
            .child(widgets::row(
                "Size",
                widgets::number_field(&self.text_size, 110.),
            ))
            .child(widgets::row(
                "Bold",
                self.switch("props-bold", style.bold, cx, |d, on| d.style.bold = on),
            ))
    }

    fn coordinates_tab(&self, drawing: &Drawing) -> gpui::Div {
        let mut body = div().flex().flex_col();
        if drawing.tool.is_freehand() {
            return body.child(
                div()
                    .py_4()
                    .text_size(px(13.))
                    .text_color(theme::muted_fg())
                    .child("A freehand stroke is moved as a whole, by dragging it on the chart."),
            );
        }
        body = body.child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .pt_2()
                .pb_1()
                .text_size(px(11.))
                .text_color(theme::muted_fg())
                .child(div().w(px(110.)).child("POINT"))
                .child(div().w(px(130.)).child("PRICE"))
                .child(
                    div()
                        .flex_1()
                        .child(format!("TIME ({})", self.zone.label(drawing.points[0].t))),
                ),
        );
        for (index, (price, time)) in self.prices.iter().zip(&self.times).enumerate() {
            let (price_on, time_on) = point_fields(drawing.tool, index);
            body = body.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(px(36.))
                    .child(
                        div()
                            .w(px(110.))
                            .text_size(px(13.))
                            .text_color(theme::fg())
                            .child(point_name(drawing.tool, index)),
                    )
                    .child(
                        div()
                            .w(px(130.))
                            .when(price_on, |el| el.child(Input::new(price).small())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .when(time_on, |el| el.child(Input::new(time).small())),
                    ),
            );
        }
        body.child(
            div()
                .pt_2()
                .text_size(px(11.))
                .text_color(theme::muted_fg())
                .child(format!(
                    "Prices have {} decimals. Times are snapped to the bars when the drawing is moved.",
                    self.digits
                )),
        )
    }

    fn visibility_tab(&self, drawing: &Drawing, cx: &mut Context<Self>) -> gpui::Div {
        let this = cx.entity();
        let all = drawing.timeframes.is_none();
        let mut body = div()
            .flex()
            .flex_col()
            .child(widgets::section("Show"))
            .child(widgets::row(
                "Hidden",
                self.switch("props-hidden", drawing.hidden, cx, |d, on| d.hidden = on),
            ))
            .child(widgets::row(
                "On every timeframe",
                self.switch("props-all-tf", all, cx, |d, on| {
                    d.timeframes = if on { None } else { Some(every_timeframe()) };
                }),
            ))
            .child(widgets::row(
                "Name in the list of drawings",
                div().w(px(200.)).child(Input::new(&self.name).small()),
            ));
        if !all {
            let shown: Vec<String> = drawing.timeframes.clone().unwrap_or_default();
            for (group, frames) in GROUPS {
                let codes: Vec<String> = frames.iter().map(|f| f.code()).collect();
                let labels: Vec<String> = frames.iter().map(|f| f.label()).collect();
                let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
                let selected: Vec<usize> = codes
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| shown.contains(c))
                    .map(|(i, _)| i)
                    .collect();
                let chip_this = this.clone();
                let codes_for_click = codes.clone();
                body = body.child(widgets::section(group)).child(chips(
                    &format!("props-tf-{group}"),
                    &label_refs,
                    &selected,
                    move |index, _window, cx| {
                        let code = codes_for_click[index].clone();
                        chip_this.update(cx, |e, cx| {
                            e.change(cx, |d| {
                                let mut list = d.timeframes.clone().unwrap_or_default();
                                if let Some(at) = list.iter().position(|c| *c == code) {
                                    list.remove(at);
                                } else {
                                    list.push(code);
                                }
                                d.timeframes = Some(list);
                            });
                        });
                    },
                ));
            }
        }
        body
    }
}

impl Render for DrawingProps {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(drawing) = self.current(cx) else {
            return div()
                .py_4()
                .text_color(theme::muted_fg())
                .child("This drawing was removed.")
                .into_any_element();
        };
        if self.levels_changed || self.levels.len() != drawing.levels().len() {
            self.make_level_fields(&drawing, window, cx);
        }
        let tabs = self.tabs();
        if !tabs.contains(&self.tab) {
            self.tab = Tab::Style;
        }
        let labels: Vec<&str> = tabs.iter().map(|t| t.label()).collect();
        let index = tabs.iter().position(|t| *t == self.tab).unwrap_or(0);
        let this = cx.entity();
        let content = match self.tab {
            Tab::Style => self.style_tab(&drawing, cx),
            Tab::Text => self.text_tab(&drawing, cx),
            Tab::Coordinates => self.coordinates_tab(&drawing),
            Tab::Visibility => self.visibility_tab(&drawing, cx),
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(widgets::segmented(
                "props-tabs",
                &labels,
                index,
                move |choice, _window, cx| {
                    let tab = tabs[choice];
                    this.update(cx, |e, cx| {
                        e.tab = tab;
                        e.swatch = None;
                        cx.notify();
                    });
                },
            ))
            .child(
                div()
                    .id("props-body")
                    .max_h(px(460.))
                    .overflow_y_scroll()
                    .pr_1()
                    .child(content),
            )
            .into_any_element()
    }
}

/// Buttons that wrap, any number of them chosen.
fn chips(
    id: &str,
    labels: &[&str],
    selected: &[usize],
    on_click: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> gpui::Div {
    let on_click = std::rc::Rc::new(on_click);
    let mut row = div().flex().flex_row().flex_wrap().gap_1();
    for (index, label) in labels.iter().enumerate() {
        let chosen = selected.contains(&index);
        let on_click = on_click.clone();
        row = row.child(
            div()
                .id(SharedString::from(format!("{id}-{index}")))
                .h(px(26.))
                .px_2p5()
                .flex()
                .items_center()
                .rounded_md()
                .border_1()
                .border_color(if chosen {
                    theme::accent()
                } else {
                    theme::border_subtle()
                })
                .when(chosen, |el| el.bg(theme::accent_selected()))
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(if chosen {
                    theme::fg()
                } else {
                    theme::muted_fg()
                })
                .hover(|s| s.bg(theme::surface_hover()))
                .on_click(move |_, window, cx| on_click(index, window, cx))
                .child(SharedString::from((*label).to_owned())),
        );
    }
    row
}

/// Every timeframe's code.
fn every_timeframe() -> Vec<String> {
    GROUPS
        .iter()
        .flat_map(|(_, frames)| frames.iter().map(|f| f.code()))
        .collect()
}

/// What a point of a drawing is called in the list of coordinates.
fn point_name(tool: Tool, index: usize) -> String {
    let names: &[&str] = match tool {
        Tool::LongPosition | Tool::ShortPosition => &["Entry", "Stop", "Target", "End"],
        Tool::Xabcd | Tool::Cypher => &["X", "A", "B", "C", "D"],
        Tool::Abcd | Tool::TrianglePattern => &["A", "B", "C", "D"],
        Tool::HeadAndShoulders => &[
            "Start",
            "Left shoulder",
            "Neckline 1",
            "Head",
            "Neckline 2",
            "Right shoulder",
            "End",
        ],
        tool if tool.is_elliott() => wave_names(tool),
        tool if tool.is_pitchfork() => &["Handle", "Tine 1", "Tine 2"],
        _ => &[],
    };
    names
        .get(index)
        .map_or_else(|| format!("Point {}", index + 1), |n| (*n).to_owned())
}

/// Which of a point's price and time can be typed (a position's stop and target sit at the
/// entry's time, its end at the entry's price).
fn point_fields(tool: Tool, index: usize) -> (bool, bool) {
    if tool.is_position() {
        match index {
            0 => (true, true),
            1 | 2 => (true, false),
            _ => (false, true),
        }
    } else {
        (true, true)
    }
}

/// Moves point `index` to a new time and/or price, keeping a position's shape.
fn set_point(drawing: &mut Drawing, index: usize, time: Option<i64>, price: Option<f64>) {
    let Some(point) = drawing.points.get_mut(index) else {
        return;
    };
    if let Some(t) = time {
        point.t = t;
    }
    if let Some(p) = price.filter(|p| p.is_finite()) {
        point.p = p;
    }
    if drawing.tool.is_position() && drawing.points.len() == 4 {
        let entry = drawing.points[0];
        for i in [1, 2] {
            drawing.points[i].t = entry.t;
        }
        drawing.points[3] = Point {
            t: drawing.points[3].t.max(entry.t + 1),
            p: entry.p,
        };
    }
}

/// A raw price written as a real one with `digits` decimals.
fn format_real(raw: f64, digits: u32) -> String {
    format!("{:.*}", digits as usize, raw / PRICE_SCALE as f64)
}

/// A time as `2025-03-14 09:30` in `zone`.
pub fn format_time(zone: Zone, time_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(zone.shift(time_ms))
        .map(|t| t.naive_utc().format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

/// Reads a time written in `zone`: `2025-03-14 09:30`, with seconds, or a date alone.
pub fn parse_time(zone: Zone, text: &str) -> Option<i64> {
    let text = text.trim();
    let local = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S"))
        .ok()
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
        })?;
    let local_ms = local.and_utc().timestamp_millis();
    // The offset depends on the moment itself: guess with the local reading, then correct.
    let guess = local_ms - zone.offset_ms(local_ms);
    Some(local_ms - zone.offset_ms(guess))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_written_and_read_in_the_charts_zone() {
        let t = 1_741_944_600_000; // 2025-03-14 09:30 UTC
        assert_eq!(format_time(Zone::Utc, t), "2025-03-14 09:30");
        assert_eq!(parse_time(Zone::Utc, "2025-03-14 09:30"), Some(t));
        assert_eq!(parse_time(Zone::Utc, " 2025-03-14 09:30:00 "), Some(t));
        assert_eq!(
            parse_time(Zone::Utc, "2025-03-14"),
            Some(t - (9 * 60 + 30) * 60_000)
        );
        assert_eq!(parse_time(Zone::Utc, "14/03/2025"), None);
        let tokyo = Zone::from_code("Asia/Tokyo");
        assert_eq!(format_time(tokyo, t), "2025-03-14 18:30");
        assert_eq!(parse_time(tokyo, "2025-03-14 18:30"), Some(t));
    }

    #[test]
    fn a_positions_points_keep_their_shape_when_typed() {
        let mut d = Drawing::new(
            1,
            Tool::LongPosition,
            vec![
                Point { t: 1_000, p: 100.0 },
                Point { t: 1_000, p: 90.0 },
                Point { t: 1_000, p: 130.0 },
                Point { t: 9_000, p: 100.0 },
            ],
        );
        set_point(&mut d, 0, Some(2_000), Some(105.0));
        assert_eq!(d.points[1].t, 2_000);
        assert_eq!(d.points[2].t, 2_000);
        assert_eq!(d.points[3], Point { t: 9_000, p: 105.0 });
        assert_eq!(point_fields(Tool::LongPosition, 3), (false, true));
        assert_eq!(point_name(Tool::HeadAndShoulders, 3), "Head");
        assert_eq!(point_name(Tool::TrendLine, 1), "Point 2");
    }
}
