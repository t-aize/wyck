//! What sits over the canvas: the legend of each band (the symbol, the prices under the pointer,
//! the indicators with their buttons), the toolbar, its menus, the labels of the order and alert
//! lines, the right-click menu, and the loading, failed and empty states.

use crate::ui::kit::icon::IconName;
use gpui::prelude::*;
use gpui::{Context, MouseButton, Window, div};

use super::settings::ChartKind;
use super::{Chart, paint};
use crate::ui::kit::menu::{self as popup};

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
            .children(self.marquee())
            .children(self.status(cx))
            .on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                opener.open(Some(event.position), cx);
            })
            .children(menu.popup(context, popup::Placement::Cursor, window, cx))
    }
}

mod headline;
mod legend;
mod menus;
