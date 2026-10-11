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
//! * [`BackupStore`]: the copies the app keeps for itself in `<config>/backups/`. Manual saves,
//!   automatic ones (at most one per interval, the oldest pruned), and the safety copies made
//!   before an import or a reset replaces something. Every copy is listed, read, restored and
//!   deleted through it.
//! * The staged restore ([`stage_import`], [`stage_reset`], [`apply_pending`]): a restore or a
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
//! let applied = backup::apply_pending(&paths, "2026-09-29T10:05:00Z")?;
//! assert_eq!(applied.imported, Some(1));
//! assert_eq!(
//!     DocumentStore::global(&paths).load_text("preferences")?.as_deref(),
//!     Some("magnet = true\n")
//! );
//! # Ok::<(), wyck::infra::storage::ConfigError>(())
//! ```

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use crate::infra::storage::documents::DocumentStore;
use crate::infra::storage::error::{ConfigError, Result};
use crate::infra::storage::fs_util::atomic_write;
use crate::infra::storage::names::is_valid_name;
use crate::infra::storage::paths::AppPaths;
use crate::infra::storage::scripts::{self, ScriptStore};
use crate::infra::storage::sealed;

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

mod staged;
mod store;

pub use staged::{
    Applied, apply_pending, cancel_import, cancel_reset, export_to_file, import_pending,
    reset_pending, stage_import, stage_import_file, stage_reset,
};
pub use store::{AutoPolicy, BackupEntry, BackupKind, BackupStore};

#[cfg(test)]
#[path = "../../../tests/unit/infra/storage/backup.rs"]
mod tests;
