//! Backups: one file that holds everything the user made, the copies kept on disk, and the
//! two-step restore.
//!
//! What is in a backup: every saved document (global and per scope, see [`crate::infra::storage::DocumentStore`])
//! and every indicator script (see [`crate::infra::storage::scripts`]). What is never in it: credentials (they stay
//! in the keyring or in `secrets/`) and `config.toml`. A backup can go in a cloud folder for that
//! reason, and can be sealed with a passphrase when it should not be readable there anyway.
//!
//! The file is a TOML document that holds the *text* of each document as it is on disk, next to the
//! scope it belongs to. Keeping the text, not parsed values, means a backup is right for whatever
//! the documents hold and stays readable by a later version that knows more fields.
//!
//! # The three parts
//!
//! * [`Backup`]: the file. Collect it from the disk, write it out (sealed or not), read it back,
//!   check it. Nothing in a backup can be put outside the config folder: every document and script
//!   name is checked when it is read.
//! * [`BackupStore`](store::BackupStore): the copies the app keeps for itself in `<config>/backups/`. Manual saves,
//!   automatic ones (at most one per interval, the oldest pruned), and the safety copies made
//!   before an import or a reset replaces something. Every copy is listed, read, restored and
//!   deleted through it.
//! * The staged restore ([`stage_import`](staged::stage_import), [`stage_reset`](staged::stage_reset), [`apply_pending`](staged::apply_pending)): a restore or a
//!   reset never writes over a running app. It is checked and put aside, and the next start applies
//!   it before anything reads the documents, after saving what it replaces as a safety copy.
//!
//! ```
//! use wyck::infra::storage::{AppPaths, DocumentStore};
//! use wyck::infra::storage::backup::{self, Backup};
//!
//! # let dir = tempfile::tempdir().unwrap();
//! let paths = AppPaths::at(dir.path());
//! DocumentStore::global(&paths).save_text("preferences", "magnet = true\n")?;
//!
//! // Save a copy in the backups folder, then ask for it to come back at the next start.
//! let store = paths.backups();
//! let saved = store.create(&Backup::collect(&paths, None, "1.0", "2026-09-29T10:00:00Z")?)?;
//! DocumentStore::global(&paths).save_text("preferences", "magnet = false\n")?;
//! store.restore(&saved.id, None)?;
//!
//! let applied = backup::staged::apply_pending(&paths, "2026-09-29T10:05:00Z")?;
//! assert_eq!(applied.imported, Some(1));
//! assert_eq!(
//!     DocumentStore::global(&paths).load_text("preferences")?.as_deref(),
//!     Some("magnet = true\n")
//! );
//! # Ok::<(), wyck::infra::storage::ConfigError>(())
//! ```

pub mod staged;
pub mod store;

use crate::infra::storage::documents::DocumentStore;
use crate::infra::storage::error::{ConfigError, Result};
use crate::infra::storage::fs_util::atomic_write;
use crate::infra::storage::names::is_valid_name;
use crate::infra::storage::paths::AppPaths;
use crate::infra::storage::scripts::{self, ScriptStore};
use crate::infra::storage::sealed;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// The value of `format` in a backup file, and the label of a sealed one.
pub const FORMAT: &str = "wyck-backup";
/// The version of the layout of the file. A file of a newer version is refused, not misread.
pub const VERSION: u32 = 1;
/// The scope name of the documents shared by every account, in [`BackupFile::scope`].
pub const GLOBAL: &str = "global";
/// The most a single document can weigh.
pub const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
/// The most a whole backup can weigh.
pub const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
/// The most documents a backup holds.
pub const MAX_DOCUMENTS: usize = 500;

/// The name of the file a checked import waits in.
const PENDING: &str = "pending-import.toml";
/// The name of the marker a full reset waits behind.
const RESET_MARKER: &str = "reset-pending";
/// The version of this crate, written in the backups the crate makes on its own.
const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A backup: the documents and scripts of an install, as text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Backup {
    /// Always [`FORMAT`].
    pub format: String,
    /// The version of the layout, at most [`VERSION`].
    pub version: u32,
    /// The version of the app that made it.
    #[serde(default)]
    pub app_version: String,
    /// When it was made, as text (the caller decides the notation).
    #[serde(default)]
    pub created: String,
    /// The saved documents.
    #[serde(default)]
    pub files: Vec<BackupFile>,
    /// The indicators written as scripts. A backup made before they existed has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<BackupScript>,
}

/// One saved document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupFile {
    /// [`GLOBAL`], or `scope:` and the name of an account's scope.
    pub scope: String,
    /// The name of the document, without its extension.
    pub name: String,
    pub content: String,
}

/// One indicator script: its id (its path in the scripts folder, without the extension) and its
/// text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupScript {
    /// The id, see [`crate::infra::storage::scripts::clean_id`].
    pub id: String,
    /// The text of the script.
    pub content: String,
}

/// What a backup holds, without the text: what a front end shows before restoring one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Contents {
    /// The names of the documents shared by every account.
    pub global: Vec<String>,
    /// The names of the documents of each scope, by scope.
    pub scopes: BTreeMap<String, Vec<String>>,
    /// How many indicator scripts.
    pub scripts: usize,
}

impl Contents {
    /// How many documents, in every scope.
    #[must_use]
    pub fn documents(&self) -> usize {
        self.global.len() + self.scopes.values().map(Vec::len).sum::<usize>()
    }

    /// Whether there is nothing at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.documents() == 0 && self.scripts == 0
    }

    /// Whether a document called `name` is shared by every account.
    #[must_use]
    pub fn has_global(&self, name: &str) -> bool {
        self.global.iter().any(|n| n == name)
    }

    /// How many scopes have a document called `name`.
    #[must_use]
    pub fn scopes_with(&self, name: &str) -> usize {
        self.scopes
            .values()
            .filter(|names| names.iter().any(|n| n == name))
            .count()
    }
}

/// The store of the documents of `scope` ([`GLOBAL`], or `scope:` and a name).
fn store_for(paths: &AppPaths, scope: &str) -> DocumentStore {
    match scope.strip_prefix("scope:") {
        Some(name) => DocumentStore::scoped(paths, name),
        None => DocumentStore::global(paths),
    }
}

/// Whether `scope` names a place documents can go.
fn safe_scope(scope: &str) -> bool {
    scope == GLOBAL || scope.strip_prefix("scope:").is_some_and(is_valid_name)
}

fn damaged(what: impl Into<String>) -> ConfigError {
    ConfigError::BackupDamaged(what.into())
}

/// The documents of a store: name and text of each. A document that cannot be read as text is left
/// out, not fatal to the rest.
fn documents_in(store: &DocumentStore) -> Result<Vec<(String, String)>> {
    Ok(store
        .list()?
        .into_iter()
        .filter_map(|name| {
            let text = store.load_text(&name).ok().flatten()?;
            Some((name, text))
        })
        .collect())
}

impl Backup {
    /// An empty backup made by `app_version` at `created`.
    #[must_use]
    pub fn new(app_version: &str, created: &str) -> Self {
        Self {
            format: FORMAT.to_owned(),
            version: VERSION,
            app_version: app_version.to_owned(),
            created: created.to_owned(),
            files: Vec::new(),
            scripts: Vec::new(),
        }
    }

    /// Reads every saved document of `paths` into a backup made by `app_version` at `created`,
    /// and the scripts of `scripts` when a folder is given (the app may keep them somewhere other
    /// than the default one, see [`AppPaths::scripts`]).
    ///
    /// # Errors
    ///
    /// [`ConfigError::Read`] when a folder cannot be listed.
    pub fn collect(
        paths: &AppPaths,
        scripts: Option<&ScriptStore>,
        app_version: &str,
        created: &str,
    ) -> Result<Self> {
        let mut backup = Self::new(app_version, created);
        for (name, content) in documents_in(&DocumentStore::global(paths))? {
            backup.files.push(BackupFile {
                scope: GLOBAL.to_owned(),
                name,
                content,
            });
        }
        for scope in DocumentStore::list_scopes(paths)? {
            for (name, content) in documents_in(&DocumentStore::scoped(paths, &scope))? {
                backup.files.push(BackupFile {
                    scope: format!("scope:{scope}"),
                    name,
                    content,
                });
            }
        }
        if let Some(scripts) = scripts {
            backup.scripts = scripts
                .read_all()
                .into_iter()
                .map(|script| BackupScript {
                    id: script.id,
                    content: script.source,
                })
                .collect();
        }
        Ok(backup)
    }

    /// What the backup holds, by name.
    #[must_use]
    pub fn contents(&self) -> Contents {
        let mut contents = Contents {
            scripts: self.scripts.len(),
            ..Contents::default()
        };
        for file in &self.files {
            match file.scope.strip_prefix("scope:") {
                Some(scope) => contents
                    .scopes
                    .entry(scope.to_owned())
                    .or_default()
                    .push(file.name.clone()),
                None => contents.global.push(file.name.clone()),
            }
        }
        contents
    }

    /// Whether it holds no document and no script.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.scripts.is_empty()
    }

    /// The text of the backup, readable.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Serialize`], which does not happen for text.
    pub fn to_text(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(ConfigError::Serialize)
    }

    /// The text of the backup, sealed with `passphrase` (see [`crate::infra::storage::sealed`]): unreadable
    /// without it.
    ///
    /// # Errors
    ///
    /// As [`sealed::seal_text`].
    pub fn to_sealed_text(&self, passphrase: &SecretString) -> Result<String> {
        sealed::seal_text(passphrase, FORMAT, &self.to_text()?)
    }

    /// The text of the backup as it should be written: sealed when a passphrase is given.
    ///
    /// # Errors
    ///
    /// As [`Self::to_text`] and [`Self::to_sealed_text`].
    pub fn to_text_with(&self, passphrase: Option<&SecretString>) -> Result<String> {
        match passphrase {
            Some(passphrase) => self.to_sealed_text(passphrase),
            None => self.to_text(),
        }
    }

    /// Reads a backup from its plain text and checks it: the format, the version, that every
    /// document has a safe place and is valid TOML, that every script has a safe id, and that the
    /// whole is a size that makes sense.
    ///
    /// # Errors
    ///
    /// [`ConfigError::NotABackup`], [`ConfigError::BackupTooNew`] or
    /// [`ConfigError::BackupDamaged`].
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > MAX_TOTAL_BYTES + 1024 * 1024 {
            return Err(damaged("it is far too large"));
        }
        let backup: Self = toml::from_str(text).map_err(|_| ConfigError::NotABackup)?;
        if backup.format != FORMAT {
            return Err(ConfigError::NotABackup);
        }
        if backup.version > VERSION {
            return Err(ConfigError::BackupTooNew {
                found: backup.version,
                supported: VERSION,
            });
        }
        backup.check()?;
        Ok(backup)
    }

    /// Reads a backup from its text, plain or sealed. A sealed one needs `passphrase`.
    ///
    /// # Errors
    ///
    /// [`ConfigError::PassphraseRequired`] for a sealed backup and no passphrase,
    /// [`ConfigError::WrongPassphrase`] when it does not open, and whatever [`Self::parse`]
    /// returns.
    pub fn read(text: &str, passphrase: Option<&SecretString>) -> Result<Self> {
        if !sealed::is_sealed(text) {
            return Self::parse(text);
        }
        let passphrase = passphrase.ok_or(ConfigError::PassphraseRequired)?;
        Self::parse(&sealed::open_text(passphrase, FORMAT, text)?)
    }

    /// Reads a backup from a file, plain or sealed, refusing one that is far too large before
    /// reading it.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Read`] when the file cannot be read, and whatever [`Self::read`] returns.
    pub fn read_file(path: &Path, passphrase: Option<&SecretString>) -> Result<Self> {
        Self::read(&read_backup_text(path)?, passphrase)
    }

    /// Writes the backup to `path`, atomically, sealed when a passphrase is given.
    ///
    /// # Errors
    ///
    /// As [`Self::to_text_with`], and [`ConfigError::Write`] on an I/O failure.
    pub fn write_file(&self, path: &Path, passphrase: Option<&SecretString>) -> Result<()> {
        atomic_write(path, self.to_text_with(passphrase)?.as_bytes())
    }

    fn check(&self) -> Result<()> {
        if self.files.len() > MAX_DOCUMENTS {
            return Err(damaged("it holds too many documents"));
        }
        if self.scripts.len() > scripts::MAX_SCRIPTS {
            return Err(damaged("it holds too many indicator scripts"));
        }
        let mut seen = HashSet::new();
        for script in &self.scripts {
            if !scripts::is_clean_id(&script.id) {
                return Err(damaged(format!(
                    "the script \"{}\" has no safe place to go",
                    script.id
                )));
            }
            if !seen.insert(script.id.to_lowercase()) {
                return Err(damaged(format!(
                    "the script \"{}\" is in it twice",
                    script.id
                )));
            }
            if script.content.len() as u64 > scripts::MAX_FILE_BYTES {
                return Err(damaged(format!(
                    "the script \"{}\" is too large",
                    script.id
                )));
            }
        }
        let mut total = 0;
        let mut seen = HashSet::new();
        for file in &self.files {
            if !safe_scope(&file.scope) || !is_valid_name(&file.name) {
                return Err(damaged(format!(
                    "\"{}\" in \"{}\" has no safe place to go",
                    file.name, file.scope
                )));
            }
            if !seen.insert((file.scope.as_str(), file.name.as_str())) {
                return Err(damaged(format!("\"{}\" is in it twice", file.name)));
            }
            if file.content.len() > MAX_DOCUMENT_BYTES {
                return Err(damaged(format!("\"{}\" is too large", file.name)));
            }
            total += file.content.len();
            if toml::from_str::<toml::Table>(&file.content).is_err() {
                return Err(damaged(format!(
                    "\"{}\" is not a valid document",
                    file.name
                )));
            }
        }
        if total > MAX_TOTAL_BYTES {
            return Err(damaged("it is far too large"));
        }
        Ok(())
    }
}

/// The text of a backup file, refused when it is far too large to be one.
fn read_backup_text(path: &Path) -> Result<String> {
    let read = |source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    };
    let len = std::fs::metadata(path).map_err(read)?.len();
    if len > (MAX_TOTAL_BYTES + 2 * 1024 * 1024) as u64 {
        return Err(damaged("it is far too large"));
    }
    std::fs::read_to_string(path).map_err(read)
}

#[cfg(test)]
mod tests {
    use super::staged::*;

    use super::store::*;
    use super::*;
    use std::time::Duration;

    fn at(dir: &Path) -> AppPaths {
        AppPaths::at(dir)
    }

    fn write(dir: &Path, relative: &str, content: &str) {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn read(dir: &Path, relative: &str) -> String {
        std::fs::read_to_string(dir.join(relative)).unwrap()
    }

    fn secret(text: &str) -> SecretString {
        SecretString::from(text.to_owned())
    }

    fn setup() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "state/preferences.toml", "magnet = true\n");
        write(dir.path(), "state/appearance.toml", "mode = \"light\"\n");
        write(dir.path(), "scopes/demo-1/drawings.toml", "next_id = 4\n");
        write(dir.path(), "scopes/demo-1/watchlists.toml", "x = 1\n");
        write(dir.path(), "scopes/live-2/drawings.toml", "next_id = 9\n");
        // Never in a backup: the app's own config, a damaged copy, and what is not a document.
        write(dir.path(), "config.toml", "client_id = \"secret\"\n");
        write(dir.path(), "state/preferences.toml.bad", "junk");
        write(dir.path(), "state/notes.txt", "hello");
        dir
    }

    fn collect(dir: &Path) -> Backup {
        Backup::collect(&at(dir), None, "1.2.3", "today").unwrap()
    }

    #[test]
    fn a_backup_holds_every_document_and_nothing_else() {
        let dir = setup();
        let backup = collect(dir.path());
        let listed: Vec<(&str, &str)> = backup
            .files
            .iter()
            .map(|f| (f.scope.as_str(), f.name.as_str()))
            .collect();
        assert_eq!(
            listed,
            [
                ("global", "appearance"),
                ("global", "preferences"),
                ("scope:demo-1", "drawings"),
                ("scope:demo-1", "watchlists"),
                ("scope:live-2", "drawings"),
            ]
        );
        assert!(
            !backup.to_text().unwrap().contains("secret"),
            "the config file stays out"
        );
        assert_eq!(
            (backup.app_version.as_str(), backup.created.as_str()),
            ("1.2.3", "today")
        );
        let contents = backup.contents();
        assert_eq!(contents.documents(), 5);
        assert!(contents.has_global("appearance"));
        assert_eq!(contents.scopes_with("drawings"), 2);
        assert_eq!(contents.scopes_with("alerts"), 0);
    }

    #[test]
    fn the_indicator_scripts_travel_in_a_backup_and_come_back() {
        let dir = setup();
        let scripts = ScriptStore::new(dir.path().join("elsewhere"));
        scripts
            .write("Trend/My average", "plot(\"a\", close);")
            .unwrap();
        scripts.write("Other", "plot(\"b\", open);").unwrap();
        let backup = Backup::collect(&at(dir.path()), Some(&scripts), "1", "now").unwrap();
        assert_eq!(backup.scripts.len(), 2);
        let text = backup.to_text().unwrap();
        assert_eq!(Backup::parse(&text).unwrap().scripts, backup.scripts);

        // Applied on another machine: the scripts land in the default folder.
        let other = tempfile::tempdir().unwrap();
        stage_import(&at(other.path()), &text, None).unwrap();
        let applied = apply_pending(&at(other.path()), "s").unwrap();
        assert_eq!(applied.imported, Some(backup.files.len() + 2));
        assert_eq!(
            read(other.path(), "indicators/Trend/My average.rhai"),
            "plot(\"a\", close);"
        );
    }

    #[test]
    fn a_script_that_is_there_and_differs_is_kept_aside_when_a_backup_is_applied() {
        let source = tempfile::tempdir().unwrap();
        let scripts = ScriptStore::in_config(&at(source.path()));
        scripts.write("Mine", "plot(\"new\", close);").unwrap();
        let text = Backup::collect(&at(source.path()), Some(&scripts), "1", "now")
            .unwrap()
            .to_text()
            .unwrap();

        let target = tempfile::tempdir().unwrap();
        write(
            target.path(),
            "indicators/Mine.rhai",
            "plot(\"old\", close);",
        );
        stage_import(&at(target.path()), &text, None).unwrap();
        let applied = apply_pending(&at(target.path()), "s").unwrap();
        assert_eq!(
            read(target.path(), "indicators/Mine.rhai"),
            "plot(\"new\", close);"
        );

        let kept = Backup::read_file(&applied.kept_in.unwrap(), None).unwrap();
        assert_eq!(kept.scripts[0].content, "plot(\"old\", close);");
    }

    #[test]
    fn a_script_with_an_unsafe_place_makes_the_backup_refused() {
        for bad in ["../escape", "a/../b", "con", "C:/x", ".hidden"] {
            let mut backup = Backup::new("", "");
            backup.scripts.push(BackupScript {
                id: bad.to_owned(),
                content: String::new(),
            });
            assert!(Backup::parse(&backup.to_text().unwrap()).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_backup_of_nothing_is_valid_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let backup = collect(dir.path());
        assert!(backup.is_empty());
        let back = Backup::parse(&backup.to_text().unwrap()).unwrap();
        assert!(back.contents().is_empty());
    }

    #[test]
    fn what_is_written_is_read_back() {
        let dir = setup();
        let backup = collect(dir.path());
        assert_eq!(Backup::parse(&backup.to_text().unwrap()).unwrap(), backup);
    }

    #[test]
    fn a_file_that_is_not_a_backup_or_is_broken_is_refused() {
        assert!(matches!(
            Backup::parse("hello"),
            Err(ConfigError::NotABackup)
        ));
        assert!(matches!(
            Backup::parse("format = \"other\"\nversion = 1\n"),
            Err(ConfigError::NotABackup)
        ));
        assert!(matches!(
            Backup::parse("format = \"wyck-backup\"\nversion = 99\n"),
            Err(ConfigError::BackupTooNew { found: 99, .. })
        ));
        let file = |name: &str, content: &str| {
            format!("[[files]]\nscope = \"global\"\nname = \"{name}\"\ncontent = \"{content}\"\n")
        };
        let head = "format = \"wyck-backup\"\nversion = 1\n";
        let broken = format!("{head}{}", file("a", "not = = toml"));
        assert!(matches!(
            Backup::parse(&broken),
            Err(ConfigError::BackupDamaged(_))
        ));
        let twice = format!("{head}{}{}", file("a", ""), file("a", ""));
        assert!(matches!(
            Backup::parse(&twice),
            Err(ConfigError::BackupDamaged(_))
        ));
    }

    #[test]
    fn a_document_cannot_go_outside_the_config_folder() {
        for (scope, name) in [
            ("global", "../evil"),
            ("global", "a/b"),
            ("global", "..\\evil"),
            ("global", ".hidden"),
            ("global", ""),
            ("scope:../x", "a"),
            ("scope:", "a"),
            ("elsewhere", "a"),
            ("scope:a/b", "a"),
        ] {
            let text = format!(
                "format = \"wyck-backup\"\nversion = 1\n[[files]]\nscope = \"{}\"\nname = \"{}\"\ncontent = \"\"\n",
                scope.replace('\\', "\\\\"),
                name.replace('\\', "\\\\")
            );
            assert!(
                Backup::parse(&text).is_err(),
                "{scope:?} {name:?} was accepted"
            );
        }
    }

    #[test]
    fn a_sealed_backup_needs_its_passphrase_and_says_which_problem_it_has() {
        let dir = setup();
        let backup = collect(dir.path());
        let text = backup.to_text_with(Some(&secret("pw"))).unwrap();
        assert!(sealed::is_sealed(&text));
        assert!(!text.contains("magnet"), "the content is not readable");

        assert!(matches!(
            Backup::read(&text, None),
            Err(ConfigError::PassphraseRequired)
        ));
        assert!(matches!(
            Backup::read(&text, Some(&secret("nope"))),
            Err(ConfigError::WrongPassphrase)
        ));
        assert_eq!(Backup::read(&text, Some(&secret("pw"))).unwrap(), backup);
        // A passphrase given for a plain backup is not needed, and not an error.
        assert_eq!(
            Backup::read(&backup.to_text().unwrap(), Some(&secret("pw"))).unwrap(),
            backup
        );
    }

    #[test]
    fn export_and_import_go_through_a_file_sealed_or_not() {
        let dir = setup();
        let out = tempfile::tempdir().unwrap();
        let plain = out.path().join("plain.toml");
        let locked = out.path().join("deep").join("locked.toml");
        let pw = secret("pw");
        let backup = export_to_file(&at(dir.path()), None, &plain, None, "1", "t").unwrap();
        export_to_file(&at(dir.path()), None, &locked, Some(&pw), "1", "t").unwrap();

        let target = tempfile::tempdir().unwrap();
        assert_eq!(
            stage_import_file(&at(target.path()), &plain, None).unwrap(),
            backup
        );
        assert!(matches!(
            stage_import_file(&at(target.path()), &locked, None),
            Err(ConfigError::PassphraseRequired)
        ));
        assert_eq!(
            stage_import_file(&at(target.path()), &locked, Some(&pw)).unwrap(),
            backup
        );
    }

    #[test]
    fn an_import_waits_and_is_applied_at_the_next_start_keeping_what_it_replaces() {
        let source = setup();
        let text = collect(source.path()).to_text().unwrap();

        // Another machine: one document differs, one is new, one is not in the backup.
        let target = tempfile::tempdir().unwrap();
        write(target.path(), "state/preferences.toml", "magnet = false\n");
        write(target.path(), "state/appearance.toml", "mode = \"light\"\n");
        write(target.path(), "scopes/other/alerts.toml", "keep = true\n");
        let staged = stage_import(&at(target.path()), &text, None).unwrap();
        assert_eq!(staged.files.len(), 5);
        assert!(import_pending(&at(target.path())));
        assert_eq!(
            read(target.path(), "state/preferences.toml"),
            "magnet = false\n",
            "nothing is written until the next start"
        );

        let applied = apply_pending(&at(target.path()), "2026-09-24 10:00").unwrap();
        assert_eq!(applied.imported, Some(5));
        assert!(!import_pending(&at(target.path())));
        assert_eq!(
            read(target.path(), "state/preferences.toml"),
            "magnet = true\n"
        );
        assert_eq!(
            read(target.path(), "scopes/live-2/drawings.toml"),
            "next_id = 9\n"
        );
        assert_eq!(
            read(target.path(), "scopes/other/alerts.toml"),
            "keep = true\n",
            "what the backup does not hold is left alone"
        );

        // What was replaced (and only that: appearance was the same) is kept, and can be restored.
        let kept = Backup::read_file(&applied.kept_in.clone().unwrap(), None).unwrap();
        assert_eq!(kept.files.len(), 1);
        assert_eq!(kept.files[0].content, "magnet = false\n");
        let entries = at(target.path()).backups().list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, BackupKind::BeforeImport);

        // Applied once.
        assert_eq!(
            apply_pending(&at(target.path()), "again").unwrap(),
            Applied::default()
        );
    }

    #[test]
    fn a_staged_import_can_be_cancelled_and_a_broken_one_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let text = collect(setup().path()).to_text().unwrap();
        stage_import(&at(dir.path()), &text, None).unwrap();
        cancel_import(&at(dir.path())).unwrap();
        assert!(!import_pending(&at(dir.path())));
        cancel_import(&at(dir.path())).unwrap();

        write(dir.path(), "pending-import.toml", "this is not a backup");
        let applied = apply_pending(&at(dir.path()), "t").unwrap();
        assert!(applied.set_aside);
        assert!(!import_pending(&at(dir.path())));
        assert!(dir.path().join("pending-import.toml.bad").is_file());
        assert!(stage_import(&at(dir.path()), "garbage", None).is_err());
        assert!(!import_pending(&at(dir.path())), "a bad file is not staged");
    }

    #[test]
    fn a_reset_wipes_every_account_but_keeps_a_copy_and_leaves_sign_in_and_scripts_alone() {
        let dir = setup();
        write(dir.path(), "indicators/Mine.rhai", "plot(\"a\", close);");
        stage_reset(&at(dir.path())).unwrap();
        assert!(reset_pending(&at(dir.path())));
        assert!(
            dir.path().join("state/preferences.toml").is_file(),
            "nothing is removed until the next start"
        );

        let applied = apply_pending(&at(dir.path()), "2026-09-24").unwrap();
        assert!(applied.reset);
        assert!(!reset_pending(&at(dir.path())));
        assert!(!dir.path().join("state").exists());
        assert!(!dir.path().join("scopes").exists());
        assert!(
            dir.path().join("config.toml").is_file(),
            "the sign-in stays"
        );
        assert!(
            dir.path().join("indicators/Mine.rhai").is_file(),
            "the scripts stay"
        );

        // The reset can be undone from the copy it made.
        let store = at(dir.path()).backups();
        let copy = store.list().unwrap().remove(0);
        assert_eq!(copy.kind, BackupKind::BeforeReset);
        assert_eq!(store.load(&copy.id, None).unwrap().files.len(), 5);
        store.restore(&copy.id, None).unwrap();
        apply_pending(&at(dir.path()), "2026-09-25").unwrap();
        assert_eq!(
            read(dir.path(), "state/preferences.toml"),
            "magnet = true\n"
        );

        // Applied once; a reset of nothing makes no copy.
        assert_eq!(
            apply_pending(&at(dir.path()), "x").unwrap(),
            Applied::default()
        );
        let empty = tempfile::tempdir().unwrap();
        stage_reset(&at(empty.path())).unwrap();
        assert_eq!(apply_pending(&at(empty.path()), "x").unwrap().kept_in, None);
    }

    #[test]
    fn a_reset_and_an_import_each_cancel_the_other() {
        let dir = tempfile::tempdir().unwrap();
        let text = collect(setup().path()).to_text().unwrap();

        stage_import(&at(dir.path()), &text, None).unwrap();
        stage_reset(&at(dir.path())).unwrap();
        assert!(reset_pending(&at(dir.path())));
        assert!(
            !import_pending(&at(dir.path())),
            "the reset cancels the import"
        );

        stage_import(&at(dir.path()), &text, None).unwrap();
        assert!(import_pending(&at(dir.path())));
        assert!(
            !reset_pending(&at(dir.path())),
            "the import cancels the reset"
        );
    }

    #[test]
    fn the_store_lists_reads_exports_and_deletes_copies() {
        let dir = setup();
        let paths = at(dir.path());
        let store = paths.backups();
        assert!(store.list().unwrap().is_empty(), "no folder yet");

        let backup = collect(dir.path());
        let first = store.create(&backup).unwrap();
        let second = store.create(&backup).unwrap();
        assert_eq!(first.id, "backup-today");
        assert_eq!(second.id, "backup-today-2", "a taken name gets a number");
        let sealed = store
            .save(
                BackupKind::Manual,
                &Backup::new("1", "later"),
                Some(&secret("pw")),
            )
            .unwrap();
        assert!(sealed.sealed);

        // Not copies: another kind of file, a name that is not one.
        write(&store.dir(), "notes.txt", "x");
        write(&store.dir(), "bad name.toml", "x");
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 3);
        assert!(listed.iter().find(|e| e.id == sealed.id).unwrap().sealed);
        assert!(!listed.iter().find(|e| e.id == first.id).unwrap().sealed);

        assert_eq!(store.load(&first.id, None).unwrap(), backup);
        assert!(matches!(
            store.load(&sealed.id, None),
            Err(ConfigError::PassphraseRequired)
        ));
        assert!(matches!(
            store.load("missing", None),
            Err(ConfigError::BackupNotFound(_))
        ));
        assert!(store.load("../config", None).is_err(), "an id is a name");

        let out = tempfile::tempdir().unwrap().keep().join("carried.toml");
        store.export(&first.id, &out).unwrap();
        assert_eq!(Backup::read_file(&out, None).unwrap(), backup);

        store.remove(&first.id).unwrap();
        store.remove(&first.id).unwrap();
        assert_eq!(store.list().unwrap().len(), 2);
    }

    #[test]
    fn prune_keeps_the_newest_of_one_kind_only() {
        let dir = tempfile::tempdir().unwrap();
        let store = at(dir.path()).backups();
        for n in 0..4 {
            store
                .save(
                    BackupKind::Automatic,
                    &Backup::new("1", &format!("t{n}")),
                    None,
                )
                .unwrap();
        }
        let manual = store.create(&Backup::new("1", "m")).unwrap();
        assert_eq!(store.prune(BackupKind::Automatic, 2).unwrap(), 2);
        let kinds: Vec<_> = store.list().unwrap().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds
                .iter()
                .filter(|k| **k == BackupKind::Automatic)
                .count(),
            2
        );
        assert!(
            store.get(&manual.id).is_ok(),
            "the other kinds are left alone"
        );
    }

    #[test]
    fn an_automatic_copy_is_made_once_per_interval_and_never_of_nothing() {
        let dir = setup();
        let paths = at(dir.path());
        let store = paths.backups();
        let policy = AutoPolicy {
            every: Duration::from_secs(3600),
            keep: 2,
        };

        let empty = tempfile::tempdir().unwrap();
        assert!(
            at(empty.path())
                .backups()
                .auto_snapshot(None, "1", "t", policy)
                .unwrap()
                .is_none(),
            "nothing to save"
        );
        let first = store
            .auto_snapshot(None, "1", "t1", policy)
            .unwrap()
            .unwrap();
        assert_eq!(first.kind, BackupKind::Automatic);
        assert!(
            store
                .auto_snapshot(None, "1", "t2", policy)
                .unwrap()
                .is_none(),
            "too soon"
        );
        let always = AutoPolicy {
            every: Duration::ZERO,
            keep: 2,
        };
        store
            .auto_snapshot(None, "1", "t3", always)
            .unwrap()
            .unwrap();
        store
            .auto_snapshot(None, "1", "t4", always)
            .unwrap()
            .unwrap();
        assert_eq!(store.list().unwrap().len(), 2, "the oldest went");
    }
}
