//! The settings of one drawing: its look (lines, fill, levels, extension), its words, the exact
//! place of its points, and where it shows.
//!
//! Every change shows on the charts at once. OK keeps the changes as one undo step, Cancel puts
//! the drawing back as it was, and Escape or the close button keep them.

pub mod actions;
pub mod chrome;
mod coordinates;
pub mod helpers;
mod levels;
mod position;
mod style;
mod text;
mod visibility;

use super::Chart;
use super::GROUPS;
use super::zone::Zone;
use crate::app::drawings::Drawings;
use crate::ui::kit::field::{SliderField, ValueChanged};
use crate::ui::kit::font_picker::{FontChosen, FontPicker};
use crate::ui::kit::icon::IconName;
use crate::ui::kit::input::{InputEvent, InputState, Textarea, TextareaState};
use crate::ui::kit::{controls, form, modal, number, theme, tokens};
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div, px};
use wyck_drawings::extras::ICONS;
use wyck_drawings::extras::notes::icon_key;
use wyck_drawings::figures::patterns::wave_names;
use wyck_drawings::look::{Cap, HAlign, LabelSide, LevelText, VAlign};
use wyck_drawings::model::style::{DASHES, Dash, Level, MIN_LINE_OPACITY};
use wyck_drawings::model::tool::Tool;
use wyck_drawings::model::{DEGREES, Drawing, MAX_LEVELS, wave_label};
use wyck_market::PRICE_SCALE;
use wyck_trading::atr_stop::{AtrStop, Smoothing};

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
                .label("Drawing settings")
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
    opacity: Entity<SliderField>,
    /// The opacity of the lines, in percent.
    line_opacity: Entity<SliderField>,
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
    /// The opacity of the tag behind the words, in percent.
    tag_opacity: Entity<SliderField>,
    /// The list of fonts of the Text tab, made when it is first opened.
    font_picker: Option<Entity<FontPicker>>,
    font_open: bool,
    _font_subscription: Option<Subscription>,
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
            SliderField::new(
                number::Kind::Share,
                f64::from(style.fill_opacity) * 100.0,
                "%",
                window,
                cx,
            )
        });
        let line_opacity = cx.new(|cx| {
            SliderField::within(
                number::Kind::Share,
                f64::from(MIN_LINE_OPACITY) * 100.0,
                100.0,
                f64::from(style.opacity) * 100.0,
                "%",
                window,
                cx,
            )
        });
        let text_size = cx.new(|cx| {
            number::state(number::Kind::Count, f64::from(style.text_size), window, cx)
                .min(6.0)
                .max(48.0)
        });
        let tag_opacity = cx.new(|cx| {
            SliderField::within(
                number::Kind::Share,
                5.0,
                100.0,
                f64::from(style.text_layout.tag_opacity()) * 100.0,
                "%",
                window,
                cx,
            )
        });
        let width =
            cx.new(|cx| number::state(number::Kind::LineWidth, f64::from(style.width), window, cx));
        let scale = cx.new(|cx| {
            number::state(
                number::Kind::Scale,
                f64::from(style.scale) * 100.0,
                window,
                cx,
            )
            .min(30.0)
            .max(500.0)
        });
        let profile_rows = cx.new(|cx| {
            number::state(
                number::Kind::Count,
                f64::from(style.profile.rows),
                window,
                cx,
            )
            .max(240.0)
        });
        let profile_area = cx.new(|cx| {
            number::state(
                number::Kind::Share,
                f64::from(style.profile.value_area) * 100.0,
                window,
                cx,
            )
            .min(10.0)
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
            cx.subscribe(&opacity, |this, _, event: &ValueChanged, cx| {
                let value = event.0;
                this.change(cx, |d| d.style.fill_opacity = (value / 100.0) as f32);
            }),
            cx.subscribe(&line_opacity, |this, _, event: &ValueChanged, cx| {
                let value = event.0;
                this.change(cx, |d| {
                    d.style.opacity = (value / 100.0).clamp(f64::from(MIN_LINE_OPACITY), 1.0) as f32
                });
            }),
            number::watch(&text_size, cx, |this, value, cx| {
                this.change(cx, |d| d.style.text_size = value as f32);
            }),
            cx.subscribe(&tag_opacity, |this, _, event: &ValueChanged, cx| {
                let value = event.0;
                this.change(cx, |d| {
                    d.style.text_layout.background_opacity = (value / 100.0).clamp(0.05, 1.0) as f32
                });
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
                    InputState::new(window, cx).default_value(helpers::format_real(point.p, digits))
                });
                subscriptions.push(number::watch(&price, cx, move |this, value, cx| {
                    let raw = value * PRICE_SCALE as f64;
                    this.change(cx, |d| helpers::set_point(d, index, None, Some(raw)));
                }));
                let time = cx.new(|cx| {
                    InputState::new(window, cx)
                        .default_value(
                            zone.format(point.t, crate::domain::chart::zone::TIME_PATTERN),
                        )
                        .placeholder("YYYY-MM-DD HH:MM")
                });
                subscriptions.push(number::watch_parsed(
                    &time,
                    cx,
                    |this, text| this.zone.parse(text),
                    move |this, t, cx| {
                        this.change(cx, |d| helpers::set_point(d, index, Some(t), None))
                    },
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
            let atr_length = cx.new(|cx| {
                number::state(number::Kind::Count, atr.length as f64, window, cx)
                    .min(1.0)
                    .max(1_000.0)
            });
            let atr_multiplier =
                cx.new(|cx| number::state(number::Kind::Multiplier, atr.multiplier, window, cx));
            let rr = cx.new(|cx| {
                number::state(number::Kind::Ratio, p.target_rr.unwrap_or(2.0), window, cx)
            });
            let atr_timeframe = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(atr.timeframe.clone().unwrap_or_default())
                    .placeholder("Chart TF, M15, H1...")
            });
            let account =
                cx.new(|cx| number::state(number::Kind::Money, p.account, window, cx).min(1.0));
            let risk = cx.new(|cx| number::state(number::Kind::Percent, p.risk, window, cx));
            let lot_size = cx.new(|cx| number::state(number::Kind::Amount, p.lot_size, window, cx));
            let leverage = cx.new(|cx| {
                number::state(number::Kind::Count, p.leverage, window, cx)
                    .min(1.0)
                    .max(10_000.0)
            });
            let point_value =
                cx.new(|cx| number::state(number::Kind::Amount, p.point_value, window, cx));
            let qty_precision = cx.new(|cx| {
                number::state(number::Kind::Count, f64::from(p.qty_precision), window, cx).max(8.0)
            });
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
                        if code.is_empty() || super::Timeframe::from_code(&code).is_some() {
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
            tag_opacity,
            font_picker: None,
            font_open: false,
            _font_subscription: None,
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
            let state = cx.new(|cx| {
                number::state(number::Kind::Level, level.value, window, cx)
                    .min(-100.0)
                    .max(100.0)
            });
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
            let width = cx.new(|cx| {
                number::state(number::Kind::LineWidth, f64::from(level.width), window, cx)
            });
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
}
