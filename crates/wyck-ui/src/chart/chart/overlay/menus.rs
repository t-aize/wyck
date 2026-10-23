//! The right-click menu of the chart, and the menu of the toolbar.

use crate::chart::chart::events::{ChartAction, ChartEvent, TicketRequest};
use crate::chart::chart::lines::to_real;
use crate::chart::chart::{
    Chart, DrawingCommand, Menu, chart_settings_ui, export_ui, indicator_picker,
};
use crate::kit::icon::IconName;
use crate::kit::{
    menu::{self as popup, Entry, Item},
    tokens,
};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, MouseButton, SharedString, Window, deferred, div, px,
};
use wyck_app::lost::lost;
use wyck_domain::base::math::{f32_of, f64_to_i64, i64_to_f64};
use wyck_domain::chart::Zone;
use wyck_domain::chart::{Geometry, axis_h};
use wyck_domain::drawings::book::Order;
use wyck_domain::market::format_price;

impl Chart {
    /// The items for the drawing under the pointer, at the top of the right-click menu.
    pub(super) fn drawing_items(
        &self,
        chart: &Entity<Self>,
        id: u64,
        cx: &App,
    ) -> Vec<popup::Section> {
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
        let long = drawing.tool == wyck_domain::drawings::model::tool::Tool::LongPosition;
        let settings = chart.clone();
        let mut about = vec![
            Entry::new("Settings...")
                .icon(IconName::Settings2)
                .on_click(move |window, cx| {
                    crate::chart::chart::glue::open_drawing_settings(&settings, id, window, cx);
                })
                .into(),
        ];
        if drawing.tool.is_position() {
            let trade = chart.clone();
            about.push(
                Entry::new(if long {
                    "Buy with these levels..."
                } else {
                    "Sell with these levels..."
                })
                .icon(IconName::ArrowLeftRight)
                .on_click(move |_, cx| trade.update(cx, |chart, cx| chart.trade_drawing(id, cx)))
                .into(),
            );
            about.push(
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
        if wyck_app::alerts::eval::is_alert_tool(drawing.tool) {
            let seed = ChartAction::AddAlertOn(crate::chart::chart::events::AlertSeed::Drawing {
                id,
                zone: wyck_app::alerts::eval::is_zone_tool(drawing.tool),
                timeframe: self.timeframe.code(),
            });
            let alert = chart.clone();
            about.push(
                Entry::new("Add alert on this drawing...")
                    .icon(IconName::BellPlus)
                    .on_click(move |_, cx| {
                        alert.update(cx, |_, cx| cx.emit(ChartEvent::Action(seed.clone())));
                    })
                    .into(),
            );
        }
        vec![
            popup::Section::new(drawing.title(), about),
            popup::Section::untitled(vec![
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
            ]),
            popup::Section::untitled(vec![
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
            ]),
            popup::Section::untitled(vec![
                Entry::new("Delete")
                    .icon(IconName::Trash)
                    .disabled(drawing.locked)
                    .danger()
                    .on_click(command(DrawingCommand::Delete))
                    .into(),
            ]),
        ]
    }

    /// The right-click menu, for the price under the pointer. `at` is where the pointer was, in
    /// window coordinates.
    pub(super) fn context_items(
        &self,
        chart: &Entity<Self>,
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
        let mut groups = Vec::new();
        if let Some(id) = self.drawing_under(x, y, cx) {
            groups.extend(self.drawing_items(chart, id, cx));
        }
        if let (Some(raw), true) = (price_raw, self.symbol.is_some() && self.replay.is_none()) {
            let text = format_price(f64_to_i64(raw.round()), digits);
            let real = to_real(raw);
            let buy_limit = ask.is_some_and(|ask| raw < i64_to_f64(ask));
            let sell_limit = bid.is_some_and(|bid| raw > i64_to_f64(bid));
            let emit = |action: ChartAction| {
                let chart = chart.clone();
                move |_: &mut Window, cx: &mut App| {
                    chart.update(cx, |_, cx| cx.emit(ChartEvent::Action(action.clone())));
                }
            };
            let ticket = |buy: bool, entry: Option<f64>| {
                ChartAction::Ticket(TicketRequest {
                    buy,
                    entry,
                    stop_loss: None,
                    take_profit: None,
                    link: None,
                })
            };
            groups.push(popup::Section::new(
                format!("Trade at {text}"),
                vec![
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
                ],
            ));
            let mut alerts: Vec<Item> = vec![
                Entry::new(format!("Add alert at {text}"))
                    .icon(IconName::BellPlus)
                    .hint(crate::kit::hints::hint("add_alert").unwrap_or_default())
                    .on_click(emit(ChartAction::AddAlert(real)))
                    .into(),
            ];
            // An alert on each indicator the chart shows.
            for study in self
                .settings()
                .studies
                .iter()
                .filter(|s| {
                    s.visible
                        && !s.is_script()
                        && s.kind != wyck_domain::indicators::kind::StudyKind::VolumeProfile
                })
                .take(6)
            {
                alerts.push(
                    Entry::new(format!("Add alert on {}...", study.title()))
                        .icon(IconName::BellPlus)
                        .on_click(emit(ChartAction::AddAlertOn(
                            crate::chart::chart::events::AlertSeed::Indicator {
                                study: Box::new(study.clone()),
                                timeframe: self.timeframe.code(),
                            },
                        )))
                        .into(),
                );
            }
            groups.push(popup::Section::new("Alerts", alerts));
        }
        let on = |f: fn(&mut Self, &mut Window, &mut Context<Self>)| {
            let chart = chart.clone();
            move |window: &mut Window, cx: &mut App| {
                chart.update(cx, |this, cx| f(this, window, cx));
            }
        };
        groups.extend([
            popup::Section::new(
                "Chart",
                vec![
                    Entry::new("Indicators...")
                        .icon(IconName::ChartSpline)
                        .on_click(on(|_, window, cx| {
                            indicator_picker::open(cx.entity(), window, cx);
                        }))
                        .into(),
                    Entry::new("Chart settings...")
                        .icon(IconName::Settings2)
                        .on_click(on(|_, window, cx| {
                            chart_settings_ui::open(cx.entity(), window, cx);
                        }))
                        .into(),
                    Entry::new("Scale settings...")
                        .icon(IconName::Ruler)
                        .on_click(on(|_, window, cx| {
                            chart_settings_ui::open_scales(cx.entity(), window, cx);
                        }))
                        .into(),
                    Entry::new("Reset chart view")
                        .icon(IconName::RotateCcw)
                        .hint("End")
                        .on_click(on(|this, _, cx| this.jump_to_latest(cx)))
                        .into(),
                ],
            ),
            popup::Section::new(
                "Copy and paste",
                vec![
                    Entry::new("Copy indicators")
                        .icon(IconName::Copy)
                        .hint(crate::kit::hints::hint("copy_indicators").unwrap_or_default())
                        .on_click(on(|_, _, cx| cx.emit(ChartEvent::CopyIndicators)))
                        .into(),
                    Entry::new("Paste on this chart")
                        .icon(IconName::ClipboardPaste)
                        .hint(crate::kit::hints::hint("paste_to_chart").unwrap_or_default())
                        .on_click(on(|_, _, cx| cx.emit(ChartEvent::Paste)))
                        .into(),
                ],
            ),
            popup::Section::new(
                "Drawings",
                vec![
                    Entry::new("Remove all drawings")
                        .icon(IconName::Trash)
                        .disabled(self.drawing_count(cx).is_none_or(|count| count == 0))
                        .on_click(on(|this, _, cx| this.clear_drawings(cx)))
                        .into(),
                ],
            ),
            popup::Section::new(
                "Export",
                vec![
                    Entry::new("Take a picture of the chart")
                        .icon(IconName::Camera)
                        .hint(crate::kit::hints::hint("screenshot").unwrap_or_default())
                        .on_click(on(|_, _, cx| cx.emit(ChartEvent::Screenshot)))
                        .into(),
                    Entry::new("Take a picture of all the charts")
                        .icon(IconName::Images)
                        .hint(crate::kit::hints::hint("screenshot_all").unwrap_or_default())
                        .disabled(self.layout_charts < 2)
                        .on_click(on(|_, _, cx| cx.emit(ChartEvent::ScreenshotAll)))
                        .into(),
                    Entry::new("Export the data...")
                        .icon(IconName::Download)
                        .on_click(on(|_, window, cx| export_ui::open(cx.entity(), window, cx)))
                        .into(),
                ],
            ),
        ]);
        popup::sections(groups)
    }
    /// The open menu of the toolbar or the corner.
    pub(super) fn menu_popup(
        &self,
        geometry: &Geometry,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.menu?;
        let card = match menu {
            Menu::Zone => {
                let now = self.clock_ms();
                let mut list = div().flex().flex_col();
                for zone in Zone::menu() {
                    list = list.child(
                        popup::parts::row(
                            SharedString::from(format!("chart-zone-{}", zone.code())),
                            zone.name(now),
                            None,
                            self.settings.zone == zone,
                        )
                        .on_click(cx.listener(
                            move |this, _event, _window, cx| {
                                this.menu = None;
                                this.edit_settings(cx, |s| s.zone = zone);
                            },
                        )),
                    );
                }
                popup::parts::card()
                    .id("chart-zone-menu")
                    .w(px(tokens::menu::DROPDOWN_WIDTH))
                    .max_h(px((f32_of(geometry.h) - 60.0).max(160.0)))
                    .overflow_y_scroll()
                    .child(popup::parts::section_title("Time zone"))
                    .child(list)
                    .into_any_element()
            }
        };
        // The zone opens from the corner at the bottom right.
        let positioned = div()
            .absolute()
            .right(px(tokens::space::xs()))
            .bottom(px(axis_h() + 4.0))
            .child(card);
        Some(
            deferred(
                popup::parts::backdrop({
                    let this = cx.entity().downgrade();
                    move |_, cx| {
                        lost(
                            "menus: menu_popup",
                            this.update(cx, |this, cx| {
                                this.menu = None;
                                cx.notify();
                            }),
                        );
                    }
                })
                .child(positioned.on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
