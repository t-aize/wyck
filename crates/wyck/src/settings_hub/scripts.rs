//! The Indicators page of the settings: the scripts and their limits.

use super::*;

impl SettingsHub {
    pub(super) fn indicators_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefs = indicators::prefs(cx);
        let dir = indicators::dir(cx);
        let default_dir = indicators::default_dir(cx);
        let using_default = prefs.folder.is_none();
        let entries = wyck_chart::study::custom::library::registry::all();
        let broken = entries.iter().filter(|e| !e.is_ready()).count();

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
                                "The default folder, inside the settings folder."
                            } else {
                                "A folder you chose."
                            }),
                    ),
            ),
            form::field(
                "Change the folder",
                Some("Every .rhai file in it (and in the folders inside it) is an indicator"),
                div()
                    .flex()
                    .flex_row()
                    .gap_1p5()
                    .child(button::action(
                        "indicators-choose",
                        "Choose...",
                        Some(IconName::FolderOpen),
                        false,
                        move |_window, cx| {
                            let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
                                files: false,
                                directories: true,
                                multiple: false,
                                prompt: Some("Choose the folder of your indicators".into()),
                            });
                            cx.spawn(async move |cx| {
                                let Ok(Ok(Some(paths))) = picked.await else {
                                    return;
                                };
                                if let Some(path) = paths.into_iter().next() {
                                    cx.update(|cx| {
                                        indicators::update_prefs(cx, |p| {
                                            p.folder = Some(path.display().to_string());
                                        });
                                    });
                                }
                            })
                            .detach();
                        },
                    ))
                    .child(
                        button::action(
                            "indicators-default",
                            "Default",
                            Some(IconName::RotateCcw),
                            false,
                            move |_window, cx| indicators::update_prefs(cx, |p| p.folder = None),
                        )
                        .disabled(using_default),
                    )
                    .child(button::action(
                        "indicators-open",
                        "Open",
                        None,
                        false,
                        move |_window, cx| indicators::open_folder(cx),
                    )),
            ),
            form::field(
                "Its default place",
                None,
                div()
                    .max_w(px(360.))
                    .truncate()
                    .text_size(px(tokens::text::small()))
                    .text_color(theme::muted_fg())
                    .child(default_dir.display().to_string()),
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
                        indicators::update_prefs(cx, |p| p.budget = prefs::Budget::ALL[choice]);
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
                "Open the editor from the button in the header (Ctrl+Shift+E). Export a script from the editor to share it.",
            ))
            .into_any_element()
    }
}
