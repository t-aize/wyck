//! What the user keeps of the export panel: the options it had last, and the presets they saved.
//!
//! It is one small document of the [`DocumentStore`]. Reading never fails: a document that is
//! missing or that cannot be read gives the defaults (the damaged one is set aside by the store),
//! so it never keeps the panel from opening. Writing is atomic.

use serde::{Deserialize, Serialize};
use wyck_config::{DocumentStore, Result};

use super::{ExportOptions, Preset};

/// The name of the document.
pub const DOCUMENT: &str = "export";
/// The most presets kept, and the longest name of one.
const MAX_PRESETS: usize = 100;
const MAX_NAME: usize = 60;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Saved {
    /// The options of the last export.
    #[serde(default)]
    pub last: ExportOptions,
    #[serde(default)]
    pub presets: Vec<Preset>,
}

impl Saved {
    /// Repairs what was read: options in range, names cleaned, no nameless preset, no repeats.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.last = self.last.normalized();
        let mut seen: Vec<String> = Vec::new();
        self.presets = self
            .presets
            .into_iter()
            .filter_map(|mut preset| {
                preset.name = clean_name(&preset.name);
                let key = preset.name.to_lowercase();
                if preset.name.is_empty() || seen.contains(&key) {
                    return None;
                }
                seen.push(key);
                preset.options = preset.options.normalized();
                Some(preset)
            })
            .take(MAX_PRESETS)
            .collect();
        self
    }

    /// Keeps `options` as the preset `name`, replacing one of the same name (whatever its case).
    /// Returns whether it was kept: a name with nothing in it is not.
    pub fn save_preset(&mut self, name: &str, options: &ExportOptions) -> bool {
        let name = clean_name(name);
        if name.is_empty() {
            return false;
        }
        let preset = Preset {
            name: name.clone(),
            options: options.clone().normalized(),
        };
        let same = self
            .presets
            .iter()
            .position(|p| p.name.to_lowercase() == name.to_lowercase());
        match same {
            Some(at) => self.presets[at] = preset,
            None if self.presets.len() < MAX_PRESETS => self.presets.push(preset),
            None => return false,
        }
        true
    }

    /// Forgets the preset `name`. Returns whether there was one.
    pub fn remove_preset(&mut self, name: &str) -> bool {
        let before = self.presets.len();
        self.presets.retain(|p| p.name != name);
        self.presets.len() != before
    }
}

/// A preset name: trimmed, on one line, not too long.
pub fn clean_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_NAME)
        .collect()
}

/// What the store holds, or the defaults.
pub fn read(store: &DocumentStore) -> Saved {
    store.load_or_default::<Saved>(DOCUMENT).normalized()
}

/// Writes `saved` in the store.
///
/// # Errors
///
/// Any error of [`DocumentStore::save`].
pub fn write(store: &DocumentStore, saved: &Saved) -> Result<()> {
    store.save(DOCUMENT, saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{Delimiter, Format};

    /// A store of its own for one test.
    fn store() -> (tempfile::TempDir, DocumentStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = DocumentStore::global(&wyck_config::AppPaths::at(dir.path()));
        (dir, store)
    }

    fn semicolons() -> ExportOptions {
        ExportOptions {
            delimiter: Delimiter::Semicolon,
            ..ExportOptions::default()
        }
    }

    #[test]
    fn what_is_saved_is_read_back() {
        let (_dir, store) = store();
        let mut saved = Saved::default();
        saved.last.format = Format::Json;
        assert!(saved.save_preset("Mine", &semicolons()));
        write(&store, &saved).unwrap();
        assert_eq!(read(&store), saved.normalized());
    }

    #[test]
    fn a_missing_or_broken_file_gives_the_defaults() {
        let (_dir, store) = store();
        assert_eq!(read(&store), Saved::default());
        store.save_text(DOCUMENT, "").unwrap();
        assert_eq!(read(&store), Saved::default());
        std::fs::write(store.path(DOCUMENT), "this is [not toml").unwrap();
        assert_eq!(read(&store), Saved::default());
        assert!(
            store.dir().join("export.toml.bad").is_file(),
            "the damaged document is kept aside"
        );
    }

    #[test]
    fn a_preset_of_the_same_name_is_replaced_whatever_its_case() {
        let mut saved = Saved::default();
        assert!(saved.save_preset("Daily", &ExportOptions::default()));
        assert!(saved.save_preset("  daily  ", &semicolons()));
        assert_eq!(saved.presets.len(), 1);
        assert_eq!(saved.presets[0].name, "daily");
        assert_eq!(saved.presets[0].options.delimiter, Delimiter::Semicolon);
        assert!(!saved.save_preset("   ", &ExportOptions::default()));
        assert!(saved.remove_preset("daily"));
        assert!(!saved.remove_preset("daily"));
        assert!(saved.presets.is_empty());
    }

    #[test]
    fn names_are_kept_on_one_line_and_short() {
        assert_eq!(clean_name("  a \n b\t c "), "a b c");
        assert_eq!(clean_name(&"x".repeat(200)).len(), MAX_NAME);
    }

    #[test]
    fn what_is_read_is_repaired() {
        let (_dir, store) = store();
        store
            .save_text(
                DOCUMENT,
                "[last]\nevery = 0\n\n[[presets]]\nname = \"  \"\n\n[[presets]]\nname = \"A\"\n[presets.options]\nlast = 0\n\n[[presets]]\nname = \"a\"\n",
            )
            .unwrap();
        let saved = read(&store);
        assert_eq!(saved.last.every, 1);
        assert_eq!(
            saved.presets.len(),
            1,
            "the nameless and the repeat are gone"
        );
        assert_eq!(saved.presets[0].options.last, 1);
    }

    #[test]
    fn there_is_a_limit_to_the_presets() {
        let mut saved = Saved::default();
        for n in 0..MAX_PRESETS {
            assert!(saved.save_preset(&format!("p{n}"), &ExportOptions::default()));
        }
        assert!(!saved.save_preset("one more", &ExportOptions::default()));
        // Replacing one still works.
        assert!(saved.save_preset("p3", &semicolons()));
        assert_eq!(saved.presets.len(), MAX_PRESETS);
    }
}
