//! The Indicators page of the settings: the scripts and their limits.

use super::{
    AnyElement, Context, IconName, SettingsHub, button, controls, div, form, indicators, prefs, px,
    theme, tokens,
};
use crate::kit::prelude::Disableable;
use gpui::prelude::*;

impl SettingsHub {
    pub(super) fn indicators_page(cx: &mut Context<Self>) -> AnyElement {
        let prefs = indicators::prefs(cx);
        let dir = indicators::dir(cx);
        let default_dir = indicators::default_dir(cx);
        let using_default = dir == default_dir;
        let entries = wyck_domain::indicators::custom::library::registry::all();
        let broken = entries.iter().filter(|e| !e.is_ready()).count();

        // Where the scripts are kept is chosen with the other folders, on the Folders page.
        let hub = cx.entity();
        let folder = vec![
            form::block(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(tokens::text::emphasis()))
                            .text_color(theme::fg())
                            .child("Indicators folder"),
                    )
                    .child(
                        div()
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::muted_fg())
                            .child(dir.display().to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(tokens::text::small()))
                            .text_color(theme::muted_fg())
                            .child(if using_default {
                                "The standard folder. Every .rhai file in it (and in the folders inside it) is an indicator."
                            } else {
                                "A folder you chose. Every .rhai file in it (and in the folders inside it) is an indicator."
                            }),
                    ),
            ),
            form::field(
                "Change the folder",
                Some("With the other folders of Wyck"),
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(button::action(
                        "indicators-folders",
                        "Folders...",
                        Some(IconName::FolderOpen),
                        false,
                        move |_window, cx| {
                            hub.update(cx, |hub, cx| {
                                hub.page = super::Page::Folders;
                                cx.notify();
                            });
                        },
                    ))
                    .child(button::action(
                        "indicators-open",
                        "Open",
                        None,
                        false,
                        move |_window, cx| indicators::open_folder(cx),
                    )),
            ),
        ];

        let scripts = vec![
            form::field(
                "Scripts found",
                Some(if broken == 0 {
                    "All of them work"
                } else {
                    "The ones with problems are marked in the editor and cannot be added"
                }),
                div()
                    .text_size(px(tokens::text::emphasis()))
                    .text_color(if broken == 0 {
                        theme::fg()
                    } else {
                        theme::destructive()
                    })
                    .child(if broken == 0 {
                        format!("{}", entries.len())
                    } else {
                        format!("{} ({broken} with problems)", entries.len())
                    }),
            ),
            form::field(
                "Read the folder now",
                Some("It is also read every moment on its own, when that is turned on"),
                button::action(
                    "indicators-reload",
                    "Read again",
                    Some(IconName::RefreshCw),
                    false,
                    move |_window, cx| indicators::reload(cx).detach(),
                ),
            ),
        ];

        let budgets: Vec<&str> = prefs::Budget::ALL.iter().map(|b| b.label()).collect();
        let budget_index = prefs::Budget::ALL
            .iter()
            .position(|b| *b == prefs.budget)
            .unwrap_or(1);
        let behavior = vec![
            form::field(
                "Read the folder on its own",
                Some(
                    "A file edited in another program shows up, and the charts that hold it update",
                ),
                controls::toggle("indicators-auto", prefs.auto_reload, move |on, _w, cx| {
                    indicators::update_prefs(cx, |p| p.auto_reload = on);
                }),
            ),
            form::field(
                "What a script may do",
                Some(
                    "A script over the limit is stopped. Light is for many charts, Heavy for scripts that loop over the bars",
                ),
                controls::segmented(
                    "indicators-budget",
                    &budgets,
                    budget_index,
                    move |choice, _window, cx| {
                        if let Some(&budget) = prefs::Budget::ALL.get(choice) {
                            indicators::update_prefs(cx, |p| p.budget = budget);
                        }
                    },
                ),
            ),
        ];

        let favorites = vec![form::field(
            "Starred indicators",
            Some("The stars in the list of indicators"),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_size(px(tokens::text::emphasis()))
                        .text_color(theme::fg())
                        .child(prefs.favorites.len().to_string()),
                )
                .child(
                    button::action(
                        "indicators-clear-favorites",
                        "Clear",
                        None,
                        false,
                        move |_window, cx| indicators::update_prefs(cx, |p| p.favorites.clear()),
                    )
                    .disabled(prefs.favorites.is_empty()),
                ),
        )];

        form::page()
            .child(form::group(IconName::FolderOpen, "Folder", folder))
            .child(form::group(IconName::CodeXml, "Scripts", scripts))
            .child(form::group(IconName::SlidersHorizontal, "Behavior", behavior))
            .child(form::group(IconName::Star, "Favorites", favorites))
            .child(form::note(
                format!(
                    "Open the editor from the button in the header{}. Export a script from the editor to share it.",
                    crate::kit::hints::suffix("toggle_indicator_editor")
                ),
            ))
            .into_any_element()
    }
}
