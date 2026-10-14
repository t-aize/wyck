//! The headline of the chart, the labels of the lines, the states of the canvas and the selection box.

use super::rgb;
use crate::ui::features::chart::data::Series;
use crate::ui::features::chart::events::ChartEvent;
use crate::ui::features::chart::scene::{Geometry, axis_h, axis_w};
use crate::ui::features::chart::settings::ChartKind;
use crate::ui::features::chart::{Chart, Load, Older};
use crate::ui::kit::icon::IconName;
use crate::ui::kit::{anim, button, controls, icon, theme, tokens};
use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, MouseButton, SharedString, div, px};
use wyck_market::format_price;

impl Chart {
    /// The button that folds the lines of the indicators away, or brings them back. Folded, it
    /// says how many there are. It sets the same option as "Indicator names" in the settings.
    pub(super) fn legend_toggle(
        &self,
        hover: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let shown = self.settings.status.indicators;
        let count = self.settings.studies.len();
        let tip = if shown {
            "Hide the indicator names".to_owned()
        } else if count == 1 {
            "Show the indicator name".to_owned()
        } else {
            format!("Show the {count} indicator names")
        };
        div()
            .id(("legend-toggle", self.id))
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .h(px(tokens::height::tiny()))
            .px_1()
            .rounded_sm()
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .text_size(px(tokens::text::small()))
            .text_color(theme::chart_muted())
            // The press is the button's, not the chart's under it.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                this.edit_settings(cx, |s| s.status.indicators = !s.status.indicators);
            }))
            .tooltip(controls::tooltip(SharedString::from(tip)))
            .child(icon::tinted(
                if shown {
                    IconName::ChevronUp
                } else {
                    IconName::ChevronDown
                },
                13.,
                theme::chart_muted(),
            ))
            .when(!shown, |el| el.child(count.to_string()))
    }

    /// The first line of the legend: the symbol (a button that changes it), the timeframe and
    /// type, and the prices of the point under the pointer.
    pub(super) fn headline(&self, compact: bool, cx: &mut Context<Self>) -> impl IntoElement {
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
            .h(px(tokens::height::tiny()))
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
            .child(icon::tinted(IconName::ChevronDown, 12., palette.text));
        let mut kind_text = self.timeframe.label();
        if self.settings.kind != ChartKind::Candles {
            kind_text.push_str(" \u{b7} ");
            kind_text.push_str(self.settings.kind.label());
        }
        if self.display.box_size > 0 && self.settings.kind != ChartKind::LineBreak {
            kind_text.push_str(&format!(" {}", format_price(self.display.box_size, digits)));
        }
        if self.settings.kind == ChartKind::Footprint && self.timeframe.bar_ms().is_some() {
            kind_text.push_str(
                if crate::ui::features::chart::footprint::supports(self.timeframe) {
                    " (tick volume)"
                } else {
                    " (needs bars of a day or less)"
                },
            );
        }
        let mut row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_x_2()
            .text_size(px(tokens::text::body()))
            .children((self.show_symbol || self.symbol.is_none()).then_some(symbol))
            .child(div().text_color(palette.text).child(kind_text))
            .children(
                self.settings
                    .status
                    .market
                    .then(|| self.market_dot())
                    .flatten(),
            );
        if !compact && !self.settings.studies.is_empty() {
            row = row.child(self.legend_toggle(hover, cx));
        }
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
                .gap_1()
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
    pub(super) fn line_labels(
        &self,
        geometry: &Geometry,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
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
                .right(px(axis_w() + 8.0))
                .h(px(tokens::height::tiny()))
                .flex()
                .flex_row()
                .items_center()
                .rounded_sm()
                .overflow_hidden()
                .border_1()
                .border_color(color)
                .bg(palette.bg)
                .text_size(px(tokens::text::small()))
                .occlude()
                .child(
                    div()
                        .h_full()
                        .px_2()
                        .flex()
                        .items_center()
                        .bg(color)
                        .text_color(palette.bg)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(line.label.clone()),
                )
                .children(line.detail.clone().map(|(text, detail_color)| {
                    div().px_2().text_color(rgb(detail_color)).child(text)
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
                            .child(icon::tinted(IconName::X, 11., palette.text_strong)),
                    )
                });
            out.push(pill.into_any_element());
        }
        out
    }

    /// Loading, failed and empty states, and the small note while older history comes in.
    pub(super) fn status(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
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
                    .child(icon::tinted(
                        IconName::ChartCandlestick,
                        28.,
                        theme::chart_muted(),
                    ))
                    .child(
                        div()
                            .text_size(px(tokens::text::title()))
                            .text_color(theme::chart_muted())
                            .child("Pick a symbol to see its chart."),
                    )
                    .into_any_element(),
            ),
            Load::Loading => out.push(
                centered()
                    .child(anim::spin(
                        icon::tinted(IconName::LoaderCircle, 22., theme::chart_muted()),
                        ("chart-loading", self.id),
                    ))
                    .child(
                        div()
                            .text_size(px(tokens::text::emphasis()))
                            .text_color(theme::chart_muted())
                            .child("Loading the chart..."),
                    )
                    .into_any_element(),
            ),
            Load::Failed(message) => out.push(
                centered()
                    .child(icon::tinted(
                        IconName::CircleAlert,
                        26.,
                        theme::destructive(),
                    ))
                    .child(
                        div()
                            .text_size(px(tokens::text::title()))
                            .text_color(theme::chart_fg())
                            .child("Could not load the chart"),
                    )
                    .child(
                        div()
                            .max_w(px(crate::ui::kit::tokens::measure::note()))
                            .text_center()
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::chart_muted())
                            .child(message.clone()),
                    )
                    .child(
                        div()
                            .pt_1()
                            .w(px(crate::ui::kit::tokens::measure::action()))
                            .child(button::primary(
                                ("chart-retry", self.id),
                                crate::ui::kit::button::Fill::Accent,
                                crate::ui::kit::button::Size::Md,
                                "Try again",
                                cx.listener(|this, _event, _window, cx| this.retry(cx)),
                            )),
                    )
                    .into_any_element(),
            ),
            Load::Ready if self.series.is_empty() => out.push(
                centered()
                    .child(
                        div()
                            .text_size(px(tokens::text::title()))
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
                    .left(px(crate::ui::kit::tokens::space::md()))
                    .bottom(px(axis_h() + 8.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_size(px(tokens::text::small()))
                    .text_color(theme::chart_muted())
                    .child(anim::spin(
                        icon::tinted(IconName::LoaderCircle, 12., theme::chart_muted()),
                        ("chart-older", self.id),
                    ))
                    .child("Loading older history")
                    .into_any_element(),
            );
        }
        out
    }
    /// The box being dragged to select drawings.
    pub(super) fn marquee(&self) -> Option<AnyElement> {
        let drag = self.drag.filter(|d| {
            d.kind == crate::ui::features::chart::input::DragKind::Marquee && d.moved
        })?;
        let (a, b) = (drag.start, drag.last);
        Some(
            div()
                .absolute()
                .left(px(a.0.min(b.0)))
                .top(px(a.1.min(b.1)))
                .w(px((a.0 - b.0).abs()))
                .h(px((a.1 - b.1).abs()))
                .bg(theme::accent_alpha(0.12))
                .border_1()
                .border_color(theme::accent_alpha(0.6))
                .into_any_element(),
        )
    }
}
