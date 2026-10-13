//! The Folders page of the settings: where each kind of data is kept, moving it to another folder
//! (a cloud folder that keeps several computers in step, say), and what a move left behind. The
//! work itself is `crate::app::locations`; this page asks for it and says what happened.
//!
//! A move waits for the next start, as an imported backup does: the documents are held in memory
//! and saved to the folder they were read from, so nothing is moved under a running app.

use super::{
    AnyElement, Context, IconName, SettingsHub, appearance, confirm, div, form, icon, px, theme,
    tokens,
};
use crate::app::locations::{self, Row};
use crate::app::storage::locations::Kind;
use crate::app::storage::relocate::{PlanError, Step};
use crate::ui::kit::prelude::Disableable;
use gpui::prelude::*;

/// A size in bytes as a few characters.
fn size(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.0} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

impl SettingsHub {
    pub(super) fn folders_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(overview) = &self.folders else {
            return form::page()
                .child(form::empty(
                    IconName::FolderOpen,
                    "The system gives Wyck no folder to keep its data in.",
                ))
                .into_any_element();
        };
        let development = overview.mode == crate::app::system::build_info::BuildMode::Development;

        let mut rows: Vec<AnyElement> = vec![form::block(form::note(if development {
            "This is a development build. It keeps its data apart from the installed app, in folders of its own, and it refuses any folder the installed app made."
        } else {
            "This is the installed app. A development build keeps its own folders and never uses these."
        }))];
        for (index, row) in overview.rows.iter().enumerate() {
            rows.extend(self.folder_row(index, row, cx));
        }
        if let Some(error) = &overview.pointer_error {
            rows.push(form::block(
                div()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::amber())
                    .child(format!(
                        "The file that remembers your folders could not be read, so every folder is in its standard place: {error}"
                    )),
            ));
        }
        let folders_group = form::group(IconName::FolderOpen, "Where your data is kept", rows);

        let mut cloud_rows: Vec<AnyElement> = vec![form::block(form::note(format!(
            "Keep your settings, drawings, alerts, watchlists, indicators and backups in a folder that OneDrive or another service keeps in step between your computers. Wyck makes a folder called {} in it, with one folder per kind of data. The logs stay on this computer, and so do your keys and the list of accounts you signed in with: nothing that signs in goes into a synced folder.",
            crate::app::storage::locations::cloud_folder_name(overview.mode)
        )))];
        let mut buttons = div().flex().flex_row().flex_wrap().gap_2();
        for (index, folder) in overview.cloud.iter().enumerate() {
            let (this, path) = (cx.entity(), folder.path.clone());
            buttons = buttons.child(
                crate::ui::kit::button::accent(("folders-cloud", index))
                    .icon(IconName::RefreshCw)
                    .label(format!("Use {}", folder.name))
                    .on_click(move |_, _window, cx| {
                        let path = path.clone();
                        this.update(cx, |hub, cx| {
                            hub.stage_result(locations::stage_cloud(&path), cx);
                        });
                    }),
            );
        }
        let this = cx.entity();
        buttons = buttons.child(
            crate::ui::kit::button::quiet("folders-cloud-choose")
                .icon(IconName::FolderOpen)
                .label("Choose another folder...")
                .on_click(move |_, _window, cx| {
                    this.update(cx, |hub, cx| hub.choose_cloud_root(cx));
                }),
        );
        cloud_rows.push(form::block(buttons));
        if overview.cloud.is_empty() {
            cloud_rows.push(form::block(form::note(
                "No OneDrive, Dropbox or similar folder was found on this computer. Choose the folder your service keeps in step.",
            )));
        }
        cloud_rows.push(form::block(
            div()
                .text_size(px(tokens::text::body()))
                .text_color(theme::amber())
                .child("Use Wyck on one computer at a time. A sync service cannot merge two changes of the same file: the last one saved wins, and the other is kept as a copy that Wyck does not read."),
        ));
        let cloud_group = form::group(
            IconName::RefreshCw,
            "Keep several computers in step",
            cloud_rows,
        );

        let mut page = form::page().child(folders_group).child(cloud_group);

        if let Some(lines) = &self.folders_wait {
            let (restart, cancel) = (cx.entity(), cx.entity());
            let mut waiting = div().flex().flex_col().gap_1();
            waiting = waiting.child(
                div()
                    .text_size(px(tokens::text::emphasis()))
                    .text_color(theme::amber())
                    .child("This waits for Wyck to start again:"),
            );
            for line in lines {
                waiting = waiting.child(
                    div()
                        .text_size(px(tokens::text::body()))
                        .text_color(theme::muted_fg())
                        .child(format!("- {line}")),
                );
            }
            waiting = waiting.child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .pt_2()
                    .child(
                        crate::ui::kit::button::accent("folders-restart")
                            .icon(IconName::RefreshCw)
                            .label("Restart now")
                            .on_click(move |_, _window, cx| {
                                restart.update(cx, |hub, cx| hub.restart_for_folders(cx));
                            }),
                    )
                    .child(
                        crate::ui::kit::button::quiet("folders-cancel")
                            .label("Cancel the move")
                            .on_click(move |_, _window, cx| {
                                cancel.update(cx, |hub, cx| hub.cancel_folders(cx));
                            }),
                    ),
            );
            page = page.child(form::group(
                IconName::TriangleAlert,
                "Waiting",
                vec![form::block(waiting)],
            ));
        }

        if let Some(notice) = &self.folders_notice {
            page = page.child(form::block(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(icon::small(
                        if notice.ok {
                            IconName::CircleCheck
                        } else {
                            IconName::Info
                        },
                        if notice.ok {
                            theme::emerald()
                        } else {
                            theme::destructive()
                        },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::fg())
                            .child(notice.text.clone()),
                    ),
            ));
        }

        if !overview.conflicts.is_empty() {
            let mut rows: Vec<AnyElement> = vec![form::block(form::note(
                "Your sync service made these when two computers changed the same file. Wyck reads only the original, so what is in a copy is not used. Open the folder to compare them by hand, then delete the copy.",
            ))];
            for path in &overview.conflicts {
                let name = path
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                rows.push(form::block(
                    div()
                        .text_size(px(tokens::text::body()))
                        .text_color(theme::fg())
                        .child(name),
                ));
            }
            let (this, folder) = (cx.entity(), self.settings_folder());
            rows.push(form::block(
                crate::ui::kit::button::quiet("folders-conflicts-open")
                    .icon(IconName::FolderOpen)
                    .label("Open the settings folder")
                    .on_click(move |_, _window, cx| {
                        this.update(cx, |_, _| {});
                        if let Some(folder) = &folder {
                            locations::reveal(folder, cx);
                        }
                    }),
            ));
            page = page.child(form::group(
                IconName::TriangleAlert,
                "Copies made by your sync service",
                rows,
            ));
        }

        if !overview.leftovers.is_empty() {
            let mut rows: Vec<AnyElement> = vec![form::block(form::note(
                "A move keeps the folder it came from, renamed, so nothing is lost if you change your mind. Delete them once you have checked the new place.",
            ))];
            for (index, left) in overview.leftovers.iter().enumerate() {
                let (this, path, open) = (cx.entity(), left.path.clone(), left.path.clone());
                let title = left
                    .path
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                rows.push(
                    form::Row::new(title)
                        .hint(format!(
                            "{} - {} file(s), {}",
                            left.path.display(),
                            left.files,
                            size(left.bytes)
                        ))
                        .control(
                            div()
                                .flex()
                                .flex_row()
                                .gap_1()
                                .child(
                                    crate::ui::kit::button::quiet(("folders-left-open", index))
                                        .icon(IconName::FolderOpen)
                                        .label("Open")
                                        .on_click(move |_, _window, cx| {
                                            locations::reveal(&open, cx);
                                        }),
                                )
                                .child(
                                    crate::ui::kit::button::danger(("folders-left-delete", index))
                                        .icon(IconName::Trash)
                                        .label("Delete")
                                        .on_click(move |_, window, cx| {
                                            let (this, path) = (this.clone(), path.clone());
                                            confirm::confirm_as(
                                                window,
                                                cx,
                                                confirm::Tone::Danger,
                                                "Delete this folder?",
                                                "The folder and everything in it is deleted for good. Do this once the new place has been checked.",
                                                move |_window, cx| {
                                                    let path = path.clone();
                                                    this.update(cx, |hub, cx| {
                                                        hub.delete_left_behind(&path, cx);
                                                    });
                                                },
                                            );
                                        }),
                                ),
                        ),
                );
            }
            page = page.child(form::group(
                IconName::Archive,
                "Folders left by a move",
                rows,
            ));
        }

        page.child(form::note(
            "The keys that sign in to your broker stay in the secure storage of this computer, and so does the list of accounts you signed in with. Backups hold no keys.",
        ))
        .into_any_element()
    }

    /// The row of one kind: where it is, and what can be done about it.
    fn folder_row(&self, index: usize, row: &Row, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let kind = row.kind;
        let (choose, reset, open) = (cx.entity(), cx.entity(), cx.entity());
        let place = if row.custom {
            "your folder"
        } else {
            "standard folder"
        };
        let mut shown = vec![
            form::Row::new(kind.label())
                .hint(format!("{}  ({place})", row.path.display()))
                .help(kind.help())
                .control(
                    div()
                        .flex()
                        .flex_row()
                        .gap_1()
                        .child(
                            crate::ui::kit::button::quiet(("folders-change", index))
                                .icon(IconName::FolderOpen)
                                .label("Change...")
                                .on_click(move |_, _window, cx| {
                                    choose.update(cx, |hub, cx| hub.choose_folder(kind, cx));
                                }),
                        )
                        .child(
                            crate::ui::kit::button::quiet(("folders-standard", index))
                                .icon(IconName::RotateCcw)
                                .label("Standard")
                                .disabled(!row.custom && row.unavailable.is_none())
                                .on_click(move |_, _window, cx| {
                                    reset.update(cx, |hub, cx| hub.reset_folder(kind, cx));
                                }),
                        )
                        .child(
                            crate::ui::kit::button::quiet(("folders-open", index))
                                .icon(IconName::ExternalLink)
                                .label("Open")
                                .on_click(move |_, _window, cx| {
                                    open.update(cx, |_, _| {});
                                    locations::open_folder(kind, cx);
                                }),
                        ),
                ),
        ];
        if let Some(problem) = &row.unavailable {
            shown.push(form::block(
                div()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::amber())
                    .child(format!(
                        "Not used now: {problem}. The standard folder is used until it can be, and what you change meanwhile stays on this computer."
                    )),
            ));
        }
        shown
    }

    /// The folder of the settings, for showing it.
    fn settings_folder(&self) -> Option<std::path::PathBuf> {
        self.folders
            .as_ref()
            .and_then(|o| o.rows.iter().find(|r| r.kind == Kind::Settings))
            .map(|row| row.path.clone())
    }

    /// Reads the folders again from the disk.
    pub(super) fn refresh_folders(&mut self) {
        self.folders = locations::overview();
        self.folders_wait = self.folders.as_ref().and_then(|o| o.waiting.clone());
    }

    fn say_folders(&mut self, ok: bool, text: impl Into<String>, cx: &mut Context<Self>) {
        self.folders_notice = Some(super::Notice {
            ok,
            text: text.into(),
        });
        cx.notify();
    }

    /// Records the answer to a request for a move: what waits, or why it cannot be.
    fn stage_result(&mut self, result: Result<Vec<Step>, PlanError>, cx: &mut Context<Self>) {
        match result {
            Ok(steps) if steps.is_empty() => {
                self.say_folders(true, "Nothing to change: it is already there.", cx);
            }
            Ok(steps) => {
                self.refresh_folders();
                self.folders_wait = Some(locations::describe(&steps));
                self.say_folders(true, "The move waits for Wyck to start again.", cx);
            }
            Err(error) => self.say_folders(false, error.to_string(), cx),
        }
    }

    /// Asks for a folder, then puts the move of `kind` there aside.
    fn choose_folder(&mut self, kind: Kind, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(format!("Choose the folder for: {}", kind.label()).into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |hub, cx| {
                hub.stage_result(locations::stage_folder(kind, Some(&path)), cx);
            });
        })
        .detach();
    }

    /// Asks for the folder a cloud service keeps in step, then puts the move of everything aside.
    fn choose_cloud_root(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose the folder your cloud service keeps in step".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |hub, cx| {
                hub.stage_result(locations::stage_cloud(&path), cx);
            });
        })
        .detach();
    }

    /// Puts the move of `kind` back to its standard folder aside.
    fn reset_folder(&mut self, kind: Kind, cx: &mut Context<Self>) {
        self.stage_result(locations::stage_folder(kind, None), cx);
    }

    fn cancel_folders(&mut self, cx: &mut Context<Self>) {
        locations::cancel_staged();
        self.refresh_folders();
        self.say_folders(true, "The move is cancelled. Nothing was changed.", cx);
    }

    /// Writes what waits to be saved, so the copy is what is on screen, and restarts: the move is
    /// made before anything is read.
    fn restart_for_folders(&mut self, cx: &mut Context<Self>) {
        self.multi.read(cx).flush_documents(cx);
        appearance::save_now(cx);
        cx.restart();
    }

    fn delete_left_behind(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        match locations::delete_leftover(path) {
            Ok(()) => {
                self.refresh_folders();
                self.say_folders(true, "The folder is deleted.", cx);
            }
            Err(error) => self.say_folders(false, error.to_string(), cx),
        }
    }
}
