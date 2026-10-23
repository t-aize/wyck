//! What the toolbar and the menus do: jump to a problem, insert text, export and import.

use super::{ConsoleTab, IndicatorEditor};
use crate::kit::input::Position;
use crate::kit::menu as popup;
use gpui::{App, Context, SharedString, Window};
use wyck_app::lost::lost;
use wyck_app::scripts as indicators;
use wyck_indicators::custom::library::registry;

impl IndicatorEditor {
    /// Puts the cursor at a place a problem points to.
    pub(super) fn jump_to(
        &mut self,
        line: usize,
        column: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.current() else {
            return;
        };
        let position = Position::new(
            super::lsp_index(line.saturating_sub(1)),
            super::lsp_index(column.saturating_sub(1)),
        );
        doc.state.update(cx, |state, cx| {
            state.set_cursor_position(position, window, cx);
            state.focus(window, cx);
        });
    }

    /// Writes `text` where the cursor is.
    pub(super) fn insert(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.current() else {
            return;
        };
        let text = SharedString::from(text.to_owned());
        doc.state.update(cx, |state, cx| {
            state.insert(text, window, cx);
            state.focus(window, cx);
        });
    }

    /// Where the cursor is in the tab, counted from 1.
    pub(super) fn cursor(&self, cx: &App) -> Option<(usize, usize)> {
        let position = self.current()?.state.read(cx).cursor_position();
        Some((position.line as usize + 1, position.character as usize + 1))
    }

    pub(super) fn step_problem(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.cursor(cx).unwrap_or((0, 0));
        let Some(doc) = self.current() else { return };
        let mut places: Vec<_> = doc
            .problems
            .iter()
            .filter(|problem| problem.line > 0)
            .map(|problem| (problem.line, problem.column.max(1)))
            .collect();
        places.sort_unstable();
        let target = if forward {
            places
                .iter()
                .copied()
                .find(|at| *at > current)
                .or_else(|| places.first().copied())
        } else {
            places
                .iter()
                .rev()
                .copied()
                .find(|at| *at < current)
                .or_else(|| places.last().copied())
        };
        if let Some((line, column)) = target {
            self.console = ConsoleTab::Problems;
            self.jump_to(line, column, window, cx);
        }
    }

    pub(super) fn export_current(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.current() else {
            return;
        };
        let id = doc.id.clone();
        let start = indicators::dir(cx);
        let name = format!("{}.rhai", id.rsplit('/').next().unwrap_or(&id));
        let picked = cx.prompt_for_new_path(&start, Some(&name));
        let task_id = id;
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else {
                return;
            };
            let done = cx.update(|cx| {
                indicators::on_library(cx, move |library| library.export(&task_id, &path))
            });
            let result = done.await;
            lost(
                "actions: export_current",
                this.update(cx, |this, cx| match result {
                    Some(Ok(path)) => {
                        this.say(true, format!("Exported to {}.", path.display()), cx);
                    }
                    Some(Err(error)) => this.say(false, error.to_string(), cx),
                    None => {}
                }),
            );
        })
        .detach();
    }

    pub fn export_all(cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose a folder to export every script to".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(dest) = paths.into_iter().next() else {
                return;
            };
            let done = cx.update(|cx| {
                let target = dest.clone();
                indicators::on_library(cx, move |library| library.export_all(&target))
            });
            let result = done.await;
            lost(
                "actions: export_all",
                this.update(cx, |this, cx| match result {
                    Some(Ok(count)) => this.say(
                        true,
                        format!("Exported {count} script(s) to {}.", dest.display()),
                        cx,
                    ),
                    Some(Err(error)) => this.say(false, error.to_string(), cx),
                    None => {}
                }),
            );
        })
        .detach();
    }

    pub fn import(window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Choose scripts (.rhai) or folders of them".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let done = cx
                .update(|_, cx| indicators::on_library(cx, move |library| library.import(&paths)));
            let Ok(done) = done else {
                return;
            };
            let report = done.await;
            lost(
                "actions: import",
                this.update_in(cx, |this, window, cx| {
                    let Some(report) = report else {
                        return;
                    };
                    let (added, skipped) = (report.imported.len(), report.skipped.len());
                    let mut text = format!("Imported {added} script(s).");
                    if let Some((path, why)) = report.skipped.first() {
                        let name = path
                            .file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                        text = format!("{text} Skipped {skipped}: {name} ({why}).");
                    }
                    this.say(skipped == 0, text, cx);
                    if let Some(first) = report.imported.first() {
                        this.open(first, window, cx);
                    }
                }),
            );
        })
        .detach();
    }

    pub(super) fn reveal_current(&mut self, cx: &mut Context<Self>) {
        if let Some(doc) = self.current()
            && let Some(entry) = registry::get(&doc.id)
        {
            indicators::reveal(cx, &entry.path);
        }
    }

    pub(super) fn menu(window: &mut Window, cx: &mut App, id: &'static str) -> popup::Menu {
        popup::Menu::new(id, window, cx)
    }
}
