//! The Alerts page of the settings: the sound and the desktop notification of an alert that fires.

use super::*;
use crate::app::alerts::sound;
use crate::app::alerts::sound::SoundKind;

/// The volumes offered, in percent.
const VOLUMES: [(&str, u8); 5] = [
    ("Low", 25),
    ("Medium", 50),
    ("Normal", 70),
    ("High", 85),
    ("Full", 100),
];

impl SettingsHub {
    pub(super) fn alerts_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let output = self.workspace.read(cx).preferences().alert_output.clone();
        let labels: Vec<&str> = SoundKind::ALL.iter().map(|k| k.label()).collect();
        let chosen = SoundKind::ALL
            .iter()
            .position(|k| *k == output.sound)
            .unwrap_or(0);
        let volume = VOLUMES
            .iter()
            .position(|(_, v)| *v == output.volume)
            .unwrap_or(2);
        let (pick, level, listen, notify, background) = (
            self.workspace.clone(),
            self.workspace.clone(),
            self.workspace.clone(),
            self.workspace.clone(),
            self.workspace.clone(),
        );
        let (choose, clear) = (self.workspace.clone(), self.workspace.clone());
        let has_file = output.custom.is_some();
        let file_name = output
            .custom
            .as_deref()
            .and_then(|c| std::path::Path::new(c).file_name())
            .map(|n| n.to_string_lossy().into_owned());

        let sound_fields = vec![
            form::field(
                "Sound",
                Some("Played when an alert fires. An alert can choose its own in its window"),
                controls::chips("alert-sound", &labels, &[chosen], move |index, _w, cx| {
                    let kind = SoundKind::ALL[index];
                    pick.update(cx, |w, cx| {
                        w.edit_preferences(cx, |p| p.alert_output.sound = kind);
                    });
                    preview(&pick, kind, cx);
                }),
            ),
            form::field(
                "Volume",
                None,
                controls::segmented(
                    "alert-volume",
                    &VOLUMES.map(|(label, _)| label),
                    volume,
                    move |index, _w, cx| {
                        let value = VOLUMES[index].1;
                        level.update(cx, |w, cx| {
                            w.edit_preferences(cx, |p| p.alert_output.volume = value);
                        });
                        preview(&level, SoundKind::Off, cx);
                    },
                ),
            ),
            form::field(
                "Listen",
                Some("Plays the sound chosen above at this volume"),
                button::action(
                    "alert-sound-preview",
                    "Play",
                    Some(IconName::Volume2),
                    false,
                    move |_w, cx| preview(&listen, SoundKind::Off, cx),
                ),
            ),
            form::field(
                "Your own sound",
                Some("A wav, ogg, mp3 or flac file, up to 15 seconds and 4 MB"),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .children(file_name.map(|name| {
                        div()
                            .max_w(px(200.))
                            .truncate()
                            .text_size(px(tokens::text::small()))
                            .text_color(theme::muted_fg())
                            .child(name)
                    }))
                    .child(button::action(
                        "alert-sound-choose",
                        "Choose...",
                        Some(IconName::FolderOpen),
                        false,
                        move |_w, cx| choose_file(choose.clone(), cx),
                    ))
                    .child(
                        button::action(
                            "alert-sound-clear",
                            "Remove",
                            Some(IconName::Trash),
                            false,
                            move |_w, cx| {
                                clear.update(cx, |w, cx| {
                                    w.edit_preferences(cx, |p| {
                                        p.alert_output.custom = None;
                                        p.alert_output = p.alert_output.clone().normalized();
                                    });
                                });
                            },
                        )
                        .disabled(!has_file),
                    ),
            ),
        ];

        let notification_fields = vec![
            form::field(
                "Desktop notification",
                Some("A notice from the system, seen when the app is behind other windows"),
                controls::toggle("alert-notify", output.notify, move |on, _w, cx| {
                    notify.update(cx, |w, cx| {
                        w.edit_preferences(cx, |p| p.alert_output.notify = on);
                    });
                }),
            ),
            form::field(
                "Only when the app is not in use",
                Some("The notice inside the app already says it when you are looking at it"),
                controls::toggle(
                    "alert-notify-background",
                    output.notify_in_background_only,
                    move |on, _w, cx| {
                        background.update(cx, |w, cx| {
                            w.edit_preferences(cx, |p| {
                                p.alert_output.notify_in_background_only = on
                            });
                        });
                    },
                ),
            ),
        ];

        form::page()
            .child(form::group(IconName::Volume2, "Sound", sound_fields))
            .child(form::group(
                IconName::Bell,
                "Notification",
                notification_fields,
            ))
            .child(form::note(
                "The System sound is the one your operating system uses for its own notices. \
                 The others are built into the app.",
            ))
            .into_any_element()
    }
}

/// Plays a sound for the user to hear it: `Off` stands for the one chosen in the settings.
fn preview(workspace: &Entity<Workspace>, kind: SoundKind, cx: &mut App) {
    let output = workspace.read(cx).preferences().alert_output.clone();
    let kind = if kind == SoundKind::Off {
        output.sound
    } else {
        kind
    };
    if kind == SoundKind::Off {
        return;
    }
    sound::speaker().play(kind, output.custom.as_deref(), output.volume);
}

/// Asks for a sound file, checks it, keeps a copy in the settings folder and makes it the sound.
fn choose_file(workspace: Entity<Workspace>, cx: &mut App) {
    let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some("Choose a sound for the alerts".into()),
    });
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = picked.await else {
            return;
        };
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        let kept = keep_copy(&path);
        cx.update(|cx| match kept {
            Ok(copy) => {
                let shown = copy.display().to_string();
                workspace.update(cx, |w, cx| {
                    w.edit_preferences(cx, |p| {
                        p.alert_output.custom = Some(shown);
                        p.alert_output.sound = SoundKind::Custom;
                    });
                });
                preview(&workspace, SoundKind::Custom, cx);
            }
            Err(message) => {
                toast::show(cx, toast::Kind::Error, "This sound cannot be used", message)
            }
        });
    })
    .detach();
}

/// Checks a file and copies it beside the settings, so it keeps working when the original moves.
fn keep_copy(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    sound::check_file(path)?;
    let paths = crate::app_paths().ok_or("The settings folder is not available")?;
    let dir = paths.state_dir().join("sounds");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // One file only: an older copy goes away.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with("custom.") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| !e.is_empty() && e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "wav".to_owned());
    let target = dir.join(format!("custom.{extension}"));
    std::fs::copy(path, &target).map_err(|e| e.to_string())?;
    Ok(target)
}
