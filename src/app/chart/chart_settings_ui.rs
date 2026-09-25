//! The settings of a chart: its type and colors, the legend, the price scale, the lines and the
//! grid, the canvas, the time zone, the trading lines, and the indicators it holds.
//!
//! It is built from the same frame as the panels of the indicators and the drawings (see
//! [`crate::app::settings_ui`]). Every change applies at once, so the chart behind the panel shows
//! the result live. OK keeps the changes, Cancel puts the chart back as it was when the panel
//! opened, and Escape or the close button keep them.

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, Rgba, SharedString, Subscription, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::InputState;
use wyck::openapi::market::PRICE_SCALE;

use super::options::{ChartColors, CrosshairStyle, ScaleMargin};
use super::settings::{ChartKind, ChartSettings, MAX_STUDIES, ScaleMode};
use super::study::StudyConfig;
use super::transform::{BoxSize, TransformSettings};
use super::zone::Zone;
use super::{Chart, footprint_ui, indicator_picker, overlay, study_settings};
use crate::app::connection::ui::icon_colored;
use crate::app::settings_ui::{self as ui, Head, Tab};
use crate::app::{modal, theme, widgets};

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
        modal::open(editor, modal::Options::new(820.0, 640.0), window, cx);
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
        }
    }

    /// The color of the theme, as `0xRRGGBB`.
    fn theme_color(self) -> u32 {
        let none = ChartColors::default();
        let palette = super::scene::Palette::for_chart(&none);
        rgb_u32(match self {
            Self::Up => palette.up,
            Self::Down => palette.down,
            Self::Line => palette.line,
            Self::Background => palette.bg,
            Self::Grid => palette.text,
            Self::Text => palette.text,
            Self::Crosshair => palette.text,
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
            let state =
                cx.new(|cx| widgets::number_state(size_value(size), 0.0, 1e9, 1.0, 6, window, cx));
            subscriptions.push(widgets::watch_number(&state, cx, move |this, value, cx| {
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
            widgets::number_state(
                f64::from(transform.line_break),
                1.0,
                10.0,
                1.0,
                0,
                window,
                cx,
            )
        });
        subscriptions.push(widgets::watch_number(&line_break, cx, |this, value, cx| {
            edit_chart(&this.chart, cx, |s| {
                s.transform.line_break = value.round() as u32;
            });
        }));
        let reversal = cx.new(|cx| {
            widgets::number_state(
                f64::from(transform.pnf_reversal),
                1.0,
                10.0,
                1.0,
                0,
                window,
                cx,
            )
        });
        subscriptions.push(widgets::watch_number(&reversal, cx, |this, value, cx| {
            edit_chart(&this.chart, cx, |s| {
                s.transform.pnf_reversal = value.round() as u32;
            });
        }));
        let (footprint, footprint_subscriptions) = footprint_ui::inputs(&chart, window, cx);
        subscriptions.extend(footprint_subscriptions);
        Self {
            chart,
            original,
            page,
            color_open: None,
            sizes,
            line_break,
            reversal,
            footprint,
            _subscriptions: subscriptions,
        }
    }

    // ---- rows ----

    fn switch_row(
        &self,
        id: &'static str,
        label: &'static str,
        hint: Option<&'static str>,
        on: bool,
        change: fn(&mut ChartSettings, bool),
    ) -> AnyElement {
        let chart = self.chart.clone();
        ui::field(
            label,
            hint,
            ui::toggle(id, on, move |on, _window, cx| {
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
        ui::field(
            label,
            hint,
            widgets::segmented(id, &names, index, move |choice, _window, cx| {
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
        let swatch = widgets::color_swatch(
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
            Button::new(SharedString::from(format!("chart-color-reset-{key:?}")))
                .ghost()
                .compact()
                .icon(IconName::RotateCcw)
                .tooltip("Back to the color of the theme")
                .cursor_pointer()
                .on_click(move |_, _window, cx| {
                    edit_chart(&reset, cx, |s| key.set(&mut s.colors, None));
                })
        });
        ui::field(
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

    // ---- pages ----

    fn kind_tiles(&self, current: ChartKind) -> AnyElement {
        let mut column = div().flex().flex_col().gap_3();
        for (title, kinds) in overlay::KIND_SECTIONS {
            let mut wrap = div().flex().flex_row().flex_wrap().gap_1p5();
            for kind in kinds.iter().copied() {
                let chosen = kind == current;
                let ink = if chosen {
                    theme::fg()
                } else {
                    theme::muted_fg()
                };
                let chart = self.chart.clone();
                wrap = wrap.child(
                    div()
                        .id(SharedString::from(format!("chart-kind-{}", kind.code())))
                        .w(px(150.))
                        .h(px(34.))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_2p5()
                        .rounded_md()
                        .border_1()
                        .border_color(if chosen {
                            theme::accent()
                        } else {
                            theme::border_subtle()
                        })
                        .cursor_pointer()
                        .text_size(px(12.))
                        .text_color(ink)
                        .when(chosen, |el| el.bg(theme::accent_selected()))
                        .when(!chosen, |el| el.hover(|s| s.bg(theme::surface_hover())))
                        .on_click(move |_, _window, cx| {
                            edit_chart(&chart, cx, |s| s.kind = kind);
                        })
                        .child(icon_colored(overlay::kind_icon(kind), 15., ink))
                        .child(kind.label()),
                );
            }
            column = column.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::muted_fg())
                            .child(title),
                    )
                    .child(wrap),
            );
        }
        ui::block(column)
    }

    /// The sizes of the price based chart types, for the one the chart shows.
    fn construction_group(&self, settings: &ChartSettings, cx: &App) -> Option<AnyElement> {
        let kind = settings.kind;
        let (digits, box_size) = {
            let chart = self.chart.read(cx);
            (chart.digits(), chart.display.box_size)
        };
        let mut rows = Vec::new();
        for (field, state) in self.sizes.iter().filter(|(f, _)| f.kind() == kind) {
            let field = *field;
            let size = field.get(&settings.transform);
            let chart = self.chart.clone();
            let state_for_mode = state.clone();
            let unit_real = super::scene::quote_unit(digits) / PRICE_SCALE as f64;
            let resolved = (box_size > 0).then(|| box_size as f64 / PRICE_SCALE as f64);
            rows.push(ui::field(
                field.label(),
                Some("Measured in ATR, in price or in percent"),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(widgets::segmented(
                        SharedString::from(format!("size-mode-{field:?}")),
                        &["ATR", "Price", "%"],
                        size_mode(size),
                        move |choice, window, cx| {
                            let next = match choice {
                                0 => BoxSize::Atr { length: 14 },
                                1 => BoxSize::Fixed {
                                    price: resolved.unwrap_or(unit_real * 10.0),
                                },
                                _ => BoxSize::Percent { percent: 0.1 },
                            };
                            edit_chart(&chart, cx, |s| field.set(&mut s.transform, next));
                            state_for_mode.update(cx, |state, cx| {
                                state.set_value(
                                    widgets::format_number(size_value(next), 6),
                                    window,
                                    cx,
                                );
                            });
                        },
                    ))
                    .child(widgets::number_field(state, 110.)),
            ));
        }
        match kind {
            ChartKind::LineBreak => rows.push(ui::field(
                "Lines to break",
                Some("A new line turns after breaking the extreme of this many"),
                widgets::number_field(&self.line_break, 110.),
            )),
            ChartKind::PointFigure => rows.push(ui::field(
                "Reversal (boxes)",
                Some("Boxes the price must go back to start a new column"),
                widgets::number_field(&self.reversal, 110.),
            )),
            _ => {}
        }
        (!rows.is_empty()).then(|| {
            ui::group(
                overlay::kind_icon(kind),
                format!("{} construction", kind.label()),
                rows,
            )
            .into_any_element()
        })
    }

    fn symbol_page(&self, settings: &ChartSettings, cx: &mut Context<Self>) -> AnyElement {
        let mut page = ui::page().child(ui::group(
            IconName::ChartCandlestick,
            "Chart type",
            [self.kind_tiles(settings.kind)],
        ));
        if let Some(group) = self.construction_group(settings, cx) {
            page = page.child(group);
        }
        if settings.kind == ChartKind::Footprint {
            page = page.children(footprint_ui::groups(
                &self.chart,
                &self.footprint,
                &settings.footprint,
            ));
        }
        let mut colors = vec![
            self.color_row(
                ColorKey::Up,
                Some("Candles, bars and columns that rose"),
                cx,
            ),
            self.color_row(
                ColorKey::Down,
                Some("Candles, bars and columns that fell"),
                cx,
            ),
        ];
        if matches!(
            settings.kind,
            ChartKind::Line | ChartKind::Step | ChartKind::Area | ChartKind::Baseline
        ) {
            colors.push(self.color_row(ColorKey::Line, None, cx));
        }
        page.child(ui::group(IconName::Palette, "Colors", colors))
            .into_any_element()
    }

    fn status_page(&self, settings: &ChartSettings) -> AnyElement {
        let s = &settings.status;
        let first_line = vec![
            self.switch_row(
                "status-prices",
                "Prices under the pointer",
                Some("Open, high, low and close of the bar"),
                s.prices,
                |s, on| s.status.prices = on,
            ),
            self.switch_row(
                "status-change",
                "Change",
                Some("From the bar before, in price and percent"),
                s.change,
                |s, on| s.status.change = on,
            ),
            self.switch_row(
                "status-market",
                "Market status",
                Some("The dot that says whether the market is open"),
                s.market,
                |s, on| s.status.market = on,
            ),
        ];
        let studies = vec![
            self.switch_row(
                "status-indicators",
                "Indicator names",
                Some("One line for each indicator, with its buttons"),
                s.indicators,
                |s, on| s.status.indicators = on,
            ),
            self.switch_row(
                "status-values",
                "Indicator values",
                Some("The numbers of each indicator at the pointer"),
                s.indicator_values,
                |s, on| s.status.indicator_values = on,
            ),
        ];
        ui::page()
            .child(ui::group(IconName::Type, "Symbol line", first_line))
            .child(ui::group(IconName::ChartSpline, "Indicators", studies))
            .into_any_element()
    }

    fn scales_page(&self, settings: &ChartSettings) -> AnyElement {
        let scale = vec![
            self.choice_row(
                "scale-mode",
                "Scale",
                None,
                ScaleMode::ALL.iter().map(|m| (*m, m.label())).collect(),
                settings.scale,
                |s, mode| s.scale = mode,
            ),
            self.switch_row(
                "scale-invert",
                "Invert the scale",
                Some("Higher prices are drawn lower"),
                settings.invert,
                |s, on| s.invert = on,
            ),
            self.choice_row(
                "scale-margin",
                "Margins",
                Some("Room above and below the prices on the automatic scale"),
                ScaleMargin::ALL.iter().map(|m| (*m, m.label())).collect(),
                settings.margin,
                |s, margin| s.margin = margin,
            ),
        ];
        let p = &settings.price_lines;
        let lines = vec![
            self.switch_row(
                "lines-last",
                "Line at the last price",
                None,
                p.last_line,
                |s, on| s.price_lines.last_line = on,
            ),
            self.switch_row(
                "lines-tag",
                "Tag with the last price",
                Some("On the price axis"),
                p.last_tag,
                |s, on| s.price_lines.last_tag = on,
            ),
            self.switch_row(
                "lines-countdown",
                "Time left in the bar",
                Some("Under the tag of the last price"),
                p.countdown,
                |s, on| s.price_lines.countdown = on,
            ),
            self.switch_row("lines-ask", "Line at the ask", None, p.ask_line, |s, on| {
                s.price_lines.ask_line = on
            }),
            self.switch_row(
                "lines-previous",
                "Close of the day before",
                Some("A dashed line at the last close of the previous day"),
                p.previous_close,
                |s, on| s.price_lines.previous_close = on,
            ),
            self.switch_row(
                "lines-days",
                "Day separators",
                Some("A vertical line where each day starts, on intraday charts"),
                p.day_breaks,
                |s, on| s.price_lines.day_breaks = on,
            ),
        ];
        let grid = vec![
            self.switch_row("grid-show", "Grid", None, settings.grid, |s, on| {
                s.grid = on
            }),
            self.switch_row(
                "grid-horizontal",
                "Horizontal lines",
                Some("At the prices of the axis"),
                settings.grid_horizontal,
                |s, on| s.grid_horizontal = on,
            ),
            self.switch_row(
                "grid-vertical",
                "Vertical lines",
                Some("At the times of the axis"),
                settings.grid_vertical,
                |s, on| s.grid_vertical = on,
            ),
        ];
        let crosshair = vec![
            self.choice_row(
                "crosshair-style",
                "Crosshair",
                Some("Off also hides its tags on the axes"),
                CrosshairStyle::ALL
                    .iter()
                    .map(|m| (*m, m.label()))
                    .collect(),
                settings.crosshair,
                |s, style| s.crosshair = style,
            ),
        ];
        ui::page()
            .child(ui::group(IconName::Ruler, "Price scale", scale))
            .child(ui::group(IconName::Tag, "Price lines", lines))
            .child(ui::group(IconName::Grid3x3, "Grid", grid))
            .child(ui::group(IconName::Crosshair, "Crosshair", crosshair))
            .into_any_element()
    }

    fn canvas_page(&self, settings: &ChartSettings, cx: &mut Context<Self>) -> AnyElement {
        let mut colors = vec![
            self.color_row(ColorKey::Background, None, cx),
            self.color_row(ColorKey::Grid, Some("Drawn faint"), cx),
            self.color_row(
                ColorKey::Text,
                Some("The numbers and labels of the axes"),
                cx,
            ),
            self.color_row(ColorKey::Crosshair, None, cx),
        ];
        if settings.colors.any() {
            let chart = self.chart.clone();
            colors.push(ui::field(
                "Colors of this chart",
                Some("Rising, falling and line colors are on the Symbol page"),
                ui::action(
                    "canvas-theme-colors",
                    "Use the theme",
                    Some(IconName::RotateCcw),
                    false,
                    move |_window, cx| {
                        edit_chart(&chart, cx, |s| s.colors = ChartColors::default())
                    },
                ),
            ));
        }
        let watermark = vec![self.switch_row(
            "canvas-watermark",
            "Symbol watermark",
            Some("The symbol and timeframe, large and faint, behind the prices"),
            settings.watermark,
            |s, on| s.watermark = on,
        )];
        ui::page()
            .child(ui::group(IconName::Palette, "Colors", colors))
            .child(ui::group(IconName::Image, "Watermark", watermark))
            .into_any_element()
    }

    fn time_page(&self, settings: &ChartSettings) -> AnyElement {
        let now = super::now_ms();
        let mut zones = div()
            .id("settings-zone-list")
            .flex()
            .flex_col()
            .max_h(px(380.))
            .overflow_y_scroll()
            .rounded_md()
            .border_1()
            .border_color(theme::border_subtle());
        for zone in Zone::menu() {
            let chosen = zone == settings.zone;
            let chart = self.chart.clone();
            zones = zones.child(
                div()
                    .id(SharedString::from(format!("settings-zone-{}", zone.code())))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(30.))
                    .px_2p5()
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(if chosen {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    })
                    .when(chosen, |el| el.bg(theme::accent_selected()))
                    .hover(|s| s.bg(theme::surface_hover()))
                    .on_click(move |_, _window, cx| {
                        edit_chart(&chart, cx, |s| s.zone = zone);
                    })
                    .child(zone.name(now))
                    .children(chosen.then(|| icon_colored(IconName::Check, 13., theme::accent()))),
            );
        }
        ui::page()
            .child(ui::group(
                IconName::Clock,
                "Time zone of the time axis",
                [ui::block(zones)],
            ))
            .child(ui::note(
                "The zone applies to the time axis, the crosshair, the day separators and the times of the drawings.",
            ))
            .into_any_element()
    }

    fn trading_page(&self, settings: &ChartSettings, cx: &App) -> AnyElement {
        let t = &settings.trading;
        let lines = vec![
            self.switch_row(
                "trading-orders",
                "Working orders",
                Some("Limit and stop orders, at their price"),
                t.orders,
                |s, on| s.trading.orders = on,
            ),
            self.switch_row(
                "trading-positions",
                "Open positions",
                Some("At the entry price, with the profit"),
                t.positions,
                |s, on| s.trading.positions = on,
            ),
            self.switch_row(
                "trading-protection",
                "Stop loss and take profit",
                None,
                t.protection,
                |s, on| s.trading.protection = on,
            ),
            self.switch_row("trading-alerts", "Price alerts", None, t.alerts, |s, on| {
                s.trading.alerts = on
            }),
        ];
        let mut page = ui::page().child(ui::group(
            IconName::ArrowLeftRight,
            "Lines on the chart",
            lines,
        ));
        if let Some(count) = self.chart.read(cx).drawing_count(cx) {
            let chart = self.chart.clone();
            page = page.child(ui::group(
                IconName::PenTool,
                "Drawings",
                [ui::field(
                    "Drawings and positions on this symbol",
                    Some(match count {
                        0 => "None yet",
                        1 => "1 drawing",
                        _ => "Several drawings",
                    }),
                    ui::action(
                        "trading-object-tree",
                        "Open the list",
                        Some(IconName::ListTree),
                        false,
                        move |window, cx| {
                            super::open_object_tree(&chart, window, cx);
                        },
                    ),
                )],
            ));
        }
        page.into_any_element()
    }

    fn study_row(&self, index: usize, config: &StudyConfig) -> AnyElement {
        let spec = config.spec();
        let button = |id: &str, icon: IconName, tip: &'static str| {
            Button::new(SharedString::from(format!("{id}-{index}")))
                .ghost()
                .compact()
                .icon(icon)
                .tooltip(tip)
                .cursor_pointer()
        };
        let (toggle, edit, remove) = (self.chart.clone(), self.chart.clone(), self.chart.clone());
        let visible = config.visible;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(40.))
            .child(
                div()
                    .flex_none()
                    .w(px(56.))
                    .text_size(px(11.))
                    .text_color(theme::accent())
                    .child(spec.short),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.))
                    .text_color(if visible {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    })
                    .truncate()
                    .child(config.title()),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .gap_0p5()
                    .child(
                        button(
                            "chart-study-eye",
                            if visible {
                                IconName::Eye
                            } else {
                                IconName::EyeOff
                            },
                            if visible { "Hide" } else { "Show" },
                        )
                        .on_click(move |_, _window, cx| {
                            edit_chart(&toggle, cx, |s| {
                                if let Some(study) = s.studies.get_mut(index) {
                                    study.visible = !study.visible;
                                }
                            });
                        }),
                    )
                    .child(
                        button("chart-study-settings", IconName::Settings2, "Settings").on_click(
                            move |_, window, cx| {
                                study_settings::open(edit.clone(), index, window, cx);
                            },
                        ),
                    )
                    .child(
                        button("chart-study-remove", IconName::Trash, "Remove").on_click(
                            move |_, _window, cx| {
                                remove.update(cx, |chart, cx| chart.remove_study(index, cx));
                            },
                        ),
                    ),
            )
            .into_any_element()
    }

    fn indicators_page(&self, settings: &ChartSettings) -> AnyElement {
        let on_chart: Vec<AnyElement> = settings
            .studies
            .iter()
            .enumerate()
            .map(|(index, config)| self.study_row(index, config))
            .collect();
        let held = if on_chart.is_empty() {
            vec![ui::block(ui::note("No indicator on this chart yet."))]
        } else {
            on_chart
        };

        let full = settings.studies.len() >= MAX_STUDIES;
        let (browse, editor, new) = (self.chart.clone(), self.chart.clone(), self.chart.clone());
        let add = if full {
            ui::block(ui::note(format!(
                "A chart holds at most {MAX_STUDIES} indicators. Remove one to add another."
            )))
        } else {
            ui::block(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .child(ui::action(
                        "chart-browse-indicators",
                        "Browse the indicators",
                        Some(IconName::ChartSpline),
                        true,
                        move |window, cx| {
                            indicator_picker::open(browse.clone(), window, cx);
                        },
                    ))
                    .child(ui::action(
                        "chart-open-editor",
                        "Indicator editor",
                        Some(IconName::CodeXml),
                        false,
                        move |window, cx| {
                            modal::close(window, cx);
                            editor.update(cx, |_, cx| {
                                cx.emit(super::ChartEvent::IndicatorEditor(
                                    super::EditorRequest::Open,
                                ));
                            });
                        },
                    ))
                    .child(ui::action(
                        "chart-new-script",
                        "New script",
                        Some(IconName::FilePlus),
                        false,
                        move |window, cx| {
                            modal::close(window, cx);
                            new.update(cx, |_, cx| {
                                cx.emit(super::ChartEvent::IndicatorEditor(
                                    super::EditorRequest::New,
                                ));
                            });
                        },
                    )),
            )
        };

        let chart = self.chart.clone();
        let layout = vec![ui::field(
            "Height of the prices",
            Some("Against the panes of the indicators. Drag a divider for any other height"),
            ui::height_picker(
                "chart-price-height",
                PRICE_HEIGHTS,
                settings.main_weight,
                move |weight, _window, cx| edit_chart(&chart, cx, |s| s.main_weight = weight),
            ),
        )];
        ui::page()
            .child(ui::group(
                IconName::ChartSpline,
                format!("On this chart ({})", settings.studies.len()),
                held,
            ))
            .child(ui::group(IconName::Plus, "Add an indicator", [add]))
            .child(ui::group(IconName::LayoutPanelTop, "Layout", layout))
            .into_any_element()
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
            Page::Indicators => self.indicators_page(&settings),
        };

        let this = cx.entity();
        let (defaults, cancel) = (cx.entity(), cx.entity());
        let footer = ui::footer(
            vec![
                ui::action(
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
                ui::action("chart-cancel", "Cancel", None, false, move |window, cx| {
                    cancel.update(cx, |e, cx| {
                        let original = e.original.clone();
                        edit_chart(&e.chart, cx, |s| *s = original);
                    });
                    modal::close(window, cx);
                })
                .into_any_element(),
                ui::action("chart-ok", "OK", None, true, |window, cx| {
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
