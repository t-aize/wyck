//! The Data page of the settings: exporting and importing a backup of everything the user made.

use super::*;

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
                    "Checked first, then applied when wyck starts again. What it replaces is kept in a folder of backups",
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
                    .text_size(px(tokens::text::EMPHASIS))
                    .text_color(theme::amber())
                    .child("A backup is waiting to be applied when wyck starts again:"),
            );
            for line in lines {
                waiting = waiting.child(
                    div()
                        .text_size(px(tokens::text::BODY))
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
                            .text_size(px(tokens::text::BODY))
                            .text_color(theme::fg())
                            .child(notice.text.clone()),
                    ),
            ));
        }
        let backup_group = form::group(IconName::Archive, "Backup", rows);

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
                            "This puts the appearance, layout, charts, drawings, watchlists, favorites and alerts of every account back to how they are on a fresh install. It cannot be undone. Wyck restarts right after. Your indicator scripts and your sign-in stay exactly as they are.",
                            move |_window, cx| {
                                reset.update(cx, |hub, cx| hub.reset_all(cx));
                            },
                        );
                    }),
            )],
        );

        form::page()
            .child(backup_group)
            .child(form::group(IconName::HardDrive, "Where it is", place))
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
        let start = wyck_config::AppPaths::documents_dir();
        let scripts_dir = indicators::dir(cx);
        let picked = cx.prompt_for_new_path(&start, Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else {
                return;
            };
            let created = chrono::Local::now().to_rfc3339();
            let result = backup::collect_with_scripts(dir, &scripts_dir, VERSION, &created)
                .map_err(backup::BackupError::from)
                .and_then(|b| backup::to_text(&b).map(|text| (b, text)))
                .and_then(|(b, text)| {
                    wyck_config::atomic_write(&path, text.as_bytes())
                        .map_err(std::io::Error::from)?;
                    Ok(b)
                });
            let _ = this.update(cx, |this, cx| match result {
                Ok(b) => {
                    let count = b.files.len();
                    this.say(
                        true,
                        format!("Saved {count} document(s) to {}.", path.display()),
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
            let result = std::fs::read_to_string(&path)
                .map_err(backup::BackupError::from)
                .and_then(|text| backup::stage(dir, &text));
            let _ = this.update(cx, |this, cx| match result {
                Ok(b) => {
                    let lines = backup::summary(&b);
                    cx.set_global(PendingSummary(lines.clone()));
                    this.waiting = Some(lines);
                    this.say(
                        true,
                        "The backup is valid and waits for the next start.",
                        cx,
                    );
                }
                Err(error) => this.say(false, error.to_string(), cx),
            });
        })
        .detach();
    }

    pub(super) fn cancel_import(&mut self, cx: &mut Context<Self>) {
        if let Some(dir) = crate::app_paths() {
            let _ = backup::cancel_pending(dir);
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
}
