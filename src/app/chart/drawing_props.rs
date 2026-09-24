//! The settings of one drawing: its look (lines, fill, levels, extension), its words, the exact
//! place of its points, and where it shows.
//!
//! Every change shows on the charts at once. OK keeps the changes as one undo step, Cancel puts
//! the drawing back as it was, and Escape or the close button keep them.

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable, Sizable};
use wyck::openapi::market::PRICE_SCALE;

use super::drawing::Drawings;
use super::drawing::extras::{ICONS, icon_key};
use super::drawing::figures::wave_names;
use super::drawing::model::{DEGREES, Dash, Drawing, Level, MAX_LEVELS, Point, Tool, wave_label};
use super::object_tree::tool_icon;
use super::timeframe::GROUPS;
use super::zone::Zone;
use crate::app::settings_ui::{self as ui, Head};
use crate::app::{modal, theme, widgets};

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
    // Opened once whatever asked is done updating, since the panel reads the drawings.
    window.defer(cx, move |window, cx| {
        let Some(drawing) = drawings.read(cx).book().get(&symbol, id).cloned() else {
            return;
        };
        let editor =
            cx.new(|cx| DrawingProps::new(drawings, symbol, &drawing, zone, digits, window, cx));
        // Escape and the close button keep the changes, like OK.
        let keep = editor.clone();
        modal::open(
            editor,
            modal::Options::new(820.0, 640.0)
                .on_dismiss(move |_window, cx| keep.update(cx, |e, cx| e.finish(true, cx))),
            window,
            cx,
        );
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Position,
    Style,
    Levels,
    Text,
    Coordinates,
    Visibility,
}

impl Tab {
    fn spec(self) -> ui::Tab {
        let (label, icon) = match self {
            Self::Position => ("Position", IconName::Calculator),
            Self::Style => ("Style", IconName::Palette),
            Self::Levels => ("Levels", IconName::SlidersHorizontal),
            Self::Text => ("Text", IconName::Type),
            Self::Coordinates => ("Coordinates", IconName::Crosshair),
            Self::Visibility => ("Visibility", IconName::Eye),
        };
        ui::Tab { label, icon }
    }
}

/// Which color's palette is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Swatch {
    Line,
    Fill,
    Text,
    Target,
    Stop,
    Entry,
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
    /// The opacity of the fill, in percent.
    opacity: Entity<InputState>,
    /// The opacity of the lines, in percent.
    line_opacity: Entity<InputState>,
    text_size: Entity<InputState>,
    text: Entity<TextareaState>,
    /// The number fields of a long or short position, when the drawing is one.
    pos: Option<PositionFields>,
    name: Entity<InputState>,
    prices: Vec<Entity<InputState>>,
    times: Vec<Entity<InputState>>,
    levels: Vec<Entity<InputState>>,
    /// Set when levels were added or removed: their fields are made again at the next render.
    levels_changed: bool,
    _subscriptions: Vec<Subscription>,
    _level_subscriptions: Vec<Subscription>,
}

/// The fields of what a position is sized with.
struct PositionFields {
    account: Entity<InputState>,
    risk: Entity<InputState>,
    lot_size: Entity<InputState>,
    leverage: Entity<InputState>,
    point_value: Entity<InputState>,
    qty_precision: Entity<InputState>,
    currency: Entity<InputState>,
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
        let line_opacity = cx.new(|cx| {
            widgets::number_state(
                f64::from(style.opacity) * 100.0,
                5.0,
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
            watch_number(&line_opacity, cx, |this, value, cx| {
                this.change(cx, |d| {
                    d.style.opacity = (value / 100.0).clamp(0.05, 1.0) as f32
                });
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
        let pos = drawing.tool.is_position().then(|| {
            let p = &drawing.style.position;
            let account =
                cx.new(|cx| widgets::number_state(p.account, 1.0, 1e12, 100.0, 2, window, cx));
            let risk = cx.new(|cx| widgets::number_state(p.risk, 0.01, 1e12, 0.25, 2, window, cx));
            let lot_size =
                cx.new(|cx| widgets::number_state(p.lot_size, 1e-8, 1e9, 1.0, 4, window, cx));
            let leverage =
                cx.new(|cx| widgets::number_state(p.leverage, 1.0, 10_000.0, 1.0, 0, window, cx));
            let point_value =
                cx.new(|cx| widgets::number_state(p.point_value, 1e-9, 1e9, 1.0, 4, window, cx));
            let qty_precision = cx.new(|cx| {
                widgets::number_state(f64::from(p.qty_precision), 0.0, 8.0, 1.0, 0, window, cx)
            });
            let currency = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(p.currency.clone())
                    .placeholder("USD")
            });
            subscriptions.push(watch_number(&account, cx, |this, value, cx| {
                this.change(cx, |d| d.style.position.account = value.max(1.0));
            }));
            subscriptions.push(watch_number(&risk, cx, |this, value, cx| {
                this.change(cx, |d| {
                    let cap = if d.style.position.risk_percent {
                        100.0
                    } else {
                        1e12
                    };
                    d.style.position.risk = value.clamp(0.01, cap);
                });
            }));
            subscriptions.push(watch_number(&lot_size, cx, |this, value, cx| {
                this.change(cx, |d| d.style.position.lot_size = value.max(1e-8));
            }));
            subscriptions.push(watch_number(&leverage, cx, |this, value, cx| {
                this.change(cx, |d| {
                    d.style.position.leverage = value.clamp(1.0, 10_000.0)
                });
            }));
            subscriptions.push(watch_number(&point_value, cx, |this, value, cx| {
                this.change(cx, |d| d.style.position.point_value = value.max(1e-9));
            }));
            subscriptions.push(watch_number(&qty_precision, cx, |this, value, cx| {
                this.change(cx, |d| {
                    d.style.position.qty_precision = value.clamp(0.0, 8.0) as u8
                });
            }));
            subscriptions.push(
                cx.subscribe(&currency, |this, state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value: String = state.read(cx).value().trim().chars().take(8).collect();
                        this.change(cx, |d| d.style.position.currency = value);
                    }
                }),
            );
            PositionFields {
                account,
                risk,
                lot_size,
                leverage,
                point_value,
                qty_precision,
                currency,
            }
        });
        let mut this = Self {
            drawings,
            symbol,
            id: drawing.id,
            tool: drawing.tool,
            before,
            finished: false,
            tab: if drawing.tool.is_position() {
                Tab::Position
            } else {
                Tab::Style
            },
            zone,
            digits,
            swatch: None,
            opacity,
            line_opacity,
            text_size,
            text,
            pos,
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
        let line = widgets::format_number(f64::from(drawing.style.opacity) * 100.0, 0);
        self.opacity
            .update(cx, |s, cx| s.set_value(opacity, window, cx));
        self.line_opacity
            .update(cx, |s, cx| s.set_value(line, window, cx));
        self.text_size
            .update(cx, |s, cx| s.set_value(size, window, cx));
        if let Some(pos) = &self.pos {
            let p = &drawing.style.position;
            let texts = [
                (&pos.account, widgets::format_number(p.account, 2)),
                (&pos.risk, widgets::format_number(p.risk, 2)),
                (&pos.lot_size, widgets::format_number(p.lot_size, 4)),
                (&pos.leverage, widgets::format_number(p.leverage, 0)),
                (&pos.point_value, widgets::format_number(p.point_value, 4)),
                (
                    &pos.qty_precision,
                    widgets::format_number(f64::from(p.qty_precision), 0),
                ),
                (&pos.currency, p.currency.clone()),
            ];
            for (state, text) in texts {
                state.update(cx, |s, cx| s.set_value(text, window, cx));
            }
        }
        self.make_level_fields(&drawing, window, cx);
    }

    /// What the position comes to with these settings, as rows of a card: the plan in figures.
    fn position_result(&self, drawing: &Drawing) -> Vec<AnyElement> {
        let p = &drawing.style.position;
        let real = |raw: f64| raw / PRICE_SCALE as f64;
        let tick = 10f64.powi(-(self.digits as i32));
        let stats = drawing.points.get(2).and_then(|_| {
            p.stats(
                real(drawing.points[0].p),
                real(drawing.points[1].p),
                real(drawing.points[2].p),
                tick,
            )
        });
        let Some(stats) = stats else {
            return vec![ui::block(ui::note(
                "Nothing to size: the stop sits on the entry.",
            ))];
        };
        let value = |text: String| div().text_size(px(13.)).text_color(theme::fg()).child(text);
        let mut rows = vec![
            ui::field("Quantity", None, value(p.format_qty(stats.qty))),
            ui::field(
                "Risk",
                Some(if stats.capped {
                    "Capped by the leverage: less than the risk asked for"
                } else {
                    "What the stop loses"
                }),
                value(p.format_plain(stats.loss)),
            ),
            ui::field("Reward", None, value(p.format_plain(stats.profit))),
            ui::field("Risk/reward", None, value(format!("{:.2}", stats.ratio))),
        ];
        if stats.qty <= 0.0 {
            rows.push(ui::block(ui::note(
                "The quantity rounds down to nothing: lower the lot size or raise the risk.",
            )));
        }
        rows
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
                        Swatch::Target => d.style.position.target_color = color,
                        Swatch::Stop => d.style.position.stop_color = color,
                        Swatch::Entry => d.style.position.entry_color = color,
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
        let mut tabs = if self.tool.is_position() {
            vec![Tab::Position, Tab::Style]
        } else {
            vec![Tab::Style]
        };
        if self.tool.has_levels() {
            tabs.push(Tab::Levels);
        }
        if self.tool.has_words() || self.tool.has_text() {
            tabs.push(Tab::Text);
        }
        tabs.push(Tab::Coordinates);
        tabs.push(Tab::Visibility);
        tabs
    }

    /// The color and opacity of the lines, with the widths and line styles the tool takes.
    fn line_group(&self, drawing: &Drawing, cx: &mut Context<Self>) -> gpui::Div {
        let tool = drawing.tool;
        let style = &drawing.style;
        let this = cx.entity();
        let mut rows: Vec<AnyElement> = vec![ui::field(
            "Color",
            Some("Opacity in percent"),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(self.swatch(Swatch::Line, style.color, "props-line-color", cx))
                .child(widgets::number_field(&self.line_opacity, 96.)),
        )];
        if tool.has_width() {
            let widths = tool.widths();
            let index = widths.iter().position(|w| (w - style.width).abs() < 0.01);
            let width_this = this.clone();
            rows.push(ui::field(
                "Width",
                None,
                ui::width_picker("props-width", &widths, index, move |choice, _window, cx| {
                    width_this.update(cx, |e, cx| {
                        e.change(cx, |d| d.style.width = tool.widths()[choice]);
                    });
                }),
            ));
        }
        if tool.has_dash() {
            let dashes = [Dash::Solid, Dash::Dashed, Dash::Dotted];
            let index = dashes.iter().position(|d| *d == style.dash).unwrap_or(0);
            let dash_this = this.clone();
            rows.push(ui::field(
                "Line style",
                None,
                ui::dash_picker("props-dash", index, move |choice, _window, cx| {
                    dash_this.update(cx, |e, cx| e.change(cx, |d| d.style.dash = dashes[choice]));
                }),
            ));
        }
        if tool.has_extend() {
            rows.push(ui::field(
                "Extend left",
                Some("Past the first point, to the edge of the chart"),
                self.switch("props-extend-left", style.extend_left, cx, |d, on| {
                    d.style.extend_left = on;
                }),
            ));
            rows.push(ui::field(
                "Extend right",
                Some("Past the last point, to the edge of the chart"),
                self.switch("props-extend-right", style.extend_right, cx, |d, on| {
                    d.style.extend_right = on;
                }),
            ));
        }
        if let Some(label) = tool.middle_label() {
            rows.push(ui::field(
                label,
                None,
                self.switch("props-middle", style.middle, cx, |d, on| {
                    d.style.middle = on
                }),
            ));
        }
        if let Some(label) = tool.labels_switch() {
            rows.push(ui::field(
                label,
                None,
                self.switch("props-labels", style.labels, cx, |d, on| {
                    d.style.labels = on
                }),
            ));
        }
        if tool.has_reverse() {
            rows.push(ui::field(
                "Reverse",
                Some("Flips the drawing upside down"),
                self.switch("props-reverse", drawing.reverse, cx, |d, on| d.reverse = on),
            ));
        }
        ui::group(IconName::PenLine, "Line", rows)
    }

    fn style_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let tool = drawing.tool;
        let style = &drawing.style;
        let this = cx.entity();
        let mut page = ui::page().child(self.line_group(drawing, cx));

        if tool == Tool::Icon {
            let keys: Vec<&str> = ICONS.iter().map(|(key, _)| *key).collect();
            let labels: Vec<&str> = ICONS.iter().map(|(_, label)| *label).collect();
            let chosen = keys
                .iter()
                .position(|key| *key == icon_key(&drawing.text))
                .unwrap_or(0);
            let icon_this = this.clone();
            page = page.child(ui::group(
                IconName::Sparkles,
                "Icon",
                [ui::block(chips(
                    "props-icon",
                    &labels,
                    &[chosen],
                    move |index, _window, cx| {
                        let key = keys[index].to_owned();
                        icon_this.update(cx, |e, cx| e.change(cx, |d| d.text = key));
                    },
                ))],
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
            page = page.child(ui::group(
                IconName::Waypoints,
                "Wave degree",
                [
                    ui::block(chips(
                        "props-degree",
                        &DEGREES,
                        &[usize::from(drawing.degree)],
                        move |index, _window, cx| {
                            degree_this
                                .update(cx, |e, cx| e.change(cx, |d| d.degree = index as u8));
                        },
                    )),
                    ui::block(ui::note(format!("Points read {}", example.join(" ")))),
                ],
            ));
        }

        if tool.has_fill() {
            let mut rows = vec![ui::field(
                "Fill",
                Some("Colors the area inside the shape"),
                self.switch("props-fill", style.fill, cx, |d, on| d.style.fill = on),
            )];
            if !tool.has_levels() {
                rows.push(ui::field(
                    "Fill color",
                    None,
                    self.swatch(Swatch::Fill, style.fill_color(), "props-fill-color", cx),
                ));
            }
            rows.push(ui::field(
                "Fill opacity",
                Some("In percent"),
                widgets::number_field(&self.opacity, 96.),
            ));
            page = page.child(ui::group(IconName::PaintBucket, "Background", rows));
        }
        page.into_any_element()
    }

    /// The inputs of a long or short position: the account, the risk, what the chart writes, and
    /// what it all comes to.
    fn position_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let Some(pos) = &self.pos else {
            return ui::page().into_any_element();
        };
        let p = &drawing.style.position;
        let this = cx.entity();

        let account = ui::group(
            IconName::Wallet,
            "Account",
            [
                ui::field(
                    "Account size",
                    Some("The balance the position is sized for"),
                    widgets::number_field(&pos.account, 130.),
                ),
                ui::field(
                    "Currency",
                    Some("Written after the amounts. Empty writes none"),
                    div().w(px(130.)).child(Input::new(&pos.currency).small()),
                ),
                ui::field(
                    "Leverage",
                    Some("Caps the quantity at account x leverage / entry price"),
                    widgets::number_field(&pos.leverage, 130.),
                ),
            ],
        );

        let mode_this = this.clone();
        let risk = ui::group(
            IconName::ShieldAlert,
            "Risk and size",
            [
                ui::field(
                    "Risk",
                    Some("Lost if the stop is hit"),
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(widgets::number_field(&pos.risk, 100.))
                        .child(widgets::segmented(
                            "props-risk-mode",
                            &["%", "Amount"],
                            usize::from(!p.risk_percent),
                            move |choice, window, cx| {
                                mode_this.update(cx, |e, cx| {
                                    e.change(cx, |d| {
                                        d.style.position.risk_percent = choice == 0;
                                        d.style.position =
                                            std::mem::take(&mut d.style.position).normalized();
                                    });
                                    e.set_fields(window, cx);
                                });
                            },
                        )),
                ),
                ui::field(
                    "Lot size",
                    Some("The step the quantity is rounded down to"),
                    widgets::number_field(&pos.lot_size, 130.),
                ),
                ui::field(
                    "Quantity decimals",
                    None,
                    widgets::number_field(&pos.qty_precision, 130.),
                ),
                ui::field(
                    "Point value",
                    Some(
                        "What one unit gains per 1.0 of price, in the account currency. 1 when the symbol is quoted in it",
                    ),
                    widgets::number_field(&pos.point_value, 130.),
                ),
            ],
        );

        let result = ui::group(
            IconName::Calculator,
            "Result",
            self.position_result(drawing),
        );

        let stat = |id: &str,
                    label: &'static str,
                    hint: Option<&'static str>,
                    on: bool,
                    cx: &mut Context<Self>,
                    set: fn(&mut Drawing, bool)| {
            ui::field(label, hint, self.switch(id, on, cx, set))
        };
        let stats = ui::group(
            IconName::ListChecks,
            "Written on the chart",
            [
                stat("pos-qty", "Quantity", None, p.show_qty, cx, |d, on| {
                    d.style.position.show_qty = on;
                }),
                stat("pos-risk", "Risk", None, p.show_risk, cx, |d, on| {
                    d.style.position.show_risk = on;
                }),
                stat(
                    "pos-amounts",
                    "Profit and loss",
                    Some("The amounts at the target and at the stop"),
                    p.show_amounts,
                    cx,
                    |d, on| d.style.position.show_amounts = on,
                ),
                stat(
                    "pos-ratio",
                    "Risk/reward ratio",
                    None,
                    p.show_ratio,
                    cx,
                    |d, on| {
                        d.style.position.show_ratio = on;
                    },
                ),
                stat(
                    "pos-price",
                    "Price of the levels",
                    None,
                    p.show_price,
                    cx,
                    |d, on| {
                        d.style.position.show_price = on;
                    },
                ),
                stat(
                    "pos-percent",
                    "Percent",
                    Some("Distance from the entry, in percent"),
                    p.show_percent,
                    cx,
                    |d, on| d.style.position.show_percent = on,
                ),
                stat(
                    "pos-ticks",
                    "Ticks",
                    Some("Distance from the entry, in the smallest price step"),
                    p.show_ticks,
                    cx,
                    |d, on| d.style.position.show_ticks = on,
                ),
                stat(
                    "pos-compact",
                    "Compact tags",
                    Some("One short figure per tag"),
                    p.compact,
                    cx,
                    |d, on| d.style.position.compact = on,
                ),
                stat(
                    "pos-always",
                    "Always show the stats",
                    Some("Off: the quantity and the amounts show only while it is selected"),
                    p.always_stats,
                    cx,
                    |d, on| d.style.position.always_stats = on,
                ),
            ],
        );
        ui::page()
            .child(account)
            .child(risk)
            .child(result)
            .child(stats)
            .child(ui::note(
                "The figures are a plan, not a quote: there is no exchange rate in them.",
            ))
            .into_any_element()
    }

    /// The look of a long or short position: its three lines, its two zones, its tags.
    fn position_style_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let tool = drawing.tool;
        let style = &drawing.style;
        let p = &style.position;
        let this = cx.entity();

        let widths = tool.widths();
        let width_index = widths.iter().position(|w| (w - style.width).abs() < 0.01);
        let width_this = this.clone();
        let dash_this = this.clone();
        let dashes = [Dash::Solid, Dash::Dashed, Dash::Dotted];
        let dash_index = dashes.iter().position(|d| *d == style.dash).unwrap_or(0);
        let lines = ui::group(
            IconName::PenLine,
            "Lines",
            [
                ui::field(
                    "Width",
                    None,
                    ui::width_picker(
                        "props-width",
                        &widths,
                        width_index,
                        move |choice, _w, cx| {
                            width_this.update(cx, |e, cx| {
                                e.change(cx, |d| d.style.width = tool.widths()[choice]);
                            });
                        },
                    ),
                ),
                ui::field(
                    "Entry line",
                    None,
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(ui::dash_picker(
                            "props-dash",
                            dash_index,
                            move |choice, _w, cx| {
                                dash_this.update(cx, |e, cx| {
                                    e.change(cx, |d| d.style.dash = dashes[choice])
                                });
                            },
                        ))
                        .child(self.swatch(Swatch::Entry, p.entry_color, "props-entry-color", cx)),
                ),
                ui::field(
                    "Target",
                    Some("The profit line and its zone"),
                    self.swatch(Swatch::Target, p.target_color, "props-target-color", cx),
                ),
                ui::field(
                    "Stop",
                    Some("The loss line and its zone"),
                    self.swatch(Swatch::Stop, p.stop_color, "props-stop-color", cx),
                ),
            ],
        );

        let background = ui::group(
            IconName::PaintBucket,
            "Background",
            [
                ui::field(
                    "Zones",
                    Some("Colors the profit and the loss zones"),
                    self.switch("props-fill", style.fill, cx, |d, on| d.style.fill = on),
                ),
                ui::field(
                    "Zone opacity",
                    Some("In percent"),
                    widgets::number_field(&self.opacity, 96.),
                ),
            ],
        );

        let tags = ui::group(
            IconName::Type,
            "Tags",
            [
                ui::field(
                    "Show the tags",
                    Some("The words on the levels"),
                    self.switch("props-labels", style.labels, cx, |d, on| {
                        d.style.labels = on
                    }),
                ),
                ui::field(
                    "Text color",
                    Some("Dark on the colored tags unless you pick one"),
                    self.swatch(
                        Swatch::Text,
                        style.text_color.unwrap_or(0x0a0a0a),
                        "props-text-color",
                        cx,
                    ),
                ),
                ui::field(
                    "Text size",
                    None,
                    widgets::number_field(&self.text_size, 96.),
                ),
                ui::field(
                    "Bold",
                    None,
                    self.switch("props-bold", style.bold, cx, |d, on| d.style.bold = on),
                ),
            ],
        );
        ui::page()
            .child(lines)
            .child(background)
            .child(tags)
            .into_any_element()
    }

    fn levels_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        let levels = drawing.levels();
        let full = levels.len() >= MAX_LEVELS;
        let (add_this, reset_this) = (this.clone(), this.clone());
        let controls = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(
                Button::new("props-level-add")
                    .cursor_pointer()
                    .when(full, |button| button.cursor_not_allowed())
                    .ghost()
                    .xsmall()
                    .icon(IconName::Plus)
                    .label("Add")
                    .disabled(full)
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
            )
            .into_any_element();

        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, level) in levels.iter().enumerate() {
            let Some(field) = self.levels.get(index) else {
                continue;
            };
            let remove_this = this.clone();
            let removable = levels.len() > 1;
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .min_h(px(42.))
                    .py_1p5()
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
                    .child(widgets::number_field(field, 120.))
                    .child(self.swatch(
                        Swatch::Level(index),
                        level.color,
                        &format!("props-level-color-{index}"),
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(
                        Button::new(SharedString::from(format!("props-level-remove-{index}")))
                            .cursor_pointer()
                            .ghost()
                            .xsmall()
                            .icon(IconName::X)
                            .tooltip("Remove this level")
                            .disabled(!removable)
                            .on_click(move |_, window, cx| {
                                remove_this.update(cx, |e, cx| e.remove_level(index, window, cx));
                            }),
                    )
                    .into_any_element(),
            );
        }
        ui::page()
            .child(ui::group_with(
                IconName::SlidersHorizontal,
                format!("Levels ({}/{MAX_LEVELS})", levels.len()),
                Some(controls),
                rows,
            ))
            .child(ui::note(
                "Each level is a ratio of the move between the points. Use the switch to hide one without losing it.",
            ))
            .into_any_element()
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

    fn text_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let style = &drawing.style;
        let mut page = ui::page();
        if drawing.tool.has_text() {
            page = page.child(ui::group(
                IconName::TextCursorInput,
                "Words",
                [ui::block(Textarea::new(&self.text).h(px(96.)))],
            ));
        }
        page.child(ui::group(
            IconName::Type,
            "Font",
            [
                ui::field(
                    "Color",
                    None,
                    self.swatch(Swatch::Text, style.text_color(), "props-text-color", cx),
                ),
                ui::field("Size", None, widgets::number_field(&self.text_size, 96.)),
                ui::field(
                    "Bold",
                    None,
                    self.switch("props-bold", style.bold, cx, |d, on| d.style.bold = on),
                ),
            ],
        ))
        .into_any_element()
    }

    fn coordinates_page(&self, drawing: &Drawing) -> AnyElement {
        if drawing.tool.is_freehand() {
            return ui::empty(
                IconName::Brush,
                "A freehand stroke is moved as a whole, by dragging it on the chart.",
            )
            .into_any_element();
        }
        let head = |text: SharedString, width: Option<f32>| {
            let cell = div().text_size(px(11.)).text_color(theme::muted_fg());
            match width {
                Some(width) => cell.w(px(width)).child(text),
                None => cell.flex_1().child(text),
            }
        };
        let mut rows: Vec<AnyElement> = vec![
            div()
                .flex()
                .flex_row()
                .gap_2()
                .pt_2()
                .pb_1()
                .child(head("POINT".into(), Some(110.)))
                .child(head("PRICE".into(), Some(130.)))
                .child(head(
                    format!("TIME ({})", self.zone.label(drawing.points[0].t)).into(),
                    None,
                ))
                .into_any_element(),
        ];
        for (index, (price, time)) in self.prices.iter().zip(&self.times).enumerate() {
            let (price_on, time_on) = point_fields(drawing.tool, index);
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(px(40.))
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
                    )
                    .into_any_element(),
            );
        }
        ui::page()
            .child(ui::group(IconName::Crosshair, "Points", rows))
            .child(ui::note(format!(
                "Prices have {} decimals. Times snap to the bars when the drawing is moved.",
                self.digits
            )))
            .into_any_element()
    }

    fn visibility_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        let all = drawing.timeframes.is_none();
        let mut page = ui::page().child(ui::group(
            IconName::Eye,
            "Display",
            [
                ui::field(
                    "Hidden",
                    Some("Keeps the drawing but does not show it"),
                    self.switch("props-hidden", drawing.hidden, cx, |d, on| d.hidden = on),
                ),
                ui::field(
                    "Locked",
                    Some("Stops it from being moved, changed or deleted"),
                    self.switch("props-locked", drawing.locked, cx, |d, on| d.locked = on),
                ),
                ui::field(
                    "Name",
                    Some("As shown in the list of drawings"),
                    div().w(px(220.)).child(Input::new(&self.name).small()),
                ),
            ],
        ));

        let mut rows: Vec<AnyElement> = vec![ui::field(
            "On every timeframe",
            None,
            self.switch("props-all-tf", all, cx, |d, on| {
                d.timeframes = if on { None } else { Some(every_timeframe()) };
            }),
        )];
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
                rows.push(ui::block(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .child(ui::note(group))
                        .child(chips(
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
                        )),
                ));
            }
        }
        page = page.child(ui::group(IconName::Clock, "Timeframes", rows));
        page.into_any_element()
    }

    /// The footer: the defaults of the tool at the left, Cancel and OK at the right.
    fn footer(&self, cx: &mut Context<Self>) -> gpui::Div {
        let has_template = self.has_template(cx);
        let (template, reset, cancel, ok) = (cx.entity(), cx.entity(), cx.entity(), cx.entity());
        ui::footer(
            vec![
                ui::action(
                    "drawing-template",
                    "Save as default",
                    Some(IconName::Save),
                    false,
                    move |_window, cx| template.update(cx, |e, cx| e.save_template(cx)),
                )
                .tooltip("New drawings of this tool start with this look")
                .into_any_element(),
                ui::action(
                    "drawing-reset",
                    if has_template { "Reset" } else { "Reset look" },
                    Some(IconName::RotateCcw),
                    false,
                    move |window, cx| reset.update(cx, |e, cx| e.reset_style(window, cx)),
                )
                .tooltip("Back to the look the tool starts with")
                .into_any_element(),
            ],
            vec![
                ui::action(
                    "drawing-cancel",
                    "Cancel",
                    None,
                    false,
                    move |window, cx| {
                        cancel.update(cx, |e, cx| e.finish(false, cx));
                        modal::close(window, cx);
                    },
                )
                .into_any_element(),
                ui::action("drawing-ok", "OK", None, true, move |window, cx| {
                    ok.update(cx, |e, cx| e.finish(true, cx));
                    modal::close(window, cx);
                })
                .into_any_element(),
            ],
        )
    }
}

impl Render for DrawingProps {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(drawing) = self.current(cx) else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .rounded_xl()
                .border_1()
                .border_color(theme::border_subtle())
                .bg(theme::bg())
                .child(ui::empty(IconName::Info, "This drawing was removed."))
                .into_any_element();
        };
        if self.levels_changed || self.levels.len() != drawing.levels().len() {
            self.make_level_fields(&drawing, window, cx);
        }
        let tabs = self.tabs();
        if !tabs.contains(&self.tab) {
            self.tab = tabs[0];
        }
        let specs: Vec<ui::Tab> = tabs.iter().map(|t| t.spec()).collect();
        let active = tabs.iter().position(|t| *t == self.tab).unwrap_or(0);
        let this = cx.entity();
        let body = match self.tab {
            Tab::Position => self.position_page(&drawing, cx),
            Tab::Style if drawing.tool.is_position() => self.position_style_page(&drawing, cx),
            Tab::Style => self.style_page(&drawing, cx),
            Tab::Levels => self.levels_page(&drawing, cx),
            Tab::Text => self.text_page(&drawing, cx),
            Tab::Coordinates => self.coordinates_page(&drawing),
            Tab::Visibility => self.visibility_page(&drawing, cx),
        };
        let head = Head {
            icon: tool_icon(drawing.tool),
            title: drawing.title().into(),
            subtitle: format!(
                "{}{}",
                self.symbol,
                if drawing.locked { " - locked" } else { "" }
            )
            .into(),
        };
        let footer = self.footer(cx);
        ui::frame(
            head,
            &specs,
            active,
            move |index, _window, cx| {
                let tab = tabs[index];
                this.update(cx, |e, cx| {
                    e.tab = tab;
                    e.swatch = None;
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
