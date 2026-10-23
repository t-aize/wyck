//! Keeps every keyboard shortcut in one place.
//!
//! - Every binding is declared in the table of `wyck_ui::shell::keymap`. A `KeyBinding::new` or a
//!   `bind_keys` anywhere else would be a shortcut the settings page does not show and the clash
//!   tests do not see.
//! - A screen that reads keys by hand (`keystroke.key`, `on_key_down`) must be one of the files
//!   named below, and its keys told in `keymap::HANDLED`, so the settings page lists them too.

use std::fs;

use crate::sources::{crates_dir, name_of, rust_files};

const KEYMAP: &str = "wyck-ui/src/shell/keymap.rs";

/// The files that read keys by hand, with what they read. Their keys are in `keymap::HANDLED`.
const READS_KEYS: &[&str] = &[
    // Up, Down, Enter and Escape in a menu.
    "wyck-ui/src/kit/menu.rs",
    // Up, Down and Enter to choose an account.
    "wyck-ui/src/shell/sign_in_gate.rs",
    // Escape to give up naming a script.
    "wyck-ui/src/editor/parts/explorer.rs",
    // The key being awaited by the page that sets the shortcuts.
    "wyck-ui/src/shell/settings_hub/shortcuts.rs",
];

const BY_HAND: &[&str] = &[
    "keystroke.key",
    ".on_key_down(",
    ".capture_key_down(",
    ".on_key_event(",
];

/// The sources of the crates that hold screens or start them, named from the `crates` folder.
const SCREEN_CRATES: &[&str] = &["wyck/src", "wyck-ui/src"];

fn sources() -> Vec<(String, String)> {
    let mut files = Vec::new();
    for krate in SCREEN_CRATES {
        rust_files(&crates_dir().join(krate), &mut files);
    }
    files
        .into_iter()
        .filter_map(|file| Some((name_of(&file), fs::read_to_string(&file).ok()?)))
        .collect()
}

#[test]
fn every_binding_is_declared_in_the_table_of_the_keymap() {
    let strays: Vec<String> = sources()
        .into_iter()
        .filter(|(name, _)| name != KEYMAP)
        .filter(|(_, text)| text.contains("KeyBinding::new(") || text.contains(".bind_keys("))
        .map(|(name, _)| name)
        .collect();
    assert!(
        strays.is_empty(),
        "bindings outside {KEYMAP} (declare them in its table): {strays:?}"
    );
}

#[test]
fn keys_read_by_hand_are_told_in_the_keymap() {
    let sources = sources();
    let undeclared: Vec<&str> = sources
        .iter()
        .filter(|(name, text)| {
            name != KEYMAP
                && !READS_KEYS.contains(&name.as_str())
                && BY_HAND.iter().any(|p| text.contains(p))
        })
        .map(|(name, _)| name.as_str())
        .collect();
    assert!(
        undeclared.is_empty(),
        "these files read keys by hand: add what they read to keymap::HANDLED, then the file to READS_KEYS in tests/guards/keymap.rs: {undeclared:?}"
    );
    let gone: Vec<&&str> = READS_KEYS
        .iter()
        .filter(|name| {
            !sources
                .iter()
                .any(|(n, text)| n == **name && BY_HAND.iter().any(|p| text.contains(p)))
        })
        .collect();
    assert!(
        gone.is_empty(),
        "READS_KEYS lists files that no longer read keys by hand: {gone:?}"
    );
}
