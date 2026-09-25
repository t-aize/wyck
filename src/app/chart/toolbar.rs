//! The bar at the top right of a chart: the chart type, the indicators, an alert, the price scale,
//! and what brings the view back, the picture and the settings.
//!
//! Its buttons open the same menus as everywhere in the app (see [`crate::app::menu`]): a card of
//! entries with icons and check marks. On a chart too small for all of them, the type and the
//! settings stay and the rest goes into one menu.

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, MouseButton, SharedString, Window, div, px};
use gpui_kit::assets::IconName;

use super::lines::to_real;
use super::overlay::{KIND_SECTIONS, kind_icon};
use super::settings::ScaleMode;
use super::study::catalog::{self, Source};
use super::view::PriceScale;
use super::{Chart, ChartAction, ChartEvent, EditorRequest, chart_settings_ui, indicator_picker};
use crate::app::connection::ui::icon_colored;
use crate::app::indicators;
use crate::app::menu::{Entry, Item, Menu, Placement};
use crate::app::theme;

/// How many starred and recent indicators the quick menu lists.
const QUICK: usize = 8;

/// A button of the bar.
fn tool(
    id: impl Into<SharedString>,
    icon: IconName,
    label: Option<String>,
    tip: &'static str,
    active: bool,
) -> gpui::Stateful<gpui::Div> {
    let ink = if active {
        theme::accent()
    } else {
        theme::muted_fg()
    };
    div()
        .id(id.into())
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .h(px(26.))
        .px_1p5()
        .rounded_md()
        .cursor_pointer()
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(ink)
        .hover(|s| s.bg(theme::surface_hover()).text_color(theme::fg()))
        // The press is the button's, not the chart's under it.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(tip).build(window, cx)
        })
        .child(icon_colored(icon, 15., ink))
        .children(label)
}

fn separator() -> gpui::Div {
    div()
        .flex_none()
        .w(px(1.))
        .h(px(16.))
        .mx_0p5()
        .bg(theme::border_hairline())
}

impl Chart {
    /// The entry of a menu that runs `run` on this chart.
    fn menu_entry(
        &self,
        menu: &Menu,
        entry: Entry,
        run: impl Fn(&mut Chart, &mut Window, &mut Context<Chart>) + 'static,
        cx: &mut Context<Self>,
    ) -> Item {
        let (chart, menu) = (cx.entity(), menu.clone());
        entry
            .on_click(move |window, cx| {
                menu.close(cx);
                chart.update(cx, |this, cx| run(this, window, cx));
            })
            .into()
    }

    /// The types of chart, in sections, the current one checked.
    fn kind_items(&self, menu: &Menu, cx: &mut Context<Self>) -> Vec<Item> {
        let mut items = Vec::new();
        for (title, kinds) in KIND_SECTIONS {
            if !items.is_empty() {
                items.push(Item::Separator);
            }
            items.push(Item::Title(title.into()));
            for kind in kinds.iter().copied() {
                items.push(
                    self.menu_entry(
                        menu,
                        Entry::new(kind.label())
                            .icon(kind_icon(kind))
                            .checked(self.settings.kind == kind),
                        move |this, _, cx| this.set_kind(kind, cx),
                        cx,
                    ),
                );
            }
        }
        items
    }

    /// The scales, the inversion and the fit.
    fn scale_items(&self, menu: &Menu, cx: &mut Context<Self>) -> Vec<Item> {
        let mut items = vec![Item::Title("Price scale".into())];
        for mode in ScaleMode::ALL {
            items.push(self.menu_entry(
                menu,
                Entry::new(mode.label()).checked(self.settings.scale == mode),
                move |this, _, cx| this.edit_settings(cx, |s| s.scale = mode),
                cx,
            ));
        }
        items.push(Item::Separator);
        items.push(
            self.menu_entry(
                menu,
                Entry::new("Invert the scale")
                    .icon(IconName::ArrowUpDown)
                    .checked(self.settings.invert),
                |this, _, cx| this.edit_settings(cx, |s| s.invert = !s.invert),
                cx,
            ),
        );
        items.push(
            self.menu_entry(
                menu,
                Entry::new("Fit the prices")
                    .icon(IconName::Scaling)
                    .hint("Alt+R")
                    .disabled(matches!(self.view.price, PriceScale::Auto)),
                |this, _, cx| this.reset_price_scale(cx),
                cx,
            ),
        );
        items.push(Item::Separator);
        items.push(self.menu_entry(
            menu,
            Entry::new("Scale settings...").icon(IconName::Ruler),
            |_, window, cx| chart_settings_ui::open_scales(cx.entity(), window, cx),
            cx,
        ));
        items
    }

    /// The starred and the recent indicators, one click to add, and the ways to the rest.
    fn indicator_items(&self, menu: &Menu, cx: &mut Context<Self>) -> Vec<Item> {
        let prefs = indicators::prefs(cx);
        let all = catalog::items();
        let mut items: Vec<Item> = Vec::new();
        let by_key = |key: &String| all.iter().find(|item| item.key() == *key && item.ready);
        let starred: Vec<_> = prefs
            .favorites
            .iter()
            .filter_map(by_key)
            .take(QUICK)
            .collect();
        let recent: Vec<_> = prefs
            .recent
            .iter()
            .filter(|key| !prefs.is_favorite(key))
            .filter_map(by_key)
            .take(QUICK)
            .collect();
        let mut section =
            |title: &'static str, list: Vec<&catalog::Item>, items: &mut Vec<Item>| {
                if list.is_empty() {
                    return;
                }
                items.push(Item::Title(title.into()));
                for item in list {
                    let (icon, config, key) = (
                        match item.source {
                            Source::Builtin(_) => IconName::Plus,
                            Source::Script(_) => IconName::CodeXml,
                        },
                        item.config(),
                        item.key(),
                    );
                    items.push(
                        self.menu_entry(
                            menu,
                            Entry::new(item.label.clone())
                                .icon(icon)
                                .hint(item.short.clone()),
                            move |this, _, cx| {
                                this.add_study(config.clone(), cx);
                                let key = key.clone();
                                indicators::update_prefs(cx, |p| p.note_recent(&key));
                            },
                            cx,
                        ),
                    );
                }
                items.push(Item::Separator);
            };
        section("Favorites", starred, &mut items);
        section("Recent", recent, &mut items);
        items.push(self.menu_entry(
            menu,
            Entry::new("All indicators...").icon(IconName::ChartSpline),
            |_, window, cx| indicator_picker::open(cx.entity(), window, cx),
            cx,
        ));
        items.push(self.menu_entry(
            menu,
            Entry::new("On this chart...").icon(IconName::ListTree),
            |_, window, cx| chart_settings_ui::open_indicators(cx.entity(), window, cx),
            cx,
        ));
        items.push(Item::Separator);
        items.push(
            self.menu_entry(
                menu,
                Entry::new("Indicator editor")
                    .icon(IconName::CodeXml)
                    .hint("Ctrl+Shift+E"),
                |_, _, cx| cx.emit(ChartEvent::IndicatorEditor(EditorRequest::Open)),
                cx,
            ),
        );
        items.push(self.menu_entry(
            menu,
            Entry::new("New script...").icon(IconName::FilePlus),
            |_, _, cx| cx.emit(ChartEvent::IndicatorEditor(EditorRequest::New)),
            cx,
        ));
        items.push(self.menu_entry(
            menu,
            Entry::new("Open the folder").icon(IconName::FolderOpen),
            |_, _, cx| indicators::open_folder(cx),
            cx,
        ));
        items
    }

    /// An alert at the last price.
    fn alert_at_last(&mut self, cx: &mut Context<Self>) {
        if let Some(price) = self.series.last_price() {
            cx.emit(ChartEvent::Action(ChartAction::AddAlert(to_real(
                price as f64,
            ))));
        }
    }

    /// The buttons at the top right of the chart.
    pub(super) fn toolbar(
        &self,
        latest: bool,
        compact: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let auto = matches!(self.view.price, PriceScale::Auto);
        let studies = self.settings.studies.len();

        let kind_menu = Menu::new(("chart-kind-menu", self.id), window, cx);
        let studies_menu = Menu::new(("chart-studies-menu", self.id), window, cx);
        let scale_menu = Menu::new(("chart-scale-menu", self.id), window, cx);
        let more_menu = Menu::new(("chart-more-menu", self.id), window, cx);

        let kind_items = if kind_menu.is_open(cx) {
            self.kind_items(&kind_menu, cx)
        } else {
            Vec::new()
        };
        let studies_items = if studies_menu.is_open(cx) {
            self.indicator_items(&studies_menu, cx)
        } else {
            Vec::new()
        };
        let scale_items = if scale_menu.is_open(cx) {
            self.scale_items(&scale_menu, cx)
        } else {
            Vec::new()
        };
        let more_items = if more_menu.is_open(cx) {
            self.overflow_items(&more_menu, cx)
        } else {
            Vec::new()
        };

        let scale_label = match (self.settings.scale, auto) {
            (ScaleMode::Linear, true) => "Auto",
            (ScaleMode::Linear, false) => "Manual",
            (ScaleMode::Log, _) => "Log",
            (ScaleMode::Percent, _) => "%",
            (ScaleMode::Indexed, _) => "100",
        };
        let scale_active = !(auto && self.settings.scale == ScaleMode::Linear);

        let toggle = |menu: &Menu| {
            let menu = menu.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| menu.toggle(cx)
        };

        let kind = div()
            .relative()
            .child(
                tool(
                    "chart-kind",
                    kind_icon(self.settings.kind),
                    (!compact).then(|| self.settings.kind.label().to_owned()),
                    "Chart type",
                    false,
                )
                .on_click(toggle(&kind_menu)),
            )
            .children(kind_menu.popup(kind_items, Placement::Below(30.), window, cx));

        let mut bar = div()
            .absolute()
            .top(px(6.))
            .right(px(super::scene::AXIS_W + 8.0))
            .flex()
            .flex_row()
            .items_center()
            .gap_0p5()
            .p_0p5()
            .rounded_lg()
            .bg(theme::bg_alpha(0.85))
            .border_1()
            .border_color(theme::border_hairline())
            .occlude()
            .child(kind);

        if !compact {
            let alert = cx.listener(|this, _, _, cx| this.alert_at_last(cx));
            bar = bar
                .child(separator())
                .child(
                    div().relative().child(
                        tool(
                            "chart-studies",
                            IconName::ChartSpline,
                            Some(if studies > 0 {
                                format!("Indicators {studies}")
                            } else {
                                "Indicators".to_owned()
                            }),
                            "Add or manage indicators",
                            studies > 0,
                        )
                        .on_click(cx.listener(|_, _, window, cx| {
                            indicator_picker::open(cx.entity(), window, cx);
                        })),
                    ),
                )
                .child(
                    div()
                        .relative()
                        .child(
                            tool(
                                "chart-studies-quick",
                                IconName::ChevronDown,
                                None,
                                "Favorites, the editor and the folder",
                                false,
                            )
                            .on_click(toggle(&studies_menu)),
                        )
                        .children(studies_menu.popup(
                            studies_items,
                            Placement::Below(30.),
                            window,
                            cx,
                        )),
                )
                .child(
                    tool(
                        "chart-alert",
                        IconName::BellPlus,
                        None,
                        "An alert at the last price (Alt+A at the pointer)",
                        false,
                    )
                    .on_click(alert),
                )
                .child(separator())
                .child(
                    div()
                        .relative()
                        .child(
                            tool(
                                "chart-scale",
                                IconName::Ruler,
                                Some(scale_label.to_owned()),
                                "Price scale",
                                scale_active,
                            )
                            .on_click(toggle(&scale_menu)),
                        )
                        .children(scale_menu.popup(scale_items, Placement::Below(30.), window, cx)),
                );
            if !auto {
                bar = bar.child(
                    tool(
                        "chart-auto-scale",
                        IconName::Scaling,
                        None,
                        "Fit the prices (Alt+R)",
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.reset_price_scale(cx))),
                );
            }
            if latest {
                bar = bar.child(
                    tool(
                        "chart-latest",
                        IconName::ChevronsRight,
                        None,
                        "Back to the latest price (End)",
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.jump_to_latest(cx))),
                );
            }
            bar = bar.child(separator()).child(
                tool(
                    "chart-picture",
                    IconName::Camera,
                    None,
                    "Take a picture (Ctrl+Shift+S)",
                    false,
                )
                .on_click(cx.listener(|_, _, _, cx| cx.emit(ChartEvent::Screenshot))),
            );
        } else {
            if latest {
                bar = bar.child(
                    tool(
                        "chart-latest",
                        IconName::ChevronsRight,
                        None,
                        "Back to the latest price (End)",
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.jump_to_latest(cx))),
                );
            }
            bar = bar.child(
                div()
                    .relative()
                    .child(
                        tool("chart-more", IconName::Ellipsis, None, "More", false)
                            .on_click(toggle(&more_menu)),
                    )
                    .children(more_menu.popup(more_items, Placement::Below(30.), window, cx)),
            );
        }
        bar.child(
            tool(
                "chart-settings",
                IconName::Settings2,
                None,
                "Chart settings",
                false,
            )
            .on_click(cx.listener(|_, _, window, cx| {
                chart_settings_ui::open(cx.entity(), window, cx);
            })),
        )
        .into_any_element()
    }

    /// What a chart too small for the whole bar keeps in one menu.
    fn overflow_items(&self, menu: &Menu, cx: &mut Context<Self>) -> Vec<Item> {
        let mut items = vec![
            self.menu_entry(
                menu,
                Entry::new("Indicators...").icon(IconName::ChartSpline),
                |_, window, cx| indicator_picker::open(cx.entity(), window, cx),
                cx,
            ),
            self.menu_entry(
                menu,
                Entry::new("Alert at the last price").icon(IconName::BellPlus),
                |this, _, cx| this.alert_at_last(cx),
                cx,
            ),
            self.menu_entry(
                menu,
                Entry::new("Take a picture").icon(IconName::Camera),
                |_, _, cx| cx.emit(ChartEvent::Screenshot),
                cx,
            ),
            Item::Separator,
        ];
        items.extend(self.scale_items(menu, cx));
        items
    }
}
