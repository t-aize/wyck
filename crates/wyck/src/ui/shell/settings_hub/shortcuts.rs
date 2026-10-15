//! The Shortcuts page of the settings: every key of the app, the ones that can be changed and the
//! ones that cannot.
//!
//! A key of a row that can be changed is a button. Pressing it waits for the new combination (Esc
//! gives up), a plus adds a second key to the action, the eraser leaves it with no key and the
//! arrow puts the default back. When another action of the same place already uses the
//! combination, the page says which and asks whether to take it from there; a combination that is
//! a fixed key, or one the widgets use everywhere, is refused.

use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, KeyDownEvent, SharedString, Window, div, px};

use super::{IconName, SettingsHub, button, form, theme, tokens};
use crate::ui::shell::keymap::{self, Def, Group, Kind};
use wyck_app::shortcuts::{Chord, ChordError, Held, MAX_KEYS_PER_ACTION, clashes, taken_by};
use wyck_ui_kit::input::InputState;

/// The key being waited for: which action, and which of its keys (`None` adds one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Capture {
    pub id: &'static str,
    pub slot: Option<usize>,
}

/// A combination waiting for the user to say whether it is taken from the action that has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Swap {
    pub capture: Capture,
    pub keys: String,
    pub from: &'static str,
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
        Group::Text => IconName::Type,
        Group::Dialogs => IconName::MessageSquare,
        Group::Picker => IconName::Search,
    }
}

/// What a combination is called on a screen.
fn show(keys: &str) -> String {
    Chord::parse(keys).map_or_else(|_| keys.to_owned(), |chord| chord.label(mac()))
}

/// Whether what was typed in the search field matches a name or one of its keys.
fn matches(label: &str, keys: &[String], query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || label.to_lowercase().contains(&query)
        || keys.iter().any(|k| k.to_lowercase().contains(&query))
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

/// A key as a small box.
fn chip(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .h(px(tokens::height::compact()))
        .px_2()
        .rounded_md()
        .border_1()
        .border_color(theme::border_subtle())
        .text_size(px(tokens::text::body()))
        .text_color(theme::muted_fg())
        .child(text.into())
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

        let mut head = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(div().flex_1().child(form::text_field(
                &self.shortcut_search,
                tokens::field::wide(),
            )));
        if !prefs.changed.is_empty() {
            let this = cx.entity();
            head = head.child(
                button::quiet("shortcuts-reset-all")
                    .icon(IconName::RotateCcw)
                    .label("Reset all")
                    .on_click(move |_, _, cx| {
                        this.update(cx, |hub, cx| hub.edit_shortcuts(cx, |p| p.reset_all()));
                    }),
            );
        }
        let mut page = form::page().child(head).child(form::note(
            "Click a key, then press the new combination. Esc gives up. Two actions of one place cannot share a key. The keys with a lock are fixed.",
        ));

        let mut shown = 0;
        for group in Group::ALL {
            let mut rows: Vec<AnyElement> = Vec::new();
            for def in keymap::DEFS.iter().filter(|def| def.group == group) {
                let keys = keymap::keys_of(def, &prefs);
                let shown_keys: Vec<String> = keys.iter().map(|k| show(k)).collect();
                if !matches(def.label, &shown_keys, &query) {
                    continue;
                }
                match def.kind {
                    Kind::Custom => rows.push(self.shortcut_row(def, &keys, &prefs, cx)),
                    Kind::Fixed => rows.push(self.fixed_row(def, &shown_keys)),
                }
                if def.kind == Kind::Custom {
                    rows.extend(self.row_notes(def, &shared, cx));
                }
            }
            for handled in keymap::HANDLED.iter().filter(|h| h.group == group) {
                let keys: Vec<String> = handled.keys.iter().map(|k| (*k).to_owned()).collect();
                if matches(handled.label, &keys, &query) {
                    rows.push(Self::handled_row(handled.label, handled.context, &keys));
                }
            }
            if rows.is_empty() {
                continue;
            }
            shown += rows.len();
            let changed_here = keymap::DEFS
                .iter()
                .any(|d| d.group == group && d.kind == Kind::Custom && prefs.is_changed(d.id));
            let reset = changed_here.then(|| {
                let this = cx.entity();
                button::icon(
                    SharedString::from(format!("shortcuts-reset-{group:?}")),
                    wyck_ui_kit::button::Size::Md,
                    IconName::RotateCcw,
                    "Back to the defaults of this group",
                )
                .on_click(move |_, _, cx| {
                    this.update(cx, |hub, cx| {
                        hub.edit_shortcuts(cx, |p| {
                            for def in keymap::DEFS
                                .iter()
                                .filter(|d| d.group == group && d.kind == Kind::Custom)
                            {
                                p.reset(def.id);
                            }
                        });
                    });
                })
                .into_any_element()
            });
            page = page.child(form::group_with(
                group_icon(group),
                group.label(),
                reset,
                rows,
            ));
        }
        page = page.child(self.library_group(&query, &mut shown, cx));
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

    /// The keys of the widget library, which the app does not own and cannot change, behind a
    /// fold.
    fn library_group(&self, query: &str, shown: &mut usize, cx: &mut Context<Self>) -> AnyElement {
        let all = keymap::library_keys(cx);
        let found: Vec<_> = all
            .iter()
            .filter(|k| {
                matches(
                    &format!("{} {}", k.action, k.context),
                    std::slice::from_ref(&k.keys),
                    query,
                )
            })
            .collect();
        if found.is_empty() {
            return div().into_any_element();
        }
        *shown += found.len();
        let open = self.library_open || !query.trim().is_empty();
        let this = cx.entity();
        let toggle = button::quiet("shortcuts-library")
            .icon(if open {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
            .label(if open { "Hide" } else { "Show" })
            .on_click(move |_, _, cx| {
                this.update(cx, |hub, cx| {
                    hub.library_open = !hub.library_open;
                    cx.notify();
                });
            })
            .into_any_element();
        let rows: Vec<AnyElement> = if open {
            found
                .iter()
                .map(|k| Self::handled_row(&k.action, &k.context, std::slice::from_ref(&k.keys)))
                .collect()
        } else {
            vec![
                form::note(format!(
                    "{} more keys come with the widgets (lists, tables, menus, the date picker). They work where the widget is.",
                    found.len()
                ))
                .into_any_element(),
            ]
        };
        form::group_with(IconName::Layers, "Keys of the widgets", Some(toggle), rows)
            .into_any_element()
    }

    /// A row for a key the app cannot change: the keys, a lock.
    fn fixed_row(&self, def: &Def, keys: &[String]) -> AnyElement {
        let mut chips = div().flex().flex_row().items_center().gap_1();
        for key in keys {
            chips = chips.child(chip(key.clone()));
        }
        chips = chips.child(wyck_ui_kit::icon::tinted(
            IconName::Lock,
            13.,
            theme::muted_fg(),
        ));
        form::Row::new(def.label).control(chips)
    }

    /// A row for keys a screen reads itself, or the library does.
    fn handled_row(label: &str, context: &str, keys: &[String]) -> AnyElement {
        let mut chips = div().flex().flex_row().items_center().gap_1();
        for key in keys {
            chips = chips.child(chip(key.clone()));
        }
        chips = chips.child(wyck_ui_kit::icon::tinted(
            IconName::Lock,
            13.,
            theme::muted_fg(),
        ));
        form::Row::new(label.to_owned())
            .hint(context.to_owned())
            .control(chips)
    }

    /// What is said under a row: that two actions share a key, that a combination was refused,
    /// that one waits for a decision.
    fn row_notes(
        &self,
        def: &'static Def,
        shared: &[wyck_app::shortcuts::Clash],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut notes = Vec::new();
        if let Some(clash) = shared
            .iter()
            .find(|c| c.context == def.context && c.ids.iter().any(|id| id == def.id))
        {
            let others: Vec<&str> = clash
                .ids
                .iter()
                .filter(|id| id.as_str() != def.id)
                .filter_map(|id| keymap::def(id).map(|d| d.label))
                .collect();
            notes.push(
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
            notes.push(form::note(text.clone()).into_any_element());
        }
        if let Some(swap) = self.swap.as_ref().filter(|s| s.capture.id == def.id) {
            let from = keymap::def(swap.from).map_or("another action", |d| d.label);
            let (take, keep) = (cx.entity(), cx.entity());
            notes.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .py_1()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::fg())
                            .child(format!(
                                "{} is used by \"{from}\". Take it from there?",
                                show(&swap.keys)
                            )),
                    )
                    .child(
                        button::accent("shortcuts-swap-take")
                            .label("Take it")
                            .on_click(move |_, _, cx| {
                                take.update(cx, |hub, cx| hub.take_swap(cx));
                            }),
                    )
                    .child(
                        button::quiet("shortcuts-swap-keep")
                            .label("Keep it there")
                            .on_click(move |_, _, cx| {
                                keep.update(cx, |hub, cx| {
                                    hub.swap = None;
                                    cx.notify();
                                });
                            }),
                    )
                    .into_any_element(),
            );
        }
        notes
    }

    fn shortcut_row(
        &self,
        def: &'static Def,
        keys: &[String],
        prefs: &wyck_app::shortcuts::ShortcutPrefs,
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
                    wyck_ui_kit::button::Size::Md,
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
        self.swap = None;
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
        let say = |hub: &mut Self, text: String, cx: &mut Context<Self>| {
            hub.shortcut_note = Some((capture.id.to_owned(), text));
            cx.notify();
        };
        if let Err(error) = chord.allowed() {
            say(self, refusal(&error), cx);
            return;
        }
        let keys = chord.keys();
        let prefs = self.workspace.read(cx).preferences().shortcuts.clone();
        let all = keymap::resolved(&prefs);
        if let Some(other) = taken_by(&all, def.id, def.context, &keys) {
            let Some(other_def) = keymap::def(&other.id) else {
                return;
            };
            if other_def.kind == Kind::Fixed {
                say(
                    self,
                    format!(
                        "{} is a fixed key of \"{}\". Choose another.",
                        show(&keys),
                        other_def.label
                    ),
                    cx,
                );
            } else {
                self.swap = Some(Swap {
                    capture,
                    keys,
                    from: other_def.id,
                });
                cx.notify();
            }
            return;
        }
        if let Some(library) = keymap::reserved_by_library(&keys, cx) {
            say(
                self,
                format!(
                    "{} is used by the widgets everywhere in the app ({}). Choose another.",
                    show(&keys),
                    library.action.to_lowercase()
                ),
                cx,
            );
            return;
        }
        self.shortcut_note = None;
        self.edit_shortcuts(cx, |p| Self::put(p, def, capture.slot, keys));
    }

    /// Takes the combination from the action that has it and gives it to the one that asked.
    fn take_swap(&mut self, cx: &mut Context<Self>) {
        let Some(swap) = self.swap.take() else {
            return;
        };
        let (Some(def), Some(from)) = (keymap::def(swap.capture.id), keymap::def(swap.from)) else {
            return;
        };
        self.edit_shortcuts(cx, |p| {
            let rest: Vec<String> = p
                .keys_of(from.id, from.keys)
                .into_iter()
                .filter(|k| *k != swap.keys)
                .collect();
            p.set(from.id, rest, from.keys);
            Self::put(p, def, swap.capture.slot, swap.keys.clone());
        });
    }

    /// Gives an action a combination, in the slot that was asked or after its other keys.
    fn put(
        prefs: &mut wyck_app::shortcuts::ShortcutPrefs,
        def: &Def,
        slot: Option<usize>,
        keys: String,
    ) {
        let mut list = prefs.keys_of(def.id, def.keys);
        match slot {
            Some(slot) if slot < list.len() => list[slot] = keys,
            _ => list.push(keys),
        }
        list.dedup();
        prefs.set(def.id, list, def.keys);
    }

    /// Changes the shortcuts, saves them and binds the keys again.
    fn edit_shortcuts(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut wyck_app::shortcuts::ShortcutPrefs),
    ) {
        self.shortcut_note = None;
        self.swap = None;
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
        let keys = vec!["Ctrl+Z".to_owned()];
        assert!(matches("Undo", &keys, ""));
        assert!(matches("Undo", &keys, "UNDO"));
        assert!(matches("Undo", &keys, "ctrl+z"));
        assert!(!matches("Undo", &keys, "kill"));
    }

    #[test]
    fn a_refusal_says_what_to_do() {
        assert!(refusal(&ChordError::NeedsModifier).contains("Add Ctrl"));
        assert!(refusal(&ChordError::Empty).contains("cannot be used"));
    }

    #[test]
    fn a_combination_is_put_in_the_slot_asked_or_after_the_others() {
        use wyck_app::shortcuts::ShortcutPrefs;
        let zoom = keymap::def("zoom_in").unwrap();
        let mut prefs = ShortcutPrefs::default();
        SettingsHub::put(&mut prefs, zoom, Some(1), "alt-z".into());
        assert_eq!(prefs.keys_of(zoom.id, zoom.keys), ["secondary-=", "alt-z"]);
        SettingsHub::put(&mut prefs, zoom, None, "alt-y".into());
        assert_eq!(
            prefs.keys_of(zoom.id, zoom.keys),
            ["secondary-=", "alt-z", "alt-y"]
        );
        // The same key twice in a row is one key.
        SettingsHub::put(&mut prefs, zoom, None, "alt-y".into());
        assert_eq!(prefs.keys_of(zoom.id, zoom.keys).len(), 3);
    }

    #[test]
    fn a_combination_taken_from_another_action_leaves_it_with_the_rest() {
        use wyck_app::shortcuts::ShortcutPrefs;
        let redo = keymap::def("redo_drawing").unwrap();
        let mut prefs = ShortcutPrefs::default();
        let rest: Vec<String> = prefs
            .keys_of(redo.id, redo.keys)
            .into_iter()
            .filter(|k| k != "secondary-y")
            .collect();
        prefs.set(redo.id, rest, redo.keys);
        assert_eq!(prefs.keys_of(redo.id, redo.keys), ["secondary-shift-z"]);
    }
}
