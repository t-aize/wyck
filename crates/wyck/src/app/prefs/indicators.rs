//! What the user set about the indicators: whether the folder is read again on its own, how much a
//! script may do, and which indicators they starred. Saved in a document of its own, so a backup
//! carries it.
//!
//! Where the folder is does not belong here: a path means something on one computer only, and this
//! document can be kept in a folder that several computers share. The folder is a choice of the
//! computer (see `crate::app::locations`); an older version kept it here as `folder`, which is
//! ignored when read and moved by `crate::app::locations::prepare`.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::domain::indicators::custom::Limits;

/// The name of the document.
pub const DOCUMENT: &str = "indicators";

/// The most indicators that are starred, and that are remembered as recent.
pub const MAX_FAVORITES: usize = 64;
pub const MAX_RECENT: usize = 8;

/// How much a script may do before it is stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Budget {
    /// For a slow machine or many charts: scripts are stopped early.
    Light,
    #[default]
    Normal,
    /// For scripts that loop over the bars.
    Heavy,
}

impl Budget {
    pub const ALL: [Self; 3] = [Self::Light, Self::Normal, Self::Heavy];

    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Normal => "Normal",
            Self::Heavy => "Heavy",
        }
    }

    pub fn limits(self) -> Limits {
        match self {
            Self::Light => Limits {
                operations: 5_000_000,
                time: Duration::from_secs(2),
            },
            Self::Normal => Limits::default(),
            Self::Heavy => Limits {
                operations: 100_000_000,
                time: Duration::from_secs(10),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prefs {
    /// Whether the folder is read again every moment, so a file edited elsewhere shows up.
    #[serde(default = "crate::wyck_core::defaults::yes")]
    pub auto_reload: bool,
    #[serde(default)]
    pub budget: Budget,
    /// Whether the old Examples folder was removed.
    #[serde(default)]
    pub examples_removed: bool,
    /// The keys of the indicators the user starred (see `catalog::Item::key`).
    #[serde(default)]
    pub favorites: Vec<String>,
    /// The keys of the indicators added last, the newest first.
    #[serde(default)]
    pub recent: Vec<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            auto_reload: true,
            budget: Budget::Normal,
            examples_removed: false,
            favorites: Vec::new(),
            recent: Vec::new(),
        }
    }
}

impl Prefs {
    /// The settings repaired: the lists have no repeats and a limit.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        dedup(&mut self.favorites, MAX_FAVORITES);
        dedup(&mut self.recent, MAX_RECENT);
        self
    }

    pub fn is_favorite(&self, key: &str) -> bool {
        self.favorites.iter().any(|k| k == key)
    }

    /// Stars the indicator, or takes the star away.
    pub fn toggle_favorite(&mut self, key: &str) {
        if let Some(at) = self.favorites.iter().position(|k| k == key) {
            self.favorites.remove(at);
        } else {
            self.favorites.push(key.to_owned());
        }
    }

    /// Remembers that the indicator was added just now.
    pub fn note_recent(&mut self, key: &str) {
        self.recent.retain(|k| k != key);
        self.recent.insert(0, key.to_owned());
        self.recent.truncate(MAX_RECENT);
    }
}

/// Removes the repeats of a list (the first stays) and cuts it to `limit`.
fn dedup(list: &mut Vec<String>, limit: usize) {
    let mut seen = std::collections::HashSet::new();
    list.retain(|item| seen.insert(item.clone()));
    list.truncate(limit);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_with_nothing_in_it_reads_as_the_defaults() {
        let prefs: Prefs = toml::from_str("").unwrap();
        assert_eq!(prefs, Prefs::default());
        assert!(prefs.auto_reload);
        assert_eq!(prefs.budget, Budget::Normal);
    }

    #[test]
    fn the_settings_are_saved_and_read_back() {
        let mut prefs = Prefs {
            auto_reload: false,
            budget: Budget::Heavy,
            examples_removed: true,
            ..Prefs::default()
        };
        prefs.toggle_favorite("builtin:rsi");
        prefs.note_recent("script:a");
        let back: Prefs = toml::from_str(&toml::to_string(&prefs).unwrap()).unwrap();
        assert_eq!(back, prefs);
    }

    #[test]
    fn the_lists_are_repaired() {
        let prefs = Prefs {
            favorites: vec!["a".into(), "b".into(), "a".into()],
            ..Prefs::default()
        }
        .normalized();
        assert_eq!(prefs.favorites, ["a", "b"]);
    }

    #[test]
    fn a_document_from_an_older_version_with_a_folder_still_reads() {
        let prefs: Prefs =
            toml::from_str("folder = \"D:\\\\Indicators\"\nauto_reload = false\n").unwrap();
        assert!(!prefs.auto_reload);
        assert!(
            !toml::to_string(&prefs).unwrap().contains("folder"),
            "the old field is not written again"
        );
    }

    #[test]
    fn the_recent_list_is_newest_first_without_repeats_and_bounded() {
        let mut prefs = Prefs::default();
        for i in 0..20 {
            prefs.note_recent(&format!("k{i}"));
        }
        prefs.note_recent("k15");
        assert_eq!(prefs.recent.len(), MAX_RECENT);
        assert_eq!(prefs.recent[0], "k15");
        assert_eq!(prefs.recent.iter().filter(|k| *k == "k15").count(), 1);
    }

    #[test]
    fn a_star_is_given_and_taken_back() {
        let mut prefs = Prefs::default();
        prefs.toggle_favorite("x");
        assert!(prefs.is_favorite("x"));
        prefs.toggle_favorite("x");
        assert!(!prefs.is_favorite("x"));
    }

    #[test]
    fn a_bigger_budget_allows_more() {
        let (a, b, c) = (
            Budget::Light.limits(),
            Budget::Normal.limits(),
            Budget::Heavy.limits(),
        );
        assert!(a.operations < b.operations && b.operations < c.operations);
        assert!(a.time < b.time && b.time < c.time);
    }
}
