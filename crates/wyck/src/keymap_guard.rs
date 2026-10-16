//! Keeps every keyboard shortcut in one place.
//!
//! - Every binding is declared in the table of `ui::shell::keymap`. A `KeyBinding::new` or a
//!   `bind_keys` anywhere else would be a shortcut the settings page does not show and the clash
//!   tests do not see.
//! - A screen that reads keys by hand (`keystroke.key`, `on_key_down`) must be one of the files
//!   named below, and its keys told in `keymap::HANDLED`, so the settings page lists them too.

use std::fs;
use std::path::{Path, PathBuf};

/// The files that read keys by hand, with what they read. Their keys are in `keymap::HANDLED`.
const READS_KEYS: &[&str] = &[
    // Up, Down, Enter and Escape in a menu.
    "wyck-ui-kit/src/menu.rs",
    // Up, Down and Enter to choose an account.
    "wyck-ui-shell/src/sign_in_gate.rs",
    // Escape to give up naming a script.
    "wyck-ui-editor/src/parts/explorer.rs",
    // The key being awaited by the page that sets the shortcuts.
    "wyck-ui-shell/src/settings_hub/shortcuts.rs",
];

const BY_HAND: &[&str] = &[
    "keystroke.key",
    ".on_key_down(",
    ".capture_key_down(",
    ".on_key_event(",
];

pub(crate) fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    // A folder that moved must fail the test, not make it pass on nothing.
    let entries =
        fs::read_dir(dir).unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The sources of the crates that hold screens, named from the `crates` folder.
const SCREEN_CRATES: &[&str] = &[
    "wyck/src",
    "wyck-ui-chart/src",
    "wyck-ui-editor/src",
    "wyck-ui-kit/src",
    "wyck-ui-shell/src",
    "wyck-ui-trading/src",
];

fn sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for krate in SCREEN_CRATES {
        rust_files(&root.join(krate), &mut files);
    }
    files
        .into_iter()
        .filter_map(|file| {
            let name = file
                .strip_prefix(&root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            Some((name, fs::read_to_string(&file).ok()?))
        })
        .filter(|(name, _)| name != "wyck/src/keymap_guard.rs")
        .collect()
}

#[test]
fn every_binding_is_declared_in_the_table_of_the_keymap() {
    let strays: Vec<String> = sources()
        .into_iter()
        .filter(|(name, _)| name != "wyck-ui-shell/src/keymap.rs")
        .filter(|(_, text)| text.contains("KeyBinding::new(") || text.contains(".bind_keys("))
        .map(|(name, _)| name)
        .collect();
    assert!(
        strays.is_empty(),
        "bindings outside wyck-ui-shell/src/keymap.rs (declare them in its table): {strays:?}"
    );
}

#[test]
fn keys_read_by_hand_are_told_in_the_keymap() {
    let sources = sources();
    let undeclared: Vec<&str> = sources
        .iter()
        .filter(|(name, text)| {
            name != "wyck-ui-shell/src/keymap.rs"
                && !READS_KEYS.contains(&name.as_str())
                && BY_HAND.iter().any(|p| text.contains(p))
        })
        .map(|(name, _)| name.as_str())
        .collect();
    assert!(
        undeclared.is_empty(),
        "these files read keys by hand: add what they read to keymap::HANDLED, then the file to READS_KEYS in keymap_guard.rs: {undeclared:?}"
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
