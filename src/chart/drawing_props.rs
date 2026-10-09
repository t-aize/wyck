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
use super::object_tree::tool_icon;
use super::timeframe::GROUPS;
use super::zone::Zone;
use crate::chart_core::drawing::extras::{ICONS, icon_key};
use crate::chart_core::drawing::figures::wave_names;
use crate::chart_core::drawing::look::{Cap, HAlign, LabelSide, LevelText, VAlign};
use crate::chart_core::drawing::model::{
    DASHES, DEGREES, Dash, Drawing, Level, MAX_LEVELS, MIN_LINE_OPACITY, Point, Tool, wave_label,
};
use crate::chart_core::study::atr_stop::{AtrStop, Smoothing};
use crate::ui::kit::field::{SliderField, ValueChanged};
use crate::ui::kit::font_picker::{FontChosen, FontPicker};
use crate::ui::kit::{button, controls, form, form::Head, modal, number, theme, tokens};

mod coordinates;
mod levels;
mod position;
mod style;
mod text;
mod visibility;

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
                    InputState::new(window, cx).default_value(format_real(point.p, digits))
                });
                subscriptions.push(number::watch(&price, cx, move |this, value, cx| {
                    let raw = value * PRICE_SCALE as f64;
                    this.change(cx, |d| set_point(d, index, None, Some(raw)));
                }));
                let time = cx.new(|cx| {
                    InputState::new(window, cx)
                        .default_value(zone.format(point.t, crate::chart_core::zone::TIME_PATTERN))
                        .placeholder("YYYY-MM-DD HH:MM")
                });
                subscriptions.push(number::watch_parsed(
                    &time,
                    cx,
                    |this, text| this.zone.parse(text),
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

    fn current(&self, cx: &App) -> Option<Drawing> {
        self.drawings
            .read(cx)
            .book()
            .get(&self.symbol, self.id)
            .cloned()
    }

    /// What a row's reset button does: puts one part of the look back to the tool's built-in
    /// value. `restore` copies that part from the built-in style into the drawing's.
    fn restore(
        &self,
        cx: &mut Context<Self>,
        restore: fn(&mut Drawing, &crate::chart_core::drawing::model::Style),
    ) -> impl Fn(&mut Window, &mut App) + 'static + use<> {
        let this = cx.entity();
        move |window, cx| {
            this.update(cx, |e, cx| {
                e.change(cx, |d| {
                    let built_in = d.tool.default_style();
                    restore(d, &built_in);
                });
                e.set_fields(window, cx);
            });
        }
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
        crate::ui::kit::toast::show(
            cx,
            crate::ui::kit::toast::Kind::Success,
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
        let opacity = f64::from(drawing.style.fill_opacity) * 100.0;
        let size = number::format(f64::from(drawing.style.text_size), 0);
        let line = f64::from(drawing.style.opacity) * 100.0;
        self.opacity.update(cx, |s, cx| s.show(opacity, window, cx));
        self.line_opacity
            .update(cx, |s, cx| s.show(line, window, cx));
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
        let tag = f64::from(drawing.style.text_layout.tag_opacity()) * 100.0;
        self.tag_opacity.update(cx, |s, cx| s.show(tag, window, cx));
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
        // The lines and the zones each have an opacity, shown as a bar in their color panel. The
        // number field beside it is kept in step. What a swatch colors decides which one it is: the
        // lines (the line, a position's entry) or the area (the fill, a position's target and stop
        // zones). The other colors have no opacity of their own.
        let kind: Option<(&'static str, bool)> = match swatch {
            Swatch::Line | Swatch::Entry => Some(("LINES", true)),
            Swatch::Fill => Some(("FILL", false)),
            Swatch::Target | Swatch::Stop => Some(("ZONE", false)),
            _ => None,
        };
        let opacity = kind.and_then(|(label, lines)| {
            let drawing = self.current(cx)?;
            let value = if lines {
                drawing.style.opacity
            } else {
                drawing.style.fill_opacity
            };
            let this = cx.entity();
            let change: crate::ui::kit::color_picker::ChangeOpacity =
                std::rc::Rc::new(move |value, window, cx| {
                    this.update(cx, |e, cx| {
                        e.change(cx, |d| {
                            if lines {
                                d.style.opacity = value.clamp(MIN_LINE_OPACITY, 1.0);
                            } else {
                                d.style.fill_opacity = value;
                            }
                        });
                        e.set_fields(window, cx);
                    });
                });
            Some(crate::ui::kit::color_picker::Opacity {
                value,
                label,
                change,
            })
        });
        controls::color_swatch_with_opacity(
            SharedString::from(id.to_owned()),
            color,
            opacity,
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

#[cfg(test)]
mod tests {
    use super::*;

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
