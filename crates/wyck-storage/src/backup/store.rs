//! The copies the app keeps in its backups folder: listed, read, restored, deleted and pruned.

use super::staged::stage_import;
use super::{Backup, read_backup_text};

use crate::error::{ConfigError, Result};
use crate::fs_util::atomic_write;
use crate::names::{is_valid_name, validate_name};
use crate::paths::AppPaths;
use crate::scripts::ScriptStore;
use crate::sealed;
use secrecy::SecretString;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{debug, info};

/// Why a copy was made: it decides the name of the file and what `BackupStore::prune` keeps.
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
    /// Everything the user made, saved right before an update was installed: what to come back
    /// to if the new version does not suit.
    BeforeUpdate,
}

/// How many copies made before an update are kept.
pub(crate) const KEPT_BEFORE_UPDATE: usize = 3;

impl BackupKind {
    /// The start of the name of the files of this kind.
    #[must_use]
    pub(crate) fn prefix(self) -> &'static str {
        match self {
            Self::Manual => "backup",
            Self::Automatic => "auto",
            Self::BeforeImport => "before-import",
            Self::BeforeReset => "before-reset",
            Self::BeforeUpdate => "before-update",
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
            Self::BeforeUpdate => "Before an update",
        }
    }

    fn of(id: &str) -> Self {
        [
            Self::Automatic,
            Self::BeforeImport,
            Self::BeforeReset,
            Self::BeforeUpdate,
        ]
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
            every: Duration::from_hours(24),
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
pub(super) fn file_stamp(text: &str) -> String {
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
pub(super) fn looks_sealed(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0_u8; 64];
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let read = file.read(&mut head).unwrap_or(0);
    let head = String::from_utf8_lossy(head.get(..read).unwrap_or_default());
    head.trim_start()
        .starts_with(&format!("format = \"{}\"", sealed::FORMAT))
}

impl AppPaths {
    /// The copies kept in the backups folder: [`BackupStore::new`].
    pub fn backups(&self) -> BackupStore {
        BackupStore::new(self)
    }
}

impl BackupStore {
    /// The copies of the install at `paths`.
    #[must_use]
    pub(crate) fn new(paths: &AppPaths) -> Self {
        Self {
            paths: paths.clone(),
        }
    }

    /// The folder the copies are in.
    pub(crate) fn dir(&self) -> PathBuf {
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
    #[cfg(test)]
    pub(crate) fn get(&self, id: &str) -> Result<BackupEntry> {
        self.list()?
            .into_iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| ConfigError::BackupNotFound(id.to_owned()))
    }

    /// Saves `backup` as a [`BackupKind::Manual`] copy, named after when it was made.
    ///
    /// # Errors
    ///
    /// As `Self::save`.
    pub fn create(&self, backup: &Backup) -> Result<BackupEntry> {
        self.save(BackupKind::Manual, backup, None)
    }

    /// Saves `backup` as a copy of `kind`, sealed when a passphrase is given. The file is named
    /// after the kind and after `backup.created`, with a number added when the name is taken.
    ///
    /// # Errors
    ///
    /// As [`Backup::to_text_with`], and [`ConfigError::Write`].
    pub(crate) fn save(
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
        let now = wyck_clock::now();
        Ok(BackupEntry {
            id,
            kind,
            bytes: text.len() as u64,
            modified: Some(now),
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
    /// As [`Backup::collect`] and `Self::save`.
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
        let now = wyck_clock::now();
        let due = newest
            .and_then(|entry| entry.modified)
            .and_then(|modified| now.duration_since(modified).ok())
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

    /// Saves a copy of the install right before an update is installed, and keeps only the newest
    /// `KEPT_BEFORE_UPDATE` of them. `app_version` is the version being left. `None` when there
    /// is nothing to save.
    ///
    /// # Errors
    ///
    /// As [`Backup::collect`] and `Self::save`.
    pub fn snapshot_before_update(
        &self,
        scripts: Option<&ScriptStore>,
        app_version: &str,
        created: &str,
    ) -> Result<Option<BackupEntry>> {
        let backup = Backup::collect(&self.paths, scripts, app_version, created)?;
        if backup.is_empty() {
            return Ok(None);
        }
        let entry = self.save(BackupKind::BeforeUpdate, &backup, None)?;
        self.prune(BackupKind::BeforeUpdate, KEPT_BEFORE_UPDATE)?;
        Ok(Some(entry))
    }

    /// The text of a copy, as it is on disk (sealed or not).
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidName`], [`ConfigError::BackupNotFound`], [`ConfigError::Read`].
    pub(crate) fn read_text(&self, id: &str) -> Result<String> {
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
    #[cfg(test)]
    pub(crate) fn load(&self, id: &str, passphrase: Option<&SecretString>) -> Result<Backup> {
        Backup::read(&self.read_text(id)?, passphrase)
    }

    /// Asks for a copy to come back at the next start (see `stage_import`). Returns what it
    /// holds.
    ///
    /// # Errors
    ///
    /// As `Self::load` and `stage_import`.
    pub fn restore(&self, id: &str, passphrase: Option<&SecretString>) -> Result<Backup> {
        stage_import(&self.paths, &self.read_text(id)?, passphrase)
    }

    /// Writes a copy to `dest`, to carry it somewhere else. The copy is written as it is: sealed
    /// stays sealed.
    ///
    /// # Errors
    ///
    /// As [`Self::read_text`], and [`ConfigError::Write`].
    #[cfg(test)]
    pub(crate) fn export(&self, id: &str, dest: &Path) -> Result<()> {
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
    pub(crate) fn prune(&self, kind: BackupKind, keep: usize) -> Result<usize> {
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

#[cfg(test)]
mod tests {

    use super::super::store::file_stamp;
    use super::super::store::*;

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
