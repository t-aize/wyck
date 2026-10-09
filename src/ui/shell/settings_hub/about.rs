//! The About page of the settings: the version, the build and the updates.

use super::{
    AnyElement, BuildMode, Context, IconName, SettingsHub, VERSION, button, confirm, div, form, px,
    theme, tokens, updates,
};
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
                    .max_w(px(440.))
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
                                "Wyck will close after saving pending workspace and appearance changes. The downloaded update is installed only after its signature is verified.",
                                move |_window, cx| updates::install(multi.clone(), cx),
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
                    |_window, cx| updates::check(cx, true),
                )
                .into_any_element(),
            ),
        };
        if let Some(action) = action {
            update_rows.push(form::field("Actions", None, action));
        }

        form::page()
            .child(form::group(IconName::Info, "wyck", rows))
            .child(form::group(IconName::Download, "Updates", update_rows))
            .child(form::note(
                "wyck is an independent project. It is not affiliated with, endorsed by, or sponsored by cTrader or Spotware Systems. Trading carries a high risk of loss, and nothing here is financial advice.",
            ))
            .into_any_element()
    }
}
