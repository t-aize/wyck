//! Saving a script, adding it to the chart, and following the folder on disk.

use super::{ConsoleTab, IndicatorEditor, show_diagnostics};
use crate::chart::chart::Chart;
use gpui::{Context, Entity, Window};
use wyck_app::lost::lost;
use wyck_app::scripts as indicators;
use wyck_app::scripts::folder::LibraryError;
use wyck_indicators::custom::library::registry;

impl IndicatorEditor {
    /// Writes the script of the tab to its file.
    pub(super) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.current() else {
            return;
        };
        if doc.conflict {
            self.say(
                false,
                "The file changed on disk. Load it or choose Keep mine before saving.",
                cx,
            );
            return;
        }
        let (id, text) = (doc.id.clone(), doc.state.read(cx).value().to_string());
        let task = {
            let (id, text) = (id.clone(), text.clone());
            indicators::on_library(cx, move |library| library.save(&id, &text))
        };
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            lost(
                "files: save",
                this.update_in(cx, |this, _window, cx| this.saved(&id, text, result, cx)),
            );
        })
        .detach();
    }

    pub(super) fn saved(
        &mut self,
        id: &str,
        text: String,
        result: Option<Result<wyck_app::scripts::folder::Changes, LibraryError>>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Some(Ok(_)) => {
                let entry = registry::get(id);
                if let Some(doc) = self.docs.iter_mut().find(|d| d.id == id) {
                    let now = doc.state.read(cx).value().to_string();
                    doc.saved = text;
                    doc.dirty = now != doc.saved;
                    doc.conflict = false;
                    doc.gone = false;
                    if let Some(entry) = &entry {
                        doc.problems.clone_from(&entry.problems);
                        show_diagnostics(&doc.state, &doc.problems, &now, cx);
                    }
                }
                match entry {
                    Some(entry) if !entry.is_ready() => {
                        let n = entry.problems.len();
                        self.say(
                            false,
                            format!("Saved, with {n} problem(s): it cannot run yet."),
                            cx,
                        );
                        self.console = ConsoleTab::Problems;
                    }
                    _ => self.say(
                        true,
                        "Saved. The charts that hold it draw the new version.",
                        cx,
                    ),
                }
            }
            Some(Err(error)) => self.say(false, error.to_string(), cx),
            None => self.say(false, "The indicators folder is not available.", cx),
        }
        self.queue_drafts(cx);
    }

    /// Saves the script of the tab and puts it on the active chart.
    pub(super) fn add_to_chart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.current() else {
            return;
        };
        if doc.conflict {
            self.say(
                false,
                "The file changed on disk. Resolve the conflict before adding it.",
                cx,
            );
            return;
        }
        let id = doc.id.clone();
        let text = doc.state.read(cx).value().to_string();
        let chart: Entity<Chart> = self.multi.read(cx).active_chart().clone();
        let task = {
            let (id, text) = (id.clone(), text.clone());
            indicators::on_library(cx, move |library| library.save(&id, &text))
        };
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            lost(
                "files: add_to_chart",
                this.update_in(cx, |editor, _window, cx| {
                    let saved = result.as_ref().is_some_and(Result::is_ok);
                    editor.saved(&id, text, result, cx);
                    if !saved || !registry::get(&id).is_some_and(|entry| entry.is_ready()) {
                        return;
                    }
                    let held = chart
                        .read(cx)
                        .settings()
                        .studies
                        .iter()
                        .any(|s| s.script.as_deref() == Some(id.as_str()));
                    if held {
                        editor.say(true, "It is on the chart already: it was updated.", cx);
                    } else {
                        let study = wyck_indicators::config::StudyConfig::for_script(&id);
                        chart.update(cx, |chart, cx| chart.add_study(study, cx));
                        indicators::update_prefs(cx, |prefs| {
                            prefs.note_recent(&format!("script:{id}"));
                        });
                        editor.say(true, "Added to the chart.", cx);
                    }
                    editor.console = ConsoleTab::Output;
                }),
            );
        })
        .detach();
    }

    /// A file changed, appeared or went: the tabs follow it.
    pub(super) fn library_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for doc in &mut self.docs {
            match registry::get(&doc.id) {
                None => doc.gone = true,
                Some(entry) => {
                    doc.gone = false;
                    if *entry.source == *doc.saved {
                        continue;
                    }
                    if doc.dirty {
                        doc.conflict = true;
                    } else {
                        // The file changed elsewhere and the tab has nothing of its own: it follows.
                        let text = entry.source.to_string();
                        doc.saved.clone_from(&text);
                        doc.problems.clone_from(&entry.problems);
                        doc.state
                            .update(cx, |state, cx| state.set_value(text.clone(), window, cx));
                        show_diagnostics(&doc.state, &doc.problems, &text, cx);
                    }
                }
            }
        }
        self.queue_drafts(cx);
        cx.notify();
    }

    /// Takes the text of the file over the tab's own.
    pub(super) fn reload_from_disk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        if let Some(entry) = registry::get(&doc.id) {
            let text = entry.source.to_string();
            doc.saved.clone_from(&text);
            doc.dirty = false;
            doc.conflict = false;
            doc.problems.clone_from(&entry.problems);
            doc.state
                .update(cx, |state, cx| state.set_value(text.clone(), window, cx));
            show_diagnostics(&doc.state, &doc.problems, &text, cx);
        }
        self.queue_drafts(cx);
        cx.notify();
    }

    /// Keeps the tab's text, and forgets the change on disk (the next save replaces it).
    pub(super) fn keep_mine(&mut self, cx: &mut Context<Self>) {
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.conflict = false;
            if let Some(entry) = registry::get(&doc.id) {
                doc.saved = entry.source.to_string();
                doc.dirty = doc.state.read(cx).value() != doc.saved.as_str();
            }
        }
        self.queue_drafts(cx);
        cx.notify();
    }
}
