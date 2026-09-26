//! What the user keeps of the export panel: the options it had last, and the presets they saved.
//!
//! It is one small TOML file in the settings folder. Reading never fails: a file that is missing
//! or that cannot be read gives the defaults, so a damaged file never keeps the panel from
//! opening. Writing goes through a temporary file that is then renamed over the old one, so a
//! crash in the middle leaves the previous file whole.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{ExportOptions, Preset};

/// The name of the file, in the settings folder.
pub const FILE: &str = "export.toml";
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

/// What `dir` holds, or the defaults.
pub fn read(dir: &Path) -> Saved {
    fs::read_to_string(dir.join(FILE))
        .ok()
        .and_then(|text| toml::from_str::<Saved>(&text).ok())
        .unwrap_or_default()
        .normalized()
}

/// Writes `saved` in `dir`, making the folder when it is not there yet.
pub fn write(dir: &Path, saved: &Saved) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let text = toml::to_string_pretty(saved).map_err(io::Error::other)?;
    let target = dir.join(FILE);
    let temporary = dir.join(format!("{FILE}.tmp"));
    fs::write(&temporary, text)?;
    fs::rename(&temporary, &target).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::chart::export::{Delimiter, Format};

    /// A folder of its own for one test.
    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("wyck-export-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn semicolons() -> ExportOptions {
        ExportOptions {
            delimiter: Delimiter::Semicolon,
            ..ExportOptions::default()
        }
    }

    #[test]
    fn what_is_saved_is_read_back() {
        let dir = folder("roundtrip");
        let mut saved = Saved::default();
        saved.last.format = Format::Json;
        assert!(saved.save_preset("Mine", &semicolons()));
        write(&dir, &saved).unwrap();
        assert_eq!(read(&dir), saved.normalized());
        assert!(!dir.join(format!("{FILE}.tmp")).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_or_broken_file_gives_the_defaults() {
        let dir = folder("broken");
        assert_eq!(read(&dir), Saved::default());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(FILE), "this is [not toml").unwrap();
        assert_eq!(read(&dir), Saved::default());
        fs::write(dir.join(FILE), "").unwrap();
        assert_eq!(read(&dir), Saved::default());
        let _ = fs::remove_dir_all(&dir);
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
        let dir = folder("repair");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(FILE),
            "[last]\nevery = 0\n\n[[presets]]\nname = \"  \"\n\n[[presets]]\nname = \"A\"\n[presets.options]\nlast = 0\n\n[[presets]]\nname = \"a\"\n",
        )
        .unwrap();
        let saved = read(&dir);
        assert_eq!(saved.last.every, 1);
        assert_eq!(
            saved.presets.len(),
            1,
            "the nameless and the repeat are gone"
        );
        assert_eq!(saved.presets[0].options.last, 1);
        let _ = fs::remove_dir_all(&dir);
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
