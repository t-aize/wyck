//! The list of indicators to add to a chart: the ones the app ships and the ones the user wrote,
//! by category, with a search, stars for the ones used most, and the way to the editor.
//!
//! It is a panel of the same frame as the settings (see [`crate::app::settings_ui`]). Adding
//! does not close it, so several indicators can be added in a row.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, FontWeight, SharedString, Subscription, Window, div, px,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};

use super::study::Placement;
use super::study::catalog::{self, Item, Source};
use super::study::intern;
use super::{Chart, ChartEvent, EditorRequest};
use crate::app::connection::ui::icon_colored;
use crate::app::indicators;
use crate::app::settings_ui::{self as ui, Head, Tab};
use crate::app::{modal, theme};

/// Opens the list of indicators for `chart`.
pub fn open(chart: Entity<Chart>, window: &mut Window, cx: &mut App) {
    // Opened once the chart that asked is no longer being updated, since the panel reads it.
    window.defer(cx, move |window, cx| {
        let picker = cx.new(|cx| Picker::new(chart, window, cx));
        let search = picker.read(cx).search.clone();
        modal::open(picker, modal::Options::new(900.0, 660.0), window, cx);
        search.update(cx, |state, cx| state.focus(window, cx));
    });
}

/// The pages of the rail that are not a category.
const FIXED: [(&str, IconName); 3] = [
    ("All", IconName::Layers),
    ("Favorites", IconName::Star),
    ("Recent", IconName::Clock),
];

/// An icon for a category the app ships or a well known folder of scripts.
fn category_icon(category: &str) -> IconName {
    match category {
        "Trend" => IconName::TrendingUp,
        "Momentum" => IconName::Gauge,
        "Volatility" => IconName::Activity,
        "Volume" => IconName::ChartColumn,
        "Examples" => IconName::FileCode,
        "Custom" => IconName::CodeXml,
        "Broken" => IconName::TriangleAlert,
        _ => IconName::Folder,
    }
}

struct Picker {
    chart: Entity<Chart>,
    search: Entity<InputState>,
    /// The page of the rail: 0 all, 1 favorites, 2 recent, then the categories.
    page: usize,
    _subscriptions: Vec<Subscription>,
}

impl Picker {
    fn new(chart: Entity<Chart>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search indicators"));
        let subscriptions = vec![
            cx.subscribe(&search, |_this, _input, _event: &InputEvent, cx| {
                cx.notify()
            }),
            cx.observe(&chart, |_this, _chart, cx| cx.notify()),
            indicators::observe(cx, |_this: &mut Self, cx| cx.notify()),
        ];
        Self {
            chart,
            search,
            page: 0,
            _subscriptions: subscriptions,
        }
    }

    /// The indicators of a page, the starred ones first.
    fn shown(&self, items: &[Item], categories: &[String], query: &str, cx: &App) -> Vec<Item> {
        let prefs = indicators::prefs(cx);
        let mut shown: Vec<Item> = items
            .iter()
            .filter(|item| item.matches(query))
            .filter(|item| match self.page {
                0 => true,
                1 => prefs.is_favorite(&item.key()),
                2 => prefs.recent.contains(&item.key()),
                n => categories
                    .get(n - FIXED.len())
                    .is_some_and(|c| *c == item.category),
            })
            .cloned()
            .collect();
        if self.page == 2 {
            // The newest first.
            shown.sort_by_key(|i| prefs.recent.iter().position(|k| *k == i.key()));
        } else {
            shown.sort_by_key(|item| !prefs.is_favorite(&item.key()));
        }
        shown
    }

    fn row(
        &self,
        number: usize,
        item: &Item,
        held: usize,
        favorite: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (add_this, star_this, edit_this) = (cx.entity(), cx.entity(), cx.entity());
        let (add_item, star_item, edit_item) = (item.clone(), item.clone(), item.clone());
        let script = match &item.source {
            Source::Script(id) => Some(id.clone()),
            Source::Builtin(_) => None,
        };
        let usable = item.ready;
        let full = {
            let chart = self.chart.read(cx);
            chart.settings().studies.len() >= chart.max_studies()
        };
        let can_add = usable && !full;
        div()
            .id(("picker-row", number))
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(gpui::rgba(0))
            .hover(|s| {
                s.bg(theme::surface_hover())
                    .border_color(theme::border_hairline())
            })
            .child(
                div()
                    .flex_none()
                    .w(px(58.))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(if usable {
                        theme::accent_selected()
                    } else {
                        theme::destructive_bg()
                    })
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if usable {
                        theme::fg()
                    } else {
                        theme::destructive()
                    })
                    .child(item.short.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(theme::fg())
                                    .truncate()
                                    .child(item.label.clone()),
                            )
                            .child(pill(if item.placement == Placement::Overlay {
                                "On the prices"
                            } else {
                                "Own pane"
                            }))
                            .child(pill(if item.is_builtin() {
                                "Built in"
                            } else {
                                "Script"
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(if usable {
                                theme::muted_fg()
                            } else {
                                theme::destructive()
                            })
                            .truncate()
                            .child(if usable {
                                if item.description.is_empty() {
                                    item.category.clone()
                                } else {
                                    item.description.clone()
                                }
                            } else {
                                format!(
                                    "{} problem(s): open it in the editor to see them",
                                    item.problems
                                )
                            }),
                    ),
            )
            .children(script.map(|_| {
                div()
                    .id(("picker-edit", number))
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::surface_pressed()))
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new("Edit the script")
                            .build(window, cx)
                    })
                    .on_click(move |_, window, cx| {
                        let Source::Script(id) = edit_item.source.clone() else {
                            return;
                        };
                        edit_this.update(cx, |picker, cx| {
                            picker.request(EditorRequest::Edit(id), window, cx);
                        });
                    })
                    .child(icon_colored(IconName::Pencil, 14., theme::muted_fg()))
            }))
            .child(
                div()
                    .id(("picker-star", number))
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::surface_pressed()))
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(if favorite {
                            "Take the star away"
                        } else {
                            "Star it: it goes to Favorites"
                        })
                        .build(window, cx)
                    })
                    .on_click(move |_, _window, cx| {
                        let key = star_item.key();
                        indicators::update_prefs(cx, |prefs| prefs.toggle_favorite(&key));
                        star_this.update(cx, |_, cx| cx.notify());
                    })
                    .child(icon_colored(
                        IconName::Star,
                        15.,
                        if favorite {
                            theme::amber()
                        } else {
                            theme::muted_fg()
                        },
                    )),
            )
            .child(
                div()
                    .id(("picker-add", number))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .h(px(28.))
                    .px_2p5()
                    .rounded_md()
                    .border_1()
                    .border_color(theme::border_subtle())
                    .text_size(px(12.))
                    .text_color(if can_add {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    })
                    .when(can_add, |el| {
                        el.cursor_pointer()
                            .hover(|s| s.bg(theme::accent_selected()).border_color(theme::accent()))
                    })
                    .when(can_add, |el| {
                        el.on_click(move |_, _window, cx| {
                            add_this.update(cx, |picker, cx| picker.add(&add_item, cx));
                        })
                    })
                    .child(icon_colored(
                        IconName::Plus,
                        13.,
                        if usable {
                            theme::fg()
                        } else {
                            theme::muted_fg()
                        },
                    ))
                    .child(if full {
                        "Limit reached".to_owned()
                    } else if held > 0 {
                        format!("Add ({held} on chart)")
                    } else {
                        "Add".to_owned()
                    }),
            )
            .into_any_element()
    }

    /// Puts the indicator on the chart.
    fn add(&mut self, item: &Item, cx: &mut Context<Self>) {
        let study = item.config();
        self.chart
            .update(cx, |chart, cx| chart.add_study(study, cx));
        let key = item.key();
        indicators::update_prefs(cx, |prefs| prefs.note_recent(&key));
        cx.notify();
    }

    /// Closes the panel and asks for the editor.
    fn request(&mut self, request: EditorRequest, window: &mut Window, cx: &mut Context<Self>) {
        modal::close(window, cx);
        self.chart
            .update(cx, |_, cx| cx.emit(ChartEvent::IndicatorEditor(request)));
    }
}

fn pill(text: &'static str) -> gpui::Div {
    div()
        .flex_none()
        .px_1p5()
        .rounded_sm()
        .bg(theme::fg_alpha(0.06))
        .text_size(px(10.5))
        .text_color(theme::muted_fg())
        .child(text)
}

impl Render for Picker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let items = catalog::items();
        let categories = catalog::categories(&items);
        let query = self.search.read(cx).value().to_string();
        let prefs = indicators::prefs(cx);
        let on_chart = self.chart.read(cx).settings().studies.clone();
        let held = |item: &Item| {
            on_chart
                .iter()
                .filter(|s| match &item.source {
                    Source::Builtin(kind) => s.kind == *kind,
                    Source::Script(id) => s.script.as_deref() == Some(id.as_str()),
                })
                .count()
        };

        // The rail: the fixed pages, then one for each category.
        let mut tabs: Vec<Tab> = FIXED
            .iter()
            .map(|(label, icon)| Tab { label, icon: *icon })
            .collect();
        for category in &categories {
            tabs.push(Tab {
                label: intern::name(category),
                icon: category_icon(category),
            });
        }
        self.page = self.page.min(tabs.len() - 1);
        let shown = self.shown(&items, &categories, &query, cx);

        let mut list = div().id("picker-list").flex().flex_col().gap_0p5();
        if shown.is_empty() {
            list = list.child(ui::empty(
                IconName::SearchX,
                match (self.page, query.is_empty()) {
                    (1, true) => "Star an indicator with the star at its right: it stays here.",
                    (2, true) => "The indicators you add show here.",
                    (_, true) => "There is nothing in this list yet.",
                    _ => "No indicator matches this search.",
                },
            ));
        }
        for (index, item) in shown.iter().enumerate() {
            let favorite = prefs.is_favorite(&item.key());
            list = list.child(self.row(index, item, held(item), favorite, cx));
        }

        let head = Head {
            icon: IconName::ChartSpline,
            title: "Indicators".into(),
            subtitle: SharedString::from(format!(
                "{} to add, {} on this chart",
                items.len(),
                on_chart.len()
            )),
        };
        let this = cx.entity();
        let (folder, new, editor, done) = (this.clone(), this.clone(), this.clone(), this.clone());
        let footer = ui::footer(
            vec![
                ui::action(
                    "picker-folder",
                    "Open the folder",
                    Some(IconName::FolderOpen),
                    false,
                    move |_window, cx| indicators::open_folder(cx),
                )
                .into_any_element(),
                ui::action(
                    "picker-editor",
                    "Editor",
                    Some(IconName::CodeXml),
                    false,
                    move |window, cx| {
                        editor.update(cx, |p, cx| p.request(EditorRequest::Open, window, cx));
                    },
                )
                .into_any_element(),
                ui::action(
                    "picker-new",
                    "New script",
                    Some(IconName::FilePlus),
                    false,
                    move |window, cx| {
                        new.update(cx, |p, cx| p.request(EditorRequest::New, window, cx));
                    },
                )
                .into_any_element(),
            ],
            vec![
                ui::action("picker-done", "Done", None, true, move |window, cx| {
                    done.update(cx, |_, _| ());
                    modal::close(window, cx);
                })
                .into_any_element(),
            ],
        );
        let _ = folder;

        let body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(Input::new(&self.search).prefix(icon_colored(
                IconName::Search,
                14.,
                theme::muted_fg(),
            )))
            .child(list);
        ui::frame(
            head,
            &tabs,
            self.page,
            move |index, _window, cx| {
                this.update(cx, |picker, cx| {
                    picker.page = index;
                    cx.notify();
                });
            },
            modal::dismiss,
            body,
            footer,
        )
    }
}
