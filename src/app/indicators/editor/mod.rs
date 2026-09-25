//! The editor of the indicator scripts, docked under the charts.
//!
//! It is a code editor with what one expects of it: colors, line numbers, folding, search, the
//! names of the language offered while typing, help under the pointer, and the mistakes of the
//! script underlined as they are made. Around it: the list of the scripts (the folder, as a tree),
//! tabs for the scripts being edited, a console for the problems and what the script printed, and
//! a reference of every function.
//!
//! Saving (Ctrl+S) writes the file; the app notices the change like it does for a file edited in
//! another program, so every chart that holds the script draws the new version at once.

pub mod assist;
pub mod lexer;
mod parts;
pub mod providers;

use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, KeyBinding, SharedString, Subscription, Task,
    Window, actions,
};
use gpui_kit::component::highlighter::{Diagnostic, DiagnosticSeverity};
use gpui_kit::component::input::language_config::LanguageConfig;
use gpui_kit::component::input::{
    AutoClosingPair, EditorState, InputEvent, InputState, Position, TabSize, set_language_config,
};

use crate::app::chart::Chart;
use crate::app::chart::study::custom::library::{LibraryError, registry};
use crate::app::chart::study::custom::run::Script;
use crate::app::chart::study::custom::templates::{TEMPLATES, Template};
use crate::app::chart::study::custom::{Problem, Severity};
use crate::app::confirm::confirm;
use crate::app::indicators;
use crate::app::menu as popup;
use crate::app::multichart::MultiChart;

actions!(
    wyck_indicator_editor,
    [SaveScript, AddToChart, CloseTab, ToggleReference]
);

/// The key context of the editor's panel.
const CONTEXT: &str = "IndicatorEditor";

/// Registers the key bindings of the editor. Call once, at startup.
pub fn init(cx: &mut App) {
    // Brackets and double quotes close themselves. Single quotes do not: they are apostrophes in a
    // comment far more often than they are a character.
    set_language_config(
        providers::LANGUAGE,
        LanguageConfig::default().auto_closing_pairs([
            AutoClosingPair::new("(", ")"),
            AutoClosingPair::new("[", "]"),
            AutoClosingPair::new("{", "}"),
            AutoClosingPair::new("\"", "\""),
        ]),
        cx,
    );
    cx.bind_keys([
        KeyBinding::new("secondary-s", SaveScript, Some(CONTEXT)),
        KeyBinding::new("secondary-enter", AddToChart, Some(CONTEXT)),
        KeyBinding::new("f5", AddToChart, Some(CONTEXT)),
        KeyBinding::new("secondary-w", CloseTab, Some(CONTEXT)),
        KeyBinding::new("secondary-alt-r", ToggleReference, Some(CONTEXT)),
    ]);
}

/// What the editor tells the dashboard around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEvent {
    /// The close button.
    Close,
    /// The button that makes the editor as tall as the window, or as it was.
    ToggleMaximize,
}

impl EventEmitter<EditorEvent> for IndicatorEditor {}

/// How long the editor waits after the last key before it checks the script.
const CHECK_DELAY: Duration = Duration::from_millis(350);

/// How long a message stays in the status bar.
const NOTICE_TIME: Duration = Duration::from_secs(6);

/// One script open in a tab.
pub(super) struct Doc {
    /// The id of the script, its path in the folder.
    pub id: String,
    pub state: Entity<EditorState>,
    /// The text as it is on disk (as last read or written).
    pub saved: String,
    /// Whether the text differs from `saved`.
    pub dirty: bool,
    /// What the last check found.
    pub problems: Vec<Problem>,
    /// The file changed on disk while the tab had changes of its own.
    pub conflict: bool,
    /// The file is no longer on disk.
    pub gone: bool,
    check: Option<Task<()>>,
    _subscription: Subscription,
}

/// A name being asked for, for a new script or a rename.
pub(super) enum Ask {
    /// A new script from a template (its position in [`TEMPLATES`]).
    New(usize),
    Rename(String),
}

pub(super) struct Prompt {
    pub ask: Ask,
    pub input: Entity<InputState>,
    _subscription: Subscription,
}

/// What the console shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConsoleTab {
    Problems,
    Output,
}

/// A line in the status bar.
pub(super) struct Notice {
    pub ok: bool,
    pub text: String,
    _clear: Task<()>,
}

pub struct IndicatorEditor {
    pub(super) multi: Entity<MultiChart>,
    pub(super) docs: Vec<Doc>,
    pub(super) active: usize,
    pub(super) filter: Entity<InputState>,
    pub(super) reference_filter: Entity<InputState>,
    pub(super) prompt: Option<Prompt>,
    pub(super) console: ConsoleTab,
    pub(super) reference_open: bool,
    /// The folders of the tree that are folded.
    pub(super) folded: BTreeSet<String>,
    pub(super) notice: Option<Notice>,
    /// The script the file menu is open for.
    pub(super) menu_target: Option<String>,
    pub(super) maximized: bool,
    pub(super) focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl IndicatorEditor {
    pub fn new(multi: Entity<MultiChart>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search scripts"));
        let reference_filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search the reference"));
        let subscriptions = vec![
            cx.subscribe(&filter, |_this, _input, _event: &InputEvent, cx| {
                cx.notify()
            }),
            cx.subscribe(
                &reference_filter,
                |_this, _input, _event: &InputEvent, cx| cx.notify(),
            ),
            indicators::observe_in(window, cx, |this, window, cx| {
                this.library_changed(window, cx);
            }),
            // What a chart says about the script (its output, its timing) changes without the
            // library changing: the console looks again whenever the charts do.
            cx.observe(&multi, |_this, _multi, cx| cx.notify()),
        ];
        Self {
            multi,
            docs: Vec::new(),
            active: 0,
            filter,
            reference_filter,
            prompt: None,
            console: ConsoleTab::Problems,
            reference_open: true,
            folded: BTreeSet::new(),
            notice: None,
            menu_target: None,
            maximized: false,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// The tab being edited.
    pub(super) fn current(&self) -> Option<&Doc> {
        self.docs.get(self.active)
    }

    /// Whether any tab has changes that are not saved.
    pub fn has_unsaved(&self) -> bool {
        self.docs.iter().any(|d| d.dirty)
    }

    pub fn is_maximized(&self) -> bool {
        self.maximized
    }

    pub fn set_maximized(&mut self, maximized: bool, cx: &mut Context<Self>) {
        self.maximized = maximized;
        cx.notify();
    }

    pub fn focus_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.current() {
            Some(doc) => doc.state.update(cx, |state, cx| state.focus(window, cx)),
            None => window.focus(&self.focus, cx),
        }
    }

    // ---- the message of the status bar ----

    pub(super) fn say(&mut self, ok: bool, text: impl Into<String>, cx: &mut Context<Self>) {
        let clear = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTICE_TIME).await;
            let _ = this.update(cx, |this, cx| {
                this.notice = None;
                cx.notify();
            });
        });
        self.notice = Some(Notice {
            ok,
            text: text.into(),
            _clear: clear,
        });
        cx.notify();
    }

    // ---- the tabs ----

    /// Opens the script `id` in a tab (or goes to its tab).
    pub fn open(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = self.docs.iter().position(|d| d.id == id) {
            self.active = at;
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
        self.show_problems(self.active, cx);
        cx.notify();
        self.focus_editor(window, cx);
    }

    pub(super) fn activate(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        if at < self.docs.len() {
            self.active = at;
            cx.notify();
            self.focus_editor(window, cx);
        }
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
        confirm(
            window,
            cx,
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

    fn remove_tab(&mut self, at: usize, cx: &mut Context<Self>) {
        if at < self.docs.len() {
            self.docs.remove(at);
            self.active = self.active.min(self.docs.len().saturating_sub(1));
            cx.notify();
        }
    }

    // ---- what happens when the text changes ----

    fn text_changed(&mut self, id: &str, cx: &mut Context<Self>) {
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
                .spawn(async move { Script::compile(&source).map(|_| ()) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let problems = result.err().unwrap_or_default();
                this.checked(&script, &text, problems, cx);
            });
        }));
        cx.notify();
    }

    /// The check of a text ended: what it found is shown, unless the text has changed since.
    fn checked(&mut self, id: &str, text: &str, problems: Vec<Problem>, cx: &mut Context<Self>) {
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
    fn show_problems(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(doc) = self.docs.get(at) else {
            return;
        };
        let text = doc.state.read(cx).value().to_string();
        show_diagnostics(&doc.state, &doc.problems, &text, cx);
    }

    // ---- saving ----

    /// Writes the script of the tab to its file.
    pub(super) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.current() else {
            return;
        };
        let (id, text) = (doc.id.clone(), doc.state.read(cx).value().to_string());
        let task = {
            let (id, text) = (id.clone(), text.clone());
            indicators::on_library(cx, move |library| library.save(&id, &text))
        };
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, _window, cx| this.saved(&id, text, result, cx));
        })
        .detach();
    }

    fn saved(
        &mut self,
        id: &str,
        text: String,
        result: Option<Result<crate::app::chart::study::custom::library::Changes, LibraryError>>,
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
                        doc.problems = entry.problems.clone();
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
    }

    /// Saves the script of the tab and puts it on the active chart.
    pub(super) fn add_to_chart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.current() else {
            return;
        };
        let id = doc.id.clone();
        self.save(window, cx);
        let chart: Entity<Chart> = self.multi.read(cx).active_chart().clone();
        // Once the file is read (a moment): the chart then has the script to look up.
        let entity = cx.entity();
        cx.spawn(async move |_this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            entity.update(cx, |editor, cx| {
                let ready = registry::get(&id).is_some_and(|e| e.is_ready());
                if !ready {
                    editor.say(false, "It has problems, so it is not put on the chart.", cx);
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
                    let study = crate::app::chart::study::StudyConfig::for_script(&id);
                    chart.update(cx, |chart, cx| chart.add_study(study, cx));
                    indicators::update_prefs(cx, |prefs| {
                        prefs.note_recent(&format!("script:{id}"));
                    });
                    editor.say(true, "Added to the chart.", cx);
                }
                editor.console = ConsoleTab::Output;
            });
        })
        .detach();
    }

    // ---- files ----

    /// A file changed, appeared or went: the tabs follow it.
    fn library_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for index in 0..self.docs.len() {
            let id = self.docs[index].id.clone();
            let entry = registry::get(&id);
            let doc = &mut self.docs[index];
            match entry {
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
                        doc.problems = entry.problems.clone();
                        doc.state
                            .update(cx, |state, cx| state.set_value(text.clone(), window, cx));
                        show_diagnostics(&doc.state, &doc.problems, &text, cx);
                    }
                }
            }
        }
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
            doc.problems = entry.problems.clone();
            doc.state
                .update(cx, |state, cx| state.set_value(text.clone(), window, cx));
            show_diagnostics(&doc.state, &doc.problems, &text, cx);
        }
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
        cx.notify();
    }

    // ---- new, rename, duplicate, delete ----

    /// Asks for a name for a new script made from the template `template`.
    pub fn ask_new(&mut self, template: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = TEMPLATES.get(template) else {
            return;
        };
        let name = t.name.to_owned();
        self.ask(Ask::New(template), &name, window, cx);
    }

    pub(super) fn ask_rename(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let stem = id.rsplit('/').next().unwrap_or(id).to_owned();
        self.ask(Ask::Rename(id.to_owned()), &stem, window, cx);
    }

    fn ask(&mut self, ask: Ask, initial: &str, window: &mut Window, cx: &mut Context<Self>) {
        let initial = initial.to_owned();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(initial));
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this, _input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.commit_prompt(window, cx);
                }
            },
        );
        input.update(cx, |state, cx| state.focus(window, cx));
        self.prompt = Some(Prompt {
            ask,
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    pub(super) fn cancel_prompt(&mut self, cx: &mut Context<Self>) {
        if self.prompt.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn commit_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        let name = prompt.input.read(cx).value().to_string();
        match prompt.ask {
            Ask::New(template) => {
                let Some(Template { source, .. }) = TEMPLATES.get(template) else {
                    return;
                };
                let source = (*source).to_owned();
                let task =
                    indicators::on_library(cx, move |library| library.create(None, &name, &source));
                self.after_create(task, window, cx);
            }
            Ask::Rename(id) => {
                let task = indicators::rename(cx, id.clone(), name);
                cx.spawn_in(window, async move |this, cx| {
                    let result = task.await;
                    let _ = this.update_in(cx, |this, _window, cx| match result {
                        Some(Ok(new_id)) => {
                            for doc in &mut this.docs {
                                if doc.id == id {
                                    doc.id.clone_from(&new_id);
                                }
                            }
                            this.say(true, format!("Renamed to {new_id}."), cx);
                        }
                        Some(Err(error)) => this.say(false, error.to_string(), cx),
                        None => {}
                    });
                })
                .detach();
            }
        }
        cx.notify();
    }

    fn after_create(
        &mut self,
        task: Task<Option<Result<String, LibraryError>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Some(Ok(id)) => {
                    this.open(&id, window, cx);
                    this.say(true, format!("Created {id}."), cx);
                }
                Some(Err(error)) => this.say(false, error.to_string(), cx),
                None => {}
            });
        })
        .detach();
    }

    pub(super) fn duplicate(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let id = id.to_owned();
        let task = indicators::on_library(cx, move |library| library.duplicate(&id));
        self.after_create(task, window, cx);
    }

    /// Asks, then takes the script out of the folder (into its trash).
    pub(super) fn delete(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (this, target) = (cx.entity(), id.to_owned());
        confirm(
            window,
            cx,
            format!("Delete {id}?"),
            "The file goes to the .trash folder inside the indicators folder, where it can still be found. Charts that hold it show it as missing.",
            move |_window, cx| {
                let this = this.clone();
                let id = target.clone();
                let task = indicators::on_library(cx, {
                    let id = id.clone();
                    move |library| library.delete(&id)
                });
                cx.spawn(async move |cx| {
                    let result = task.await;
                    this.update(cx, |this, cx| match result {
                        Some(Ok(())) => {
                            if let Some(at) = this.docs.iter().position(|d| d.id == id) {
                                this.docs[at].gone = true;
                            }
                            this.say(true, "Deleted.", cx);
                        }
                        Some(Err(error)) => this.say(false, error.to_string(), cx),
                        None => {}
                    });
                })
                .detach();
            },
        );
    }

    // ---- moving the cursor ----

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
            line.saturating_sub(1) as u32,
            column.saturating_sub(1) as u32,
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

    // ---- exporting and importing ----

    pub(super) fn export_current(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.current() else {
            return;
        };
        let id = doc.id.clone();
        let start = indicators::dir(cx);
        let name = format!("{}.rhai", id.rsplit('/').next().unwrap_or(&id));
        let picked = cx.prompt_for_new_path(&start, Some(&name));
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else {
                return;
            };
            let done = cx.update(|cx| {
                indicators::on_library(cx, move |library| library.export(&task_id, &path))
            });
            let result = done.await;
            let _ = this.update(cx, |this, cx| match result {
                Some(Ok(path)) => this.say(true, format!("Exported to {}.", path.display()), cx),
                Some(Err(error)) => this.say(false, error.to_string(), cx),
                None => {}
            });
        })
        .detach();
    }

    pub fn export_all(&mut self, cx: &mut Context<Self>) {
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
            let _ = this.update(cx, |this, cx| match result {
                Some(Ok(count)) => this.say(
                    true,
                    format!("Exported {count} script(s) to {}.", dest.display()),
                    cx,
                ),
                Some(Err(error)) => this.say(false, error.to_string(), cx),
                None => {}
            });
        })
        .detach();
    }

    pub fn import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(report) = report else {
                    return;
                };
                let (added, skipped) = (report.imported.len(), report.skipped.len());
                let mut text = format!("Imported {added} script(s).");
                if let Some((path, why)) = report.skipped.first() {
                    text.push_str(&format!(
                        " Skipped {skipped}: {} ({why}).",
                        path.file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                    ));
                }
                this.say(skipped == 0, text, cx);
                if let Some(first) = report.imported.first() {
                    this.open(first, window, cx);
                }
            });
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

    pub(super) fn menu(&self, window: &mut Window, cx: &mut App, id: &'static str) -> popup::Menu {
        popup::Menu::new(id, window, cx)
    }
}

/// A code editor with the language of the scripts.
fn new_state(
    text: &str,
    window: &mut Window,
    cx: &mut Context<IndicatorEditor>,
) -> Entity<EditorState> {
    let text = text.to_owned();
    let state = cx.new(|cx| {
        let mut state = EditorState::new(window, cx)
            .language(providers::LANGUAGE)
            .line_number(true)
            .folding(true)
            .indent_guides(true)
            .auto_close(true)
            .smart_indent(true)
            .tab_size(TabSize {
                tab_size: 4,
                hard_tabs: false,
            })
            .default_value(text);
        state.lsp_mut().completion_provider = Some(Rc::new(providers::Completions));
        state.lsp_mut().hover_provider = Some(Rc::new(providers::Help));
        state
    });
    state.update(cx, |state, cx| {
        state.set_highlighter_factory(providers::highlighter_factory(), cx);
    });
    state
}

/// Underlines the problems of a script in its editor.
fn show_diagnostics(state: &Entity<EditorState>, problems: &[Problem], text: &str, cx: &mut App) {
    let lines: Vec<&str> = text.lines().collect();
    let diagnostics: Vec<Diagnostic> = problems
        .iter()
        .map(|problem| {
            let line = problem.line.saturating_sub(1);
            let column = problem.column.saturating_sub(1);
            // The word at the place, or one character.
            let width = lines.get(line).map_or(1, |l| {
                l.chars()
                    .skip(column)
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .count()
                    .max(1)
            });
            let severity = match problem.severity {
                Severity::Error => DiagnosticSeverity::Error,
                Severity::Warning => DiagnosticSeverity::Warning,
            };
            Diagnostic::new(
                Position::new(line as u32, column as u32)
                    ..Position::new(line as u32, (column + width) as u32),
                problem.message.clone(),
            )
            .with_severity(severity)
            .with_source("script")
        })
        .collect();
    state.update(cx, |state, cx| {
        if let Some(set) = state.diagnostics_mut() {
            set.clear();
            set.extend(diagnostics);
        }
        cx.notify();
    });
}
