//! The open tabs of the editor: opening, closing, and what the check says about the text.

use super::{CHECK_DELAY, Doc, IndicatorEditor, new_state, show_diagnostics};
use gpui::{Context, Window};
use wyck_indicators::custom::Problem;
use wyck_indicators::custom::library::registry;
use wyck_indicators::custom::run::Script;
use wyck_ui_kit::confirm::{Tone, confirm_as};
use wyck_ui_kit::input::InputEvent;

impl IndicatorEditor {
    /// Opens the script `id` in a tab (or goes to its tab).
    pub fn open(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = self.docs.iter().position(|d| d.id == id) {
            self.active = at;
            self.queue_drafts(cx);
            cx.notify();
            self.focus_editor(window, cx);
            return;
        }
        let Some(entry) = registry::get(id) else {
            self.say(false, format!("There is no script called \"{id}\"."), cx);
            return;
        };
        let text = entry.source.to_string();
        let state = new_state(&text, window, cx);
        let script = id.to_owned();
        let subscription = cx.subscribe(&state, move |this, _state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.text_changed(&script, cx);
            }
        });
        let doc = Doc {
            id: id.to_owned(),
            state,
            saved: text,
            dirty: false,
            problems: entry.problems.clone(),
            conflict: false,
            gone: false,
            check: None,
            _subscription: subscription,
        };
        self.docs.push(doc);
        self.active = self.docs.len() - 1;
        self.queue_drafts(cx);
        self.show_problems(self.active, cx);
        cx.notify();
        self.focus_editor(window, cx);
    }

    pub(super) fn activate(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        if at < self.docs.len() {
            self.active = at;
            self.queue_drafts(cx);
            cx.notify();
            self.focus_editor(window, cx);
        }
    }

    pub(super) fn cycle_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.docs.is_empty() {
            return;
        }
        let next = if forward {
            (self.active + 1) % self.docs.len()
        } else {
            (self.active + self.docs.len() - 1) % self.docs.len()
        };
        self.activate(next, window, cx);
    }

    /// Closes the tab at `at`, asking first when it has changes.
    pub(super) fn close_tab(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.docs.get(at) else {
            return;
        };
        if !doc.dirty {
            self.remove_tab(at, cx);
            return;
        }
        let (this, id) = (cx.entity(), doc.id.clone());
        confirm_as(
            window,
            cx,
            Tone::Danger,
            format!("Close {id}?"),
            "This script has changes that are not saved. Closing it throws them away.",
            move |_window, cx| {
                this.update(cx, |editor, cx| {
                    if let Some(at) = editor.docs.iter().position(|d| d.id == id) {
                        editor.remove_tab(at, cx);
                    }
                });
            },
        );
    }

    pub(super) fn remove_tab(&mut self, at: usize, cx: &mut Context<Self>) {
        if at < self.docs.len() {
            self.docs.remove(at);
            self.active = self.active.min(self.docs.len().saturating_sub(1));
            self.queue_drafts(cx);
            cx.notify();
        }
    }

    pub(super) fn text_changed(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(doc) = self.docs.iter_mut().find(|d| d.id == id) else {
            return;
        };
        let text = doc.state.read(cx).value().to_string();
        doc.dirty = text != doc.saved;
        let script = id.to_owned();
        doc.check = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CHECK_DELAY).await;
            let source = text.clone();
            let result = cx
                .background_executor()
                .spawn(async move { Script::compile(&source).map(|script| script.warnings) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let problems = match result {
                    Ok(warnings) | Err(warnings) => warnings,
                };
                this.checked(&script, &text, problems, cx);
            });
        }));
        self.queue_drafts(cx);
        cx.notify();
    }

    /// The check of a text ended: what it found is shown, unless the text has changed since.
    pub(super) fn checked(
        &mut self,
        id: &str,
        text: &str,
        problems: Vec<Problem>,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.docs.iter_mut().find(|d| d.id == id) else {
            return;
        };
        if doc.state.read(cx).value() != text {
            return;
        }
        doc.problems = problems;
        show_diagnostics(&doc.state, &doc.problems, text, cx);
        cx.notify();
    }

    /// Puts what is known of the script from the library in the tab, and underlines it.
    pub(super) fn show_problems(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(doc) = self.docs.get(at) else {
            return;
        };
        let text = doc.state.read(cx).value().to_string();
        show_diagnostics(&doc.state, &doc.problems, &text, cx);
    }
}
