//! What sits over the canvas: the legend of each band (the symbol, the prices under the pointer,
//! the indicators with their buttons), the toolbar, its menus, the labels of the order and alert
//! lines, the right-click menu, and the loading, failed and empty states.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, FontWeight, MouseButton, SharedString, Window, deferred, div,
    px,
};
use gpui_kit::assets::IconName;
use wyck::openapi::market::format_price;

use super::data::Series;
use super::drawing::book::Order;
use super::lines::to_real;
use super::scene::{AXIS_H, AXIS_W, Geometry, price};
use super::settings::ChartKind;
use super::study::{Placement, PlotKind, ValueFormat};
use super::zone::Zone;
use super::{
    Chart, ChartAction, ChartEvent, DrawingCommand, EditorRequest, Load, Menu, Older,
    chart_settings_ui, export_ui, indicator_picker, paint, study_settings,
};
use crate::app::connection::ui;
use crate::app::menu::{self as popup, Entry, Item};
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
        ChartKind::Footprint => IconName::Rows3,
        ChartKind::VolumeCandles => IconName::ChartCandlestick,
        ChartKind::VolumeBars => IconName::ChartBarBig,
        ChartKind::Tpo => IconName::ChartNoAxesGantt,
    }
}

/// The kinds of the menu, in sections.
pub(super) const KIND_SECTIONS: [(&str, &[ChartKind]); 4] = [
    (
        "Bars",
        &[
            ChartKind::Candles,
            ChartKind::Hollow,
            ChartKind::VolumeCandles,
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
            ChartKind::VolumeBars,
        ],
    ),
    (
        "Order flow and profiles",
        &[ChartKind::Footprint, ChartKind::Tpo],
    ),
];

fn rgb(color: u32) -> gpui::Rgba {
    gpui::rgb(color)
}

impl Render for Chart {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.open_pending_settings(window, cx);
        self.schedule_customs(cx);
        let entity = cx.entity();
        let compact = self.is_compact();
        let latest = !self.view.is_following() && !self.shown().is_empty();
        let geometry = self.geometry();
        let menu = popup::Menu::new(("chart-context-menu", self.id), window, cx);
        let context = if menu.is_open(cx) {
            self.context_items(&entity, menu.position(cx), cx)
        } else {
            Vec::new()
        };
        let opener = menu.clone();

        div()
            .id(("chart", self.id))
            .relative()
            .flex_1()
            .w_full()
            .min_h_0()
            .overflow_hidden()
            .bg(self.palette().bg)
            .child(paint::surface(&entity, self.bounds.clone()))
            .children(self.legends(&geometry, compact, cx))
            .children(self.line_labels(&geometry, cx))
            // The toolbar belongs to the chart being worked on, as long as it has the room.
            .children(
                self.shows_toolbar()
                    .then(|| self.toolbar(latest, compact, window, cx)),
            )
            .children(self.menu_popup(&geometry, cx))
            .children(self.drawing_hint(&geometry, cx))
            .children(self.status(cx))
            .on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                opener.open(Some(event.position), cx);
            })
            .children(menu.popup(context, popup::Placement::Cursor, window, cx))
    }
}

impl Chart {
    /// The items for the drawing under the pointer, at the top of the right-click menu.
    fn drawing_items(&self, chart: &Entity<Chart>, id: u64, cx: &App) -> Vec<Item> {
        let Some(drawing) = self.drawings.as_ref().and_then(|drawings| {
            let symbol = self.symbol_name()?;
            drawings.read(cx).book().get(&symbol, id).cloned()
        }) else {
            return Vec::new();
        };
        let command = |command: DrawingCommand| {
            let chart = chart.clone();
            move |_: &mut Window, cx: &mut App| {
                chart.update(cx, |chart, cx| chart.drawing_command(id, command, cx));
            }
        };
        let long = drawing.tool == super::drawing::model::Tool::LongPosition;
        let settings = chart.clone();
        let mut items = vec![
            Item::Title(drawing.title().into()),
            Entry::new("Settings...")
                .icon(IconName::Settings2)
                .on_click(move |window, cx| {
                    super::glue::open_drawing_settings(&settings, id, window, cx);
                })
                .into(),
        ];
        if drawing.tool.is_position() {
            let trade = chart.clone();
            items.push(
                Entry::new(if long {
                    "Buy with these levels..."
                } else {
                    "Sell with these levels..."
                })
                .icon(IconName::ArrowLeftRight)
                .on_click(move |_, cx| trade.update(cx, |chart, cx| chart.trade_drawing(id, cx)))
                .into(),
            );
            items.push(
                Entry::new(if long {
                    "Flip to a short position"
                } else {
                    "Flip to a long position"
                })
                .icon(IconName::ArrowUpDown)
                .disabled(drawing.locked)
                .on_click(command(DrawingCommand::Flip))
                .into(),
            );
        }
        items.extend([
            Entry::new("Duplicate")
                .icon(IconName::Copy)
                .on_click(command(DrawingCommand::Duplicate))
                .into(),
            Entry::new("Bring to front")
                .icon(IconName::BringToFront)
                .on_click(command(DrawingCommand::Order(Order::Front)))
                .into(),
            Entry::new("Send to back")
                .icon(IconName::SendToBack)
                .on_click(command(DrawingCommand::Order(Order::Back)))
                .into(),
            Entry::new(if drawing.hidden { "Show" } else { "Hide" })
                .icon(if drawing.hidden {
                    IconName::Eye
                } else {
                    IconName::EyeOff
                })
                .on_click(command(DrawingCommand::Hidden(!drawing.hidden)))
                .into(),
            Entry::new(if drawing.locked { "Unlock" } else { "Lock" })
                .icon(if drawing.locked {
                    IconName::LockOpen
                } else {
                    IconName::Lock
                })
                .on_click(command(DrawingCommand::Lock(!drawing.locked)))
                .into(),
            Entry::new("Delete")
                .icon(IconName::Trash)
                .disabled(drawing.locked)
                .danger()
                .on_click(command(DrawingCommand::Delete))
                .into(),
            Item::Separator,
        ]);
        items
    }

    /// The right-click menu, for the price under the pointer. `at` is where the pointer was, in
    /// window coordinates.
    fn context_items(
        &self,
        chart: &Entity<Chart>,
        at: Option<gpui::Point<gpui::Pixels>>,
        cx: &App,
    ) -> Vec<Item> {
        let origin = self.bounds.get().map(|b| b.origin);
        let (x, y) = match (at, origin) {
            (Some(at), Some(origin)) => {
                let at = at - origin;
                (f32::from(at.x), f32::from(at.y))
            }
            _ => self.context_at.unwrap_or((0.0, 0.0)),
        };
        let (price_raw, bid, ask, digits) = (self.price_at(y), self.bid, self.ask, self.digits());
        let mut items = Vec::new();
        if let Some(id) = self.drawing_under(x, y, cx) {
            items = self.drawing_items(chart, id, cx);
        }
        if let (Some(raw), true) = (price_raw, self.symbol.is_some()) {
            let text = format_price(raw.round() as i64, digits);
            let real = to_real(raw);
            let buy_limit = ask.is_some_and(|ask| raw < ask as f64);
            let sell_limit = bid.is_some_and(|bid| raw > bid as f64);
            let emit = |action: ChartAction| {
                let chart = chart.clone();
                move |_: &mut Window, cx: &mut App| {
                    chart.update(cx, |_, cx| cx.emit(ChartEvent::Action(action.clone())));
                }
            };
            let ticket = |buy: bool, entry: Option<f64>| ChartAction::Ticket {
                buy,
                entry,
                stop_loss: None,
                take_profit: None,
            };
            items.extend([
                Entry::new(format!(
                    "Buy {} {text}",
                    if buy_limit { "limit" } else { "stop" }
                ))
                .icon(IconName::ArrowBigUp)
                .on_click(emit(ticket(true, Some(real))))
                .into(),
                Entry::new(format!(
                    "Sell {} {text}",
                    if sell_limit { "limit" } else { "stop" }
                ))
                .icon(IconName::ArrowBigDown)
                .on_click(emit(ticket(false, Some(real))))
                .into(),
                Entry::new("New order...")
                    .icon(IconName::Plus)
                    .on_click(emit(ticket(true, None)))
                    .into(),
                Item::Separator,
                Entry::new(format!("Add alert at {text}"))
                    .icon(IconName::BellPlus)
                    .hint("Alt+A")
                    .on_click(emit(ChartAction::AddAlert(real)))
                    .into(),
                Item::Separator,
            ]);
        }
        let on = |f: fn(&mut Chart, &mut Window, &mut Context<Chart>)| {
            let chart = chart.clone();
            move |window: &mut Window, cx: &mut App| {
                chart.update(cx, |this, cx| f(this, window, cx));
            }
        };
        items.extend([
            Entry::new("Indicators...")
                .icon(IconName::ChartSpline)
                .on_click(on(|_, window, cx| {
                    indicator_picker::open(cx.entity(), window, cx)
                }))
                .into(),
            Entry::new("Chart settings...")
                .icon(IconName::Settings2)
                .on_click(on(|_, window, cx| {
                    chart_settings_ui::open(cx.entity(), window, cx)
                }))
                .into(),
            Item::Separator,
            Entry::new("Reset chart view")
                .icon(IconName::RotateCcw)
                .hint("End")
                .on_click(on(|this, _, cx| this.jump_to_latest(cx)))
                .into(),
            Entry::new("Take a picture of the chart")
                .icon(IconName::Camera)
                .hint("Ctrl+Shift+S")
                .on_click(on(|_, _, cx| cx.emit(ChartEvent::Screenshot)))
                .into(),
            Entry::new("Export the data...")
                .icon(IconName::Download)
                .on_click(on(|_, window, cx| export_ui::open(cx.entity(), window, cx)))
                .into(),
        ]);
        items
    }
}

impl Chart {
    /// The background of a button of the legend while the pointer is over it. It is made from the
    /// text color of the chart, so it shows on a light chart under a dark theme too (the theme's
    /// own hover color would be a dark patch there).
    fn hover_color(&self) -> gpui::Rgba {
        gpui::Rgba {
            a: 0.12,
            ..self.palette().text_strong
        }
    }

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
            .filter(|_| self.settings.status.indicator_values)
            .map(|i| self.study_values(study, i))
            .unwrap_or_default();
        // An indicator written as a script: which one, and what is wrong with it if anything.
        let script = config.script.clone().filter(|_| config.is_script());
        let alert: Option<String> = script.as_ref().and_then(|id| {
            if !config.is_ready() {
                Some(
                    super::study::custom::library::registry::get(id)
                        .and_then(|e| e.problems.first().map(|p| p.message.clone()))
                        .unwrap_or_else(|| {
                            format!("The script \"{id}\" is not in the indicators folder")
                        }),
                )
            } else {
                self.custom_status(study)
                    .and_then(|s| s.first_error().map(|p| p.message.clone()))
            }
        });
        let hover = self.hover_color();
        let button = |id: &str, icon: IconName, tooltip: &'static str| {
            div()
                .id(SharedString::from(format!("{id}-{study}")))
                .flex()
                .items_center()
                .justify_center()
                .size(px(20.))
                .rounded_sm()
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                // The press is the button's, not the chart's under it.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(ui::icon_colored(icon, 13., theme::chart_muted()))
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
            .hover(move |s| s.bg(hover))
            .child(
                div()
                    .text_color(if alert.is_some() {
                        theme::destructive()
                    } else if visible {
                        theme::chart_fg()
                    } else {
                        theme::chart_muted()
                    })
                    .when(!visible, |el| el.opacity(0.6))
                    .child(config.title()),
            )
            .children(alert.map(|message| {
                div()
                    .id(SharedString::from(format!("study-alert-{study}")))
                    .child(ui::icon_colored(
                        IconName::TriangleAlert,
                        13.,
                        theme::destructive(),
                    ))
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(message.clone())
                            .build(window, cx)
                    })
            }))
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
                    .opacity(if visible { 0.0 } else { 1.0 })
                    .group_hover(group, |s| s.opacity(1.0))
                    .child(
                        button(
                            "study-eye",
                            if visible {
                                IconName::EyeOff
                            } else {
                                IconName::Eye
                            },
                            if visible { "Hide" } else { "Show" },
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_settings(cx, |s| {
                                if let Some(study) = s.studies.get_mut(study) {
                                    study.visible = !study.visible;
                                }
                            });
                        })),
                    )
                    .children(script.map(|id| {
                        button("study-code", IconName::CodeXml, "Edit the script").on_click(
                            cx.listener(move |_this, _, _, cx| {
                                cx.emit(ChartEvent::IndicatorEditor(EditorRequest::Edit(
                                    id.clone(),
                                )));
                            }),
                        )
                    }))
                    .child(
                        button("study-settings", IconName::Settings2, "Settings").on_click(
                            cx.listener(move |_this, _, window, cx| {
                                study_settings::open(cx.entity(), study, window, cx);
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
        let right = AXIS_W + if self.shows_toolbar() { 196.0 } else { 8.0 };
        let mut main = div()
            .absolute()
            .top(px(6.))
            .left(px(8.))
            .right(px(right))
            .flex()
            .flex_col()
            .gap_0p5()
            .child(self.headline(compact, cx));
        if !compact && self.settings.status.indicators {
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
            if !self.settings.status.indicators {
                continue;
            }
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

    /// What to do next with the drawing tool that is picked, at the bottom of the prices.
    fn drawing_hint(&self, geometry: &Geometry, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.selected {
            return None;
        }
        let drawings = self.drawings.as_ref()?;
        let symbol = self.symbol_name()?;
        let book = drawings.read(cx).book();
        let tool = book.tool()?;
        let text = match book.progress(&symbol) {
            Some((tool, _, _)) if tool.is_freehand() => {
                "Keep the button down and draw; let go to finish".to_owned()
            }
            Some((super::drawing::model::Tool::ArrowPath, placed, _)) => format!(
                "Click or drag to place point {}. Double click, Enter or Esc finishes. Backspace takes back the last point",
                placed + 1
            ),
            Some((_, placed, needed)) => format!(
                "Click or drag to place point {} of {needed}. Backspace takes back the last point. Esc or right click cancels",
                placed + 1
            ),
            None if tool.is_freehand() => "Press and draw on the chart".to_owned(),
            None if tool.anchors() == 1 || tool.is_position() => {
                "Click on the chart to place it. Esc goes back to the pointer".to_owned()
            }
            None => {
                "Click or press where it starts, then click or drag to each next point. Esc goes back to the pointer".to_owned()
            }
        };
        let bottom = (geometry.h - geometry.main().bottom()) as f32 + 10.0;
        Some(
            div()
                .absolute()
                .left_0()
                .right(px(AXIS_W))
                .bottom(px(bottom))
                .flex()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_1()
                        .rounded_full()
                        .bg(theme::surface_alpha(0.92))
                        .border_1()
                        .border_color(theme::border_subtle())
                        .text_size(px(11.))
                        .text_color(theme::muted_fg())
                        .child(
                            div()
                                .text_color(theme::fg())
                                .font_weight(FontWeight::MEDIUM)
                                .child(tool.label()),
                        )
                        .child(text),
                )
                .into_any_element(),
        )
    }

    /// Whether the toolbar shows: on the chart being worked on, when it has the room, and while
    /// one of its menus is open.
    fn shows_toolbar(&self) -> bool {
        let wide = self
            .bounds
            .get()
            .is_some_and(|b| f32::from(b.size.width) >= 360.0);
        (self.selected && wide) || self.menu.is_some()
    }

    /// A dot that says whether the symbol's market is open, with when it opens or closes.
    fn market_dot(&self) -> Option<AnyElement> {
        use wyck::openapi::market::MarketStatus;
        let status = self.market_status()?;
        let now = super::now_ms();
        let within = |at: Option<i64>| at.map(|t| super::drawing::geometry::duration_text(t - now));
        let (color, text) = match &status {
            MarketStatus::Open { closes_at } => (
                theme::emerald(),
                match within(*closes_at) {
                    Some(left) => format!("Market open, closes in {left}"),
                    None => "Market open around the clock".to_owned(),
                },
            ),
            MarketStatus::Closed { opens_at, holiday } => {
                let why = holiday
                    .as_ref()
                    .map(|name| format!(" for {name}"))
                    .unwrap_or_default();
                (
                    theme::chart_muted(),
                    match within(*opens_at) {
                        Some(left) => format!("Market closed{why}, opens in {left}"),
                        None => format!("Market closed{why}"),
                    },
                )
            }
            MarketStatus::CloseOnly => (
                theme::amber(),
                "Close only: positions can be closed, not opened".to_owned(),
            ),
            MarketStatus::Disabled => (
                theme::destructive(),
                "Trading on this symbol is disabled by the broker".to_owned(),
            ),
        };
        let hover = self.hover_color();
        let text = SharedString::from(text);
        Some(
            div()
                .id(("market-status", self.id))
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_default()
                .hover(move |s| s.bg(hover))
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
                })
                .child(
                    div()
                        .size(px(7.))
                        .rounded_full()
                        .bg(color)
                        .when(!status.is_open(), |el| {
                            el.bg(gpui::rgba(0x00000000)).border_1().border_color(color)
                        }),
                )
                .into_any_element(),
        )
    }

    /// Where the legend ends, from the top of the chart: what floats over the chart (the bar of a
    /// selected drawing) goes under it.
    pub fn legend_bottom(&self) -> f32 {
        let compact = self.is_compact();
        let mut bottom = 6.0 + 22.0;
        if !compact && self.settings.status.indicators {
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

    /// The volume, the delta and what each side traded in the bar that opens at `time_ms`, added to
    /// the legend of a footprint.
    fn flow_values(&self, time_ms: i64, row: gpui::Div) -> gpui::Div {
        let Some((sell, buy)) = self.flow.bar(time_ms).map(|b| b.totals()) else {
            return row;
        };
        let delta = buy as i64 - sell as i64;
        let tone = if delta >= 0 {
            theme::chart_up()
        } else {
            theme::chart_down()
        };
        let pair = |label: &'static str, text: String, tone: gpui::Rgba| {
            div()
                .flex()
                .flex_row()
                .gap_0p5()
                .child(div().text_color(theme::chart_muted()).child(label))
                .child(div().text_color(tone).child(text))
        };
        row.child(pair(
            "Vol",
            super::footprint::compact(sell + buy),
            theme::chart_fg(),
        ))
        .child(pair("Delta", super::footprint::signed(delta), tone))
        .child(pair(
            "Buy",
            super::footprint::compact(buy),
            theme::chart_up(),
        ))
        .child(pair(
            "Sell",
            super::footprint::compact(sell),
            theme::chart_down(),
        ))
    }

    /// The first line of the legend: the symbol (a button that changes it), the timeframe and
    /// type, and the prices of the point under the pointer.
    fn headline(&self, compact: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let digits = self.digits();
        let palette = self.palette();
        let hover = self.hover_color();
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
            .h(px(20.))
            .px_1()
            .rounded_sm()
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|_this, _, _, cx| cx.emit(ChartEvent::PickSymbol)))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.text_strong)
                    .child(name),
            )
            .child(ui::icon_colored(IconName::ChevronDown, 12., palette.text));
        let mut kind_text = self.timeframe.label();
        if self.settings.kind != ChartKind::Candles {
            kind_text.push_str(" \u{b7} ");
            kind_text.push_str(self.settings.kind.label());
        }
        if self.display.box_size > 0 && self.settings.kind != ChartKind::LineBreak {
            kind_text.push_str(&format!(" {}", format_price(self.display.box_size, digits)));
        }
        if self.settings.kind == ChartKind::Footprint && self.timeframe.bar_ms().is_some() {
            kind_text.push_str(if super::footprint::supports(self.timeframe) {
                " (tick volume)"
            } else {
                " (needs bars of a day or less)"
            });
        }
        let mut row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_x_2()
            .text_size(px(12.))
            .child(symbol)
            .child(div().text_color(palette.text).child(kind_text))
            .children(
                self.settings
                    .status
                    .market
                    .then(|| self.market_dot())
                    .flatten(),
            );
        if self.flow_busy() {
            row = row.child(
                div()
                    .text_color(palette.text)
                    .child("loading order flow..."),
            );
        }
        let Some(index) = self.shown_index().filter(|_| self.settings.status.prices) else {
            return row;
        };
        let value = |label: &'static str, price: i64, tone: gpui::Rgba| {
            div()
                .flex()
                .flex_row()
                .gap_0p5()
                .child(div().text_color(palette.text).child(label))
                .child(div().text_color(tone).child(format_price(price, digits)))
        };
        match self.shown() {
            Series::Bars(bars) => {
                if let Some(bar) = bars.get(index) {
                    let tone = if bar.close >= bar.open {
                        palette.up
                    } else {
                        palette.down
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
                            .children(
                                self.settings
                                    .status
                                    .change
                                    .then(|| div().text_color(tone).child(change_text)),
                            );
                    }
                    if self.settings.kind == ChartKind::Footprint {
                        row = self.flow_values(bar.time_ms, row);
                    }
                }
            }
            Series::Ticks(ticks) => {
                if let Some(tick) = ticks.get(index) {
                    row = row.child(value("Bid", tick.price, palette.text_strong));
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
        let (palette, hover) = (self.palette(), self.hover_color());
        let main = geometry.main();
        let mut out = Vec::new();
        for line in self
            .lines
            .iter()
            .filter(|line| self.settings.trading.shows(line.id))
        {
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
                .bg(palette.bg)
                .text_size(px(11.))
                .occlude()
                .child(
                    div()
                        .h_full()
                        .px_1p5()
                        .flex()
                        .items_center()
                        .bg(color)
                        .text_color(palette.bg)
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
                            .hover(move |s| s.bg(hover))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |_this, _, _, cx| {
                                cx.emit(ChartEvent::LineClosed(id));
                            }))
                            .child(ui::icon_colored(IconName::X, 11., palette.text_strong)),
                    )
                });
            out.push(pill.into_any_element());
        }
        out
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
        // The zone opens from the corner at the bottom right.
        let positioned = div()
            .absolute()
            .right(px(4.))
            .bottom(px(AXIS_H + 4.0))
            .child(card);
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
                        theme::chart_muted(),
                    ))
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(theme::chart_muted())
                            .child("Pick a symbol to see its chart."),
                    )
                    .into_any_element(),
            ),
            Load::Loading => out.push(
                centered()
                    .child(anim::spin(
                        ui::icon_colored(IconName::LoaderCircle, 22., theme::chart_muted()),
                        ("chart-loading", self.id),
                    ))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(theme::chart_muted())
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
                            .text_color(theme::chart_fg())
                            .child("Could not load the chart"),
                    )
                    .child(
                        div()
                            .max_w(px(440.))
                            .text_center()
                            .text_size(px(12.))
                            .text_color(theme::chart_muted())
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
                            .text_color(theme::chart_muted())
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
                    .text_color(theme::chart_muted())
                    .child(anim::spin(
                        ui::icon_colored(IconName::LoaderCircle, 12., theme::chart_muted()),
                        ("chart-older", self.id),
                    ))
                    .child("Loading older history")
                    .into_any_element(),
            );
        }
        out
    }
}
