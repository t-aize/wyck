//! Keyboard shortcuts as data: how a key combination is written, which ones are allowed, what the
//! user changed, and which actions end up sharing a combination.
//!
//! A combination is written the way gpui writes it: modifiers, then the key, joined by `-`
//! (`secondary-shift-z`, `alt-1`, `f9`). `secondary` is Cmd on a Mac and Ctrl elsewhere, so one
//! default works on every system. What the screens bind and draw lives in `ui::shell::keymap`;
//! this file knows nothing about actions, only about ids and combinations.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// The most combinations one action can have.
pub const MAX_KEYS_PER_ACTION: usize = 3;

/// Why a combination cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChordError {
    /// The text is empty or has no key.
    #[error("no key")]
    Empty,
    /// A part of the text is not a modifier and not a key.
    #[error("\"{0}\" is not a key")]
    UnknownKey(String),
    /// The same modifier twice.
    #[error("\"{0}\" is written twice")]
    Repeated(String),
    /// A letter, a digit or a sign typed alone (or with Shift only) would be taken from the text
    /// fields.
    #[error("add Ctrl, Alt or Cmd: this key alone would be typed into text fields")]
    NeedsModifier,
}

/// A key and the modifiers held with it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Chord {
    /// Cmd on a Mac, Ctrl elsewhere.
    pub secondary: bool,
    /// The Control key itself. Only different from `secondary` on a Mac.
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The key, in gpui's lower case names (`a`, `7`, `f9`, `pageup`, `,`).
    pub key: String,
}

/// The modifiers a key event came with, as the platform reports them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    /// Cmd on a Mac, the Windows or Super key elsewhere.
    pub platform: bool,
}

/// Keys that are not text, and so can be bound with no modifier.
const NAMED: [&str; 14] = [
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "pageup",
    "pagedown",
    "delete",
    "backspace",
    "insert",
    "enter",
    "escape",
    "tab",
];

/// Keys that are not text, so they can be bound with no modifier at all.
const PLAIN_OK: [&str; 11] = [
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "pageup",
    "pagedown",
    "delete",
    "backspace",
    "insert",
];

fn is_function_key(key: &str) -> bool {
    key.strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n))
}

fn is_key(key: &str) -> bool {
    if NAMED.contains(&key) || key == "space" || is_function_key(key) {
        return true;
    }
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.is_ascii_alphanumeric() || ",./;'[]\\-=+`".contains(c),
        _ => false,
    }
}

impl Chord {
    /// Reads a combination, with the modifier names gpui knows (`cmd`, `control`, `option`...).
    ///
    /// # Errors
    ///
    /// The text has no key, or a part that is neither a modifier nor a key.
    pub fn parse(text: &str) -> Result<Self, ChordError> {
        let text = text.trim().to_lowercase();
        if text.is_empty() {
            return Err(ChordError::Empty);
        }
        // `-` is the separator and also a key: "secondary--" is the minus key.
        let (modifiers, key) = match text.strip_suffix('-') {
            Some(rest) => (rest.strip_suffix('-').unwrap_or(rest), "-"),
            None => match text.rsplit_once('-') {
                Some((modifiers, key)) => (modifiers, key),
                None => ("", text.as_str()),
            },
        };
        let mut chord = Self {
            secondary: false,
            ctrl: false,
            alt: false,
            shift: false,
            key: key.to_owned(),
        };
        for part in modifiers.split('-').filter(|p| !p.is_empty()) {
            let flag = match part {
                "secondary" | "cmd" | "super" | "win" => &mut chord.secondary,
                "ctrl" | "control" => &mut chord.ctrl,
                "alt" | "option" => &mut chord.alt,
                "shift" => &mut chord.shift,
                other => return Err(ChordError::UnknownKey(other.to_owned())),
            };
            if *flag {
                return Err(ChordError::Repeated(part.to_owned()));
            }
            *flag = true;
        }
        if !is_key(&chord.key) {
            return Err(ChordError::UnknownKey(chord.key));
        }
        Ok(chord)
    }

    /// The combination as the screens bind it, with the modifiers always in the same order.
    #[must_use]
    pub fn keys(&self) -> String {
        let mut text = String::new();
        for (on, name) in [
            (self.secondary, "secondary"),
            (self.ctrl, "ctrl"),
            (self.alt, "alt"),
            (self.shift, "shift"),
        ] {
            if on {
                text.push_str(name);
                text.push('-');
            }
        }
        text.push_str(&self.key);
        text
    }

    /// The combination as it is read on a screen: `Ctrl+Shift+Z`, `Cmd+Option+1`, `F9`.
    #[must_use]
    pub fn label(&self, mac: bool) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".into());
        }
        if self.secondary {
            parts.push(if mac { "Cmd" } else { "Ctrl" }.into());
        }
        if self.alt {
            parts.push(if mac { "Option" } else { "Alt" }.into());
        }
        if self.shift {
            parts.push("Shift".into());
        }
        parts.push(key_label(&self.key));
        parts.join("+")
    }

    /// Whether the combination may be bound.
    ///
    /// # Errors
    ///
    /// [`ChordError::NeedsModifier`] for a key that is text.
    pub fn allowed(&self) -> Result<(), ChordError> {
        let has_modifier = self.secondary || self.ctrl || self.alt;
        let plain_key = is_function_key(&self.key) || PLAIN_OK.contains(&self.key.as_str());
        if has_modifier || plain_key {
            Ok(())
        } else {
            Err(ChordError::NeedsModifier)
        }
    }

    /// The combination a key press makes, or `None` for a key that cannot be bound (a modifier
    /// alone, the Windows key, a key with no name here).
    #[must_use]
    pub fn from_event(held: Held, key: &str, mac: bool) -> Option<Self> {
        let key = key.to_lowercase();
        if !is_key(&key) {
            return None;
        }
        // On a Mac Cmd is the secondary key and Control is a key of its own. Elsewhere Control is
        // the secondary key and the Windows key cannot be used.
        let (secondary, ctrl) = if mac {
            (held.platform, held.control)
        } else if held.platform {
            return None;
        } else {
            (held.control, false)
        };
        Some(Self {
            secondary,
            ctrl,
            alt: held.alt,
            shift: held.shift,
            key,
        })
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.keys())
    }
}

fn key_label(key: &str) -> String {
    match key {
        "pageup" => "Page Up".into(),
        "pagedown" => "Page Down".into(),
        "up" => "Up".into(),
        "down" => "Down".into(),
        "left" => "Left".into(),
        "right" => "Right".into(),
        "escape" => "Esc".into(),
        other if is_function_key(other) => other.to_uppercase(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

/// Writes a combination in the canonical form, or says why it cannot be used.
///
/// # Errors
///
/// What [`Chord::parse`] and [`Chord::allowed`] say.
pub fn canonical(text: &str) -> Result<String, ChordError> {
    let chord = Chord::parse(text)?;
    chord.allowed()?;
    Ok(chord.keys())
}

/// What the user changed about the shortcuts. An action that is not here keeps its default keys;
/// an action with an empty list has no key at all.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutPrefs {
    pub changed: BTreeMap<String, Vec<String>>,
}

impl ShortcutPrefs {
    /// The combinations of an action: the ones the user chose, or `defaults`.
    #[must_use]
    pub fn keys_of(&self, id: &str, defaults: &[&str]) -> Vec<String> {
        match self.changed.get(id) {
            Some(keys) => keys.clone(),
            None => defaults.iter().map(|k| (*k).to_owned()).collect(),
        }
    }

    /// Whether the user changed the action.
    #[must_use]
    pub fn is_changed(&self, id: &str) -> bool {
        self.changed.contains_key(id)
    }

    /// Gives an action these combinations. Keys the same as the defaults are not kept as a change.
    pub fn set(&mut self, id: &str, keys: Vec<String>, defaults: &[&str]) {
        let same = keys.len() == defaults.len() && keys.iter().zip(defaults).all(|(a, b)| a == b);
        if same {
            self.changed.remove(id);
        } else {
            self.changed.insert(id.to_owned(), keys);
        }
    }

    /// Back to the default keys of one action.
    pub fn reset(&mut self, id: &str) {
        self.changed.remove(id);
    }

    /// Back to the default keys of everything.
    pub fn reset_all(&mut self) {
        self.changed.clear();
    }

    /// Puts a file that was edited by hand, or written by another version, in order: every
    /// combination is written the canonical way, the ones that cannot be used are dropped, and no
    /// action has more than [`MAX_KEYS_PER_ACTION`] or the same one twice.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        for keys in self.changed.values_mut() {
            let mut kept: Vec<String> = Vec::new();
            for key in keys.iter() {
                if let Ok(key) = canonical(key)
                    && !kept.contains(&key)
                    && kept.len() < MAX_KEYS_PER_ACTION
                {
                    kept.push(key);
                }
            }
            *keys = kept;
        }
        self
    }
}

/// An action with the combinations it ends up with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub id: String,
    /// Where the combination works: two actions only clash in the same context.
    pub context: String,
    pub keys: Vec<String>,
}

/// Two or more actions that share a combination in one context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clash {
    pub keys: String,
    pub context: String,
    pub ids: Vec<String>,
}

/// The first action, other than `id`, that uses `keys` in `context`.
#[must_use]
pub fn taken_by<'a>(
    all: &'a [Resolved],
    id: &str,
    context: &str,
    keys: &str,
) -> Option<&'a Resolved> {
    all.iter()
        .find(|r| r.id != id && r.context == context && r.keys.iter().any(|k| k == keys))
}

/// Every combination shared by two actions of one context.
#[must_use]
pub fn clashes(all: &[Resolved]) -> Vec<Clash> {
    let mut found: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for resolved in all {
        for keys in &resolved.keys {
            let ids = found
                .entry((resolved.context.clone(), keys.clone()))
                .or_default();
            if !ids.contains(&resolved.id) {
                ids.push(resolved.id.clone());
            }
        }
    }
    found
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|((context, keys), ids)| Clash { keys, context, ids })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_combination_is_written_the_same_whatever_the_order_of_its_modifiers() {
        let chord = Chord::parse("shift-alt-secondary-z").unwrap();
        assert_eq!(chord.keys(), "secondary-alt-shift-z");
        assert_eq!(Chord::parse("cmd-k").unwrap().keys(), "secondary-k");
        assert_eq!(Chord::parse("Option-1").unwrap().keys(), "alt-1");
    }

    #[test]
    fn the_minus_and_the_plus_are_keys() {
        assert_eq!(Chord::parse("secondary--").unwrap().key, "-");
        assert_eq!(Chord::parse("secondary-+").unwrap().key, "+");
        assert_eq!(Chord::parse("secondary-,").unwrap().keys(), "secondary-,");
    }

    #[test]
    fn a_text_that_is_not_a_combination_says_what_is_wrong() {
        assert_eq!(Chord::parse(""), Err(ChordError::Empty));
        assert_eq!(
            Chord::parse("secondary-banana"),
            Err(ChordError::UnknownKey("banana".into()))
        );
        assert_eq!(
            Chord::parse("hyper-a"),
            Err(ChordError::UnknownKey("hyper".into()))
        );
        assert_eq!(
            Chord::parse("alt-alt-a"),
            Err(ChordError::Repeated("alt".into()))
        );
    }

    #[test]
    fn a_key_that_is_typed_in_text_fields_needs_a_modifier() {
        assert_eq!(canonical("b"), Err(ChordError::NeedsModifier));
        assert_eq!(canonical("shift-b"), Err(ChordError::NeedsModifier));
        assert_eq!(canonical("space"), Err(ChordError::NeedsModifier));
        assert_eq!(canonical("alt-b").unwrap(), "alt-b");
        assert_eq!(canonical("f9").unwrap(), "f9");
        assert_eq!(canonical("left").unwrap(), "left");
        assert_eq!(canonical("delete").unwrap(), "delete");
    }

    #[test]
    fn a_key_press_becomes_a_combination_on_each_system() {
        let held = Held {
            control: true,
            shift: true,
            ..Held::default()
        };
        assert_eq!(
            Chord::from_event(held, "Z", false).unwrap().keys(),
            "secondary-shift-z"
        );
        // On a Mac Control is a key of its own and Cmd is the secondary one.
        assert_eq!(
            Chord::from_event(held, "z", true).unwrap().keys(),
            "ctrl-shift-z"
        );
        let cmd = Held {
            platform: true,
            ..Held::default()
        };
        assert_eq!(
            Chord::from_event(cmd, "k", true).unwrap().keys(),
            "secondary-k"
        );
        assert_eq!(Chord::from_event(cmd, "k", false), None);
        assert_eq!(Chord::from_event(Held::default(), "shift", false), None);
    }

    #[test]
    fn a_combination_is_read_the_way_the_system_names_its_keys() {
        let chord = Chord::parse("secondary-alt-pageup").unwrap();
        assert_eq!(chord.label(false), "Ctrl+Alt+Page Up");
        assert_eq!(chord.label(true), "Cmd+Option+Page Up");
        assert_eq!(Chord::parse("f9").unwrap().label(false), "F9");
        assert_eq!(Chord::parse("secondary-,").unwrap().label(false), "Ctrl+,");
        assert_eq!(Chord::parse("ctrl-p").unwrap().label(true), "Ctrl+P");
    }

    #[test]
    fn what_the_user_did_not_change_keeps_its_default_keys() {
        let mut prefs = ShortcutPrefs::default();
        assert_eq!(prefs.keys_of("undo", &["secondary-z"]), ["secondary-z"]);
        prefs.set("undo", vec!["alt-u".into()], &["secondary-z"]);
        assert_eq!(prefs.keys_of("undo", &["secondary-z"]), ["alt-u"]);
        assert!(prefs.is_changed("undo"));
        // Going back to the defaults by hand is not a change.
        prefs.set("undo", vec!["secondary-z".into()], &["secondary-z"]);
        assert!(!prefs.is_changed("undo"));
    }

    #[test]
    fn an_action_can_be_left_with_no_key_and_then_reset() {
        let mut prefs = ShortcutPrefs::default();
        prefs.set("undo", Vec::new(), &["secondary-z"]);
        assert!(prefs.keys_of("undo", &["secondary-z"]).is_empty());
        prefs.reset("undo");
        assert_eq!(prefs.keys_of("undo", &["secondary-z"]), ["secondary-z"]);
        prefs.set("redo", vec!["alt-r".into()], &[]);
        prefs.reset_all();
        assert!(prefs.changed.is_empty());
    }

    #[test]
    fn a_file_edited_by_hand_is_put_in_order() {
        let mut prefs = ShortcutPrefs::default();
        prefs.changed.insert(
            "undo".into(),
            vec![
                "shift-alt-u".into(),
                "alt-u".into(),
                "nonsense-key".into(),
                "b".into(),
                "alt-1".into(),
                "alt-2".into(),
                "alt-3".into(),
            ],
        );
        let prefs = prefs.normalized();
        assert_eq!(
            prefs.changed["undo"],
            ["alt-shift-u", "alt-u", "alt-1"],
            "canonical, no duplicate, no unusable key, three at most"
        );
    }

    #[test]
    fn two_actions_with_the_same_keys_in_one_context_clash() {
        let all = vec![
            Resolved {
                id: "a".into(),
                context: "Dashboard".into(),
                keys: vec!["alt-x".into()],
            },
            Resolved {
                id: "b".into(),
                context: "Dashboard".into(),
                keys: vec!["alt-y".into(), "alt-x".into()],
            },
            Resolved {
                id: "c".into(),
                context: "Editor".into(),
                keys: vec!["alt-x".into()],
            },
        ];
        assert_eq!(
            clashes(&all),
            vec![Clash {
                keys: "alt-x".into(),
                context: "Dashboard".into(),
                ids: vec!["a".into(), "b".into()],
            }]
        );
        assert_eq!(taken_by(&all, "a", "Dashboard", "alt-x").unwrap().id, "b");
        assert!(taken_by(&all, "c", "Editor", "alt-x").is_none());
    }

    #[test]
    fn the_serialized_form_keeps_only_what_was_changed() {
        let mut prefs = ShortcutPrefs::default();
        prefs.set("undo", vec!["alt-u".into()], &["secondary-z"]);
        let text = toml::to_string(&prefs).unwrap();
        assert!(text.contains("alt-u"));
        let back: ShortcutPrefs = toml::from_str(&text).unwrap();
        assert_eq!(back, prefs);
        let empty: ShortcutPrefs = toml::from_str("").unwrap();
        assert!(empty.changed.is_empty());
    }
}
