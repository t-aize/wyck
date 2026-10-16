//! The settings of the charts: the type and the indicators of the chart being worked on, and the
//! look every chart shares: the legend, the price scale, the lines and the
//! grid, the canvas, the time zone, the trading lines, and the indicators it holds.
//!
//! It is built from the same frame as the panels of the indicators and the drawings (see
//! [`wyck_ui_kit::form`]). Every change applies at once, so the chart behind the panel shows
//! the result live. OK keeps the changes, Cancel puts the chart back as it was when the panel
//! opened, and Escape or the close button keep them.

pub mod data_pages;
pub mod look_pages;

use super::{Chart, construction_ui, footprint_ui, tpo_ui, volume_ui};
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div};
use wyck_chart::ChartColors;
use wyck_chart::Rgba;
use wyck_chart::{BoxSize, TransformSettings};
use wyck_chart::{ChartKind, ChartSettings};
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::input::InputState;
use wyck_ui_kit::{button, controls, form, form::Head, form::Tab, modal, number};

/// The tallest the list of time zones grows before it scrolls.
const ZONE_LIST_MAX: f32 = 380.0;
/// The width of a label before its control.
const LABEL_WIDTH: f32 = 56.0;

/// How tall the prices are against the panes of the indicators: a name and the weight it sets.
const PRICE_HEIGHTS: &[(&str, f32)] = &[
    ("Small", 1.5),
    ("Normal", 3.0),
    ("Large", 5.0),
    ("Huge", 8.0),
];

/// Opens the settings of `chart`.
pub fn open(chart: Entity<Chart>, window: &mut Window, cx: &mut App) {
    open_at(chart, Page::Symbol, window, cx);
}

/// Opens the price scale, the lines and the grid of `chart`.
pub fn open_scales(chart: Entity<Chart>, window: &mut Window, cx: &mut App) {
    open_at(chart, Page::Scales, window, cx);
}

/// Opens the indicators of `chart`.
pub fn open_indicators(chart: Entity<Chart>, window: &mut Window, cx: &mut App) {
    open_at(chart, Page::Indicators, window, cx);
}

fn open_at(chart: Entity<Chart>, page: Page, window: &mut Window, cx: &mut App) {
    // Opened once the chart that asked is no longer being updated, since the panel reads it.
    window.defer(cx, move |window, cx| {
        let editor = cx.new(|cx| ChartSettingsEditor::new(chart, page, window, cx));
        modal::open(
            editor,
            modal::Options::new(820.0, 640.0).label("Chart settings"),
            window,
            cx,
        );
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Symbol,
    Status,
    Scales,
    Canvas,
    Time,
    Trading,
    Indicators,
}

impl Page {
    const ALL: [Self; 7] = [
        Self::Symbol,
        Self::Status,
        Self::Scales,
        Self::Canvas,
        Self::Time,
        Self::Trading,
        Self::Indicators,
    ];

    fn tab(self) -> Tab {
        let (label, icon) = match self {
            Self::Symbol => ("Symbol", IconName::ChartCandlestick),
            Self::Status => ("Legend", IconName::Type),
            Self::Scales => ("Scales and lines", IconName::Ruler),
            Self::Canvas => ("Canvas", IconName::Palette),
            Self::Time => ("Time zone", IconName::Clock),
            Self::Trading => ("Trading", IconName::ArrowLeftRight),
            Self::Indicators => ("Indicators", IconName::ChartSpline),
        };
        Tab { label, icon }
    }
}

/// The colors a chart can override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorKey {
    Up,
    Down,
    Line,
    Background,
    Grid,
    Text,
    Crosshair,
    KagiYang,
    KagiYin,
    PnfUp,
    PnfDown,
    Tpo,
    TpoPoc,
    TpoValueArea,
    TpoIb,
    TpoSingle,
}

impl ColorKey {
    fn label(self) -> &'static str {
        match self {
            Self::Up => "Rising",
            Self::Down => "Falling",
            Self::Line => "Line",
            Self::Background => "Background",
            Self::Grid => "Grid lines",
            Self::Text => "Axis text",
            Self::Crosshair => "Crosshair",
            Self::KagiYang => "Thick line (yang)",
            Self::KagiYin => "Thin line (yin)",
            Self::PnfUp => "X (rising)",
            Self::PnfDown => "O (falling)",
            Self::Tpo => "Marks",
            Self::TpoPoc => "Point of control",
            Self::TpoValueArea => "Value area",
            Self::TpoIb => "Initial balance",
            Self::TpoSingle => "Single prints",
        }
    }

    fn get(self, colors: &ChartColors) -> Option<u32> {
        match self {
            Self::Up => colors.up,
            Self::Down => colors.down,
            Self::Line => colors.line,
            Self::Background => colors.background,
            Self::Grid => colors.grid,
            Self::Text => colors.text,
            Self::Crosshair => colors.crosshair,
            Self::KagiYang => colors.kagi_yang,
            Self::KagiYin => colors.kagi_yin,
            Self::PnfUp => colors.pnf_up,
            Self::PnfDown => colors.pnf_down,
            Self::Tpo => colors.tpo,
            Self::TpoPoc => colors.tpo_poc,
            Self::TpoValueArea => colors.tpo_value_area,
            Self::TpoIb => colors.tpo_ib,
            Self::TpoSingle => colors.tpo_single,
        }
    }

    fn set(self, colors: &mut ChartColors, color: Option<u32>) {
        match self {
            Self::Up => colors.up = color,
            Self::Down => colors.down = color,
            Self::Line => colors.line = color,
            Self::Background => colors.background = color,
            Self::Grid => colors.grid = color,
            Self::Text => colors.text = color,
            Self::Crosshair => colors.crosshair = color,
            Self::KagiYang => colors.kagi_yang = color,
            Self::KagiYin => colors.kagi_yin = color,
            Self::PnfUp => colors.pnf_up = color,
            Self::PnfDown => colors.pnf_down = color,
            Self::Tpo => colors.tpo = color,
            Self::TpoPoc => colors.tpo_poc = color,
            Self::TpoValueArea => colors.tpo_value_area = color,
            Self::TpoIb => colors.tpo_ib = color,
            Self::TpoSingle => colors.tpo_single = color,
        }
    }

    /// The color of the theme, as `0xRRGGBB`.
    fn theme_color(self) -> u32 {
        let none = ChartColors::default();
        let palette = super::palette::palette_for_chart(&none);
        rgb_u32(match self {
            Self::Up => palette.up,
            Self::Down => palette.down,
            Self::Line => palette.line,
            Self::Background => palette.bg,
            Self::Grid => palette.text,
            Self::Text => palette.text,
            Self::Crosshair => palette.text,
            Self::KagiYang => palette.kagi_yang,
            Self::KagiYin => palette.kagi_yin,
            Self::PnfUp => palette.pnf_up,
            Self::PnfDown => palette.pnf_down,
            Self::Tpo => palette.tpo,
            Self::TpoPoc => palette.tpo_poc,
            Self::TpoValueArea => palette.tpo_value_area,
            Self::TpoIb => palette.tpo_ib,
            Self::TpoSingle => palette.tpo_single,
        })
    }
}

fn rgb_u32(color: Rgba) -> u32 {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    byte(color.r) << 16 | byte(color.g) << 8 | byte(color.b)
}

/// Which size of the price based chart types a field sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SizeField {
    Renko,
    Kagi,
    PointFigure,
    Range,
}

impl SizeField {
    const ALL: [Self; 4] = [Self::Renko, Self::Kagi, Self::PointFigure, Self::Range];

    fn label(self) -> &'static str {
        match self {
            Self::Renko => "Box size",
            Self::Kagi => "Reversal",
            Self::PointFigure => "Box size",
            Self::Range => "Range of a bar",
        }
    }

    fn kind(self) -> ChartKind {
        match self {
            Self::Renko => ChartKind::Renko,
            Self::Kagi => ChartKind::Kagi,
            Self::PointFigure => ChartKind::PointFigure,
            Self::Range => ChartKind::Range,
        }
    }

    fn get(self, t: &TransformSettings) -> BoxSize {
        match self {
            Self::Renko => t.renko_box,
            Self::Kagi => t.kagi_reversal,
            Self::PointFigure => t.pnf_box,
            Self::Range => t.range,
        }
    }

    fn set(self, t: &mut TransformSettings, size: BoxSize) {
        match self {
            Self::Renko => t.renko_box = size,
            Self::Kagi => t.kagi_reversal = size,
            Self::PointFigure => t.pnf_box = size,
            Self::Range => t.range = size,
        }
    }
}

fn size_value(size: BoxSize) -> f64 {
    match size {
        BoxSize::Atr { length } => f64::from(length),
        BoxSize::Fixed { price } => price,
        BoxSize::Percent { percent } => percent,
    }
}

fn size_mode(size: BoxSize) -> usize {
    match size {
        BoxSize::Atr { .. } => 0,
        BoxSize::Fixed { .. } => 1,
        BoxSize::Percent { .. } => 2,
    }
}

fn edit_chart(chart: &Entity<Chart>, cx: &mut App, change: impl FnOnce(&mut ChartSettings)) {
    chart.update(cx, |chart, cx| chart.edit_settings(cx, change));
}

struct ChartSettingsEditor {
    chart: Entity<Chart>,
    /// How the chart was when the panel opened, for Cancel.
    original: ChartSettings,
    page: Page,
    /// The color whose palette is open.
    color_open: Option<ColorKey>,
    sizes: Vec<(SizeField, Entity<InputState>)>,
    line_break: Entity<InputState>,
    reversal: Entity<InputState>,
    footprint: Vec<(footprint_ui::Field, Entity<InputState>)>,
    volume: volume_ui::Inputs,
    construction: construction_ui::Inputs,
    tpo: tpo_ui::Inputs,
    _subscriptions: Vec<Subscription>,
}

impl ChartSettingsEditor {
    fn new(chart: Entity<Chart>, page: Page, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let original = chart.read(cx).settings.clone();
        let transform = original.transform;
        let mut subscriptions = vec![cx.observe(&chart, |_this, _chart, cx| cx.notify())];
        let mut sizes = Vec::new();
        for field in SizeField::ALL {
            let size = field.get(&transform);
            let state = cx.new(|cx| {
                number::state(number::Kind::Amount, size_value(size), window, cx).max(1e9)
            });
            subscriptions.push(number::watch(&state, cx, move |this, value, cx| {
                if value > 0.0 {
                    edit_chart(&this.chart, cx, |s| {
                        let size = match field.get(&s.transform) {
                            BoxSize::Atr { .. } => BoxSize::Atr {
                                length: value.round().max(1.0) as u32,
                            },
                            BoxSize::Fixed { .. } => BoxSize::Fixed { price: value },
                            BoxSize::Percent { .. } => BoxSize::Percent { percent: value },
                        };
                        field.set(&mut s.transform, size);
                    });
                }
            }));
            sizes.push((field, state));
        }
        let line_break = cx.new(|cx| {
            number::state(
                number::Kind::Count,
                f64::from(transform.line_break),
                window,
                cx,
            )
            .min(1.0)
            .max(10.0)
        });
        subscriptions.push(number::watch(&line_break, cx, |this, value, cx| {
            edit_chart(&this.chart, cx, |s| {
                s.transform.line_break = value.round() as u32;
            });
        }));
        let reversal = cx.new(|cx| {
            number::state(
                number::Kind::Count,
                f64::from(transform.pnf_reversal),
                window,
                cx,
            )
            .min(1.0)
            .max(10.0)
        });
        subscriptions.push(number::watch(&reversal, cx, |this, value, cx| {
            edit_chart(&this.chart, cx, |s| {
                s.transform.pnf_reversal = value.round() as u32;
            });
        }));
        let (footprint, footprint_subscriptions) = footprint_ui::inputs(&chart, window, cx);
        subscriptions.extend(footprint_subscriptions);
        let (volume, volume_subscriptions) = volume_ui::inputs(&chart, window, cx);
        subscriptions.extend(volume_subscriptions);
        let (construction, construction_subscriptions) =
            construction_ui::inputs(&chart, window, cx);
        subscriptions.extend(construction_subscriptions);
        let (tpo, tpo_subscriptions) = tpo_ui::inputs(&chart, window, cx);
        subscriptions.extend(tpo_subscriptions);
        Self {
            chart,
            original,
            page,
            color_open: None,
            sizes,
            line_break,
            reversal,
            footprint,
            volume,
            construction,
            tpo,
            _subscriptions: subscriptions,
        }
    }

    fn switch_row(
        &self,
        id: &'static str,
        label: &'static str,
        hint: Option<&'static str>,
        on: bool,
        change: fn(&mut ChartSettings, bool),
    ) -> AnyElement {
        let chart = self.chart.clone();
        form::field(
            label,
            hint,
            controls::toggle(id, on, move |on, _window, cx| {
                edit_chart(&chart, cx, |s| change(s, on));
            }),
        )
    }

    fn choice_row<T: Copy + PartialEq + 'static>(
        &self,
        id: &'static str,
        label: &'static str,
        hint: Option<&'static str>,
        options: Vec<(T, &'static str)>,
        current: T,
        change: fn(&mut ChartSettings, T),
    ) -> AnyElement {
        let names: Vec<&'static str> = options.iter().map(|(_, name)| *name).collect();
        let index = options
            .iter()
            .position(|(value, _)| *value == current)
            .unwrap_or(usize::MAX);
        let chart = self.chart.clone();
        form::field(
            label,
            hint,
            controls::segmented(id, &names, index, move |choice, _window, cx| {
                let value = options[choice].0;
                edit_chart(&chart, cx, |s| change(s, value));
            }),
        )
    }

    fn color_row(
        &self,
        key: ColorKey,
        hint: Option<&'static str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = key.get(&self.chart.read(cx).settings.colors);
        let shown = value.unwrap_or_else(|| key.theme_color());
        let open = self.color_open == Some(key);
        let this = cx.entity();
        let (pick, reset) = (self.chart.clone(), self.chart.clone());
        let swatch = controls::color_swatch(
            SharedString::from(format!("chart-color-{key:?}")),
            shown,
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
            move |color, _window, cx| {
                edit_chart(&pick, cx, |s| key.set(&mut s.colors, Some(color)));
            },
        );
        let back = value.is_some().then(|| {
            wyck_ui_kit::button::quiet(SharedString::from(format!("chart-color-reset-{key:?}")))
                .icon(IconName::RotateCcw)
                .tooltip("Back to the color of the theme")
                .on_click(move |_, _window, cx| {
                    edit_chart(&reset, cx, |s| key.set(&mut s.colors, None));
                })
        });
        form::field(
            key.label(),
            hint,
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .children(back)
                .child(swatch),
        )
    }
}

impl Render for ChartSettingsEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (settings, subtitle) = {
            let chart = self.chart.read(cx);
            let name = chart
                .symbol()
                .map_or_else(|| "No symbol".to_owned(), |s| s.name.to_string());
            (
                chart.settings.clone(),
                format!(
                    "{name} - {} - {}",
                    chart.timeframe().label(),
                    chart.settings.kind.label()
                ),
            )
        };
        let tabs: Vec<Tab> = Page::ALL.iter().map(|p| p.tab()).collect();
        let active = Page::ALL.iter().position(|p| *p == self.page).unwrap_or(0);
        let head = Head {
            icon: IconName::Settings2,
            title: "Chart settings".into(),
            subtitle: subtitle.into(),
        };
        let body = match self.page {
            Page::Symbol => self.symbol_page(&settings, cx),
            Page::Status => self.status_page(&settings),
            Page::Scales => self.scales_page(&settings),
            Page::Canvas => self.canvas_page(&settings, cx),
            Page::Time => self.time_page(&settings),
            Page::Trading => self.trading_page(&settings, cx),
            Page::Indicators => self.indicators_page(&settings, cx),
        };
        // How a chart looks is how every chart looks: the pages that set it say so.
        let body = if matches!(
            self.page,
            Page::Status | Page::Scales | Page::Canvas | Page::Time | Page::Trading
        ) {
            form::page()
                .child(form::note("These settings are the same for every chart."))
                .child(body)
                .into_any_element()
        } else {
            body
        };

        let this = cx.entity();
        let (defaults, cancel) = (cx.entity(), cx.entity());
        let footer = form::footer(
            vec![
                button::action(
                    "chart-defaults",
                    "Defaults",
                    Some(IconName::RotateCcw),
                    false,
                    move |_window, cx| {
                        defaults.update(cx, |e, cx| {
                            e.color_open = None;
                            edit_chart(&e.chart, cx, |s| {
                                *s = std::mem::take(s).with_default_look();
                            });
                        });
                    },
                )
                .into_any_element(),
            ],
            vec![
                button::action("chart-cancel", "Cancel", None, false, move |window, cx| {
                    cancel.update(cx, |e, cx| {
                        let original = e.original.clone();
                        edit_chart(&e.chart, cx, |s| *s = original);
                    });
                    modal::close(window, cx);
                })
                .into_any_element(),
                button::action("chart-ok", "OK", None, true, |window, cx| {
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
                this.update(cx, |e, cx| {
                    e.page = Page::ALL[index];
                    e.color_open = None;
                    cx.notify();
                });
            },
            modal::dismiss,
            body,
            footer,
        )
    }
}
