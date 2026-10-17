//! The chart type and the indicators in the header: the two things that change the chart being
//! worked on. Their menus are built by the chart (see `Chart::kind_items`), so each entry runs on it.

use gpui::prelude::*;
use gpui::{Context, SharedString, Window, div};

use super::Dashboard;
use wyck_ui_chart::chart::{Chart, kind_icon};
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::menu::{Menu, Placement};
use wyck_ui_kit::prelude::Selectable;
use wyck_ui_kit::{button, tokens};

impl Dashboard {
    /// The chart type, the indicators and the quick list of indicators, for the active chart.
    /// `labels` is false in a narrow window, where only the icons stay.
    pub(super) fn chart_buttons(
        &mut self,
        labels: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let chart = self.multi.read(cx).active_chart().clone();
        let (kind, studies) = {
            let settings = chart.read(cx).settings();
            (settings.kind, settings.studies.len())
        };
        let kind_menu = Menu::new("header-chart-kind-menu", window, cx);
        let quick_menu = Menu::new("header-chart-studies-menu", window, cx);
        let kind_items = if kind_menu.is_open(cx) {
            chart.update(cx, |chart, cx| chart.kind_items(&kind_menu, cx))
        } else {
            Vec::new()
        };
        let quick_items = if quick_menu.is_open(cx) {
            chart.update(cx, |_, cx| Chart::indicator_items(&quick_menu, cx))
        } else {
            Vec::new()
        };

        let toggle_kind = kind_menu.clone();
        let toggle_quick = quick_menu.clone();
        let picker_target = chart.clone();

        let kind_button = button::quiet("header-chart-kind")
            .icon(kind_icon(kind))
            .tooltip("Chart type")
            .on_click(move |_, _window, cx| toggle_kind.toggle(cx));
        let studies_button = button::quiet("header-chart-studies")
            .icon(IconName::ChartSpline)
            .selected(studies > 0)
            .tooltip("Add or manage indicators")
            .on_click(move |_, window, cx| {
                Chart::open_indicator_picker(&picker_target, window, cx);
            });
        let (kind_button, studies_button) = if labels {
            (
                kind_button.label(SharedString::from(kind.label())),
                studies_button.label(if studies > 0 {
                    SharedString::from(format!("Indicators {studies}"))
                } else {
                    SharedString::from("Indicators")
                }),
            )
        } else {
            (kind_button, studies_button)
        };

        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(
                div()
                    .relative()
                    .child(kind_button)
                    .children(kind_menu.popup(
                        kind_items,
                        Placement::Below(tokens::height::compact()),
                        window,
                        cx,
                    )),
            )
            .child(studies_button)
            .child(
                div()
                    .relative()
                    .child(
                        button::quiet("header-chart-studies-quick")
                            .icon(IconName::ChevronDown)
                            .tooltip("Favorites and recent indicators")
                            .on_click(move |_, _window, cx| toggle_quick.toggle(cx)),
                    )
                    .children(quick_menu.popup(
                        quick_items,
                        Placement::Below(tokens::height::compact()),
                        window,
                        cx,
                    )),
            )
    }
}
