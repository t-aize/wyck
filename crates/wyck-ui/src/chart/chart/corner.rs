//! What sits over the corners of the chart: the buttons of the price scale in the bottom right
//! corner (auto, log, percent), the time zone at the right end of the time axis, and the way back
//! to the latest price.

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, MouseButton, SharedString, div, px};
use wyck_chart::{axis_h, axis_w, zone_w};

use super::{Chart, Menu};
use crate::kit::icon::IconName;
use crate::kit::{controls, icon, theme, tokens};
use wyck_chart::PriceScale;
use wyck_chart::ScaleMode;

/// A toggle of the price scale.
fn toggle(
    id: impl Into<SharedString>,
    label: &'static str,
    tip: impl Into<SharedString>,
    on: bool,
) -> gpui::Stateful<gpui::Div> {
    let ink = if on {
        theme::accent()
    } else {
        theme::muted_fg()
    };
    div()
        .id(id.into())
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .h(px(tokens::height::tiny()))
        .min_w(px(tokens::height::tiny()))
        .px_1()
        .rounded_sm()
        .cursor_pointer()
        .text_size(px(tokens::text::caption()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(ink)
        .when(on, |el| el.bg(theme::accent_selected()))
        .hover(|s| s.bg(theme::surface_hover()).text_color(theme::fg()))
        .tooltip(controls::tooltip(tip))
        .child(label)
}

impl Chart {
    /// Whether the view is away from the latest bar, so the way back is offered.
    fn away_from_latest(&self) -> bool {
        !self.view.is_following() && !self.shown().is_empty()
    }

    /// The corner buttons, the zone and the way back, over the chart.
    pub(super) fn corner(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let auto = matches!(self.view.price, PriceScale::Auto);
        let scale = self.settings.scale;
        // The press is the button's, not the chart's under it.
        let stop = |_: &gpui::MouseDownEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
            cx.stop_propagation();
        };

        let buttons = div()
            .absolute()
            .right_0()
            .bottom_0()
            .w(px(axis_w()))
            .h(px(axis_h()))
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .occlude()
            .on_mouse_down(MouseButton::Left, stop)
            .child(
                toggle(
                    "chart-scale-auto",
                    "A",
                    format!(
                        "Fit the prices automatically{}",
                        crate::kit::hints::suffix("reset_price_scale")
                    ),
                    auto,
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle_auto_price_scale(cx))),
            )
            .child(
                toggle(
                    "chart-scale-log",
                    "log",
                    format!(
                        "Logarithmic scale{}",
                        crate::kit::hints::suffix("toggle_log_scale")
                    ),
                    scale == ScaleMode::Log,
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle_scale(ScaleMode::Log, cx))),
            )
            .child(
                toggle(
                    "chart-scale-percent",
                    "%",
                    format!(
                        "Percent scale{}",
                        crate::kit::hints::suffix("toggle_percent_scale")
                    ),
                    scale == ScaleMode::Percent,
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle_scale(ScaleMode::Percent, cx))),
            );

        // Over the label of the time zone, which the scene draws: a click opens the list of zones.
        let zone = div()
            .id(("chart-zone", self.id))
            .absolute()
            .right(px(axis_w()))
            .bottom_0()
            .w(px(zone_w()))
            .h(px(axis_h()))
            .cursor_pointer()
            .occlude()
            .on_mouse_down(MouseButton::Left, stop)
            .tooltip(controls::tooltip("Time zone"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.menu = Some(Menu::Zone);
                cx.notify();
            }));

        let mut out = vec![zone.into_any_element(), buttons.into_any_element()];
        if self.away_from_latest() {
            out.push(
                div()
                    .absolute()
                    .right(px(axis_w() + tokens::space::sm()))
                    .bottom(px(axis_h() + tokens::space::sm()))
                    .child(
                        div()
                            .id(("chart-latest", self.id))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(tokens::height::compact()))
                            .rounded_md()
                            .cursor_pointer()
                            .bg(theme::bg_alpha(0.85))
                            .border_1()
                            .border_color(theme::border_hairline())
                            .occlude()
                            .on_mouse_down(MouseButton::Left, stop)
                            .hover(|s| s.bg(theme::surface_hover()))
                            .tooltip(controls::tooltip("Back to the latest price (End)"))
                            .child(icon::tinted(
                                IconName::ChevronsRight,
                                15.,
                                theme::muted_fg(),
                            ))
                            .on_click(cx.listener(|this, _, _, cx| this.jump_to_latest(cx))),
                    )
                    .into_any_element(),
            );
        }
        out
    }
}
