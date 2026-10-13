//! The small bar at the top right of a chart: it only shows when the view is away from the latest
//! price, with the buttons that bring it back.
//!
//! The chart type and the indicators are in the header of the dashboard (see [`super::header_menus`]).
//! The alert, the picture and the settings are in the right-click menu and have their keys.

use crate::ui::kit::icon::IconName;
use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, MouseButton, SharedString, div, px};

use super::view::PriceScale;
use super::{Chart, scene};
use crate::ui::kit::{controls, icon, theme, tokens};

/// A button of the bar.
fn tool(
    id: impl Into<SharedString>,
    icon: IconName,
    tip: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let ink = theme::muted_fg();
    div()
        .id(id.into())
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .h(px(tokens::height::compact()))
        .px_2()
        .rounded_md()
        .cursor_pointer()
        .text_size(px(tokens::text::body()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(ink)
        .hover(|s| s.bg(theme::surface_hover()).text_color(theme::fg()))
        // The press is the button's, not the chart's under it.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .tooltip(controls::tooltip(tip))
        .child(icon::tinted(icon, 15., ink))
}

impl Chart {
    /// Whether the price scale is the automatic one.
    fn auto_price_scale(&self) -> bool {
        matches!(self.view.price, PriceScale::Auto)
    }

    /// Whether the view is away from the latest bar, so the way back is offered.
    fn away_from_latest(&self) -> bool {
        !self.view.is_following() && !self.shown().is_empty()
    }

    /// Whether the bar has a button to show: on the chart being worked on, when it has the room.
    pub(super) fn shows_toolbar(&self) -> bool {
        let wide = self
            .bounds
            .get()
            .is_some_and(|b| f32::from(b.size.width) >= 360.0);
        self.selected && wide && (self.away_from_latest() || !self.auto_price_scale())
    }

    /// The buttons at the top right of the chart.
    pub(super) fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut bar = div()
            .absolute()
            .top(px(tokens::space::sm()))
            .right(px(scene::axis_w() + 8.0))
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .p_1()
            .rounded_lg()
            .bg(theme::bg_alpha(0.85))
            .border_1()
            .border_color(theme::border_hairline())
            .occlude();
        if !self.auto_price_scale() {
            bar = bar.child(
                tool(
                    "chart-auto-scale",
                    IconName::Scaling,
                    format!(
                        "Fit the prices{}",
                        crate::ui::kit::hints::suffix("reset_price_scale")
                    ),
                )
                .on_click(cx.listener(|this, _, _, cx| this.reset_price_scale(cx))),
            );
        }
        if self.away_from_latest() {
            bar = bar.child(
                tool(
                    "chart-latest",
                    IconName::ChevronsRight,
                    "Back to the latest price (End)",
                )
                .on_click(cx.listener(|this, _, _, cx| this.jump_to_latest(cx))),
            );
        }
        bar.into_any_element()
    }
}
