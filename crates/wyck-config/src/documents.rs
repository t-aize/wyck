//! Typed TOML documents next to the app config: the place for anything a front end wants to
//! remember between runs (layouts, favorites, drawings) without this crate knowing what it is.
//!
//! A [`DocumentStore`] is a directory. Each document is one TOML file named after the document,
//! read and written whole through serde, so the schema lives with the code that owns the data and
//! this module only provides what must be right in one place: where the files go, that a write
//! is atomic, and that a file that can no longer be read never takes the app down.
//!
//! There are two kinds of store. [`DocumentStore::global`] is shared by everything (how the user
//! likes to work), and [`DocumentStore::scoped`] belongs to one named scope, chosen by the caller
//! (things named by the broker behind an account, such as symbols, that mean nothing under
//! another). The caller picks a scope that outlives a sign-in, such as the account number, so that
//! signing out and in again finds everything where it was. Neither ever holds a secret: those
//! stay in [`crate::secret`].

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use tracing::{debug, warn};

use crate::error::{ConfigError, Result};
use crate::fs_util::atomic_write;
use crate::paths::AppPaths;

/// A directory of TOML documents. Cheap to clone and safe to hand to another thread: it holds
/// only a path, and every call goes to the file system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentStore {
    dir: PathBuf,
}

impl DocumentStore {
    /// The documents shared by every profile.
    pub fn global(paths: &AppPaths) -> Self {
        Self {
            dir: paths.config_dir().join("state"),
        }
    }

    /// The documents of one scope, such as `demo-45970491`. Whatever the name holds, the
    /// directory stays inside the config directory.
    pub fn scoped(paths: &AppPaths, scope: &str) -> Self {
        Self {
            dir: paths
                .config_dir()
                .join("scopes")
                .join(safe_component(scope)),
        }
    }

    /// Where the document `name` lives.
    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.toml", safe_component(name)))
    }

    /// The directory the documents are in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Reads a document, or `None` when it was never written.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Read`] on an I/O failure other than "not found", and
    /// [`ConfigError::Parse`] if the file is not valid TOML for `T`.
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
    /// replacing an older one) and gives the default too. The user loses that document's
    /// contents but not the app, and the damaged file stays there to be recovered by hand.
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
    ///
    /// # Errors
    ///
    /// [`ConfigError::Serialize`] if `value` cannot be written as TOML, and
    /// [`ConfigError::Write`] on an I/O failure.
    pub fn save<T: Serialize>(&self, name: &str, value: &T) -> Result<()> {
        let text = toml::to_string_pretty(value).map_err(ConfigError::Serialize)?;
        let path = self.path(name);
        atomic_write(&path, text.as_bytes())?;
        debug!(path = %path.display(), bytes = text.len(), "saved a document");
        Ok(())
    }

    /// Deletes a document. Missing is fine.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Write`] if the file exists and cannot be removed.
    pub fn remove(&self, name: &str) -> Result<()> {
        let path = self.path(name);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(ConfigError::Write { path, source }),
        }
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

/// A file or directory name made of what was given, with anything that could point elsewhere
/// (separators, dots at the start, control characters) replaced, so a name from outside can
/// never leave the store.
fn safe_component(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "_".to_owned()
    } else {
        cleaned
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
        let store = DocumentStore::global(&AppPaths::at(dir.path()));
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
        assert_ne!(a.dir(), DocumentStore::global(&paths).dir());
    }

    #[test]
    fn removing_a_document_that_is_not_there_is_fine() {
        let (_dir, store) = store();
        store.remove("never").unwrap();
        store.save("gone", &Sample::default()).unwrap();
        store.remove("gone").unwrap();
        assert_eq!(store.load::<Sample>("gone").unwrap(), None);
    }
}
