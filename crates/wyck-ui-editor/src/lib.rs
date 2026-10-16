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

mod actions;
mod files;
mod parts;
mod prompts;
mod providers;
mod tabs;

use gpui::prelude::*;
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, MouseMoveEvent, Subscription, Task, Window,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::Duration;
use wyck_app::lost::lost;
use wyck_app::scripts as indicators;
use wyck_app::workspace::Saver;
use wyck_indicators::custom::library::registry;
use wyck_indicators::custom::{Problem, Severity};
use wyck_ui_chart::multichart::MultiChart;
use wyck_ui_kit::input::LanguageConfig;
use wyck_ui_kit::input::{
    AutoClosingPair, EditorState, InputEvent, InputState, Position, TabSize, set_language_config,
};
use wyck_ui_kit::input::{Diagnostic, DiagnosticSeverity};

#[expect(
    clippy::derive_partial_eq_without_eq,
    reason = "gpui::actions! derives PartialEq without Eq, in code that is not ours"
)]
mod action {
    gpui::actions!(
        wyck_indicator_editor,
        [
            SaveScript,
            AddToChart,
            CloseTab,
            ToggleReference,
            NextProblem,
            PreviousProblem,
            NextTab,
            PreviousTab
        ]
    );
}
pub use action::*;

/// A line or a column of a text as the number the language server protocol counts in.
fn lsp_index(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// The key context of the editor's panel.
pub const CONTEXT: &str = "IndicatorEditor";

/// Prepares the editor. Call once, at startup. Its key bindings are in `wyck_ui_shell::keymap`.
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
const DRAFT_DOCUMENT: &str = "indicator_editor_drafts";
#[derive(Default, Serialize, Deserialize)]
struct Drafts {
    directory: String,
    active: usize,
    tabs: Vec<DraftTab>,
}

#[derive(Serialize, Deserialize)]
struct DraftTab {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    buffer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base: Option<String>,
}

/// One script open in a tab.
pub(crate) struct Doc {
    /// The id of the script, its path in the folder.
    pub(crate) id: String,
    pub(crate) state: Entity<EditorState>,
    /// The text as it is on disk (as last read or written).
    pub(crate) saved: String,
    /// Whether the text differs from `saved`.
    pub(crate) dirty: bool,
    /// What the last check found.
    pub(crate) problems: Vec<Problem>,
    /// The file changed on disk while the tab had changes of its own.
    pub(crate) conflict: bool,
    /// The file is no longer on disk.
    pub(crate) gone: bool,
    check: Option<Task<()>>,
    _subscription: Subscription,
}

/// A name being asked for, for a new script or a rename.
pub(crate) enum Ask {
    /// A new script from a template (its position in [`TEMPLATES`]).
    New(usize),
    Rename(String),
}

pub(crate) struct Prompt {
    pub(crate) ask: Ask,
    pub(crate) input: Entity<InputState>,
    _subscription: Subscription,
}

/// What the console shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConsoleTab {
    Problems,
    Output,
}

/// A line in the status bar.
pub(crate) struct Notice {
    pub(crate) ok: bool,
    pub(crate) text: String,
    _clear: Task<()>,
}

#[derive(Clone, Copy)]
pub(crate) enum ResizeSide {
    Explorer,
    Reference,
    Console,
}

#[derive(Clone, Copy)]
pub(crate) struct ResizeDrag {
    pub(crate) side: ResizeSide,
    pub(crate) start: f32,
    pub(crate) size: f32,
}

pub struct IndicatorEditor {
    pub(crate) multi: Entity<MultiChart>,
    pub(crate) docs: Vec<Doc>,
    pub(crate) active: usize,
    pub(crate) filter: Entity<InputState>,
    pub(crate) reference_filter: Entity<InputState>,
    pub(crate) prompt: Option<Prompt>,
    pub(crate) console: ConsoleTab,
    pub(crate) reference_open: bool,
    /// The folders of the tree that are folded.
    pub(crate) folded: BTreeSet<String>,
    pub(crate) notice: Option<Notice>,
    /// The script the file menu is open for.
    pub(crate) menu_target: Option<String>,
    pub(crate) maximized: bool,
    pub(crate) explorer_width: f32,
    pub(crate) reference_width: f32,
    pub(crate) console_height: f32,
    pub(crate) resize: Option<ResizeDrag>,
    /// Writes the unsaved work a moment after the last change; `None` when nothing is remembered.
    draft_saver: Option<Saver<Drafts>>,
    drafts_loaded: bool,
    pub(crate) focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl IndicatorEditor {
    pub fn new(multi: Entity<MultiChart>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let draft_store = indicators::draft_store(cx);
        if let Some(store) = draft_store.clone() {
            let load = cx
                .background_executor()
                .spawn(async move { store.load_or_default::<Drafts>(DRAFT_DOCUMENT) });
            cx.spawn_in(window, async move |this, cx| {
                let drafts = load.await;
                lost(
                    "lib: new",
                    this.update_in(cx, |editor, window, cx| {
                        editor.restore_drafts(drafts, window, cx);
                    }),
                );
            })
            .detach();
        }
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search names or code"));
        let reference_filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search the reference"));
        let subscriptions = vec![
            cx.subscribe(&filter, |_this, _input, _event: &InputEvent, cx| {
                cx.notify();
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
            reference_open: false,
            folded: BTreeSet::new(),
            notice: None,
            menu_target: None,
            maximized: false,
            explorer_width: 236.0,
            reference_width: 320.0,
            console_height: 148.0,
            resize: None,
            drafts_loaded: draft_store.is_none(),
            draft_saver: draft_store.map(|store| Saver::new(store, DRAFT_DOCUMENT)),
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    fn restore_drafts(&mut self, drafts: Drafts, window: &mut Window, cx: &mut Context<Self>) {
        if drafts.directory == indicators::dir(cx).to_string_lossy() {
            for tab in drafts.tabs {
                if self.docs.iter().any(|doc| doc.id == tab.id) {
                    continue;
                }
                let entry = registry::get(&tab.id);
                let Some(text) = tab
                    .buffer
                    .clone()
                    .or_else(|| entry.as_ref().map(|entry| entry.source.to_string()))
                else {
                    continue;
                };
                let base = tab
                    .base
                    .or_else(|| entry.as_ref().map(|entry| entry.source.to_string()))
                    .unwrap_or_default();
                let dirty = text != base;
                let conflict = dirty
                    && entry
                        .as_ref()
                        .is_some_and(|entry| entry.source.as_ref() != base);
                let state = new_state(&text, window, cx);
                let id = tab.id.clone();
                let subscription =
                    cx.subscribe(&state, move |this, _state, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) {
                            this.text_changed(&id, cx);
                        }
                    });
                let problems = entry
                    .as_ref()
                    .map_or_else(Vec::new, |entry| entry.problems.clone());
                self.docs.push(Doc {
                    id: tab.id,
                    state,
                    saved: base,
                    dirty,
                    problems,
                    conflict,
                    gone: entry.is_none(),
                    check: None,
                    _subscription: subscription,
                });
                let index = self.docs.len() - 1;
                self.show_problems(index, cx);
                if dirty && let Some(id) = self.docs.get(index).map(|doc| doc.id.clone()) {
                    self.text_changed(&id, cx);
                }
            }
            if !self.docs.is_empty() {
                self.active = drafts.active.min(self.docs.len() - 1);
            }
        }
        self.drafts_loaded = true;
        self.queue_drafts(cx);
        cx.notify();
    }

    fn queue_drafts(&mut self, cx: &mut Context<Self>) {
        if !self.drafts_loaded {
            return;
        }
        let Some(saver) = &self.draft_saver else {
            return;
        };
        saver.schedule(Drafts {
            directory: indicators::dir(cx).to_string_lossy().into_owned(),
            active: self.active,
            tabs: self
                .docs
                .iter()
                .map(|doc| DraftTab {
                    id: doc.id.clone(),
                    buffer: doc.dirty.then(|| doc.state.read(cx).value().to_string()),
                    base: doc.dirty.then(|| doc.saved.clone()),
                })
                .collect(),
        });
    }

    fn drag_resize(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(drag) = self.resize else { return };
        let position = match drag.side {
            ResizeSide::Console => f32::from(event.position.y),
            _ => f32::from(event.position.x),
        };
        let delta = position - drag.start;
        match drag.side {
            ResizeSide::Explorer => self.explorer_width = (drag.size + delta).clamp(170.0, 440.0),
            ResizeSide::Reference => self.reference_width = (drag.size - delta).clamp(220.0, 540.0),
            ResizeSide::Console => self.console_height = (drag.size - delta).clamp(80.0, 360.0),
        }
        cx.notify();
    }

    fn end_resize(&mut self, cx: &mut Context<Self>) {
        if self.resize.take().is_some() {
            cx.notify();
        }
    }

    /// The tab being edited.
    pub(crate) fn current(&self) -> Option<&Doc> {
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

    pub(crate) fn say(&mut self, ok: bool, text: impl Into<String>, cx: &mut Context<Self>) {
        let clear = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTICE_TIME).await;
            lost(
                "lib: say",
                this.update(cx, |this, cx| {
                    this.notice = None;
                    cx.notify();
                }),
            );
        });
        self.notice = Some(Notice {
            ok,
            text: text.into(),
            _clear: clear,
        });
        cx.notify();
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
                Position::new(lsp_index(line), lsp_index(column))
                    ..Position::new(lsp_index(line), lsp_index(column + width)),
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

#[cfg(test)]
mod draft_tests {

    use super::*;

    #[test]
    fn unsaved_multiline_script_survives_the_draft_document() {
        let drafts = Drafts {
            directory: "C:/indicators".into(),
            active: 0,
            tabs: vec![DraftTab {
                id: "SMC/Gap".into(),
                buffer: Some("indicator(#{ name: \"Gap\" });\n// unsaved\n".into()),
                base: Some("indicator(#{ name: \"Gap\" });\n".into()),
            }],
        };
        let text = toml::to_string(&drafts).unwrap();
        let restored: Drafts = toml::from_str(&text).unwrap();
        assert_eq!(restored.tabs[0].buffer, drafts.tabs[0].buffer);
        assert_eq!(restored.tabs[0].base, drafts.tabs[0].base);
    }
}
