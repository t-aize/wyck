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
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::infra::storage::documents::DocumentStore;
use crate::infra::storage::error::{ConfigError, Result};
use crate::infra::storage::fs_util::atomic_write;
use crate::infra::storage::names::{is_valid_name, validate_name};
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

// ---- the file ----

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
    /// Its text.
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

// ---- the copies kept on disk ----

/// Why a copy was made: it decides the name of the file and what [`BackupStore::prune`] keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackupKind {
    /// Made on request, or dropped in the folder by hand.
    Manual,
    /// Made by [`BackupStore::auto_snapshot`].
    Automatic,
    /// What an import replaced, saved right before it was applied.
    BeforeImport,
    /// What a reset removed, saved right before it was applied.
    BeforeReset,
}

impl BackupKind {
    /// The start of the name of the files of this kind.
    #[must_use]
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Manual => "backup",
            Self::Automatic => "auto",
            Self::BeforeImport => "before-import",
            Self::BeforeReset => "before-reset",
        }
    }

    /// A few words for a list.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "Saved by you",
            Self::Automatic => "Automatic",
            Self::BeforeImport => "Before an import",
            Self::BeforeReset => "Before a reset",
        }
    }

    fn of(id: &str) -> Self {
        [Self::Automatic, Self::BeforeImport, Self::BeforeReset]
            .into_iter()
            .find(|kind| {
                id.strip_prefix(kind.prefix())
                    .is_some_and(|rest| rest.starts_with('-'))
            })
            .unwrap_or(Self::Manual)
    }
}

/// One copy in the backups folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupEntry {
    /// The name of the copy, the name of its file without the extension. What the other methods of
    /// [`BackupStore`] take.
    pub id: String,
    /// Why it was made.
    pub kind: BackupKind,
    /// The file.
    pub path: PathBuf,
    /// Its size in bytes.
    pub bytes: u64,
    /// When the file was last written, when the system says.
    pub modified: Option<SystemTime>,
    /// Whether it needs a passphrase to be read (told from the start of the file).
    pub sealed: bool,
}

/// When and how many automatic copies [`BackupStore::auto_snapshot`] keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoPolicy {
    /// No new automatic copy while the last one is younger than this.
    pub every: Duration,
    /// How many automatic copies are kept; the oldest go.
    pub keep: usize,
}

impl Default for AutoPolicy {
    /// One a day, the last seven.
    fn default() -> Self {
        Self {
            every: Duration::from_secs(24 * 60 * 60),
            keep: 7,
        }
    }
}

/// The copies the app keeps in `<config>/backups/`. Holds a path, so it is cheap to clone and
/// every call goes to the file system. Get one with [`AppPaths::backups`].
#[derive(Debug, Clone)]
pub struct BackupStore {
    paths: AppPaths,
}

/// `text` as the tail of a file name: what is not a letter or a digit becomes `-`, and runs of
/// them are one.
fn file_stamp(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    if out.is_empty() {
        "now".to_owned()
    } else {
        out.chars().take(40).collect()
    }
}

/// Whether the file starts like a sealed document, without reading the whole of it: a sealed
/// document is written with `format` first, so a list of many large copies stays quick.
fn looks_sealed(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0_u8; 64];
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let read = file.read(&mut head).unwrap_or(0);
    let head = String::from_utf8_lossy(&head[..read]);
    head.trim_start()
        .starts_with(&format!("format = \"{}\"", sealed::FORMAT))
}

impl BackupStore {
    /// The copies of the install at `paths`.
    #[must_use]
    pub fn new(paths: &AppPaths) -> Self {
        Self {
            paths: paths.clone(),
        }
    }

    /// The folder the copies are in.
    pub fn dir(&self) -> PathBuf {
        self.paths.backups_dir()
    }

    fn path_of(&self, id: &str) -> Result<PathBuf> {
        validate_name(id)?;
        Ok(self.dir().join(format!("{id}.toml")))
    }

    /// Every copy, the newest first. Files that are not a `.toml` with a valid name are not
    /// listed. A missing folder gives an empty list.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Read`] on an I/O failure other than "not found".
    pub fn list(&self) -> Result<Vec<BackupEntry>> {
        let dir = self.dir();
        let read = match std::fs::read_dir(&dir) {
            Ok(read) => read,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(ConfigError::Read { path: dir, source }),
        };
        let mut entries: Vec<BackupEntry> = read
            .filter_map(std::result::Result::ok)
            .filter_map(|item| {
                let path = item.path();
                let meta = item.metadata().ok().filter(std::fs::Metadata::is_file)?;
                let name = item.file_name().into_string().ok()?;
                let id = name.strip_suffix(".toml").filter(|id| is_valid_name(id))?;
                let sealed = looks_sealed(&path);
                Some(BackupEntry {
                    id: id.to_owned(),
                    kind: BackupKind::of(id),
                    bytes: meta.len(),
                    modified: meta.modified().ok(),
                    sealed,
                    path,
                })
            })
            .collect();
        entries.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| b.id.cmp(&a.id)));
        Ok(entries)
    }

    /// One copy by id.
    ///
    /// # Errors
    ///
    /// [`ConfigError::BackupNotFound`] when there is none, and what [`Self::list`] returns.
    pub fn get(&self, id: &str) -> Result<BackupEntry> {
        self.list()?
            .into_iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| ConfigError::BackupNotFound(id.to_owned()))
    }

    /// Saves `backup` as a [`BackupKind::Manual`] copy, named after when it was made.
    ///
    /// # Errors
    ///
    /// As [`Self::save`].
    pub fn create(&self, backup: &Backup) -> Result<BackupEntry> {
        self.save(BackupKind::Manual, backup, None)
    }

    /// Saves `backup` as a copy of `kind`, sealed when a passphrase is given. The file is named
    /// after the kind and after `backup.created`, with a number added when the name is taken.
    ///
    /// # Errors
    ///
    /// As [`Backup::to_text_with`], and [`ConfigError::Write`].
    pub fn save(
        &self,
        kind: BackupKind,
        backup: &Backup,
        passphrase: Option<&SecretString>,
    ) -> Result<BackupEntry> {
        let text = backup.to_text_with(passphrase)?;
        let stem = format!("{}-{}", kind.prefix(), file_stamp(&backup.created));
        let mut id = stem.clone();
        let mut number = 2;
        while self.path_of(&id)?.exists() {
            id = format!("{stem}-{number}");
            number += 1;
        }
        let path = self.path_of(&id)?;
        atomic_write(&path, text.as_bytes())?;
        info!(id, kind = ?kind, "saved a backup");
        Ok(BackupEntry {
            id,
            kind,
            bytes: text.len() as u64,
            modified: Some(SystemTime::now()),
            sealed: passphrase.is_some(),
            path,
        })
    }

    /// Saves an automatic copy of the install when the last one is older than `policy.every`,
    /// then keeps only the newest `policy.keep`. Returns the copy, or `None` when it is not time
    /// yet or there is nothing to save. The scripts of `scripts` are part of it when given.
    ///
    /// # Errors
    ///
    /// As [`Backup::collect`] and [`Self::save`].
    pub fn auto_snapshot(
        &self,
        scripts: Option<&ScriptStore>,
        app_version: &str,
        created: &str,
        policy: AutoPolicy,
    ) -> Result<Option<BackupEntry>> {
        let newest = self
            .list()?
            .into_iter()
            .find(|entry| entry.kind == BackupKind::Automatic);
        let due = newest
            .and_then(|entry| entry.modified)
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_none_or(|age| age >= policy.every);
        if !due {
            return Ok(None);
        }
        let backup = Backup::collect(&self.paths, scripts, app_version, created)?;
        if backup.is_empty() {
            return Ok(None);
        }
        let entry = self.save(BackupKind::Automatic, &backup, None)?;
        self.prune(BackupKind::Automatic, policy.keep)?;
        Ok(Some(entry))
    }

    /// The text of a copy, as it is on disk (sealed or not).
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidName`], [`ConfigError::BackupNotFound`], [`ConfigError::Read`].
    pub fn read_text(&self, id: &str) -> Result<String> {
        let path = self.path_of(id)?;
        if !path.is_file() {
            return Err(ConfigError::BackupNotFound(id.to_owned()));
        }
        read_backup_text(&path)
    }

    /// A copy, read and checked.
    ///
    /// # Errors
    ///
    /// As [`Self::read_text`] and [`Backup::read`].
    pub fn load(&self, id: &str, passphrase: Option<&SecretString>) -> Result<Backup> {
        Backup::read(&self.read_text(id)?, passphrase)
    }

    /// Asks for a copy to come back at the next start (see [`stage_import`]). Returns what it
    /// holds.
    ///
    /// # Errors
    ///
    /// As [`Self::load`] and [`stage_import`].
    pub fn restore(&self, id: &str, passphrase: Option<&SecretString>) -> Result<Backup> {
        stage_import(&self.paths, &self.read_text(id)?, passphrase)
    }

    /// Writes a copy to `dest`, to carry it somewhere else. The copy is written as it is: sealed
    /// stays sealed.
    ///
    /// # Errors
    ///
    /// As [`Self::read_text`], and [`ConfigError::Write`].
    pub fn export(&self, id: &str, dest: &Path) -> Result<()> {
        atomic_write(dest, self.read_text(id)?.as_bytes())
    }

    /// Deletes a copy. Missing is fine.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidName`], [`ConfigError::Write`] when the file exists and cannot go.
    pub fn remove(&self, id: &str) -> Result<()> {
        let path = self.path_of(id)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(ConfigError::Write { path, source }),
        }
    }

    /// Keeps the newest `keep` copies of `kind` and deletes the others. Returns how many went.
    ///
    /// # Errors
    ///
    /// As [`Self::list`] and [`Self::remove`].
    pub fn prune(&self, kind: BackupKind, keep: usize) -> Result<usize> {
        let old: Vec<_> = self
            .list()?
            .into_iter()
            .filter(|entry| entry.kind == kind)
            .skip(keep)
            .collect();
        for entry in &old {
            self.remove(&entry.id)?;
        }
        if !old.is_empty() {
            debug!(count = old.len(), kind = ?kind, "pruned old backups");
        }
        Ok(old.len())
    }
}

// ---- the staged restore and reset ----

/// Checks the text of a backup (plain, or sealed with `passphrase`) and puts it aside, to be
/// applied when the app starts again by [`apply_pending`]. Returns what it holds. A reset that was
/// waiting is cancelled: the last thing asked for wins.
///
/// What waits is the plain text: a backup holds no credentials.
///
/// # Errors
///
/// As [`Backup::read`], and [`ConfigError::Write`].
pub fn stage_import(
    paths: &AppPaths,
    text: &str,
    passphrase: Option<&SecretString>,
) -> Result<Backup> {
    let backup = Backup::read(text, passphrase)?;
    atomic_write(
        &paths.config_dir().join(PENDING),
        backup.to_text()?.as_bytes(),
    )?;
    let _ = cancel_reset(paths);
    Ok(backup)
}

/// [`stage_import`] for the backup file at `path`.
///
/// # Errors
///
/// As [`stage_import`], and [`ConfigError::Read`].
pub fn stage_import_file(
    paths: &AppPaths,
    path: &Path,
    passphrase: Option<&SecretString>,
) -> Result<Backup> {
    stage_import(paths, &read_backup_text(path)?, passphrase)
}

/// Whether an import waits for the next start.
#[must_use]
pub fn import_pending(paths: &AppPaths) -> bool {
    paths.config_dir().join(PENDING).is_file()
}

/// Forgets an import that waits, for one the user changed their mind about. Nothing waiting is
/// fine.
///
/// # Errors
///
/// [`ConfigError::Write`] when the file exists and cannot go.
pub fn cancel_import(paths: &AppPaths) -> Result<()> {
    remove_marker(&paths.config_dir().join(PENDING))
}

/// Asks for every saved document of every account to go back to how it is on a fresh install, at
/// the next start. Cancels an import that was waiting. What a reset never touches: `config.toml`
/// (the accounts and which one is active), the credentials and the indicator scripts, so signing
/// in again is not needed. What it removes is saved first, as a [`BackupKind::BeforeReset`] copy.
///
/// # Errors
///
/// [`ConfigError::Write`].
pub fn stage_reset(paths: &AppPaths) -> Result<()> {
    atomic_write(&paths.config_dir().join(RESET_MARKER), b"")?;
    cancel_import(paths)
}

/// Whether a reset waits for the next start.
#[must_use]
pub fn reset_pending(paths: &AppPaths) -> bool {
    paths.config_dir().join(RESET_MARKER).is_file()
}

/// Forgets a reset that waits. Nothing waiting is fine.
///
/// # Errors
///
/// [`ConfigError::Write`] when the marker exists and cannot go.
pub fn cancel_reset(paths: &AppPaths) -> Result<()> {
    remove_marker(&paths.config_dir().join(RESET_MARKER))
}

fn remove_marker(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ConfigError::Write {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// What [`apply_pending`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// A reset was applied.
    pub reset: bool,
    /// An import was applied, with how many documents and scripts it wrote.
    pub imported: Option<usize>,
    /// An import that could not be read was set aside as `pending-import.toml.bad`.
    pub set_aside: bool,
    /// The safety copy of what the reset or the import replaced, when it replaced anything.
    pub kept_in: Option<PathBuf>,
}

/// Applies what waits, if anything does: a reset, or an import. Call it once at the start of the
/// app, before anything reads the documents. `created` is when it is (any notation, it names the
/// safety copy).
///
/// What is replaced is saved first (see [`BackupKind::BeforeReset`] and
/// [`BackupKind::BeforeImport`]); when that fails nothing is touched and the error comes back, so
/// nothing is lost to a full disk. An import applied over a document that already holds the same
/// text leaves it alone. An import that cannot be read is set aside, not tried again.
///
/// # Errors
///
/// [`ConfigError::Read`] and [`ConfigError::Write`]. What waits stays until it is applied.
pub fn apply_pending(paths: &AppPaths, created: &str) -> Result<Applied> {
    let mut applied = Applied::default();
    if reset_pending(paths) {
        applied.kept_in = apply_reset(paths, created)?;
        applied.reset = true;
    }
    let path = paths.config_dir().join(PENDING);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(applied),
        Err(source) => return Err(ConfigError::Read { path, source }),
    };
    let Ok(backup) = Backup::parse(&text) else {
        warn!("the staged import cannot be read, setting it aside");
        let _ = std::fs::rename(&path, paths.config_dir().join(format!("{PENDING}.bad")));
        applied.set_aside = true;
        return Ok(applied);
    };
    let (written, kept) = apply_import(paths, &backup, created)?;
    remove_marker(&path)?;
    applied.imported = Some(written);
    applied.kept_in = kept.or(applied.kept_in);
    Ok(applied)
}

fn apply_reset(paths: &AppPaths, created: &str) -> Result<Option<PathBuf>> {
    let before = Backup::collect(paths, None, CRATE_VERSION, created)?;
    let kept = if before.is_empty() {
        None
    } else {
        Some(
            paths
                .backups()
                .save(BackupKind::BeforeReset, &before, None)?
                .path,
        )
    };
    for folder in [paths.state_dir(), paths.scopes_dir()] {
        match std::fs::remove_dir_all(&folder) {
            Ok(()) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(ConfigError::Write {
                    path: folder,
                    source,
                });
            }
        }
    }
    remove_marker(&paths.config_dir().join(RESET_MARKER))?;
    info!("reset every saved document");
    Ok(kept)
}

fn apply_import(
    paths: &AppPaths,
    backup: &Backup,
    created: &str,
) -> Result<(usize, Option<PathBuf>)> {
    let scripts = ScriptStore::in_config(paths);
    // What differs from what is there is what the import replaces: saved before anything is written.
    let mut replaced = Backup::new(CRATE_VERSION, created);
    for file in &backup.files {
        // A document that cannot be read as text is not worth a copy, and must not stop the import.
        let old = store_for(paths, &file.scope)
            .load_text(&file.name)
            .ok()
            .flatten();
        if old.as_ref().is_some_and(|old| *old != file.content) {
            replaced.files.push(BackupFile {
                content: old.unwrap_or_default(),
                ..file.clone()
            });
        }
    }
    for script in &backup.scripts {
        let old = scripts.read(&script.id).ok().flatten();
        if old.as_ref().is_some_and(|old| *old != script.content) {
            replaced.scripts.push(BackupScript {
                id: script.id.clone(),
                content: old.unwrap_or_default(),
            });
        }
    }
    let kept = if replaced.is_empty() {
        None
    } else {
        Some(
            paths
                .backups()
                .save(BackupKind::BeforeImport, &replaced, None)?
                .path,
        )
    };
    for file in &backup.files {
        store_for(paths, &file.scope).save_text(&file.name, &file.content)?;
    }
    for script in &backup.scripts {
        if scripts.read(&script.id).ok().flatten().as_deref() != Some(script.content.as_str()) {
            scripts.write(&script.id, &script.content)?;
        }
    }
    let written = backup.files.len() + backup.scripts.len();
    info!(written, "restored a backup");
    Ok((written, kept))
}

// ---- export and import of a file ----

/// Writes a backup of everything to `dest` (sealed when a passphrase is given) and returns what
/// went in. The one call behind an "Export" button: `scripts` is the folder the indicators are
/// really in (see [`AppPaths::scripts`] for the default).
///
/// # Errors
///
/// As [`Backup::collect`] and [`Backup::write_file`].
pub fn export_to_file(
    paths: &AppPaths,
    scripts: Option<&ScriptStore>,
    dest: &Path,
    passphrase: Option<&SecretString>,
    app_version: &str,
    created: &str,
) -> Result<Backup> {
    let backup = Backup::collect(paths, scripts, app_version, created)?;
    backup.write_file(dest, passphrase)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn file_names_made_from_a_date_are_safe() {
        assert_eq!(
            file_stamp("2026-09-29T10:15:00+02:00"),
            "2026-09-29T10-15-00-02-00"
        );
        assert_eq!(file_stamp("../../x"), "x");
        assert_eq!(file_stamp(""), "now");
        assert!(is_valid_name(&format!("backup-{}", file_stamp("a b/c\\d"))));
    }
}
