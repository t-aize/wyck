//! The About page of the settings: the version, the build and the updates.

use super::{
    AnyElement, BuildMode, Context, IconName, SettingsHub, VERSION, button, confirm, div, form, px,
    theme, tokens, updates,
};
use crate::app::diagnostics;
use gpui::prelude::*;

impl SettingsHub {
    pub(super) fn about_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let facts = [
            ("Version", VERSION.to_owned()),
            ("Build mode", BuildMode::CURRENT.label().to_owned()),
            ("License", "Apache License 2.0".to_owned()),
            ("Built with", "Rust and GPUI".to_owned()),
        ];
        let rows: Vec<AnyElement> = facts
            .into_iter()
            .map(|(label, value)| {
                form::field(
                    label,
                    None,
                    div()
                        .text_size(px(tokens::text::emphasis()))
                        .text_color(theme::fg())
                        .child(value),
                )
            })
            .collect();

        // Where this build keeps its state: a development build and the installed app do not
        // share it, so the page says which one this is.
        let keeps_apart = BuildMode::CURRENT == BuildMode::Development;
        let folder = crate::app_paths().map_or_else(
            || "Not available on this system".to_owned(),
            |paths| paths.state_dir().display().to_string(),
        );
        let mut place_rows: Vec<AnyElement> = [
            (
                "Settings folder",
                Some(if keeps_apart {
                    "A development build has its own folder, apart from the installed app. Settings > Folders moves it"
                } else {
                    "Everything the look and the charts remember, one readable file per kind of data. Settings > Folders moves it"
                }),
                folder,
            ),
            (
                "Credential store",
                Some("Where the sign-in tokens are kept, in the system keyring"),
                format!(
                    "Service {}",
                    crate::app::storage::identity::keyring_service()
                ),
            ),
        ]
        .into_iter()
        .map(|(label, help, value)| {
            form::field(
                label,
                help,
                div()
                    .max_w(px(wyck_ui_kit::tokens::measure::note()))
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::fg())
                    .child(value),
            )
        })
        .collect();

        // The log files are what a bug report needs, so the page says where they are and opens
        // the folder, and says so when the app wrote a report after a panic.
        if let Some(logs) = diagnostics::logs_dir() {
            let reports = diagnostics::report_count(&logs);
            place_rows.push(form::field(
                "Log folder",
                Some("The log of each day and, after a crash, an error report. Send them with a bug report"),
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .max_w(px(wyck_ui_kit::tokens::measure::note()))
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::fg())
                            .child(logs.display().to_string()),
                    )
                    .child(
                        div().flex().flex_row().child(button::action(
                            "about-open-logs",
                            "Open the log folder",
                            Some(IconName::FolderOpen),
                            false,
                            |_window, cx| diagnostics::open_logs_folder(cx),
                        )),
                    ),
            ));
            if reports > 0 {
                place_rows.push(form::field(
                    "Error reports",
                    None,
                    div()
                        .text_size(px(tokens::text::body()))
                        .text_color(theme::fg())
                        .child(match reports {
                            1 => "1 error report was saved in the log folder".to_owned(),
                            n => format!("{n} error reports were saved in the log folder"),
                        }),
                ));
            }
        }

        let update_state = updates::state(cx);
        let mut update_rows = vec![form::field(
            "Status",
            None,
            div()
                .text_size(px(tokens::text::emphasis()))
                .text_color(theme::fg())
                .child(update_state.status()),
        )];
        if let updates::UpdateState::Available {
            notes: Some(notes), ..
        } = &update_state
            && !notes.trim().is_empty()
        {
            update_rows.push(form::field(
                "Release notes",
                None,
                div()
                    .max_w(px(wyck_ui_kit::tokens::measure::note()))
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::muted_fg())
                    .child(notes.clone()),
            ));
        }

        let action = match update_state {
            updates::UpdateState::Disabled
            | updates::UpdateState::Checking
            | updates::UpdateState::Downloading { .. }
            | updates::UpdateState::Installing { .. } => None,
            updates::UpdateState::Available {
                automatic: true, ..
            } => {
                let multi = self.multi.clone();
                Some(
                    button::action(
                        "settings-update-install",
                        "Update and restart",
                        Some(IconName::Download),
                        true,
                        move |window, cx| {
                            let multi = multi.clone();
                            confirm::confirm(
                                window,
                                cx,
                                "Update and restart?",
                                "Wyck will close after saving pending workspace and appearance changes. A copy of everything you made is kept first (Settings > Data and backup), and the downloaded update is installed only after its signature is verified.",
                                move |_window, cx| {
                                    let multi = multi.clone();
                                    updates::install(move |cx| multi.read(cx).flush_documents(cx), cx);
                                },
                            );
                        },
                    )
                    .into_any_element(),
                )
            }
            updates::UpdateState::Available {
                automatic: false, ..
            } => Some(
                button::action(
                    "settings-update-open-release",
                    "Open download page",
                    Some(IconName::ExternalLink),
                    true,
                    |_window, cx| updates::open_releases(cx),
                )
                .into_any_element(),
            ),
            updates::UpdateState::Idle
            | updates::UpdateState::UpToDate
            | updates::UpdateState::Failed { .. } => Some(
                button::action(
                    "settings-update-check",
                    "Check again",
                    Some(IconName::RefreshCw),
                    false,
                    |_window, cx| updates::check(cx, Some(super::announce_update)),
                )
                .into_any_element(),
            ),
        };
        if let Some(action) = action {
            update_rows.push(form::field("Actions", None, action));
        }

        let brand = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(wyck_ui_kit::layout::brand_mark(56.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(tokens::text::display()))
                            .text_color(theme::fg())
                            .child("Wyck"),
                    )
                    .child(
                        div()
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::muted_fg())
                            .child("A trading terminal for cTrader"),
                    ),
            );
        form::page()
            .child(brand)
            .child(form::group(IconName::Info, "wyck", rows))
            .child(form::group(IconName::HardDrive, "Storage", place_rows))
            .child(form::group(IconName::Download, "Updates", update_rows))
            .child(form::note(
                "wyck is an independent project. It is not affiliated with, endorsed by, or sponsored by cTrader or Spotware Systems. Trading carries a high risk of loss, and nothing here is financial advice.",
            ))
            .into_any_element()
    }
}
