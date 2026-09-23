//! What sits over the canvas: the legend of each band (the symbol, the prices under the pointer,
//! the indicators with their buttons), the toolbar, its menus, the labels of the order and alert
//! lines, the right-click menu, and the loading, failed and empty states.

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Entity, FontWeight, MouseButton, SharedString, Window, deferred, div, px,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use wyck::openapi::market::format_price;

use super::data::Series;
use super::drawing::book::Order;
use super::lines::to_real;
use super::scene::{AXIS_H, AXIS_W, Geometry, price};
use super::settings::{ChartKind, ScaleMode};
use super::study::{Placement, PlotKind, ValueFormat};
use super::view::PriceScale;
use super::zone::Zone;
use super::{Chart, ChartAction, ChartEvent, DrawingCommand, Load, Menu, Older, paint, study_ui};
use crate::app::connection::ui;
use crate::app::{anim, theme};

pub fn kind_icon(kind: ChartKind) -> IconName {
    match kind {
        ChartKind::Candles | ChartKind::Hollow => IconName::ChartCandlestick,
        ChartKind::HeikinAshi => IconName::ChartNoAxesCombined,
        ChartKind::Bars => IconName::ChartNoAxesColumn,
        ChartKind::Line => IconName::ChartLine,
        ChartKind::Step => IconName::Activity,
        ChartKind::Area => IconName::ChartArea,
        ChartKind::Baseline => IconName::ChartSpline,
        ChartKind::Renko => IconName::Blocks,
        ChartKind::LineBreak => IconName::ChartColumnIncreasing,
        ChartKind::Kagi => IconName::Spline,
        ChartKind::PointFigure => IconName::Grid3x3,
        ChartKind::Range => IconName::ChartColumnBig,
    }
}

/// The kinds of the menu, in sections.
const KIND_SECTIONS: [(&str, &[ChartKind]); 3] = [
    (
        "Bars",
        &[
            ChartKind::Candles,
            ChartKind::Hollow,
            ChartKind::HeikinAshi,
            ChartKind::Bars,
        ],
    ),
    (
        "Lines",
        &[
            ChartKind::Line,
            ChartKind::Step,
            ChartKind::Area,
            ChartKind::Baseline,
        ],
    ),
    (
        "Price based",
        &[
            ChartKind::Renko,
            ChartKind::LineBreak,
            ChartKind::Kagi,
            ChartKind::PointFigure,
            ChartKind::Range,
        ],
    ),
];

fn rgb(color: u32) -> gpui::Rgba {
    gpui::rgb(color)
}

impl Render for Chart {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.open_pending_settings(window, cx);
        let entity = cx.entity();
        let compact = self.is_compact();
        let latest = !self.view.is_following() && !self.shown().is_empty();
        let geometry = self.geometry();
        let menu_entity = entity.clone();

        div()
            .id(("chart", self.id))
            .relative()
            .flex_1()
            .w_full()
            .min_h_0()
            .overflow_hidden()
            .bg(theme::bg())
            .child(paint::surface(&entity, self.bounds.clone()))
            .children(self.legends(&geometry, compact, cx))
            .children(self.line_labels(&geometry, cx))
            .children((!compact || self.menu.is_some()).then(|| self.toolbar(latest, compact, cx)))
            .children(self.menu_popup(&geometry, cx))
            .children(self.status(cx))
            .context_menu(move |menu, window, cx| context_menu(&menu_entity, menu, window, cx))
    }
}

/// The items for the drawing under the pointer, at the top of the right-click menu.
fn drawing_menu(
    chart: &Entity<Chart>,
    id: u64,
    mut menu: PopupMenu,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let Some(drawing) = chart.read(cx).drawings.as_ref().and_then(|drawings| {
        let symbol = chart.read(cx).symbol_name()?;
        drawings.read(cx).book().get(&symbol, id).cloned()
    }) else {
        return menu;
    };
    let command = |command: DrawingCommand| {
        let chart = chart.clone();
        move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
            chart.update(cx, |chart, cx| chart.drawing_command(id, command, cx));
        }
    };
    let settings = chart.clone();
    menu = menu.label(drawing.title()).item(
        PopupMenuItem::new("Settings...")
            .icon(IconName::Settings2)
            .on_click(move |_, window, cx| {
                super::glue::open_drawing_settings(&settings, id, window, cx);
            }),
    );
    if drawing.tool.is_position() {
        let trade = chart.clone();
        menu = menu.item(
            PopupMenuItem::new(
                if drawing.tool == super::drawing::model::Tool::LongPosition {
                    "Buy with these levels..."
                } else {
                    "Sell with these levels..."
                },
            )
            .icon(IconName::ArrowLeftRight)
            .on_click(move |_, _, cx| trade.update(cx, |chart, cx| chart.trade_drawing(id, cx))),
        );
    }
    menu.item(
        PopupMenuItem::new("Duplicate")
            .icon(IconName::Copy)
            .on_click(command(DrawingCommand::Duplicate)),
    )
    .item(
        PopupMenuItem::new("Bring to front")
            .icon(IconName::BringToFront)
            .on_click(command(DrawingCommand::Order(Order::Front))),
    )
    .item(
        PopupMenuItem::new("Send to back")
            .icon(IconName::SendToBack)
            .on_click(command(DrawingCommand::Order(Order::Back))),
    )
    .item(
        PopupMenuItem::new("Hide")
            .icon(IconName::EyeOff)
            .on_click(command(DrawingCommand::Hide)),
    )
    .item(
        PopupMenuItem::new(if drawing.locked { "Unlock" } else { "Lock" })
            .icon(if drawing.locked {
                IconName::LockOpen
            } else {
                IconName::Lock
            })
            .on_click(command(DrawingCommand::Lock(!drawing.locked))),
    )
    .item(
        PopupMenuItem::new("Delete")
            .icon(IconName::Trash)
            .disabled(drawing.locked)
            .on_click(command(DrawingCommand::Delete)),
    )
    .separator()
}

/// The right-click menu, for the price under the pointer.
fn context_menu(
    chart: &Entity<Chart>,
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let (price_raw, bid, ask, digits, has_symbol, under) = {
        let this = chart.read(cx);
        let origin = this.bounds.get().map(|b| b.origin);
        let (x, y) = match origin {
            Some(origin) => {
                let at = window.mouse_position() - origin;
                (f32::from(at.x), f32::from(at.y))
            }
            None => this.context_at.unwrap_or((0.0, 0.0)),
        };
        (
            this.price_at(y),
            this.bid,
            this.ask,
            this.digits(),
            this.symbol.is_some(),
            this.drawing_under(x, y, cx),
        )
    };
    let mut menu = menu.min_w(px(230.));
    if let Some(id) = under {
        menu = drawing_menu(chart, id, menu, cx);
    }
    if let (Some(raw), true) = (price_raw, has_symbol) {
        let text = format_price(raw.round() as i64, digits);
        let real = to_real(raw);
        let buy_limit = ask.is_some_and(|ask| raw < ask as f64);
        let sell_limit = bid.is_some_and(|bid| raw > bid as f64);
        let emit = |action: ChartAction| {
            let chart = chart.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                chart.update(cx, |_, cx| cx.emit(ChartEvent::Action(action.clone())));
            }
        };
        menu = menu
            .item(
                PopupMenuItem::new(format!(
                    "Buy {} {text}",
                    if buy_limit { "limit" } else { "stop" }
                ))
                .icon(IconName::ArrowBigUp)
                .on_click(emit(ChartAction::Ticket {
                    buy: true,
                    entry: Some(real),
                    stop_loss: None,
                    take_profit: None,
                })),
            )
            .item(
                PopupMenuItem::new(format!(
                    "Sell {} {text}",
                    if sell_limit { "limit" } else { "stop" }
                ))
                .icon(IconName::ArrowBigDown)
                .on_click(emit(ChartAction::Ticket {
                    buy: false,
                    entry: Some(real),
                    stop_loss: None,
                    take_profit: None,
                })),
            )
            .item(
                PopupMenuItem::new("New order...")
                    .icon(IconName::Plus)
                    .on_click(emit(ChartAction::Ticket {
                        buy: true,
                        entry: None,
                        stop_loss: None,
                        take_profit: None,
                    })),
            )
            .separator()
            .item(
                PopupMenuItem::new(format!("Add alert at {text}"))
                    .icon(IconName::BellPlus)
                    .on_click(emit(ChartAction::AddAlert(real))),
            )
            .separator();
    }
    let on = |f: fn(&mut Chart, &mut Window, &mut Context<Chart>)| {
        let chart = chart.clone();
        move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut gpui::App| {
            chart.update(cx, |this, cx| f(this, window, cx));
        }
    };
    menu.item(
        PopupMenuItem::new("Indicators...")
            .icon(IconName::ChartSpline)
            .on_click(on(|this, window, cx| {
                study_ui::open_picker(cx.entity(), this, window, cx)
            })),
    )
    .item(
        PopupMenuItem::new("Chart settings...")
            .icon(IconName::Settings2)
            .on_click(on(|_, window, cx| {
                study_ui::open_chart_settings(cx.entity(), window, cx)
            })),
    )
    .separator()
    .item(
        PopupMenuItem::new("Reset chart view")
            .icon(IconName::RotateCcw)
            .on_click(on(|this, _, cx| this.jump_to_latest(cx))),
    )
    .item(
        PopupMenuItem::new("Take a picture of the chart")
            .icon(IconName::Camera)
            .on_click(on(|_, _, cx| cx.emit(ChartEvent::Screenshot))),
    )
}

impl Chart {
    /// The number under the pointer (or the newest) of each plot of an indicator, with its color.
    fn study_values(&self, study: usize, index: usize) -> Vec<(String, u32)> {
        let config = &self.settings.studies[study];
        let Some(Some(output)) = self.display.studies.get(study) else {
            return Vec::new();
        };
        let format = match config.spec().placement {
            Placement::Overlay => ValueFormat::Price,
            Placement::Pane => config.spec().format,
        };
        output
            .plots
            .iter()
            .filter(|plot| config.plot_style(plot.key).visible)
            .map(|plot| {
                let at = index as i64 - plot.offset;
                let value = usize::try_from(at)
                    .ok()
                    .and_then(|i| plot.values.get(i).copied())
                    .unwrap_or(f64::NAN);
                let color = match (&plot.up, plot.kind) {
                    (Some(up), PlotKind::Histogram) => {
                        let rising = usize::try_from(at)
                            .ok()
                            .and_then(|i| up.get(i).copied())
                            .unwrap_or(true);
                        if rising {
                            super::study::UP_COLOR
                        } else {
                            super::study::DOWN_COLOR
                        }
                    }
                    _ => config.plot_style(plot.key).color,
                };
                (price::format_value(value, format, self.digits()), color)
            })
            .filter(|(text, _)| !text.is_empty())
            .collect()
    }

    /// One indicator's line in a legend: its title, its values, and the buttons that appear
    /// when the pointer is over it.
    fn study_row(&self, study: usize, index: Option<usize>, cx: &mut Context<Self>) -> AnyElement {
        let config = &self.settings.studies[study];
        let visible = config.visible;
        let group = SharedString::from(format!("study-row-{study}"));
        let values = index
            .map(|i| self.study_values(study, i))
            .unwrap_or_default();
        let button = |id: &str, icon: IconName, tooltip: &'static str| {
            div()
                .id(SharedString::from(format!("{id}-{study}")))
                .flex()
                .items_center()
                .justify_center()
                .size(px(20.))
                .rounded_sm()
                .cursor_pointer()
                .hover(|s| s.bg(theme::surface_hover()))
                // The press is the button's, not the chart's under it.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(ui::icon_colored(icon, 13., theme::muted_fg()))
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip).build(window, cx)
                })
        };
        div()
            .id(group.clone())
            .group(group.clone())
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(20.))
            .pl_1()
            .rounded_sm()
            .text_size(px(12.))
            .hover(|s| s.bg(gpui::rgba(0x0a0a0acc)))
            .child(
                div()
                    .text_color(if visible {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    })
                    .when(!visible, |el| el.opacity(0.6))
                    .child(config.title()),
            )
            .children(visible.then(|| {
                div().flex().flex_row().gap_2().children(
                    values
                        .into_iter()
                        .map(|(text, color)| div().text_color(rgb(color)).child(text)),
                )
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_0p5()
                    .opacity(0.0)
                    .group_hover(group, |s| s.opacity(1.0))
                    .child(
                        button(
                            "study-eye",
                            if visible {
                                IconName::Eye
                            } else {
                                IconName::EyeOff
                            },
                            "Show or hide",
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_settings(cx, |s| {
                                if let Some(study) = s.studies.get_mut(study) {
                                    study.visible = !study.visible;
                                }
                            });
                        })),
                    )
                    .child(
                        button("study-settings", IconName::Settings2, "Settings").on_click(
                            cx.listener(move |_this, _, window, cx| {
                                study_ui::open_study_settings(cx.entity(), study, window, cx);
                            }),
                        ),
                    )
                    .child(
                        button("study-remove", IconName::X, "Remove").on_click(
                            cx.listener(move |this, _, _, cx| this.remove_study(study, cx)),
                        ),
                    ),
            )
            .into_any_element()
    }

    /// The legend of the prices (symbol, prices, overlays) and of each indicator pane.
    fn legends(
        &self,
        geometry: &Geometry,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut out = Vec::new();
        let index = self.shown_index();
        // The legend wraps before the toolbar at the top right (or the price axis, when the
        // toolbar is hidden).
        let right = AXIS_W + if compact { 8.0 } else { 196.0 };
        let mut main = div()
            .absolute()
            .top(px(6.))
            .left(px(8.))
            .right(px(right))
            .flex()
            .flex_col()
            .gap_0p5()
            .child(self.headline(compact, cx));
        if !compact && self.settings.trade_buttons {
            main = main.children(self.trade_buttons(cx));
        }
        if !compact {
            for (study, config) in self.settings.studies.iter().enumerate() {
                if config.spec().placement == Placement::Overlay {
                    main = main.child(self.study_row(study, index, cx));
                }
            }
        }
        out.push(main.into_any_element());
        for (i, pane) in self.settings.panes().iter().enumerate() {
            let Some(band) = geometry.bands.get(i + 1) else {
                continue;
            };
            out.push(
                div()
                    .absolute()
                    .top(px(band.top as f32 + 3.0))
                    .left(px(8.))
                    .child(self.study_row(pane.study, index, cx))
                    .into_any_element(),
            );
        }
        out
    }

    /// Where the legend ends, from the top of the chart: what floats over the chart (the bar of a
    /// selected drawing) goes under it.
    pub fn legend_bottom(&self) -> f32 {
        let compact = self.is_compact();
        let mut bottom = 6.0 + 22.0;
        if !compact && self.settings.trade_buttons && self.bid.is_some() && self.ask.is_some() {
            bottom += 50.0;
        }
        if !compact {
            let overlays = self
                .settings
                .studies
                .iter()
                .filter(|s| s.spec().placement == Placement::Overlay)
                .count();
            bottom += overlays as f32 * 22.0;
        }
        bottom + 8.0
    }

    /// Sell at the bid and buy at the ask, at market, with the spread between them.
    fn trade_buttons(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (bid, ask) = (self.bid?, self.ask?);
        let digits = self.digits();
        let pip = 10f64.powi(i32::try_from(digits).unwrap_or(5).saturating_sub(1).max(0));
        let spread = (ask - bid) as f64 / wyck::openapi::market::PRICE_SCALE as f64 * pip;
        let button = |id: &'static str, label: &'static str, price: i64, color: u32, buy: bool| {
            div()
                .id(id)
                .flex()
                .flex_col()
                .min_w(px(84.))
                .px_2()
                .py_0p5()
                .rounded_md()
                .border_1()
                .border_color(rgb(color))
                .bg(theme::bg())
                .cursor_pointer()
                .hover(move |s| s.bg(gpui::rgba(color << 8 | 0x33)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |_this, _, _, cx| {
                    cx.emit(ChartEvent::Action(ChartAction::Market { buy }));
                }))
                .child(
                    div()
                        .text_size(px(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(color))
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(theme::fg())
                        .child(format_price(price, digits)),
                )
        };
        Some(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .pt_0p5()
                .child(button("chart-sell", "SELL", bid, 0xef5350, false))
                .child(
                    div()
                        .min_w(px(28.))
                        .text_center()
                        .text_size(px(10.))
                        .text_color(theme::muted_fg())
                        .child(format!("{spread:.1}")),
                )
                .child(button("chart-buy", "BUY", ask, 0x26a69a, true))
                .into_any_element(),
        )
    }

    /// The first line of the legend: the symbol (a button that changes it), the timeframe and
    /// type, and the prices of the point under the pointer.
    fn headline(&self, compact: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let digits = self.digits();
        let name = self
            .symbol
            .as_ref()
            .map_or_else(|| SharedString::from("Symbol"), |s| s.name.clone());
        let symbol = div()
            .id("chart-symbol")
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .px_1()
            .rounded_sm()
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface_hover()))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|_this, _, _, cx| cx.emit(ChartEvent::PickSymbol)))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::fg())
                    .child(name),
            )
            .child(ui::icon_colored(
                IconName::ChevronDown,
                12.,
                theme::muted_fg(),
            ));
        let mut kind_text = self.timeframe.label();
        if self.settings.kind != ChartKind::Candles {
            kind_text.push_str(" \u{b7} ");
            kind_text.push_str(self.settings.kind.label());
        }
        if self.display.box_size > 0 && self.settings.kind != ChartKind::LineBreak {
            kind_text.push_str(&format!(" {}", format_price(self.display.box_size, digits)));
        }
        let mut row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_x_2()
            .text_size(px(12.))
            .child(symbol)
            .child(div().text_color(theme::muted_fg()).child(kind_text));
        let Some(index) = self.shown_index() else {
            return row;
        };
        let value = |label: &'static str, price: i64, tone: gpui::Rgba| {
            div()
                .flex()
                .flex_row()
                .gap_0p5()
                .child(div().text_color(theme::muted_fg()).child(label))
                .child(div().text_color(tone).child(format_price(price, digits)))
        };
        match self.shown() {
            Series::Bars(bars) => {
                if let Some(bar) = bars.get(index) {
                    let tone = if bar.close >= bar.open {
                        theme::chart_up()
                    } else {
                        theme::chart_down()
                    };
                    let previous = index
                        .checked_sub(1)
                        .and_then(|i| bars.get(i))
                        .map_or(bar.open, |b| b.close);
                    let change = bar.close - previous;
                    let pct = if previous != 0 {
                        change as f64 / previous as f64 * 100.0
                    } else {
                        0.0
                    };
                    let sign = if change >= 0 { "+" } else { "" };
                    let change_text =
                        format!("{sign}{} ({sign}{pct:.2}%)", format_price(change, digits));
                    if compact {
                        row = row
                            .child(value("C", bar.close, tone))
                            .child(div().text_color(tone).child(format!("{sign}{pct:.2}%")));
                    } else {
                        row = row
                            .child(value("O", bar.open, tone))
                            .child(value("H", bar.high, tone))
                            .child(value("L", bar.low, tone))
                            .child(value("C", bar.close, tone))
                            .child(div().text_color(tone).child(change_text))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_0p5()
                                    .child(div().text_color(theme::muted_fg()).child("Vol"))
                                    .child(
                                        div()
                                            .text_color(theme::fg())
                                            .child(price::count(bar.volume as f64)),
                                    ),
                            );
                    }
                }
            }
            Series::Ticks(ticks) => {
                if let Some(tick) = ticks.get(index) {
                    row = row.child(value("Bid", tick.price, theme::fg()));
                }
            }
        }
        row
    }

    /// The labels of the order, position and alert lines, over the right of the plot.
    fn line_labels(&self, geometry: &Geometry, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(map) = self.main_map() else {
            return Vec::new();
        };
        let main = geometry.main();
        let mut out = Vec::new();
        for line in &self.lines {
            let raw = self.line_price(line.id, line.raw_price());
            let y = map.y(raw) as f32;
            if !(main.top as f32 + 2.0..=main.bottom() as f32 - 2.0).contains(&y) {
                continue;
            }
            let id = line.id;
            let color = rgb(line.color);
            let pill = div()
                .id(SharedString::from(format!("line-{id:?}")))
                .absolute()
                .top(px(y - 10.0))
                .right(px(AXIS_W + 8.0))
                .h(px(20.))
                .flex()
                .flex_row()
                .items_center()
                .rounded_sm()
                .overflow_hidden()
                .border_1()
                .border_color(color)
                .bg(theme::bg())
                .text_size(px(11.))
                .occlude()
                .child(
                    div()
                        .h_full()
                        .px_1p5()
                        .flex()
                        .items_center()
                        .bg(color)
                        .text_color(theme::bg())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(line.label.clone()),
                )
                .children(line.detail.clone().map(|(text, detail_color)| {
                    div().px_1p5().text_color(rgb(detail_color)).child(text)
                }))
                .when(line.closable, |el| {
                    el.child(
                        div()
                            .id(SharedString::from(format!("line-close-{id:?}")))
                            .h_full()
                            .px_1()
                            .flex()
                            .items_center()
                            .border_l_1()
                            .border_color(color)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::surface_hover()))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |_this, _, _, cx| {
                                cx.emit(ChartEvent::LineClosed(id));
                            }))
                            .child(ui::icon_colored(IconName::X, 11., theme::fg())),
                    )
                });
            out.push(pill.into_any_element());
        }
        out
    }

    /// The buttons at the top right: chart type, indicators, price scale, settings, picture,
    /// and the ones that bring the view back.
    fn toolbar(&self, latest: bool, compact: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let auto = matches!(self.view.price, PriceScale::Auto);
        let open = self.menu;
        let menu_button = |id: &'static str, icon: IconName, tip: &'static str, menu: Menu| {
            Button::new(id)
                .ghost()
                .compact()
                .icon(icon)
                .tooltip(tip)
                .toggled(open == Some(menu))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.menu = if this.menu == Some(menu) {
                        None
                    } else {
                        Some(menu)
                    };
                    cx.notify();
                }))
        };
        let scale_label = match (self.settings.scale, auto) {
            (ScaleMode::Linear, true) => "Auto",
            (ScaleMode::Linear, false) => "Manual",
            (ScaleMode::Log, _) => "Log",
            (ScaleMode::Percent, _) => "%",
            (ScaleMode::Indexed, _) => "100",
        };
        div()
            .absolute()
            .top(px(6.))
            .right(px(AXIS_W + 8.0))
            .flex()
            .flex_row()
            .items_center()
            .gap_0p5()
            .p_0p5()
            .rounded_lg()
            .bg(gpui::rgba(0x0a0a0acc))
            .border_1()
            .border_color(theme::border_hairline())
            .occlude()
            .child(menu_button(
                "chart-kind",
                kind_icon(self.settings.kind),
                "Chart type",
                Menu::Kind,
            ))
            .when(!compact, |el| {
                el.child(
                    Button::new("chart-studies")
                        .ghost()
                        .compact()
                        .icon(IconName::ChartSpline)
                        .tooltip("Indicators")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _event, window, cx| {
                            study_ui::open_picker(cx.entity(), this, window, cx);
                        })),
                )
                .child(
                    div()
                        .id("chart-scale")
                        .h(px(26.))
                        .px_2()
                        .flex()
                        .items_center()
                        .rounded_md()
                        .cursor_pointer()
                        .text_size(px(11.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if auto && self.settings.scale == ScaleMode::Linear {
                            theme::muted_fg()
                        } else {
                            theme::accent()
                        })
                        .when(open == Some(Menu::Scale), |el| {
                            el.bg(theme::accent_selected())
                        })
                        .hover(|s| s.bg(theme::surface_hover()))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.menu = if this.menu == Some(Menu::Scale) {
                                None
                            } else {
                                Some(Menu::Scale)
                            };
                            cx.notify();
                        }))
                        .child(scale_label),
                )
                .child(
                    Button::new("chart-settings")
                        .ghost()
                        .compact()
                        .icon(IconName::Settings2)
                        .tooltip("Chart settings")
                        .cursor_pointer()
                        .on_click(cx.listener(|_this, _event, window, cx| {
                            study_ui::open_chart_settings(cx.entity(), window, cx);
                        })),
                )
                .child(
                    Button::new("chart-picture")
                        .ghost()
                        .compact()
                        .icon(IconName::Camera)
                        .tooltip("Take a picture (Ctrl+Shift+S)")
                        .cursor_pointer()
                        .on_click(cx.listener(|_this, _event, _window, cx| {
                            cx.emit(ChartEvent::Screenshot);
                        })),
                )
            })
            .when(!auto, |el| {
                el.child(
                    Button::new("chart-auto-scale")
                        .ghost()
                        .compact()
                        .icon(IconName::Scaling)
                        .tooltip("Fit the prices (double click the price axis)")
                        .cursor_pointer()
                        .on_click(
                            cx.listener(|this, _event, _window, cx| this.reset_price_scale(cx)),
                        ),
                )
            })
            .when(latest, |el| {
                el.child(
                    Button::new("chart-latest")
                        .ghost()
                        .compact()
                        .icon(IconName::ChevronsRight)
                        .tooltip("Back to the latest price (End)")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _event, _window, cx| this.jump_to_latest(cx))),
                )
            })
    }

    fn menu_card(&self) -> gpui::Div {
        div()
            .p_1()
            .flex()
            .flex_col()
            .rounded_lg()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border_subtle())
            .shadow_lg()
            .occlude()
    }

    fn menu_row(
        &self,
        id: SharedString,
        selected: bool,
        icon: Option<IconName>,
        label: impl Into<SharedString>,
    ) -> gpui::Stateful<gpui::Div> {
        let tone = if selected {
            theme::fg()
        } else {
            theme::muted_fg()
        };
        div()
            .id(id)
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(28.))
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(tone)
            .when(selected, |el| el.bg(theme::accent_selected()))
            .hover(|style| style.bg(theme::surface_hover()).text_color(theme::fg()))
            .children(icon.map(|icon| ui::icon_colored(icon, 15., tone)))
            .child(div().flex_1().child(label.into()))
            .children(selected.then(|| ui::icon_colored(IconName::Check, 13., theme::accent())))
    }

    fn section_title(title: &'static str) -> gpui::Div {
        div()
            .px_2()
            .pt_1p5()
            .pb_0p5()
            .text_size(px(10.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::muted_fg())
            .child(title.to_uppercase())
    }

    /// The open menu of the toolbar or the corner.
    fn menu_popup(&self, geometry: &Geometry, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.menu?;
        let card = match menu {
            Menu::Kind => {
                let mut card = self.menu_card().id("chart-kind-menu").w(px(210.));
                for (title, kinds) in KIND_SECTIONS {
                    card = card.child(Self::section_title(title));
                    for kind in kinds.iter().copied() {
                        card = card.child(
                            self.menu_row(
                                SharedString::from(format!("chart-kind-{}", kind.code())),
                                self.settings.kind == kind,
                                Some(kind_icon(kind)),
                                kind.label(),
                            )
                            .on_click(cx.listener(
                                move |this, _event, _window, cx| {
                                    this.menu = None;
                                    this.set_kind(kind, cx);
                                },
                            )),
                        );
                    }
                }
                card.into_any_element()
            }
            Menu::Scale => {
                let mut card = self.menu_card().id("chart-scale-menu").w(px(210.));
                card = card.child(Self::section_title("Price scale"));
                for mode in ScaleMode::ALL {
                    card = card.child(
                        self.menu_row(
                            SharedString::from(format!("chart-scale-{mode:?}")),
                            self.settings.scale == mode,
                            None,
                            mode.label(),
                        )
                        .on_click(cx.listener(
                            move |this, _event, _window, cx| {
                                this.menu = None;
                                this.edit_settings(cx, |s| s.scale = mode);
                            },
                        )),
                    );
                }
                card = card
                    .child(div().my_1().h(px(1.)).bg(theme::border_hairline()))
                    .child(
                        self.menu_row(
                            "chart-scale-invert".into(),
                            self.settings.invert,
                            Some(IconName::ArrowUpDown),
                            "Invert the scale",
                        )
                        .on_click(cx.listener(
                            |this, _event, _window, cx| {
                                this.edit_settings(cx, |s| s.invert = !s.invert);
                            },
                        )),
                    )
                    .child(
                        self.menu_row(
                            "chart-scale-auto".into(),
                            matches!(self.view.price, PriceScale::Auto),
                            Some(IconName::Scaling),
                            "Fit the prices (Alt+R)",
                        )
                        .on_click(cx.listener(
                            |this, _event, _window, cx| {
                                this.menu = None;
                                this.reset_price_scale(cx);
                            },
                        )),
                    );
                card.into_any_element()
            }
            Menu::Zone => {
                let now = super::now_ms();
                let mut list = div().flex().flex_col();
                for zone in Zone::menu() {
                    list = list.child(
                        self.menu_row(
                            SharedString::from(format!("chart-zone-{}", zone.code())),
                            self.settings.zone == zone,
                            None,
                            zone.name(now),
                        )
                        .on_click(cx.listener(
                            move |this, _event, _window, cx| {
                                this.menu = None;
                                this.edit_settings(cx, |s| s.zone = zone);
                            },
                        )),
                    );
                }
                self.menu_card()
                    .id("chart-zone-menu")
                    .w(px(230.))
                    .max_h(px((geometry.h as f32 - 60.0).max(160.0)))
                    .overflow_y_scroll()
                    .child(Self::section_title("Time zone"))
                    .child(list)
                    .into_any_element()
            }
        };
        let positioned = match menu {
            // The zone opens from the corner at the bottom right.
            Menu::Zone => div()
                .absolute()
                .right(px(4.))
                .bottom(px(AXIS_H + 4.0))
                .child(card),
            _ => div()
                .absolute()
                .top(px(40.))
                .right(px(AXIS_W + 8.0))
                .child(card),
        };
        Some(
            deferred(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.menu = None;
                            cx.notify();
                        }),
                    )
                    .child(positioned.on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// Loading, failed and empty states, and the small note while older history comes in.
    fn status(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let centered = || {
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .px_6()
        };
        let mut out: Vec<AnyElement> = Vec::new();
        match &self.load {
            Load::Idle => out.push(
                centered()
                    .child(ui::icon_colored(
                        IconName::ChartCandlestick,
                        28.,
                        theme::muted_fg(),
                    ))
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(theme::muted_fg())
                            .child("Pick a symbol to see its chart."),
                    )
                    .into_any_element(),
            ),
            Load::Loading => out.push(
                centered()
                    .child(anim::spin(
                        ui::icon_colored(IconName::LoaderCircle, 22., theme::muted_fg()),
                        ("chart-loading", self.id),
                    ))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(theme::muted_fg())
                            .child("Loading the chart..."),
                    )
                    .into_any_element(),
            ),
            Load::Failed(message) => out.push(
                centered()
                    .child(ui::icon_colored(
                        IconName::CircleAlert,
                        26.,
                        theme::destructive(),
                    ))
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(theme::fg())
                            .child("Could not load the chart"),
                    )
                    .child(
                        div()
                            .max_w(px(440.))
                            .text_center()
                            .text_size(px(12.))
                            .text_color(theme::muted_fg())
                            .child(message.clone()),
                    )
                    .child(div().pt_1().w(px(200.)).child(ui::primary_button(
                        ("chart-retry", self.id),
                        "Try again",
                        cx.listener(|this, _event, _window, cx| this.retry(cx)),
                    )))
                    .into_any_element(),
            ),
            Load::Ready if self.series.is_empty() => out.push(
                centered()
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(theme::muted_fg())
                            .child("No prices yet for this timeframe. New ones will show up here."),
                    )
                    .into_any_element(),
            ),
            Load::Ready => {}
        }
        if matches!(self.older, Older::Loading) {
            out.push(
                div()
                    .absolute()
                    .left(px(12.))
                    .bottom(px(AXIS_H + 8.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_size(px(11.))
                    .text_color(theme::muted_fg())
                    .child(anim::spin(
                        ui::icon_colored(IconName::LoaderCircle, 12., theme::muted_fg()),
                        ("chart-older", self.id),
                    ))
                    .child("Loading older history")
                    .into_any_element(),
            );
        }
        out
    }
}
