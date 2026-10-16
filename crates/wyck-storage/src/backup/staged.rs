//! Restoring and resetting without writing over a running app: the change waits for the next start.

use super::store::BackupKind;
use super::{
    Backup, BackupFile, BackupScript, CRATE_VERSION, PENDING, RESET_MARKER, read_backup_text,
    store_for,
};

use crate::error::{ConfigError, Result};
use crate::fs_util::{atomic_write, read_text};
use crate::locations::Kind;
use crate::paths::AppPaths;
use crate::scripts::ScriptStore;
use secrecy::SecretString;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Checks the text of a backup (plain, or sealed with `passphrase`) and puts it aside, to be
/// applied when the app starts again by [`apply_pending`]. Returns what it holds. A reset that was
/// waiting is cancelled: the last thing asked for wins.
///
/// What waits is the plain text: a backup holds no credentials.
///
/// # Errors
///
/// As [`Backup::read`], and [`ConfigError::Write`].
pub(crate) fn stage_import(
    paths: &AppPaths,
    text: &str,
    passphrase: Option<&SecretString>,
) -> Result<Backup> {
    let backup = Backup::read(text, passphrase)?;
    atomic_write(
        &paths.config_dir().join(PENDING),
        backup.to_text()?.as_bytes(),
    )?;
    if let Err(error) = cancel_reset(paths) {
        warn!(%error, "could not cancel a reset that waits");
    }
    Ok(backup)
}

/// `stage_import` for the backup file at `path`.
///
/// # Errors
///
/// As `stage_import`, and [`ConfigError::Read`].
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
pub(crate) fn reset_pending(paths: &AppPaths) -> bool {
    paths.config_dir().join(RESET_MARKER).is_file()
}

/// Forgets a reset that waits. Nothing waiting is fine.
///
/// # Errors
///
/// [`ConfigError::Write`] when the marker exists and cannot go.
pub(crate) fn cancel_reset(paths: &AppPaths) -> Result<()> {
    remove_marker(&paths.config_dir().join(RESET_MARKER))
}

/// Removes everything in `dir` but the marker that says the folder is Wyck's. A missing folder is
/// fine.
fn clear_contents(dir: &Path) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(source),
    };
    for entry in entries.flatten() {
        if entry.file_name() == crate::locations::MARKER {
            continue;
        }
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

pub(super) fn remove_marker(path: &Path) -> Result<()> {
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
    let text = match read_text(&path) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(applied),
        Err(source) => return Err(ConfigError::Read { path, source }),
    };
    let Ok(backup) = Backup::parse(&text) else {
        warn!("the staged import cannot be read, setting it aside");
        if let Err(error) =
            std::fs::rename(&path, paths.config_dir().join(format!("{PENDING}.bad")))
        {
            warn!(%error, "could not set the staged import aside");
        }
        applied.set_aside = true;
        return Ok(applied);
    };
    let (written, kept) = apply_import(paths, &backup, created)?;
    remove_marker(&path)?;
    applied.imported = Some(written);
    applied.kept_in = kept.or(applied.kept_in);
    Ok(applied)
}

pub(super) fn apply_reset(paths: &AppPaths, created: &str) -> Result<Option<PathBuf>> {
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
    for kind in [Kind::Settings, Kind::Accounts] {
        let folder = paths.dir(kind);
        let cleared = if paths.is_custom(kind) {
            // A folder the user chose (a cloud folder, say) stays, with its marker: it is emptied,
            // not removed from under whatever syncs it.
            clear_contents(&folder)
        } else {
            match std::fs::remove_dir_all(&folder) {
                Err(source) if source.kind() != std::io::ErrorKind::NotFound => Err(source),
                _ => Ok(()),
            }
        };
        cleared.map_err(|source| ConfigError::Write {
            path: folder,
            source,
        })?;
    }
    remove_marker(&paths.config_dir().join(RESET_MARKER))?;
    info!("reset every saved document");
    Ok(kept)
}

pub(super) fn apply_import(
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

/// Writes a backup of everything to `dest` (sealed when a passphrase is given) and returns what
/// went in. The one call behind an "Export" button: `scripts` is the folder the indicators are
/// really in (see `AppPaths::scripts` for the default).
///
/// # Errors
///
/// As [`Backup::collect`] and `Backup::write_file`.
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
