//! The legend of each band: the symbol, the prices under the pointer and the indicators.

use super::rgb;
use crate::ui::features::chart::scene::{Geometry, axis_w, price};
use crate::ui::features::chart::study::{Placement, PlotKind, ValueFormat};
use crate::ui::features::chart::{Chart, ChartEvent, EditorRequest, study_settings};
use crate::ui::kit::icon::IconName;
use crate::ui::kit::{controls, icon, theme, tokens};
use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, MouseButton, SharedString, div, px};

impl Chart {
    /// The background of a button of the legend while the pointer is over it. It is made from the
    /// text color of the chart, so it shows on a light chart under a dark theme too (the theme's
    /// own hover color would be a dark patch there).
    pub(super) fn hover_color(&self) -> gpui::Rgba {
        gpui::Rgba {
            a: 0.12,
            ..self.palette().text_strong
        }
    }

    /// The number under the pointer (or the newest) of each plot of an indicator, with its color.
    pub(super) fn study_values(&self, study: usize, index: usize) -> Vec<(String, u32)> {
        let config = &self.settings.studies[study];
        let Some(Some(output)) = self.display.studies.get(study) else {
            return Vec::new();
        };
        let format = match config.spec().placement {
            Placement::Overlay => ValueFormat::Price,
            Placement::Pane => config.value_format(),
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
                            crate::ui::features::chart::study::UP_COLOR
                        } else {
                            crate::ui::features::chart::study::DOWN_COLOR
                        }
                    }
                    _ => config.plot_style(plot.key).color,
                };
                (price::format_value(value, format, self.digits()), color)
            })
            .filter(|(text, _)| !text.is_empty())
            .collect()
    }

    /// One indicator's line in a legend. Keep its actions before the changing values so their
    /// hit targets stay in place while the pointer moves across bars.
    pub(super) fn study_row(
        &self,
        study: usize,
        index: Option<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let config = &self.settings.studies[study];
        let visible = config.visible;
        let group = SharedString::from(format!("study-row-{study}"));
        let values = index
            .filter(|_| self.settings.status.indicator_values && config.legend_values)
            .map(|i| self.study_values(study, i))
            .unwrap_or_default();
        // An indicator written as a script: which one, and what is wrong with it if anything.
        let script = config.script.clone().filter(|_| config.is_script());
        let alert: Option<String> = script.as_ref().and_then(|id| {
            if !config.is_ready() {
                Some(
                    crate::ui::features::chart::study::custom::library::registry::get(id)
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
                .size(px(crate::ui::kit::tokens::height::tiny()))
                .rounded_sm()
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                // The press is the button's, not the chart's under it.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(icon::tinted(icon, 13., theme::chart_muted()))
                .tooltip(controls::tooltip(tooltip))
        };
        div()
            .id(group)
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(tokens::height::tiny()))
            .pl_1()
            .rounded_sm()
            .text_size(px(tokens::text::body()))
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
                    .child(icon::tinted(
                        IconName::TriangleAlert,
                        13.,
                        theme::destructive(),
                    ))
                    .tooltip(controls::tooltip(message.clone()))
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .flex_none()
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
            .children(visible.then(|| {
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .children(
                        values
                            .into_iter()
                            .map(|(text, color)| div().text_color(rgb(color)).child(text)),
                    )
            }))
            .into_any_element()
    }

    /// The legend of the prices (symbol, prices, overlays) and of each indicator pane.
    pub(super) fn legends(
        &self,
        geometry: &Geometry,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut out = Vec::new();
        let index = self.shown_index();
        // The legend wraps before the toolbar at the top right (or the price axis, when the
        // toolbar is hidden).
        let right = axis_w() + if self.shows_toolbar() { 196.0 } else { 8.0 };
        let mut main = div()
            .absolute()
            .top(px(crate::ui::kit::tokens::space::sm()))
            .left(px(crate::ui::kit::tokens::space::sm()))
            .right(px(right))
            .flex()
            .flex_col()
            .gap_1()
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
                    .left(px(crate::ui::kit::tokens::space::sm()))
                    .child(self.study_row(pane.study, index, cx))
                    .into_any_element(),
            );
        }
        out
    }

    /// What to do next with the drawing tool that is picked, at the bottom of the prices.
    pub(super) fn drawing_hint(
        &self,
        geometry: &Geometry,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
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
            Some((crate::domain::drawings::model::Tool::ArrowPath, placed, _)) => format!(
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
        let bottom = ((geometry.h - geometry.main().bottom()) as f32 + 10.0).max(self.hint_floor);
        Some(
            div()
                .absolute()
                .left_0()
                .right(px(axis_w()))
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
                        .text_size(px(tokens::text::small()))
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
    pub(super) fn shows_toolbar(&self) -> bool {
        let wide = self
            .bounds
            .get()
            .is_some_and(|b| f32::from(b.size.width) >= 360.0);
        (self.selected && wide) || self.menu.is_some()
    }

    /// A dot that says whether the symbol's market is open, with when it opens or closes.
    pub(super) fn market_dot(&self) -> Option<AnyElement> {
        use crate::domain::market::MarketStatus;
        let status = self.market_status()?;
        let now = crate::app::market_data::now_ms();
        let within =
            |at: Option<i64>| at.map(|t| crate::domain::drawings::geometry::duration_text(t - now));
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
                .size(px(crate::ui::kit::tokens::height::tiny()))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_default()
                .hover(move |s| s.bg(hover))
                .tooltip(controls::tooltip(text.clone()))
                .child(
                    div()
                        .size(px(crate::ui::kit::tokens::swatch::dot()))
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
    pub(super) fn flow_values(&self, time_ms: i64, row: gpui::Div) -> gpui::Div {
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
                .gap_1()
                .child(div().text_color(theme::chart_muted()).child(label))
                .child(div().text_color(tone).child(text))
        };
        row.child(pair(
            "Vol",
            crate::ui::features::chart::footprint::compact(sell + buy),
            theme::chart_fg(),
        ))
        .child(pair(
            "Delta",
            crate::ui::features::chart::footprint::signed(delta),
            tone,
        ))
        .child(pair(
            "Buy",
            crate::ui::features::chart::footprint::compact(buy),
            theme::chart_up(),
        ))
        .child(pair(
            "Sell",
            crate::ui::features::chart::footprint::compact(sell),
            theme::chart_down(),
        ))
    }
}
