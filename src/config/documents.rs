//! Typed TOML documents next to the app config (watchlist, risk limits).

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use tracing::{debug, warn};

use crate::config::error::{ConfigError, Result};
use crate::config::fs_util::atomic_write;
use crate::config::paths::AppPaths;

const MAX_NAME_LEN: usize = 100;

/// `name` with every character that is not a letter, digit, `-` or `_` replaced by `_`, so a
/// name can never point outside its folder.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .take(MAX_NAME_LEN)
        .collect();
    if cleaned.is_empty() {
        "_".to_owned()
    } else {
        cleaned
    }
}

/// A directory of TOML documents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentStore {
    dir: PathBuf,
}

impl DocumentStore {
    /// The documents of one scope, such as `demo-45970491`.
    pub fn scoped(paths: &AppPaths, scope: &str) -> Self {
        Self {
            dir: paths.scopes_dir().join(sanitize(scope)),
        }
    }

    /// Where the document `name` lives.
    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.toml", sanitize(name)))
    }

    /// The directory the documents are in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Reads a document, or `None` when it was never written.
    pub fn load<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>> {
        let path = self.path(name);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(ConfigError::Read { path, source }),
        };
        let value = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source: Box::new(source),
        })?;
        debug!(path = %path.display(), "loaded a document");
        Ok(Some(value))
    }

    /// Reads a document, and never fails: a document that was never written gives the default,
    /// and one that cannot be read or parsed is set aside next to itself (as `name.toml.bad`,
    /// replacing an older one) and gives the default too.
    pub fn load_or_default<T: DeserializeOwned + Default>(&self, name: &str) -> T {
        match self.load(name) {
            Ok(Some(value)) => value,
            Ok(None) => T::default(),
            Err(error) => {
                warn!(%error, document = name, "a saved document could not be read, starting fresh");
                self.set_aside(name);
                T::default()
            }
        }
    }

    /// Writes a document, atomically: a crash halfway leaves the old file, never half of a new
    /// one.
    pub fn save<T: Serialize>(&self, name: &str, value: &T) -> Result<()> {
        let text = toml::to_string_pretty(value).map_err(ConfigError::Serialize)?;
        let path = self.path(name);
        atomic_write(&path, text.as_bytes())?;
        debug!(path = %path.display(), bytes = text.len(), "saved a document");
        Ok(())
    }

    fn set_aside(&self, name: &str) {
        let path = self.path(name);
        let mut bad = path.clone().into_os_string();
        bad.push(".bad");
        if let Err(error) = std::fs::rename(&path, PathBuf::from(&bad)) {
            warn!(%error, path = %path.display(), "could not set the damaged document aside");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::collections::BTreeMap;

    #[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
    struct Sample {
        #[serde(default)]
        title: String,
        #[serde(default)]
        numbers: Vec<i64>,
        #[serde(default)]
        by_symbol: BTreeMap<String, Vec<f64>>,
    }

    fn store() -> (tempfile::TempDir, DocumentStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = DocumentStore::scoped(&AppPaths::at(dir.path()), "test");
        (dir, store)
    }

    #[test]
    fn a_document_that_was_never_written_is_absent_not_an_error() {
        let (_dir, store) = store();
        assert_eq!(store.load::<Sample>("nothing").unwrap(), None);
        assert_eq!(
            store.load_or_default::<Sample>("nothing"),
            Sample::default()
        );
    }

    #[test]
    fn a_document_round_trips_including_awkward_keys() {
        let (_dir, store) = store();
        let mut by_symbol = BTreeMap::new();
        by_symbol.insert("US100.cash".to_owned(), vec![1.5, -2.0]);
        by_symbol.insert("EUR/USD \"quoted\"".to_owned(), vec![]);
        let sample = Sample {
            title: "Scalping".into(),
            numbers: vec![-3, 0, i64::MAX],
            by_symbol,
        };
        store.save("layout", &sample).unwrap();
        assert_eq!(store.load::<Sample>("layout").unwrap(), Some(sample));
    }

    #[test]
    fn saving_replaces_the_previous_contents() {
        let (_dir, store) = store();
        store
            .save(
                "a",
                &Sample {
                    title: "one".into(),
                    ..Sample::default()
                },
            )
            .unwrap();
        store
            .save(
                "a",
                &Sample {
                    title: "two".into(),
                    ..Sample::default()
                },
            )
            .unwrap();
        assert_eq!(store.load::<Sample>("a").unwrap().unwrap().title, "two");
        let leftovers = std::fs::read_dir(store.dir())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("tmp")
            })
            .count();
        assert_eq!(leftovers, 0, "no temporary file is left behind");
    }

    #[test]
    fn a_damaged_document_is_set_aside_and_the_app_starts_fresh() {
        let (_dir, store) = store();
        std::fs::create_dir_all(store.dir()).unwrap();
        std::fs::write(store.path("prefs"), b"title = [not valid").unwrap();

        let loaded: Sample = store.load_or_default("prefs");

        assert_eq!(loaded, Sample::default());
        assert!(
            !store.path("prefs").exists(),
            "the bad file is out of the way"
        );
        let kept = store.dir().join("prefs.toml.bad");
        assert_eq!(std::fs::read(kept).unwrap(), b"title = [not valid");
        // The next save works and reads back.
        store.save("prefs", &Sample::default()).unwrap();
        assert!(store.load::<Sample>("prefs").unwrap().is_some());
    }

    #[test]
    fn unknown_fields_and_missing_fields_are_tolerated() {
        let (_dir, store) = store();
        std::fs::create_dir_all(store.dir()).unwrap();
        std::fs::write(store.path("x"), b"title = \"kept\"\nfrom_the_future = 3\n").unwrap();
        let loaded: Sample = store.load_or_default("x");
        assert_eq!(loaded.title, "kept");
        assert!(loaded.numbers.is_empty());
    }

    #[test]
    fn a_name_can_never_leave_the_store() {
        let (_dir, store) = store();
        for name in ["../secrets/key", "..\\..\\x", "/etc/passwd", "a/b", ""] {
            let path = store.path(name);
            assert_eq!(path.parent().unwrap(), store.dir(), "{name:?} -> {path:?}");
        }
        let paths = AppPaths::at("/base");
        let scoped = DocumentStore::scoped(&paths, "../../elsewhere");
        assert_eq!(scoped.dir().parent().unwrap(), Path::new("/base/scopes"));
    }

    #[test]
    fn scopes_do_not_see_each_others_documents() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        let a = DocumentStore::scoped(&paths, "demo-1");
        let b = DocumentStore::scoped(&paths, "live-1");
        a.save(
            "w",
            &Sample {
                title: "a".into(),
                ..Sample::default()
            },
        )
        .unwrap();
        assert_eq!(b.load::<Sample>("w").unwrap(), None);
        assert_ne!(a.dir(), DocumentStore::scoped(&paths, "other").dir());
    }
}
