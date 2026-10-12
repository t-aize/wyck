//! The Shortcuts page of the settings: every action with its keys, and the means to change them.
//!
//! A key of a row is a button. Pressing it waits for the new combination (Esc gives up), a plus
//! adds a second key to the action, the eraser leaves it with no key and the arrow puts the
//! default back. A combination another action of the same place already uses is refused, with
//! the name of that action, so a shortcut never stops working without anyone knowing it.

use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, KeyDownEvent, SharedString, Window, div};

use super::{IconName, SettingsHub, button, form};
use crate::domain::shortcuts::{Chord, ChordError, Held, MAX_KEYS_PER_ACTION, clashes, taken_by};
use crate::ui::kit::input::InputState;
use crate::ui::kit::prelude::Disableable;
use crate::ui::shell::keymap::{self, Def, Group};

/// The key being waited for: which action, and which of its keys (`None` adds one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Capture {
    pub id: &'static str,
    pub slot: Option<usize>,
}

fn mac() -> bool {
    cfg!(target_os = "macos")
}

fn group_icon(group: Group) -> IconName {
    match group {
        Group::Navigation => IconName::Compass,
        Group::Trading => IconName::ArrowLeftRight,
        Group::Chart => IconName::ChartCandlestick,
        Group::Drawing => IconName::PenLine,
        Group::Editor => IconName::CodeXml,
    }
}

/// What a combination is called on a screen.
fn show(keys: &str) -> String {
    Chord::parse(keys).map_or_else(|_| keys.to_owned(), |chord| chord.label(mac()))
}

/// Whether `def` matches what was typed in the search field: its name or one of its keys.
fn matches(def: &Def, keys: &[String], query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || def.label.to_lowercase().contains(&query)
        || keys.iter().any(|k| show(k).to_lowercase().contains(&query))
}

/// Why a combination was refused, in words.
fn refusal(error: &ChordError) -> String {
    match error {
        ChordError::NeedsModifier => {
            "That key would be typed into text fields. Add Ctrl, Alt or Cmd.".to_owned()
        }
        other => format!("That combination cannot be used: {other}."),
    }
}

impl SettingsHub {
    pub(super) fn new_shortcut_search(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        cx.new(|cx| InputState::new(window, cx).placeholder("Search an action or a key"))
    }

    pub(super) fn shortcuts_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefs = self.workspace.read(cx).preferences().shortcuts.clone();
        let all = keymap::resolved(&prefs);
        let query = self.shortcut_search.read(cx).value().to_string();
        let shared = clashes(&all);

        let reset_all = {
            let this = cx.entity();
            button::quiet("shortcuts-reset-all")
                .icon(IconName::RotateCcw)
                .label("Reset all")
                .disabled(prefs.changed.is_empty())
                .on_click(move |_, _, cx| {
                    this.update(cx, |hub, cx| hub.edit_shortcuts(cx, |p| p.reset_all()));
                })
        };
        let mut page = form::page().child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(div().flex_1().child(form::text_field(
                    &self.shortcut_search,
                    crate::ui::kit::tokens::field::wide(),
                )))
                .child(reset_all),
        );
        page = page.child(form::note(
            "Click a key, then press the new combination. Esc gives up. Several actions can wait for the same key in different places, but not in one.",
        ));

        let mut shown = 0;
        for group in Group::ALL {
            let mut rows: Vec<AnyElement> = Vec::new();
            for def in keymap::DEFS.iter().filter(|def| def.group == group) {
                let keys = prefs.keys_of(def.id, def.keys);
                if !matches(def, &keys, &query) {
                    continue;
                }
                rows.push(self.shortcut_row(def, &keys, &prefs, cx));
                let clash = shared
                    .iter()
                    .find(|c| c.context == def.context && c.ids.iter().any(|id| id == def.id));
                if let Some(clash) = clash {
                    let others: Vec<&str> = clash
                        .ids
                        .iter()
                        .filter(|id| id.as_str() != def.id)
                        .filter_map(|id| keymap::def(id).map(|d| d.label))
                        .collect();
                    rows.push(
                        form::note(format!(
                            "{} is also used by {}. Only one of them will work.",
                            show(&clash.keys),
                            others.join(", ")
                        ))
                        .into_any_element(),
                    );
                }
                if let Some((id, text)) = &self.shortcut_note
                    && id == def.id
                {
                    rows.push(form::note(text.clone()).into_any_element());
                }
            }
            if !rows.is_empty() {
                shown += rows.iter().filter(|_| true).count();
                page = page.child(form::group(group_icon(group), group.label(), rows));
            }
        }
        if shown == 0 {
            page = page.child(form::empty(
                IconName::Search,
                "No action or key matches what you typed.",
            ));
        }
        div()
            .id("shortcuts-page")
            .track_focus(&self.capture_focus)
            .capture_key_down(cx.listener(Self::on_capture_key))
            .child(page)
            .into_any_element()
    }

    fn shortcut_row(
        &self,
        def: &'static Def,
        keys: &[String],
        prefs: &crate::domain::shortcuts::ShortcutPrefs,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = def.id;
        let this = cx.entity();
        let mut controls = div().flex().flex_row().items_center().gap_1();
        for (slot, keys) in keys.iter().enumerate() {
            let waiting = self.capturing
                == Some(Capture {
                    id,
                    slot: Some(slot),
                });
            let this = this.clone();
            let button = button::outlined(SharedString::from(format!("shortcut-{id}-{slot}")))
                .label(if waiting {
                    "Press the keys".to_owned()
                } else {
                    show(keys)
                })
                .toggled(waiting)
                .tooltip("Change this key")
                .on_click(move |_, window, cx| {
                    this.update(cx, |hub, cx| {
                        hub.start_capture(
                            Capture {
                                id,
                                slot: Some(slot),
                            },
                            window,
                            cx,
                        );
                    });
                });
            controls = controls.child(button);
        }
        if keys.len() < MAX_KEYS_PER_ACTION {
            let waiting = self.capturing == Some(Capture { id, slot: None });
            let this = this.clone();
            controls = controls.child(
                button::quiet(SharedString::from(format!("shortcut-{id}-add")))
                    .icon(IconName::Plus)
                    .label(if waiting {
                        "Press the keys".to_owned()
                    } else if keys.is_empty() {
                        "Set a key".to_owned()
                    } else {
                        String::new()
                    })
                    .toggled(waiting)
                    .tooltip("Add a key")
                    .on_click(move |_, window, cx| {
                        this.update(cx, |hub, cx| {
                            hub.start_capture(Capture { id, slot: None }, window, cx);
                        });
                    }),
            );
        }
        if !keys.is_empty() {
            let this = this.clone();
            controls = controls.child(
                button::icon(
                    SharedString::from(format!("shortcut-{id}-clear")),
                    IconName::Eraser,
                    "Leave without a key",
                )
                .on_click(move |_, _, cx| {
                    this.update(cx, |hub, cx| {
                        hub.edit_shortcuts(cx, |p| p.set(id, Vec::new(), def.keys));
                    });
                }),
            );
        }
        let reset = this.clone();
        form::Row::new(def.label)
            .reset(prefs.is_changed(id), move |_, cx| {
                reset.update(cx, |hub, cx| hub.edit_shortcuts(cx, |p| p.reset(id)));
            })
            .control(controls)
    }

    fn start_capture(&mut self, capture: Capture, window: &mut Window, cx: &mut Context<Self>) {
        self.capturing = Some(capture);
        self.shortcut_note = None;
        window.focus(&self.capture_focus, cx);
        cx.notify();
    }

    /// A key was pressed on the page. While a key is awaited it is the new combination, and
    /// nothing else sees it, so that pressing a shortcut that already exists does not run it.
    fn on_capture_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(capture) = self.capturing else {
            return;
        };
        cx.stop_propagation();
        let keystroke = &event.keystroke;
        let held = Held {
            control: keystroke.modifiers.control,
            alt: keystroke.modifiers.alt,
            shift: keystroke.modifiers.shift,
            platform: keystroke.modifiers.platform,
        };
        let none_held = !(held.control || held.alt || held.shift || held.platform);
        if keystroke.key == "escape" && none_held {
            self.capturing = None;
            cx.notify();
            return;
        }
        let Some(chord) = Chord::from_event(held, &keystroke.key, mac()) else {
            return;
        };
        self.assign(capture, chord, cx);
    }

    fn assign(&mut self, capture: Capture, chord: Chord, cx: &mut Context<Self>) {
        let Some(def) = keymap::def(capture.id) else {
            self.capturing = None;
            return;
        };
        self.capturing = None;
        if let Err(error) = chord.allowed() {
            self.shortcut_note = Some((capture.id.to_owned(), refusal(&error)));
            cx.notify();
            return;
        }
        let keys = chord.keys();
        let prefs = self.workspace.read(cx).preferences().shortcuts.clone();
        let all = keymap::resolved(&prefs);
        if let Some(other) = taken_by(&all, def.id, def.context, &keys) {
            let name = keymap::def(&other.id).map_or("another action", |d| d.label);
            self.shortcut_note = Some((
                capture.id.to_owned(),
                format!(
                    "{} is already used by \"{name}\". Choose another, or change that one first.",
                    show(&keys)
                ),
            ));
            cx.notify();
            return;
        }
        let mut list = prefs.keys_of(def.id, def.keys);
        match capture.slot {
            Some(slot) if slot < list.len() => list[slot] = keys.clone(),
            _ => list.push(keys.clone()),
        }
        list.dedup();
        self.shortcut_note = None;
        self.edit_shortcuts(cx, |p| p.set(def.id, list, def.keys));
    }

    /// Changes the shortcuts, saves them and binds the keys again.
    fn edit_shortcuts(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut crate::domain::shortcuts::ShortcutPrefs),
    ) {
        self.shortcut_note = None;
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| change(&mut prefs.shortcuts));
        });
        let shortcuts = self.workspace.read(cx).preferences().shortcuts.clone();
        keymap::apply(&shortcuts, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_search_finds_an_action_by_its_name_or_by_its_key() {
        let undo = keymap::def("undo_drawing").unwrap();
        let keys = vec!["secondary-z".to_owned()];
        assert!(matches(undo, &keys, ""));
        assert!(matches(undo, &keys, "UNDO"));
        assert!(matches(undo, &keys, "ctrl+z") || matches(undo, &keys, "cmd+z"));
        assert!(!matches(undo, &keys, "kill"));
    }

    #[test]
    fn a_refusal_says_what_to_do() {
        assert!(refusal(&ChordError::NeedsModifier).contains("Add Ctrl"));
        assert!(refusal(&ChordError::Empty).contains("cannot be used"));
    }
}
