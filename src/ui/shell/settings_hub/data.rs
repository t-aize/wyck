//! The Data page of the settings: exporting and importing a backup of everything the user made, and
//! the copies the app keeps of it. The work itself is done by `crate::infra::storage::backup`; this page asks
//! for it and says what happened.

use super::*;
use crate::infra::storage::backup::{BackupStore, Contents};
use crate::infra::storage::scripts::ScriptStore;

/// The most copies the page lists.
const LISTED_COPIES: usize = 8;

/// What a backup holds, in words a person reads: which kinds of things, and how many accounts.
fn describe(contents: &Contents) -> Vec<String> {
    let mut lines = Vec::new();
    if contents.has_global("appearance") {
        lines.push("Appearance: themes, colors, font".to_owned());
    }
    if contents.has_global("preferences") {
        lines.push("Layout, charts and their indicators, favorites, ticket settings".to_owned());
    }
    for (name, label) in [
        ("drawings", "Drawings and saved looks"),
        ("watchlists", "Watchlists"),
        ("alerts", "Price alerts"),
    ] {
        let accounts = contents.scopes_with(name);
        if accounts > 0 {
            lines.push(format!("{label} ({accounts} account(s))"));
        }
    }
    let known = [
        "appearance",
        "preferences",
        "drawings",
        "watchlists",
        "alerts",
    ];
    let others = contents
        .global
        .iter()
        .chain(contents.scopes.values().flatten())
        .filter(|name| !known.contains(&name.as_str()))
        .count();
    if others > 0 {
        lines.push(format!("{others} other document(s)"));
    }
    if contents.scripts > 0 {
        lines.push(format!("Indicator scripts ({})", contents.scripts));
    }
    if lines.is_empty() {
        lines.push("Nothing: the backup is empty".to_owned());
    } else if !contents.scopes.is_empty() {
        let accounts: Vec<&str> = contents.scopes.keys().map(String::as_str).collect();
        lines.push(format!("Accounts: {}", accounts.join(", ")));
    }
    lines
}

/// The copies in the backups folder, newest first.
pub(super) fn list_copies() -> Vec<BackupEntry> {
    let Some(paths) = crate::app_paths() else {
        return Vec::new();
    };
    paths.backups().list().unwrap_or_else(|error| {
        tracing::warn!(%error, "could not list the saved backups");
        Vec::new()
    })
}

/// A size in bytes as a few characters.
fn size(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.0} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

/// When a copy was made, in the time of the user.
fn when(entry: &BackupEntry) -> String {
    entry.modified.map_or_else(
        || "unknown date".to_owned(),
        |time| {
            chrono::DateTime::<chrono::Local>::from(time)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        },
    )
}

impl SettingsHub {
    pub(super) fn data_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let (export, import, cancel, restart, open_folder) = (
            cx.entity(),
            cx.entity(),
            cx.entity(),
            cx.entity(),
            cx.entity(),
        );
        let dir = crate::app_paths();
        let mut rows: Vec<AnyElement> = vec![
            form::field(
                "Export everything",
                Some(
                    "One file: your look, charts and indicators, drawings and their saved looks, favorites, watchlists and alerts. No sign-in is in it",
                ),
                Button::new("backup-export")
                    .cursor_pointer()
                    .primary()
                    .small()
                    .icon(IconName::FileDown)
                    .label("Export")
                    .on_click(move |_, window, cx| {
                        export.update(cx, |e, cx| e.export(window, cx));
                    }),
            ),
            form::field(
                "Import a backup",
                Some(
                    "Checked first, then applied when wyck starts again. What it replaces is saved first as a copy you can restore",
                ),
                Button::new("backup-import")
                    .cursor_pointer()
                    .ghost()
                    .small()
                    .icon(IconName::FileUp)
                    .label("Import")
                    .on_click(move |_, window, cx| {
                        import.update(cx, |e, cx| e.import(window, cx));
                    }),
            ),
        ];
        if let Some(lines) = &self.waiting {
            let mut waiting = div().flex().flex_col().gap_1();
            waiting = waiting.child(
                div()
                    .text_size(px(tokens::text::emphasis()))
                    .text_color(theme::amber())
                    .child("A backup is waiting to be applied when wyck starts again:"),
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
                    .pt_1p5()
                    .child(
                        Button::new("backup-restart")
                            .cursor_pointer()
                            .primary()
                            .small()
                            .icon(IconName::RefreshCw)
                            .label("Restart now")
                            .on_click(move |_, _window, cx| {
                                restart.update(cx, |_, _| {});
                                cx.restart();
                            }),
                    )
                    .child(
                        Button::new("backup-cancel")
                            .cursor_pointer()
                            .ghost()
                            .small()
                            .label("Cancel the import")
                            .on_click(move |_, _window, cx| {
                                cancel.update(cx, |e, cx| e.cancel_import(cx));
                            }),
                    ),
            );
            rows.push(form::block(waiting));
        }
        if let Some(notice) = &self.notice {
            rows.push(form::block(
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
        let backup_group = form::group(IconName::Archive, "Backup", rows);

        let mut copy_rows: Vec<AnyElement> = vec![form::field(
            "Save a copy now",
            Some(
                "Kept in the backups folder, next to the automatic ones (one a day, the last seven)",
            ),
            Button::new("backup-save-copy")
                .cursor_pointer()
                .ghost()
                .small()
                .icon(IconName::Archive)
                .label("Save a copy")
                .on_click({
                    let this = cx.entity();
                    move |_, _window, cx| this.update(cx, |hub, cx| hub.save_copy(cx))
                }),
        )];
        for (index, entry) in self.copies.iter().take(LISTED_COPIES).enumerate() {
            let (restore, delete, id) = (cx.entity(), cx.entity(), entry.id.clone());
            let delete_id = id.clone();
            let title = format!("{} - {}", entry.kind.label(), when(entry));
            let hint = format!(
                "{}{}",
                size(entry.bytes),
                if entry.sealed {
                    ", locked with a passphrase"
                } else {
                    ""
                }
            );
            copy_rows.push(
                form::Row::new(title).hint(hint).control(
                    div()
                        .flex()
                        .flex_row()
                        .gap_1()
                        .child(
                            Button::new(("backup-restore", index))
                                .cursor_pointer()
                                .ghost()
                                .small()
                                .icon(IconName::RotateCcw)
                                .label("Restore")
                                .disabled(entry.sealed)
                                .on_click(move |_, _window, cx| {
                                    let id = id.clone();
                                    restore.update(cx, |hub, cx| hub.restore_copy(&id, cx));
                                }),
                        )
                        .child(
                            Button::new(("backup-delete", index))
                                .cursor_pointer()
                                .ghost()
                                .small()
                                .icon(IconName::Trash)
                                .on_click(move |_, _window, cx| {
                                    let id = delete_id.clone();
                                    delete.update(cx, |hub, cx| hub.delete_copy(&id, cx));
                                }),
                        ),
                ),
            );
        }
        if self.copies.len() > LISTED_COPIES {
            copy_rows.push(form::block(form::note(format!(
                "{} older copies are in the backups folder",
                self.copies.len() - LISTED_COPIES
            ))));
        }
        let copies_group = form::group(IconName::HardDrive, "Saved copies", copy_rows);

        let mut place: Vec<AnyElement> = vec![form::field(
            "Settings folder",
            Some("Where everything is kept, one readable file per kind of data"),
            Button::new("backup-folder")
                .cursor_pointer()
                .ghost()
                .small()
                .icon(IconName::FolderOpen)
                .label("Open")
                .disabled(dir.is_none())
                .on_click(move |_, _window, cx| {
                    open_folder.update(cx, |_, _| {});
                    if let Some(paths) = crate::app_paths() {
                        cx.reveal_path(paths.config_dir());
                    }
                }),
        )];
        if let Some(dir) = &dir {
            place.push(form::block(form::note(
                dir.config_dir().display().to_string(),
            )));
            let dir = dir.backups().dir();
            place.push(form::field(
                "Backups folder",
                Some("Every saved copy, one readable file each"),
                Button::new("backup-copies-folder")
                    .cursor_pointer()
                    .ghost()
                    .small()
                    .icon(IconName::FolderOpen)
                    .label("Open")
                    .on_click(move |_, _window, cx| {
                        let _ = std::fs::create_dir_all(&dir);
                        cx.reveal_path(&dir);
                    }),
            ));
        }

        let reset = cx.entity();
        let danger_group = form::group(
            IconName::TriangleAlert,
            "Danger zone",
            vec![form::field(
                "Reset everything",
                Some(
                    "Appearance, layout, charts and their indicators, favorites, ticket settings, drawings and their saved looks, watchlists and alerts: all of it goes back to how it is on a fresh install, for every account. Your indicator scripts and your sign-in are not touched",
                ),
                Button::new("settings-reset-all")
                    .cursor_pointer()
                    .danger()
                    .small()
                    .icon(IconName::TriangleAlert)
                    .label("Reset everything")
                    .on_click(move |_, window, cx| {
                        let reset = reset.clone();
                        confirm::confirm(
                            window,
                            cx,
                            "Reset everything?",
                            "This puts the appearance, layout, charts, drawings, watchlists, favorites and alerts of every account back to how they are on a fresh install. A copy of what is removed is kept in the backups folder. Wyck restarts right after. Your indicator scripts and your sign-in stay exactly as they are.",
                            move |_window, cx| {
                                reset.update(cx, |hub, cx| hub.reset_all(cx));
                            },
                        );
                    }),
            )],
        );

        form::page()
            .child(backup_group)
            .child(copies_group)
            .child(form::group(IconName::FolderOpen, "Where it is", place))
            .child(form::note(
                "The keys that sign in to your broker are not in a backup: they stay in the system's secure storage.",
            ))
            .child(danger_group)
            .into_any_element()
    }

    /// Asks where to save, then writes the backup there.
    pub(super) fn export(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = crate::app_paths() else {
            self.say(false, "The settings folder could not be found.", cx);
            return;
        };
        // What waits to be saved goes to the disk first, so the backup is what is on screen.
        self.multi.read(cx).flush_documents(cx);
        appearance::save_now(cx);
        let stamp = chrono::Local::now().format("%Y-%m-%d").to_string();
        let name = format!("wyck-backup-{stamp}.toml");
        let start = crate::infra::storage::AppPaths::documents_dir();
        let scripts_dir = indicators::dir(cx);
        let picked = cx.prompt_for_new_path(&start, Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else {
                return;
            };
            let created = chrono::Local::now().to_rfc3339();
            let scripts = ScriptStore::new(scripts_dir);
            let result =
                backup::export_to_file(dir, Some(&scripts), &path, None, VERSION, &created);
            let _ = this.update(cx, |this, cx| match result {
                Ok(b) => {
                    let count = b.files.len() + b.scripts.len();
                    this.say(
                        true,
                        format!("Saved {count} item(s) to {}.", path.display()),
                        cx,
                    );
                }
                Err(error) => this.say(false, error.to_string(), cx),
            });
        })
        .detach();
    }

    /// Asks for a backup, checks it, and sets it aside for the next start.
    pub(super) fn import(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = crate::app_paths() else {
            self.say(false, "The settings folder could not be found.", cx);
            return;
        };
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a wyck backup".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let result = backup::stage_import_file(dir, &path, None);
            let _ = this.update(cx, |this, cx| match result {
                Ok(b) => this.staged(&b, cx),
                Err(error) => this.say(false, error.to_string(), cx),
            });
        })
        .detach();
    }

    pub(super) fn cancel_import(&mut self, cx: &mut Context<Self>) {
        if let Some(dir) = crate::app_paths() {
            let _ = backup::cancel_import(dir);
        }
        self.waiting = None;
        self.say(true, "The import is cancelled. Nothing was changed.", cx);
    }

    /// Stages a full reset and restarts right away, so it applies before anything is loaded.
    pub(super) fn reset_all(&mut self, cx: &mut Context<Self>) {
        let Some(dir) = crate::app_paths() else {
            self.say(false, "The settings folder could not be found.", cx);
            return;
        };
        if let Err(error) = backup::stage_reset(dir) {
            self.say(false, error.to_string(), cx);
            return;
        }
        self.waiting = None;
        cx.restart();
    }

    /// Remembers what waits for the next start, and says so.
    fn staged(&mut self, backup: &backup::Backup, cx: &mut Context<Self>) {
        let lines = describe(&backup.contents());
        cx.set_global(PendingSummary(lines.clone()));
        self.waiting = Some(lines);
        self.say(
            true,
            "The backup is valid and waits for the next start.",
            cx,
        );
    }

    /// Saves a copy of everything in the backups folder.
    pub(super) fn save_copy(&mut self, cx: &mut Context<Self>) {
        let Some(paths) = crate::app_paths() else {
            self.say(false, "The settings folder could not be found.", cx);
            return;
        };
        // What waits to be saved goes to the disk first, so the copy is what is on screen.
        self.multi.read(cx).flush_documents(cx);
        appearance::save_now(cx);
        let scripts = ScriptStore::new(indicators::dir(cx));
        let created = chrono::Local::now().to_rfc3339();
        let store: BackupStore = paths.backups();
        let result = backup::Backup::collect(paths, Some(&scripts), VERSION, &created)
            .and_then(|b| store.create(&b));
        match result {
            Ok(entry) => {
                self.copies = list_copies();
                self.say(true, format!("Saved a copy: {}.", entry.path.display()), cx);
            }
            Err(error) => self.say(false, error.to_string(), cx),
        }
    }

    /// Asks for a saved copy to come back at the next start.
    pub(super) fn restore_copy(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(paths) = crate::app_paths() else {
            self.say(false, "The settings folder could not be found.", cx);
            return;
        };
        match paths.backups().restore(id, None) {
            Ok(b) => self.staged(&b, cx),
            Err(error) => self.say(false, error.to_string(), cx),
        }
    }

    /// Deletes a saved copy.
    pub(super) fn delete_copy(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(paths) = crate::app_paths() else {
            return;
        };
        match paths.backups().remove(id) {
            Ok(()) => {
                self.copies = list_copies();
                self.say(true, "The copy is deleted.", cx);
            }
            Err(error) => self.say(false, error.to_string(), cx),
        }
    }
}
