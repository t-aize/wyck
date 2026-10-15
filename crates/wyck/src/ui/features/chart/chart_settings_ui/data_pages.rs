//! The pages for the canvas, the time zone, the trading lines and the indicators of the chart.

use super::{ChartSettingsEditor, ColorKey, LABEL_WIDTH, PRICE_HEIGHTS, ZONE_LIST_MAX, edit_chart};
use crate::ui::features::chart::events::{ChartEvent, EditorRequest};
use crate::ui::features::chart::study::config::StudyConfig;
use crate::ui::features::chart::{indicator_picker, open_object_tree, study_settings};
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, SharedString, div, px};
use wyck_chart::ChartColors;
use wyck_chart::ChartSettings;
use wyck_chart::Zone;
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::{button, controls, form, icon, modal, theme, tokens};

impl ChartSettingsEditor {
    pub(super) fn canvas_page(
        &self,
        settings: &ChartSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            colors.push(form::field(
                "Colors of the charts",
                Some("Rising, falling and line colors are on the Symbol page"),
                button::action(
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
        form::page()
            .child(form::group(IconName::Palette, "Colors", colors))
            .child(form::group(IconName::Image, "Watermark", watermark))
            .into_any_element()
    }

    pub(super) fn time_page(&self, settings: &ChartSettings) -> AnyElement {
        let now = crate::app::market_data::now_ms();
        let mut zones = div()
            .id("settings-zone-list")
            .flex()
            .flex_col()
            .max_h(px(ZONE_LIST_MAX))
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
                    .h(px(tokens::height::control()))
                    .px_3()
                    .cursor_pointer()
                    .text_size(px(tokens::text::body()))
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
                    .children(chosen.then(|| icon::tinted(IconName::Check, 13., theme::accent()))),
            );
        }
        form::page()
            .child(form::group(
                IconName::Clock,
                "Time zone of the time axis",
                [form::block(zones)],
            ))
            .child(form::note(
                "The zone applies to the time axis, the crosshair, the day separators and the times of the drawings.",
            ))
            .into_any_element()
    }

    pub(super) fn trading_page(&self, settings: &ChartSettings, cx: &App) -> AnyElement {
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
        let mut page = form::page().child(form::group(
            IconName::ArrowLeftRight,
            "Lines on the chart",
            lines,
        ));
        if let Some(count) = self.chart.read(cx).drawing_count(cx) {
            let chart = self.chart.clone();
            page = page.child(form::group(
                IconName::PenTool,
                "Drawings",
                [form::field(
                    "Drawings and positions on this symbol",
                    Some(match count {
                        0 => "None yet",
                        1 => "1 drawing",
                        _ => "Several drawings",
                    }),
                    button::action(
                        "trading-object-tree",
                        "Open the list",
                        Some(IconName::ListTree),
                        false,
                        move |window, cx| {
                            open_object_tree(&chart, window, cx);
                        },
                    ),
                )],
            ));
        }
        page.into_any_element()
    }

    pub(super) fn study_row(&self, index: usize, config: &StudyConfig) -> AnyElement {
        let spec = config.spec();
        let button = |id: &str, icon: IconName, tip: &'static str| {
            wyck_ui_kit::button::quiet(SharedString::from(format!("{id}-{index}")))
                .icon(icon)
                .tooltip(tip)
        };
        let (toggle, edit, remove) = (self.chart.clone(), self.chart.clone(), self.chart.clone());
        let visible = config.visible;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(wyck_ui_kit::tokens::height::row()))
            .child(
                div()
                    .flex_none()
                    .w(px(LABEL_WIDTH))
                    .text_size(px(tokens::text::small()))
                    .text_color(theme::accent())
                    .child(spec.short),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(tokens::text::emphasis()))
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
                    .gap_1()
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

    pub(super) fn indicators_page(&self, settings: &ChartSettings, cx: &App) -> AnyElement {
        let on_chart: Vec<AnyElement> = settings
            .studies
            .iter()
            .enumerate()
            .map(|(index, config)| self.study_row(index, config))
            .collect();
        let held = if on_chart.is_empty() {
            vec![form::block(form::note("No indicator on this chart yet."))]
        } else {
            on_chart
        };

        let limit = self.chart.read(cx).max_studies();
        let full = settings.studies.len() >= limit;
        let (browse, editor, new) = (self.chart.clone(), self.chart.clone(), self.chart.clone());
        let add = if full {
            form::block(form::note(format!(
                "This chart allows {limit} indicators. Change the limit in Settings{}.",
                wyck_ui_kit::hints::suffix("open_settings")
            )))
        } else {
            form::block(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .child(button::action(
                        "chart-browse-indicators",
                        "Browse the indicators",
                        Some(IconName::ChartSpline),
                        true,
                        move |window, cx| {
                            indicator_picker::open(browse.clone(), window, cx);
                        },
                    ))
                    .child(button::action(
                        "chart-open-editor",
                        "Indicator editor",
                        Some(IconName::CodeXml),
                        false,
                        move |window, cx| {
                            modal::close(window, cx);
                            editor.update(cx, |_, cx| {
                                cx.emit(ChartEvent::IndicatorEditor(EditorRequest::Open));
                            });
                        },
                    ))
                    .child(button::action(
                        "chart-new-script",
                        "New script",
                        Some(IconName::FilePlus),
                        false,
                        move |window, cx| {
                            modal::close(window, cx);
                            new.update(cx, |_, cx| {
                                cx.emit(ChartEvent::IndicatorEditor(EditorRequest::New));
                            });
                        },
                    )),
            )
        };

        let chart = self.chart.clone();
        let layout = vec![form::field(
            "Height of the prices",
            Some("Against the panes of the indicators. Drag a divider for any other height"),
            controls::height_picker(
                "chart-price-height",
                PRICE_HEIGHTS,
                settings.main_weight,
                move |weight, _window, cx| edit_chart(&chart, cx, |s| s.main_weight = weight),
            ),
        )];
        form::page()
            .child(form::group(
                IconName::ChartSpline,
                format!("On this chart ({})", settings.studies.len()),
                held,
            ))
            .child(form::group(IconName::Plus, "Add an indicator", [add]))
            .child(form::group(IconName::LayoutPanelTop, "Layout", layout))
            .into_any_element()
    }
}
