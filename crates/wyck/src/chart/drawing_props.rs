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
use wyck_openapi::market::PRICE_SCALE;

use super::Chart;
use super::drawing::Drawings;
use super::drawing::extras::{ICONS, icon_key};
use super::drawing::figures::wave_names;
use super::drawing::look::{Cap, HAlign, LabelSide, LevelText, VAlign};
use super::drawing::model::{
    DASHES, DEGREES, Dash, Drawing, Level, MAX_LEVELS, Point, Tool, wave_label,
};
use super::object_tree::tool_icon;
use super::timeframe::GROUPS;
use super::zone::Zone;
use wyck_chart::study::atr_stop::{AtrStop, Smoothing};
use wyck_ui::form::Head;
use wyck_ui::{button, controls, form, modal, number, theme};

pub struct PropsContext {
    pub zone: Zone,
    pub digits: u32,
    pub atr_seed: Option<AtrStop>,
    pub chart: Option<Entity<Chart>>,
}

/// Opens the settings of drawing `id` of `symbol`.
pub fn open(
    drawings: Entity<Drawings>,
    symbol: String,
    id: u64,
    context: PropsContext,
    window: &mut Window,
    cx: &mut App,
) {
    // Opened once whatever asked is done updating, since the panel reads the drawings.
    window.defer(cx, move |window, cx| {
        let Some(drawing) = drawings.read(cx).book().get(&symbol, id).cloned() else {
            return;
        };
        let editor =
            cx.new(|cx| DrawingProps::new(drawings, symbol, &drawing, context, window, cx));
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
    fn spec(self) -> form::Tab {
        let (label, icon) = match self {
            Self::Position => ("Position", IconName::Calculator),
            Self::Style => ("Style", IconName::Palette),
            Self::Levels => ("Levels", IconName::SlidersHorizontal),
            Self::Text => ("Text", IconName::Type),
            Self::Coordinates => ("Coordinates", IconName::Crosshair),
            Self::Visibility => ("Visibility", IconName::Eye),
        };
        form::Tab { label, icon }
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
    TextBackground,
    MeasureDown,
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
    /// The width of the lines, in pixels, typed for any value the presets do not have.
    width: Entity<InputState>,
    /// The size of a marker, in percent of its usual size.
    scale: Entity<InputState>,
    /// The rows of a volume profile (0 lets the height decide), and its value area in percent.
    profile_rows: Entity<InputState>,
    profile_area: Entity<InputState>,
    /// The name a look is about to be saved under.
    template_name: Entity<InputState>,
    text_size: Entity<InputState>,
    text: Entity<TextareaState>,
    /// The number fields of a long or short position, when the drawing is one.
    pos: Option<PositionFields>,
    atr_seed: Option<AtrStop>,
    chart: Option<Entity<Chart>>,
    name: Entity<InputState>,
    prices: Vec<Entity<InputState>>,
    times: Vec<Entity<InputState>>,
    levels: Vec<Entity<InputState>>,
    /// The width of the line of each level (0 is the width of the drawing).
    level_widths: Vec<Entity<InputState>>,
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
    atr_length: Entity<InputState>,
    atr_multiplier: Entity<InputState>,
    rr: Entity<InputState>,
    atr_timeframe: Entity<InputState>,
}

/// A switch of the measure tool: whether it applies, its id, its name, its state, and what it sets.
type MeasureFlag = (
    bool,
    &'static str,
    &'static str,
    bool,
    fn(&mut Drawing, bool),
);

impl DrawingProps {
    fn new(
        drawings: Entity<Drawings>,
        symbol: String,
        drawing: &Drawing,
        context: PropsContext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let PropsContext {
            zone,
            digits,
            atr_seed,
            chart,
        } = context;
        let before = drawings.read(cx).book().drawings(&symbol).to_vec();
        let style = &drawing.style;
        let opacity = cx.new(|cx| {
            number::state(
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
            number::state(
                f64::from(style.opacity) * 100.0,
                5.0,
                100.0,
                5.0,
                0,
                window,
                cx,
            )
        });
        let text_size =
            cx.new(|cx| number::state(f64::from(style.text_size), 6.0, 48.0, 1.0, 0, window, cx));
        let width =
            cx.new(|cx| number::state(f64::from(style.width), 0.5, 40.0, 0.5, 1, window, cx));
        let scale = cx.new(|cx| {
            number::state(
                f64::from(style.scale) * 100.0,
                30.0,
                500.0,
                10.0,
                0,
                window,
                cx,
            )
        });
        let profile_rows = cx.new(|cx| {
            number::state(
                f64::from(style.profile.rows),
                0.0,
                240.0,
                1.0,
                0,
                window,
                cx,
            )
        });
        let profile_area = cx.new(|cx| {
            number::state(
                f64::from(style.profile.value_area) * 100.0,
                10.0,
                100.0,
                5.0,
                0,
                window,
                cx,
            )
        });
        let template_name = cx.new(|cx| InputState::new(window, cx).placeholder("Name this look"));
        let text = cx.new(|cx| TextareaState::new(window, cx).default_value(drawing.text.clone()));
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(drawing.name.clone())
                .placeholder(drawing.tool.label())
        });
        let mut subscriptions = vec![
            cx.observe(&drawings, |_this, _drawings, cx| cx.notify()),
            number::watch(&opacity, cx, |this, value, cx| {
                this.change(cx, |d| d.style.fill_opacity = (value / 100.0) as f32);
            }),
            number::watch(&line_opacity, cx, |this, value, cx| {
                this.change(cx, |d| {
                    d.style.opacity = (value / 100.0).clamp(0.05, 1.0) as f32
                });
            }),
            number::watch(&text_size, cx, |this, value, cx| {
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
        if let Some(chart) = &chart {
            subscriptions.push(cx.observe(chart, |_, _, cx| cx.notify()));
        }
        let mut prices = Vec::new();
        let mut times = Vec::new();
        if !drawing.tool.is_freehand() {
            let current = chart
                .as_ref()
                .and_then(|chart| chart.read(cx).resolved_position(drawing));
            let shown = current.as_ref().unwrap_or(drawing);
            for (index, point) in shown.points.iter().enumerate() {
                let price = cx.new(|cx| {
                    InputState::new(window, cx).default_value(format_real(point.p, digits))
                });
                subscriptions.push(number::watch(&price, cx, move |this, value, cx| {
                    let raw = value * PRICE_SCALE as f64;
                    this.change(cx, |d| set_point(d, index, None, Some(raw)));
                }));
                let time = cx.new(|cx| {
                    InputState::new(window, cx)
                        .default_value(format_time(zone, point.t))
                        .placeholder("YYYY-MM-DD HH:MM")
                });
                subscriptions.push(number::watch_parsed(
                    &time,
                    cx,
                    |this, text| parse_time(this.zone, text),
                    move |this, t, cx| this.change(cx, |d| set_point(d, index, Some(t), None)),
                ));
                prices.push(price);
                times.push(time);
            }
        }
        let pos = drawing.tool.is_position().then(|| {
            let p = &drawing.style.position;
            let atr = p
                .atr_stop
                .clone()
                .or_else(|| atr_seed.clone())
                .unwrap_or_default();
            let atr_length =
                cx.new(|cx| number::state(atr.length as f64, 1.0, 1_000.0, 1.0, 0, window, cx));
            let atr_multiplier =
                cx.new(|cx| number::state(atr.multiplier, 0.01, 1_000.0, 0.1, 2, window, cx));
            let rr = cx.new(|cx| {
                number::state(
                    p.target_rr.unwrap_or(2.0),
                    0.01,
                    1_000.0,
                    0.1,
                    2,
                    window,
                    cx,
                )
            });
            let atr_timeframe = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(atr.timeframe.clone().unwrap_or_default())
                    .placeholder("Chart TF, M15, H1...")
            });
            let account = cx.new(|cx| number::state(p.account, 1.0, 1e12, 100.0, 2, window, cx));
            let risk = cx.new(|cx| number::state(p.risk, 0.01, 1e12, 0.25, 2, window, cx));
            let lot_size = cx.new(|cx| number::state(p.lot_size, 1e-8, 1e9, 1.0, 4, window, cx));
            let leverage =
                cx.new(|cx| number::state(p.leverage, 1.0, 10_000.0, 1.0, 0, window, cx));
            let point_value =
                cx.new(|cx| number::state(p.point_value, 1e-9, 1e9, 1.0, 4, window, cx));
            let qty_precision = cx
                .new(|cx| number::state(f64::from(p.qty_precision), 0.0, 8.0, 1.0, 0, window, cx));
            let currency = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(p.currency.clone())
                    .placeholder("USD")
            });
            subscriptions.push(number::watch(&account, cx, |this, value, cx| {
                this.change(cx, |d| d.style.position.account = value.max(1.0));
            }));
            subscriptions.push(number::watch(&risk, cx, |this, value, cx| {
                this.change(cx, |d| {
                    let cap = if d.style.position.risk_percent {
                        100.0
                    } else {
                        1e12
                    };
                    d.style.position.risk = value.clamp(0.01, cap);
                });
            }));
            subscriptions.push(number::watch(&lot_size, cx, |this, value, cx| {
                this.change(cx, |d| d.style.position.lot_size = value.max(1e-8));
            }));
            subscriptions.push(number::watch(&leverage, cx, |this, value, cx| {
                this.change(cx, |d| {
                    d.style.position.leverage = value.clamp(1.0, 10_000.0)
                });
            }));
            subscriptions.push(number::watch(&point_value, cx, |this, value, cx| {
                this.change(cx, |d| d.style.position.point_value = value.max(1e-9));
            }));
            subscriptions.push(number::watch(&qty_precision, cx, |this, value, cx| {
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
            subscriptions.push(number::watch(&atr_length, cx, |this, value, cx| {
                this.change(cx, |d| {
                    if let Some(atr) = &mut d.style.position.atr_stop {
                        atr.length = (value as usize).clamp(1, 1_000);
                    }
                });
            }));
            subscriptions.push(number::watch(&atr_multiplier, cx, |this, value, cx| {
                this.change(cx, |d| {
                    if let Some(atr) = &mut d.style.position.atr_stop {
                        atr.multiplier = value.clamp(0.01, 1_000.0);
                    }
                });
            }));
            subscriptions.push(number::watch(&rr, cx, |this, value, cx| {
                this.change(cx, |d| {
                    if d.style.position.target_rr.is_some() {
                        d.style.position.target_rr = Some(value.clamp(0.01, 1_000.0));
                    }
                });
            }));
            subscriptions.push(cx.subscribe(
                &atr_timeframe,
                |this, state, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        let code = state.read(cx).value().trim().to_ascii_uppercase();
                        if code.is_empty()
                            || super::timeframe::Timeframe::from_code(&code).is_some()
                        {
                            this.change(cx, |d| {
                                if let Some(atr) = &mut d.style.position.atr_stop {
                                    atr.timeframe = (!code.is_empty()).then_some(code);
                                }
                            });
                        }
                    }
                },
            ));
            PositionFields {
                account,
                risk,
                lot_size,
                leverage,
                point_value,
                qty_precision,
                currency,
                atr_length,
                atr_multiplier,
                rr,
                atr_timeframe,
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
            width,
            scale,
            profile_rows,
            profile_area,
            template_name,
            text_size,
            text,
            pos,
            atr_seed,
            chart,
            name,
            prices,
            times,
            levels: Vec::new(),
            level_widths: Vec::new(),
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
        self.level_widths.clear();
        for (index, level) in drawing.levels().iter().enumerate() {
            let state = cx.new(|cx| number::state(level.value, -100.0, 100.0, 0.1, 4, window, cx));
            self._level_subscriptions
                .push(number::watch(&state, cx, move |this, value, cx| {
                    this.change(cx, |d| {
                        d.levels = d.levels();
                        if let Some(level) = d.levels.get_mut(index) {
                            level.value = value;
                        }
                    });
                }));
            self.levels.push(state);
            let width =
                cx.new(|cx| number::state(f64::from(level.width), 0.0, 40.0, 0.5, 1, window, cx));
            self._level_subscriptions
                .push(number::watch(&width, cx, move |this, value, cx| {
                    this.change(cx, |d| {
                        d.levels = d.levels();
                        if let Some(level) = d.levels.get_mut(index) {
                            level.width = value.clamp(0.0, 40.0) as f32;
                        }
                    });
                }));
            self.level_widths.push(width);
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
        wyck_ui::toast::show(
            cx,
            wyck_ui::toast::Kind::Success,
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
        let opacity = number::format(f64::from(drawing.style.fill_opacity) * 100.0, 0);
        let size = number::format(f64::from(drawing.style.text_size), 0);
        let line = number::format(f64::from(drawing.style.opacity) * 100.0, 0);
        self.opacity
            .update(cx, |s, cx| s.set_value(opacity, window, cx));
        self.line_opacity
            .update(cx, |s, cx| s.set_value(line, window, cx));
        let numbers = [
            (
                &self.width,
                number::format(f64::from(drawing.style.width), 1),
            ),
            (
                &self.scale,
                number::format(f64::from(drawing.style.scale) * 100.0, 0),
            ),
            (
                &self.profile_rows,
                number::format(f64::from(drawing.style.profile.rows), 0),
            ),
            (
                &self.profile_area,
                number::format(f64::from(drawing.style.profile.value_area) * 100.0, 0),
            ),
        ];
        for (state, text) in numbers {
            state.update(cx, |s, cx| s.set_value(text, window, cx));
        }
        self.text_size
            .update(cx, |s, cx| s.set_value(size, window, cx));
        if let Some(pos) = &self.pos {
            let p = &drawing.style.position;
            let texts = [
                (&pos.account, number::format(p.account, 2)),
                (&pos.risk, number::format(p.risk, 2)),
                (&pos.lot_size, number::format(p.lot_size, 4)),
                (&pos.leverage, number::format(p.leverage, 0)),
                (&pos.point_value, number::format(p.point_value, 4)),
                (
                    &pos.qty_precision,
                    number::format(f64::from(p.qty_precision), 0),
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
    fn position_result(&self, drawing: &Drawing, cx: &App) -> Vec<AnyElement> {
        let resolved = self
            .chart
            .as_ref()
            .and_then(|chart| chart.read(cx).resolved_position(drawing));
        if drawing.style.position.atr_stop.is_some() && resolved.is_none() {
            return vec![form::block(form::note(
                "ATR unavailable for the selected chart and timeframe.",
            ))];
        }
        let drawing = resolved.as_ref().unwrap_or(drawing);
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
            return vec![form::block(form::note(
                "Nothing to size: the stop sits on the entry.",
            ))];
        };
        let value = |text: String| div().text_size(px(13.)).text_color(theme::fg()).child(text);
        let mut rows = vec![
            form::field("Quantity", None, value(p.format_qty(stats.qty))),
            form::field(
                "Risk",
                Some(if stats.capped {
                    "Capped by the leverage: less than the risk asked for"
                } else {
                    "What the stop loses"
                }),
                value(p.format_plain(stats.loss)),
            ),
            form::field("Reward", None, value(p.format_plain(stats.profit))),
            form::field("Risk/reward", None, value(format!("{:.2}", stats.ratio))),
        ];
        if stats.qty <= 0.0 {
            rows.push(form::block(form::note(
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
        controls::color_swatch(
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
                        Swatch::TextBackground => {
                            d.style.text_layout.background_color = Some(color);
                        }
                        Swatch::MeasureDown => d.style.measure.down_color = color,
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
        controls::toggle(
            SharedString::from(id.to_owned()),
            on,
            move |checked, _window, cx| {
                this.update(cx, |e, cx| e.change(cx, |d| set(d, checked)));
            },
        )
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
        if self.tool.has_words() || self.tool.has_text() || self.tool.takes_label() {
            tabs.push(Tab::Text);
        }
        tabs.push(Tab::Coordinates);
        tabs.push(Tab::Visibility);
        tabs
    }

    // ---- the parts every tool can have ----

    /// A choice of what ends a line, for `set` to apply.
    fn cap_picker(
        &self,
        id: &'static str,
        current: Cap,
        cx: &mut Context<Self>,
        set: fn(&mut Drawing, Cap),
    ) -> impl IntoElement + use<> {
        let caps = [Cap::None, Cap::Arrow, Cap::Circle];
        let this = cx.entity();
        controls::segmented(
            id,
            &["None", "Arrow", "Dot"],
            caps.iter().position(|c| *c == current).unwrap_or(0),
            move |choice, _window, cx| {
                this.update(cx, |e, cx| e.change(cx, |d| set(d, caps[choice])));
            },
        )
    }

    /// The width of the lines: the presets of the tool, and a field for any other.
    fn width_row(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let tool = drawing.tool;
        let widths = tool.widths();
        let index = widths
            .iter()
            .position(|w| (w - drawing.style.width).abs() < 0.01);
        let this = cx.entity();
        form::field(
            "Width",
            Some("Pick one, or type any width in pixels"),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(controls::width_picker(
                    "props-width",
                    &widths,
                    index,
                    move |choice, window, cx| {
                        this.update(cx, |e, cx| {
                            e.change(cx, |d| d.style.width = tool.widths()[choice]);
                            e.set_fields(window, cx);
                        });
                    },
                ))
                .child(number::field(&self.width, 84.)),
        )
    }

    /// The caps of a line, for the tools that have some.
    fn caps_rows(&self, drawing: &Drawing, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (tool, caps) = (drawing.tool, drawing.style.caps);
        let mut rows = Vec::new();
        if tool.has_start_cap() {
            rows.push(form::field(
                "Start of the line",
                None,
                self.cap_picker("props-cap-start", caps.start, cx, |d, cap| {
                    d.style.caps.start = cap;
                }),
            ));
        }
        if tool.has_end_cap() {
            rows.push(form::field(
                "End of the line",
                None,
                self.cap_picker("props-cap-end", caps.end, cx, |d, cap| {
                    d.style.caps.end = cap
                }),
            ));
        }
        rows
    }

    /// What a measuring tool writes, and the color of a move down.
    fn measure_group(&self, drawing: &Drawing, cx: &mut Context<Self>) -> gpui::Div {
        let tool = drawing.tool;
        let look = drawing.style.measure;
        // Which numbers each tool has to write.
        let (price, percent, bars, time, angle) = match tool {
            Tool::PriceRange => (true, true, false, false, false),
            Tool::DateRange => (false, false, true, true, false),
            Tool::TrendAngle => (false, false, false, false, true),
            Tool::InfoLine => (true, true, true, true, true),
            _ => (true, true, true, true, false),
        };
        let flags: [MeasureFlag; 5] = [
            (
                price,
                "measure-price",
                "Price change",
                look.price,
                |d, on| {
                    d.style.measure.price = on;
                },
            ),
            (
                percent,
                "measure-percent",
                "Percent change",
                look.percent,
                |d, on| d.style.measure.percent = on,
            ),
            (
                bars,
                "measure-bars",
                "Number of bars",
                look.bars,
                |d, on| {
                    d.style.measure.bars = on;
                },
            ),
            (time, "measure-time", "Time span", look.time, |d, on| {
                d.style.measure.time = on;
            }),
            (angle, "measure-angle", "Angle", look.angle, |d, on| {
                d.style.measure.angle = on;
            }),
        ];
        let mut rows: Vec<AnyElement> = Vec::new();
        for (show, id, label, on, set) in flags {
            if show {
                rows.push(form::field(label, None, self.switch(id, on, cx, set)));
            }
        }
        if tool == Tool::Measure {
            rows.push(form::field(
                "Color of a move down",
                Some("The color of the drawing is for a move up"),
                self.swatch(Swatch::MeasureDown, look.down_color, "props-down-color", cx),
            ));
        }
        form::group(IconName::Ruler, "Numbers written", rows)
    }

    /// The size of a marker.
    fn marker_group(&self) -> gpui::Div {
        form::group(
            IconName::Ruler,
            "Marker",
            [form::field(
                "Size",
                Some("In percent of its usual size"),
                number::field(&self.scale, 96.),
            )],
        )
    }

    /// How a volume profile is cut.
    fn profile_group(&self) -> gpui::Div {
        form::group(
            IconName::ChartBarBig,
            "Profile",
            [
                form::field(
                    "Rows",
                    Some("0 lets the height on the screen decide"),
                    number::field(&self.profile_rows, 96.),
                ),
                form::field(
                    "Value area",
                    Some("The share of the volume it holds, in percent"),
                    number::field(&self.profile_area, 96.),
                ),
            ],
        )
    }

    /// Looks saved under a name: apply one, delete one, or keep the look of this drawing.
    fn templates_group(&self, drawing: &Drawing, cx: &mut Context<Self>) -> gpui::Div {
        let names: Vec<String> = self
            .drawings
            .read(cx)
            .book()
            .named_templates(drawing.tool)
            .iter()
            .map(|t| t.name.clone())
            .collect();
        let this = cx.entity();
        let mut rows: Vec<AnyElement> = Vec::new();
        if names.is_empty() {
            rows.push(form::block(form::note(
                "No saved look for this tool yet. Name the current one below to keep it.",
            )));
        }
        for (index, name) in names.iter().enumerate() {
            let (apply, delete) = (this.clone(), this.clone());
            let (apply_name, delete_name) = (name.clone(), name.clone());
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .min_h(px(40.))
                    .child(
                        Button::new(("props-template", index))
                            .cursor_pointer()
                            .ghost()
                            .small()
                            .icon(IconName::Bookmark)
                            .label(name.clone())
                            .tooltip("Apply this look to the drawing")
                            .on_click(move |_, window, cx| {
                                apply.update(cx, |e, cx| e.apply_named(&apply_name, window, cx));
                            }),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new(("props-template-delete", index))
                            .cursor_pointer()
                            .ghost()
                            .xsmall()
                            .icon(IconName::X)
                            .tooltip("Forget this look")
                            .on_click(move |_, _window, cx| {
                                delete.update(cx, |e, cx| e.delete_named(&delete_name, cx));
                            }),
                    )
                    .into_any_element(),
            );
        }
        let save = this.clone();
        rows.push(form::field(
            "Save this look",
            Some("Under a name, for this tool. The same name replaces it"),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(170.))
                        .child(Input::new(&self.template_name).small()),
                )
                .child(
                    Button::new("props-template-save")
                        .cursor_pointer()
                        .primary()
                        .small()
                        .icon(IconName::BookmarkPlus)
                        .label("Save")
                        .on_click(move |_, window, cx| {
                            save.update(cx, |e, cx| e.save_named(window, cx));
                        }),
                ),
        ));
        form::group(IconName::Bookmark, "Saved looks", rows)
    }

    /// Puts the look saved under `name` on the drawing.
    fn apply_named(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let found = self
            .drawings
            .read(cx)
            .book()
            .named_templates(self.tool)
            .into_iter()
            .find(|t| t.name == name)
            .map(|t| (t.style.clone(), t.levels.clone()));
        let Some((style, levels)) = found else {
            return;
        };
        self.change(cx, |d| {
            d.style = style;
            d.levels = levels;
        });
        self.set_fields(window, cx);
    }

    /// Keeps the look of the drawing under the name that was typed.
    fn save_named(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.template_name.read(cx).value().to_string();
        let (symbol, id) = (self.symbol.clone(), self.id);
        let saved = self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.save_named_template(&symbol, id, &name))
        });
        if saved {
            self.template_name
                .update(cx, |state, cx| state.set_value("", window, cx));
            wyck_ui::toast::show(
                cx,
                wyck_ui::toast::Kind::Success,
                "Look saved",
                format!("{} is in the saved looks of this tool.", name.trim()),
            );
        } else {
            wyck_ui::toast::show(
                cx,
                wyck_ui::toast::Kind::Warning,
                "Give the look a name",
                "Type a name first, then save.",
            );
        }
        cx.notify();
    }

    fn delete_named(&mut self, name: &str, cx: &mut Context<Self>) {
        let tool = self.tool;
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.delete_named_template(tool, name))
        });
        cx.notify();
    }

    /// The color and opacity of the lines, with the widths and line styles the tool takes.
    fn line_group(&self, drawing: &Drawing, cx: &mut Context<Self>) -> gpui::Div {
        let tool = drawing.tool;
        let style = &drawing.style;
        let this = cx.entity();
        let mut rows: Vec<AnyElement> = vec![form::field(
            "Color",
            Some("Opacity in percent"),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(self.swatch(Swatch::Line, style.color, "props-line-color", cx))
                .child(number::field(&self.line_opacity, 96.)),
        )];
        if tool.has_width() {
            rows.push(self.width_row(drawing, cx));
        }
        if tool.has_dash() {
            let index = DASHES.iter().position(|d| *d == style.dash).unwrap_or(0);
            let dash_this = this.clone();
            rows.push(form::field(
                "Line style",
                None,
                controls::dash_picker("props-dash", index, move |choice, _window, cx| {
                    dash_this.update(cx, |e, cx| e.change(cx, |d| d.style.dash = DASHES[choice]));
                }),
            ));
        }
        rows.extend(self.caps_rows(drawing, cx));
        if tool.has_extend() {
            rows.push(form::field(
                "Extend left",
                Some("Past the first point, to the edge of the chart"),
                self.switch("props-extend-left", style.extend_left, cx, |d, on| {
                    d.style.extend_left = on;
                }),
            ));
            rows.push(form::field(
                "Extend right",
                Some("Past the last point, to the edge of the chart"),
                self.switch("props-extend-right", style.extend_right, cx, |d, on| {
                    d.style.extend_right = on;
                }),
            ));
        }
        if let Some(label) = tool.middle_label() {
            rows.push(form::field(
                label,
                None,
                self.switch("props-middle", style.middle, cx, |d, on| {
                    d.style.middle = on
                }),
            ));
        }
        if let Some(label) = tool.labels_switch() {
            rows.push(form::field(
                label,
                None,
                self.switch("props-labels", style.labels, cx, |d, on| {
                    d.style.labels = on
                }),
            ));
        }
        if tool.has_reverse() {
            rows.push(form::field(
                "Reverse",
                Some("Flips the drawing upside down"),
                self.switch("props-reverse", drawing.reverse, cx, |d, on| d.reverse = on),
            ));
        }
        form::group(IconName::PenLine, "Line", rows)
    }

    fn style_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let tool = drawing.tool;
        let style = &drawing.style;
        let this = cx.entity();
        let mut page = form::page().child(self.line_group(drawing, cx));
        if tool.has_measure_look() {
            page = page.child(self.measure_group(drawing, cx));
        }
        if tool.has_size() {
            page = page.child(self.marker_group());
        }
        if tool.has_profile() {
            page = page.child(self.profile_group());
        }

        if tool == Tool::Icon {
            let keys: Vec<&str> = ICONS.iter().map(|(key, _)| *key).collect();
            let labels: Vec<&str> = ICONS.iter().map(|(_, label)| *label).collect();
            let chosen = keys
                .iter()
                .position(|key| *key == icon_key(&drawing.text))
                .unwrap_or(0);
            let icon_this = this.clone();
            page = page.child(form::group(
                IconName::Sparkles,
                "Icon",
                [form::block(chips(
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
            page = page.child(form::group(
                IconName::Waypoints,
                "Wave degree",
                [
                    form::block(chips(
                        "props-degree",
                        &DEGREES,
                        &[usize::from(drawing.degree)],
                        move |index, _window, cx| {
                            degree_this
                                .update(cx, |e, cx| e.change(cx, |d| d.degree = index as u8));
                        },
                    )),
                    form::block(form::note(format!("Points read {}", example.join(" ")))),
                ],
            ));
        }

        if tool.has_fill() {
            let mut rows = vec![form::field(
                "Fill",
                Some("Colors the area inside the shape"),
                self.switch("props-fill", style.fill, cx, |d, on| d.style.fill = on),
            )];
            if !tool.has_levels() {
                rows.push(form::field(
                    "Fill color",
                    None,
                    self.swatch(Swatch::Fill, style.fill_color(), "props-fill-color", cx),
                ));
            }
            rows.push(form::field(
                "Fill opacity",
                Some("In percent"),
                number::field(&self.opacity, 96.),
            ));
            page = page.child(form::group(IconName::PaintBucket, "Background", rows));
        }
        page.child(self.templates_group(drawing, cx))
            .into_any_element()
    }

    /// The inputs of a long or short position: the account, the risk, what the chart writes, and
    /// what it all comes to.
    fn position_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let Some(pos) = &self.pos else {
            return form::page().into_any_element();
        };
        let p = &drawing.style.position;
        let this = cx.entity();

        let account = form::group(
            IconName::Wallet,
            "Account",
            [
                form::field(
                    "Account size",
                    Some("The balance the position is sized for"),
                    number::field(&pos.account, 130.),
                ),
                form::field(
                    "Currency",
                    Some("Written after the amounts. Empty writes none"),
                    div().w(px(130.)).child(Input::new(&pos.currency).small()),
                ),
                form::field(
                    "Leverage",
                    Some("Caps the quantity at account x leverage / entry price"),
                    number::field(&pos.leverage, 130.),
                ),
            ],
        );

        let mode_this = this.clone();
        let risk = form::group(
            IconName::ShieldAlert,
            "Risk and size",
            [
                form::field(
                    "Risk",
                    Some("Lost if the stop is hit"),
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(number::field(&pos.risk, 100.))
                        .child(controls::segmented(
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
                form::field(
                    "Lot size",
                    Some("The step the quantity is rounded down to"),
                    number::field(&pos.lot_size, 130.),
                ),
                form::field(
                    "Quantity decimals",
                    None,
                    number::field(&pos.qty_precision, 130.),
                ),
                form::field(
                    "Point value",
                    Some(
                        "What one unit gains per 1.0 of price, in the account currency. 1 when the symbol is quoted in it",
                    ),
                    number::field(&pos.point_value, 130.),
                ),
            ],
        );

        let atr_seed = self.atr_seed.clone().unwrap_or_default();
        let mode_this = this.clone();
        let target_this = this.clone();
        let mut levels = vec![
            form::field(
                "Stop loss",
                None,
                controls::segmented(
                    "pos-stop-mode",
                    &["Fixed price", "ATR x"],
                    usize::from(p.atr_stop.is_some()),
                    move |choice, _, cx| {
                        mode_this.update(cx, |editor, cx| {
                            editor.change(cx, |d| {
                                d.style.position.atr_stop = (choice == 1).then(|| atr_seed.clone());
                            })
                        });
                    },
                ),
            ),
            form::field(
                "Take profit",
                None,
                controls::segmented(
                    "pos-target-mode",
                    &["Fixed price", "Risk multiple"],
                    usize::from(p.target_rr.is_some()),
                    move |choice, _, cx| {
                        target_this.update(cx, |editor, cx| {
                            editor.change(cx, |d| {
                                d.style.position.target_rr = (choice == 1).then_some(2.0);
                            })
                        });
                    },
                ),
            ),
        ];
        if let Some(atr) = &p.atr_stop {
            let smooth_this = this.clone();
            let bar_this = this.clone();
            let smooth_index = Smoothing::ALL
                .iter()
                .position(|m| *m == atr.smoothing)
                .unwrap_or(0);
            levels.push(form::field(
                "ATR length",
                None,
                number::field(&pos.atr_length, 100.),
            ));
            levels.push(form::field(
                "ATR multiplier",
                None,
                number::field(&pos.atr_multiplier, 100.),
            ));
            levels.push(form::field(
                "ATR smoothing",
                None,
                controls::segmented(
                    "pos-atr-smoothing",
                    &["RMA", "SMA", "EMA", "WMA"],
                    smooth_index,
                    move |choice, _, cx| {
                        smooth_this.update(cx, |editor, cx| {
                            editor.change(cx, |d| {
                                if let Some(atr) = &mut d.style.position.atr_stop {
                                    atr.smoothing = Smoothing::ALL[choice];
                                }
                            })
                        });
                    },
                ),
            ));
            levels.push(form::field(
                "ATR timeframe",
                Some(
                    "Leave blank to follow this chart, or enter a timeframe code such as M15 or H1",
                ),
                div()
                    .w(px(130.))
                    .child(Input::new(&pos.atr_timeframe).small()),
            ));
            levels.push(form::field(
                "ATR bar",
                None,
                controls::segmented(
                    "pos-atr-bar",
                    &["Last closed", "Current"],
                    usize::from(atr.current_bar),
                    move |choice, _, cx| {
                        bar_this.update(cx, |editor, cx| {
                            editor.change(cx, |d| {
                                if let Some(atr) = &mut d.style.position.atr_stop {
                                    atr.current_bar = choice == 1;
                                }
                            })
                        });
                    },
                ),
            ));
        }
        if p.target_rr.is_some() {
            levels.push(form::field(
                "Risk multiple",
                None,
                number::field(&pos.rr, 100.),
            ));
        }
        let levels = form::group(IconName::ChartNoAxesCombined, "Protection levels", levels);

        let result = form::group(
            IconName::Calculator,
            "Result",
            self.position_result(drawing, cx),
        );

        let stat = |id: &str,
                    label: &'static str,
                    hint: Option<&'static str>,
                    on: bool,
                    cx: &mut Context<Self>,
                    set: fn(&mut Drawing, bool)| {
            form::field(label, hint, self.switch(id, on, cx, set))
        };
        let stats = form::group(
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
                    "pos-pips",
                    "Pips",
                    Some("Distance from the entry using the symbol's pip size, when available"),
                    p.show_pips,
                    cx,
                    |d, on| d.style.position.show_pips = on,
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
                    "Always show the tags",
                    Some("Off: show the level tags only while the position is selected"),
                    p.always_stats,
                    cx,
                    |d, on| d.style.position.always_stats = on,
                ),
            ],
        );
        form::page()
            .child(account)
            .child(risk)
            .child(levels)
            .child(result)
            .child(stats)
            .child(form::note(
                "The figures are a plan, not a quote: there is no exchange rate in them.",
            ))
            .into_any_element()
    }

    /// The look of a long or short position: its three lines, its two zones, its tags.
    fn position_style_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let style = &drawing.style;
        let p = &style.position;
        let this = cx.entity();

        let dash_this = this.clone();
        let dash_index = DASHES.iter().position(|d| *d == style.dash).unwrap_or(0);
        let lines = form::group(
            IconName::PenLine,
            "Lines",
            [
                self.width_row(drawing, cx),
                form::field(
                    "Entry line",
                    None,
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(controls::dash_picker(
                            "props-dash",
                            dash_index,
                            move |choice, _w, cx| {
                                dash_this.update(cx, |e, cx| {
                                    e.change(cx, |d| d.style.dash = DASHES[choice])
                                });
                            },
                        ))
                        .child(self.swatch(Swatch::Entry, p.entry_color, "props-entry-color", cx)),
                ),
                form::field(
                    "Target",
                    Some("The profit line and its zone"),
                    self.swatch(Swatch::Target, p.target_color, "props-target-color", cx),
                ),
                form::field(
                    "Stop",
                    Some("The loss line and its zone"),
                    self.swatch(Swatch::Stop, p.stop_color, "props-stop-color", cx),
                ),
            ],
        );

        let background = form::group(
            IconName::PaintBucket,
            "Background",
            [
                form::field(
                    "Zones",
                    Some("Colors the profit and the loss zones"),
                    self.switch("props-fill", style.fill, cx, |d, on| d.style.fill = on),
                ),
                form::field(
                    "Zone opacity",
                    Some("In percent"),
                    number::field(&self.opacity, 96.),
                ),
            ],
        );

        let tags = form::group(
            IconName::Type,
            "Tags",
            [
                form::field(
                    "Show the tags",
                    Some("The words on the levels"),
                    self.switch("props-labels", style.labels, cx, |d, on| {
                        d.style.labels = on
                    }),
                ),
                form::field(
                    "Text color",
                    Some("Dark on the colored tags unless you pick one"),
                    self.swatch(
                        Swatch::Text,
                        style.text_color.unwrap_or(0x0a0a0a),
                        "props-text-color",
                        cx,
                    ),
                ),
                form::field("Text size", None, number::field(&self.text_size, 96.)),
                form::field(
                    "Bold",
                    None,
                    self.switch("props-bold", style.bold, cx, |d, on| d.style.bold = on),
                ),
            ],
        );
        form::page()
            .child(lines)
            .child(background)
            .child(tags)
            .child(self.templates_group(drawing, cx))
            .into_any_element()
    }

    /// What the levels are called on the chart, and the side their labels stand on.
    fn captions_group(&self, drawing: &Drawing, cx: &mut Context<Self>) -> gpui::Div {
        let style = &drawing.style;
        let this = cx.entity();
        let labels: Vec<&str> = LevelText::ALL.iter().map(|t| t.label()).collect();
        let index = LevelText::ALL
            .iter()
            .position(|t| *t == style.level_text)
            .unwrap_or(0);
        let mut rows = vec![form::field(
            "Caption",
            Some("What is written beside each level"),
            controls::segmented(
                "props-level-text",
                &labels,
                index,
                move |choice, _window, cx| {
                    this.update(cx, |e, cx| {
                        e.change(cx, |d| d.style.level_text = LevelText::ALL[choice]);
                    });
                },
            ),
        )];
        if drawing.tool.has_label_side() {
            let side_this = cx.entity();
            rows.push(form::field(
                "Side",
                Some("Where the captions stand: left or right of the levels"),
                controls::segmented(
                    "props-level-side",
                    &["Left", "Right"],
                    usize::from(style.label_side == LabelSide::Right),
                    move |choice, _window, cx| {
                        side_this.update(cx, |e, cx| {
                            e.change(cx, |d| {
                                d.style.label_side = if choice == 0 {
                                    LabelSide::Left
                                } else {
                                    LabelSide::Right
                                };
                            });
                        });
                    },
                ),
            ));
        }
        form::group(IconName::Tag, "Captions", rows)
    }

    /// The button of a level that picks its line style: the drawing's own, then solid, dashed and
    /// dotted, one click at a time.
    fn level_dash_button(
        &self,
        index: usize,
        current: Option<Dash>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let this = cx.entity();
        let next = match current {
            None => Some(Dash::Solid),
            Some(Dash::Solid) => Some(Dash::Dashed),
            Some(Dash::Dashed) => Some(Dash::Dotted),
            Some(Dash::Dotted) => None,
        };
        let glyph: AnyElement = match current {
            None => div()
                .text_size(px(11.))
                .text_color(theme::muted_fg())
                .child("Auto")
                .into_any_element(),
            Some(Dash::Solid) => controls::dash_glyph(0, theme::fg()).into_any_element(),
            Some(Dash::Dashed) => controls::dash_glyph(1, theme::fg()).into_any_element(),
            Some(Dash::Dotted) => controls::dash_glyph(2, theme::fg()).into_any_element(),
        };
        div()
            .id(("props-level-dash", index))
            .flex()
            .items_center()
            .justify_center()
            .w(px(44.))
            .h(px(28.))
            .rounded_md()
            .border_1()
            .border_color(theme::border_subtle())
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface_hover()))
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(
                    "Line style of this level: the drawing's, solid, dashed, dotted",
                )
                .m_1()
                .build(window, cx)
            })
            .on_click(move |_, _window, cx| {
                this.update(cx, |e, cx| {
                    e.change(cx, |d| {
                        d.levels = d.levels();
                        if let Some(level) = d.levels.get_mut(index) {
                            level.dash = next;
                        }
                    });
                });
            })
            .child(glyph)
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
                    .child(number::field(field, 100.))
                    .child(self.swatch(
                        Swatch::Level(index),
                        level.color,
                        &format!("props-level-color-{index}"),
                        cx,
                    ))
                    .children(
                        self.level_widths
                            .get(index)
                            .map(|width| number::field(width, 84.)),
                    )
                    .child(self.level_dash_button(index, level.dash, cx))
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
        form::page()
            .child(self.captions_group(drawing, cx))
            .child(form::group_with(
                IconName::SlidersHorizontal,
                format!("Levels ({}/{MAX_LEVELS})", levels.len()),
                Some(controls),
                rows,
            ))
            .child(form::note(
                "Each level is a ratio of the move between the points. The switch hides one without losing it. A width of 0 follows the drawing, and the line button cycles through the drawing's style, solid, dashed and dotted.",
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
                ..Level::default()
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

    /// A choice of how the label sits, for `set` to apply. `names` are the words for each choice.
    fn align_picker<const N: usize>(
        &self,
        id: &'static str,
        names: [&'static str; N],
        current: usize,
        cx: &mut Context<Self>,
        set: fn(&mut Drawing, usize),
    ) -> AnyElement {
        let this = cx.entity();
        controls::segmented(id, &names, current, move |choice, _window, cx| {
            this.update(cx, |e, cx| e.change(cx, |d| set(d, choice)));
        })
        .into_any_element()
    }

    /// The words a line or a shape carries, and where they stand.
    fn label_groups(&self, drawing: &Drawing, cx: &mut Context<Self>) -> Vec<gpui::Div> {
        let tool = drawing.tool;
        let layout = drawing.style.text_layout;
        // On a line the words go above it, on it or below it; in a shape, at the top, the middle
        // or the bottom.
        let on_a_line = matches!(
            tool,
            Tool::HorizontalLine | Tool::CrossLine | Tool::HorizontalRay | Tool::VerticalLine
        ) || (tool.anchors() == 2 && !tool.is_box());
        let vertical = if on_a_line {
            ["Above", "On it", "Below"]
        } else {
            ["Top", "Middle", "Bottom"]
        };
        let horizontal = self.align_picker(
            "label-align",
            ["Start", "Center", "End"],
            match layout.align {
                HAlign::Start => 0,
                HAlign::Center => 1,
                HAlign::End => 2,
            },
            cx,
            |d, choice| {
                d.style.text_layout.align = [HAlign::Start, HAlign::Center, HAlign::End][choice];
            },
        );
        let across = self.align_picker(
            "label-valign",
            vertical,
            match layout.valign {
                VAlign::Auto | VAlign::Top => 0,
                VAlign::Middle => 1,
                VAlign::Bottom => 2,
            },
            cx,
            |d, choice| {
                d.style.text_layout.valign = [VAlign::Top, VAlign::Middle, VAlign::Bottom][choice];
            },
        );
        let mut placement = vec![
            form::field("Along the drawing", None, horizontal),
            form::field("Across the drawing", None, across),
            form::field(
                "Background",
                Some("Puts the words on a filled tag"),
                self.switch("label-background", layout.background, cx, |d, on| {
                    d.style.text_layout.background = on
                }),
            ),
        ];
        if layout.background {
            placement.push(form::field(
                "Tag color",
                None,
                self.swatch(
                    Swatch::TextBackground,
                    layout.background_color.unwrap_or(0x1b1d24),
                    "label-background-color",
                    cx,
                ),
            ));
        }
        vec![
            form::group(
                IconName::TextCursorInput,
                "Label",
                [form::block(Textarea::new(&self.text).h(px(72.)))],
            ),
            form::group(IconName::Move, "Placement", placement),
        ]
    }

    fn text_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let style = &drawing.style;
        let mut page = form::page();
        if drawing.tool.has_text() {
            page = page.child(form::group(
                IconName::TextCursorInput,
                "Words",
                [form::block(Textarea::new(&self.text).h(px(96.)))],
            ));
        }
        if drawing.tool.takes_label() {
            for group in self.label_groups(drawing, cx) {
                page = page.child(group);
            }
        }
        page.child(form::group(
            IconName::Type,
            "Font",
            [
                form::field(
                    "Color",
                    None,
                    self.swatch(Swatch::Text, style.text_color(), "props-text-color", cx),
                ),
                form::field("Size", None, number::field(&self.text_size, 96.)),
                form::field(
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
            return form::empty(
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
        form::page()
            .child(form::group(IconName::Crosshair, "Points", rows))
            .child(form::note(format!(
                "Prices have {} decimals. Times snap to the bars when the drawing is moved.",
                self.digits
            )))
            .into_any_element()
    }

    fn visibility_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        let all = drawing.timeframes.is_none();
        let mut page = form::page().child(form::group(
            IconName::Eye,
            "Display",
            [
                form::field(
                    "Hidden",
                    Some("Keeps the drawing but does not show it"),
                    self.switch("props-hidden", drawing.hidden, cx, |d, on| d.hidden = on),
                ),
                form::field(
                    "Locked",
                    Some("Stops it from being moved, changed or deleted"),
                    self.switch("props-locked", drawing.locked, cx, |d, on| d.locked = on),
                ),
                form::field(
                    "Name",
                    Some("As shown in the list of drawings"),
                    div().w(px(220.)).child(Input::new(&self.name).small()),
                ),
            ],
        ));

        let mut rows: Vec<AnyElement> = vec![form::field(
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
                rows.push(form::block(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .child(form::note(group))
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
        page = page.child(form::group(IconName::Clock, "Timeframes", rows));
        page.into_any_element()
    }

    /// The footer: the defaults of the tool at the left, Cancel and OK at the right.
    fn footer(&self, cx: &mut Context<Self>) -> gpui::Div {
        let has_template = self.has_template(cx);
        let (template, reset, cancel, ok) = (cx.entity(), cx.entity(), cx.entity(), cx.entity());
        form::footer(
            vec![
                button::action(
                    "drawing-template",
                    "Save as default",
                    Some(IconName::Save),
                    false,
                    move |_window, cx| template.update(cx, |e, cx| e.save_template(cx)),
                )
                .tooltip("New drawings of this tool start with this look")
                .into_any_element(),
                button::action(
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
                button::action(
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
                button::action("drawing-ok", "OK", None, true, move |window, cx| {
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
                .child(form::empty(IconName::Info, "This drawing was removed."))
                .into_any_element();
        };
        if self.levels_changed || self.levels.len() != drawing.levels().len() {
            self.make_level_fields(&drawing, window, cx);
        }
        let tabs = self.tabs();
        if !tabs.contains(&self.tab) {
            self.tab = tabs[0];
        }
        let specs: Vec<form::Tab> = tabs.iter().map(|t| t.spec()).collect();
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
        form::frame(
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
        if drawing.tool.is_position() && index == 1 {
            drawing.style.position.atr_stop = None;
        }
        if drawing.tool.is_position() && index == 2 {
            drawing.style.position.target_rr = None;
        }
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
