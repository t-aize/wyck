//! The pages that set how the chart looks: its type, symbol, status, scales and colors.

use super::{ChartSettingsEditor, ColorKey, edit_chart, size_mode, size_value};
use crate::ui::features::chart::{construction_ui, footprint_ui, overlay, tpo_ui, volume_ui};
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, SharedString, div, px};
use wyck_chart::BoxSize;
use wyck_chart::quote_unit;
use wyck_chart::{ChartKind, ChartSettings, ScaleMode};
use wyck_chart::{CrosshairStyle, ScaleMargin};
use wyck_market::PRICE_SCALE;
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::{controls, form, icon, number, theme, tokens};

impl ChartSettingsEditor {
    pub(super) fn kind_tiles(&self, current: ChartKind) -> AnyElement {
        let mut column = div().flex().flex_col().gap_3();
        for (title, kinds) in overlay::KIND_SECTIONS {
            let mut wrap = div().flex().flex_row().flex_wrap().gap_2();
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
                        .w(px(wyck_ui_kit::tokens::field::select()))
                        .h(px(tokens::height::large()))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .rounded_md()
                        .border_1()
                        .border_color(if chosen {
                            theme::accent()
                        } else {
                            theme::border_subtle()
                        })
                        .cursor_pointer()
                        .text_size(px(tokens::text::body()))
                        .text_color(ink)
                        .when(chosen, |el| el.bg(theme::accent_selected()))
                        .when(!chosen, |el| el.hover(|s| s.bg(theme::surface_hover())))
                        .on_click(move |_, _window, cx| {
                            edit_chart(&chart, cx, |s| s.kind = kind);
                        })
                        .child(icon::tinted(overlay::kind_icon(kind), 15., ink))
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
                            .text_size(px(tokens::text::small()))
                            .text_color(theme::muted_fg())
                            .child(title),
                    )
                    .child(wrap),
            );
        }
        form::block(column)
    }

    /// The sizes of the price based chart types, for the one the chart shows.
    pub(super) fn construction_group(
        &self,
        settings: &ChartSettings,
        cx: &App,
    ) -> Option<AnyElement> {
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
            let unit_real = quote_unit(digits) / PRICE_SCALE as f64;
            let resolved = (box_size > 0).then(|| box_size as f64 / PRICE_SCALE as f64);
            rows.push(form::field(
                field.label(),
                Some("Measured in ATR, in price or in percent"),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(controls::segmented(
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
                                state.set_value(number::format(size_value(next), 6), window, cx);
                            });
                        },
                    ))
                    .child(number::field(state, tokens::field::number())),
            ));
        }
        match kind {
            ChartKind::LineBreak => rows.push(form::field(
                "Lines to break",
                Some("A new line turns after breaking the extreme of this many"),
                number::field(&self.line_break, tokens::field::number()),
            )),
            ChartKind::PointFigure => rows.push(form::field(
                "Reversal (boxes)",
                Some("Boxes the price must go back to start a new column"),
                number::field(&self.reversal, tokens::field::number()),
            )),
            _ => {}
        }
        rows.extend(construction_ui::rows(
            kind,
            &self.chart,
            &self.construction,
            &settings.transform,
        ));
        (!rows.is_empty()).then(|| {
            form::group(
                overlay::kind_icon(kind),
                format!("{} construction", kind.label()),
                rows,
            )
            .into_any_element()
        })
    }

    pub(super) fn symbol_page(
        &self,
        settings: &ChartSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut page = form::page().child(form::group(
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
        match settings.kind {
            ChartKind::VolumeCandles => {
                page = page.children(volume_ui::candle_groups(
                    &self.chart,
                    &self.volume,
                    &settings.volume_candles,
                ));
            }
            ChartKind::VolumeBars => {
                let resolved = self.chart.read(cx).display.volume_size;
                page = page.child(volume_ui::bar_group(
                    &self.chart,
                    &self.volume,
                    &settings.volume_bars,
                    resolved,
                ));
            }
            ChartKind::Tpo => {
                page = page.children(tpo_ui::groups(&self.chart, &self.tpo, &settings.tpo));
            }
            _ => {}
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
        let own: &[(ColorKey, &'static str)] = match settings.kind {
            ChartKind::Kagi => &[
                (ColorKey::KagiYang, "Follows the rising color"),
                (ColorKey::KagiYin, "Follows the falling color"),
            ],
            ChartKind::PointFigure => &[
                (ColorKey::PnfUp, "Follows the rising color"),
                (ColorKey::PnfDown, "Follows the falling color"),
            ],
            ChartKind::Tpo => &[
                (ColorKey::Tpo, "The letters and blocks of a profile"),
                (ColorKey::TpoPoc, "The row with the most marks"),
                (
                    ColorKey::TpoValueArea,
                    "The rows around it that hold most marks",
                ),
                (ColorKey::TpoIb, "The range of the first periods"),
                (ColorKey::TpoSingle, "Rows only one period visited"),
            ],
            _ => &[],
        };
        for (key, hint) in own {
            colors.push(self.color_row(*key, Some(hint), cx));
        }
        page.child(form::group(IconName::Palette, "Colors", colors))
            .into_any_element()
    }

    pub(super) fn status_page(&self, settings: &ChartSettings) -> AnyElement {
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
        form::page()
            .child(form::group(IconName::Type, "Symbol line", first_line))
            .child(form::group(IconName::ChartSpline, "Indicators", studies))
            .into_any_element()
    }

    pub(super) fn scales_page(&self, settings: &ChartSettings) -> AnyElement {
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
        form::page()
            .child(form::group(IconName::Ruler, "Price scale", scale))
            .child(form::group(IconName::Tag, "Price lines", lines))
            .child(form::group(IconName::Grid3x3, "Grid", grid))
            .child(form::group(IconName::Crosshair, "Crosshair", crosshair))
            .into_any_element()
    }
}
