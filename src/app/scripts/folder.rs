//! The folder of scripted indicators, and the list of what is in it.
//!
//! Every `.rhai` file in the folder (and in the folders inside it, a few levels deep) is an
//! indicator. A [`Library`] reads a folder: it lists the files, compiles the ones that are new or
//! changed, and does the things a user does to files (create, save, rename, duplicate, delete,
//! import, export). What it read is published to the
//! [`registry`], which is what the rest of
//! the app looks indicators up in.
//!
//! Nothing here is drawn or waits for a window: a library can be given to another thread, and
//! reading a folder is what the app does in the background to notice a file edited elsewhere.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::indicators::custom::library::{Entry, registry};
use crate::domain::indicators::custom::run::Problem;
use crate::infra::storage::scripts::{ScriptFile, ScriptStore};

// The rules of the folder (the extension, the limits, which names are allowed) are owned by
// `infra::storage`, the same ones the backup and the settings folder use.
/// The extension of an indicator file, the biggest script kept (in bytes), the most indicators a
/// library holds and how many folders deep it looks.
pub use crate::infra::storage::scripts::{EXTENSION, MAX_DEPTH, MAX_FILE_BYTES, MAX_SCRIPTS};

/// The folder a deleted indicator goes to, inside the library's own.
const TRASH: &str = crate::infra::storage::scripts::TRASH;

/// Why an indicator cannot be created, renamed or found.
#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("\"{0}\" cannot be used as a name: {1}")]
    BadName(String, &'static str),
    #[error("an indicator called \"{0}\" already exists")]
    Exists(String),
    #[error("there is no indicator called \"{0}\"")]
    Missing(String),
    #[error("a library holds at most {0} indicators")]
    Full(usize),
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl From<crate::infra::storage::ConfigError> for LibraryError {
    fn from(error: crate::infra::storage::ConfigError) -> Self {
        use crate::infra::storage::ConfigError;
        match error {
            ConfigError::InvalidName { name, reason } => Self::BadName(name, reason),
            ConfigError::Read { path, source } | ConfigError::Write { path, source } => {
                Self::Io { path, source }
            }
            other => Self::Io {
                path: PathBuf::new(),
                source: other.into(),
            },
        }
    }
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> LibraryError + '_ {
    move |source| LibraryError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// One part of an id: a file or folder name. Letters, digits, spaces and `- _ . ( ) % + , &`.
pub fn clean_part(name: &str) -> Result<String, LibraryError> {
    crate::infra::storage::scripts::clean_part(name).map_err(LibraryError::from)
}

/// A whole id (`folder/name`), every part checked, at most [`MAX_DEPTH`] folders deep.
pub fn clean_id(id: &str) -> Result<String, LibraryError> {
    crate::infra::storage::scripts::clean_id(id).map_err(LibraryError::from)
}

/// Reads and compiles one file.
fn load(id: &str, path: &Path, len: u64) -> Entry {
    if len > MAX_FILE_BYTES {
        return Entry::broken(
            id,
            path,
            Arc::from(""),
            vec![Problem::error(
                0,
                0,
                format!("the file is bigger than {} KB", MAX_FILE_BYTES / 1024),
            )],
        );
    }
    match std::fs::read_to_string(path) {
        Ok(text) => Entry::compile(id, path, text),
        Err(error) => Entry::broken(
            id,
            path,
            Arc::from(""),
            vec![Problem::error(
                0,
                0,
                format!("the file cannot be read: {error}"),
            )],
        ),
    }
}

/// A file found in the folder: what [`ScriptStore::scan`] gives.
type Found = ScriptFile;

fn scan(root: &Path) -> Vec<Found> {
    ScriptStore::new(root).scan()
}

/// What a read of the folder changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub removed: Vec<String>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

/// What an import did.
#[derive(Debug, Default)]
pub struct ImportReport {
    /// The ids of the indicators that were added.
    pub imported: Vec<String>,
    /// The files that were not, and why.
    pub skipped: Vec<(PathBuf, ImportSkip)>,
}

/// Why a file was left out of an import. The text is meant to be shown as it is, after "it".
#[derive(Debug, thiserror::Error)]
pub enum ImportSkip {
    #[error("it is not a .{} file", EXTENSION)]
    NotAScript,
    #[error("it cannot be read: {0}")]
    Unreadable(#[source] std::io::Error),
    #[error("it is bigger than {} KB", MAX_FILE_BYTES / 1024)]
    TooBig,
    #[error("it is not a text file")]
    NotText,
    #[error("the library is full ({} indicators)", MAX_SCRIPTS)]
    Full,
    #[error(transparent)]
    Library(#[from] LibraryError),
}

struct Known {
    modified: Option<SystemTime>,
    len: u64,
    entry: Arc<Entry>,
}

/// A folder of indicators, as last read.
pub struct Library {
    dir: PathBuf,
    known: BTreeMap<String, Known>,
}

impl Library {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            known: BTreeMap::new(),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Reads another folder from now on.
    pub fn set_dir(&mut self, dir: impl Into<PathBuf>) {
        self.dir = dir.into();
        self.known.clear();
    }

    /// Makes the folder if it is not there.
    ///
    /// # Errors
    ///
    /// When the folder cannot be made.
    pub fn ensure_dir(&self) -> Result<(), LibraryError> {
        std::fs::create_dir_all(&self.dir).map_err(io_error(&self.dir))
    }

    /// Every indicator read so far, by id.
    pub fn entries(&self) -> Vec<Arc<Entry>> {
        self.known.values().map(|k| k.entry.clone()).collect()
    }

    pub fn get(&self, id: &str) -> Option<Arc<Entry>> {
        self.known.get(id).map(|k| k.entry.clone())
    }

    /// Where the file of `id` is (or would be).
    pub fn path_of(&self, id: &str) -> PathBuf {
        ScriptStore::new(&self.dir).path_of(id)
    }

    /// Reads the folder again: files that are new or edited are compiled, files that are gone
    /// are forgotten, the others are left as they are.
    pub fn refresh(&mut self) -> Changes {
        let mut changes = Changes::default();
        let mut previous = std::mem::take(&mut self.known);
        let mut next = BTreeMap::new();
        for found in scan(&self.dir).into_iter().take(MAX_SCRIPTS) {
            let old = previous.remove(&found.id);
            match old {
                Some(k) if k.modified == found.modified && k.len == found.len => {
                    next.insert(found.id, k);
                }
                old => {
                    let entry = Arc::new(load(&found.id, &found.path, found.len));
                    match &old {
                        None => changes.added.push(found.id.clone()),
                        // Saved again with the same text: nothing changed for anyone.
                        Some(k) if k.entry.stamp != entry.stamp => {
                            changes.changed.push(found.id.clone());
                        }
                        Some(_) => {}
                    }
                    let entry = match old {
                        Some(k) if k.entry.stamp == entry.stamp => k.entry,
                        _ => entry,
                    };
                    next.insert(
                        found.id,
                        Known {
                            modified: found.modified,
                            len: found.len,
                            entry,
                        },
                    );
                }
            }
        }
        changes.removed = previous.into_keys().collect();
        self.known = next;
        changes
    }

    /// Publishes what was read.
    pub fn publish(&self) -> bool {
        registry::install(self.entries())
    }

    fn unique_id(&self, folder: Option<&str>, name: &str) -> Result<String, LibraryError> {
        let name = clean_part(name)?;
        let prefix = match folder {
            Some(folder) if !folder.is_empty() => format!("{}/", clean_id(folder)?),
            _ => String::new(),
        };
        let mut candidate = format!("{prefix}{name}");
        let mut number = 2;
        while self.path_of(&candidate).exists() {
            candidate = format!("{prefix}{name} ({number})");
            number += 1;
            if number > 999 {
                return Err(LibraryError::Exists(name));
            }
        }
        clean_id(&candidate)
    }

    fn write(&self, id: &str, source: &str) -> Result<PathBuf, LibraryError> {
        Ok(ScriptStore::new(&self.dir).write(id, source)?)
    }

    /// Makes a new indicator named `name` (in `folder` when given) and reads it. The id is
    /// `name`, or `name (2)` and so on when it is taken.
    ///
    /// # Errors
    ///
    /// When the name is not allowed, the library is full or the file cannot be written.
    pub fn create(
        &mut self,
        folder: Option<&str>,
        name: &str,
        source: &str,
    ) -> Result<String, LibraryError> {
        if self.known.len() >= MAX_SCRIPTS {
            return Err(LibraryError::Full(MAX_SCRIPTS));
        }
        let id = self.unique_id(folder, name)?;
        self.write(&id, source)?;
        self.refresh();
        Ok(id)
    }

    /// Replaces the text of the indicator `id`, atomically, and reads it.
    ///
    /// # Errors
    ///
    /// When the file cannot be written.
    pub fn save(&mut self, id: &str, source: &str) -> Result<Changes, LibraryError> {
        let id = clean_id(id)?;
        self.write(&id, source)?;
        Ok(self.refresh())
    }

    /// A copy of `id` under a new name. Returns the id of the copy.
    ///
    /// # Errors
    ///
    /// When `id` is unknown or the copy cannot be written.
    pub fn duplicate(&mut self, id: &str) -> Result<String, LibraryError> {
        let source = self
            .get(id)
            .ok_or_else(|| LibraryError::Missing(id.to_owned()))?
            .source
            .clone();
        let (folder, name) = match id.rsplit_once('/') {
            Some((folder, name)) => (Some(folder), name),
            None => (None, id),
        };
        self.create(folder, &format!("{name} copy"), &source)
    }

    /// Renames the indicator `id` (its file) to `new_name`, in the same folder. Returns the new
    /// id. A chart that holds the old id must be told (see the app).
    ///
    /// # Errors
    ///
    /// When `id` is unknown, the name is taken or not allowed, or the file cannot be moved.
    pub fn rename(&mut self, id: &str, new_name: &str) -> Result<String, LibraryError> {
        if !self.known.contains_key(id) {
            return Err(LibraryError::Missing(id.to_owned()));
        }
        let name = clean_part(new_name)?;
        let new_id = match id.rsplit_once('/') {
            Some((folder, _)) => format!("{folder}/{name}"),
            None => name,
        };
        if new_id == id {
            return Ok(new_id);
        }
        let (from, to) = (self.path_of(id), self.path_of(&new_id));
        // A different spelling of the same name (case) is the same file on some systems.
        if to.exists() && !new_id.eq_ignore_ascii_case(id) {
            return Err(LibraryError::Exists(new_id));
        }
        std::fs::rename(&from, &to).map_err(io_error(&from))?;
        self.refresh();
        Ok(new_id)
    }

    /// Takes the indicator `id` out of the folder, into the trash inside it, where it can still
    /// be found.
    ///
    /// # Errors
    ///
    /// When `id` is unknown or the file cannot be moved.
    pub fn delete(&mut self, id: &str) -> Result<(), LibraryError> {
        if !self.known.contains_key(id) {
            return Err(LibraryError::Missing(id.to_owned()));
        }
        let from = self.path_of(id);
        let trash = self.dir.join(TRASH);
        std::fs::create_dir_all(&trash).map_err(io_error(&trash))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let to = trash.join(format!("{}-{stamp}.{EXTENSION}", id.replace('/', "-")));
        std::fs::rename(&from, &to).map_err(io_error(&from))?;
        self.refresh();
        Ok(())
    }

    /// Adds the `.rhai` files of `paths` (and of the folders among them) to the library.
    pub fn import(&mut self, paths: &[PathBuf]) -> ImportReport {
        let mut report = ImportReport::default();
        let mut files = Vec::new();
        for path in paths {
            if path.is_dir() {
                scripts_in(path, &mut files);
            } else {
                files.push(path.clone());
            }
        }
        for path in files {
            match self.import_one(&path) {
                Ok(id) => report.imported.push(id),
                Err(why) => report.skipped.push((path, why)),
            }
        }
        self.refresh();
        report
    }

    fn import_one(&mut self, path: &Path) -> Result<String, ImportSkip> {
        let is_script = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION));
        if !is_script {
            return Err(ImportSkip::NotAScript);
        }
        let meta = std::fs::metadata(path).map_err(ImportSkip::Unreadable)?;
        if meta.len() > MAX_FILE_BYTES {
            return Err(ImportSkip::TooBig);
        }
        let source = std::fs::read_to_string(path).map_err(|_| ImportSkip::NotText)?;
        if self.known.len() >= MAX_SCRIPTS {
            return Err(ImportSkip::Full);
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("indicator");
        // A name the library cannot use is replaced by a plain one, so a file is not lost for it.
        let name = clean_part(stem).unwrap_or_else(|_| {
            let plain: String = stem
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { ' ' })
                .collect();
            clean_part(plain.trim()).unwrap_or_else(|_| "Imported".to_owned())
        });
        let id = self.unique_id(None, &name)?;
        self.write(&id, &source)?;
        Ok(id)
    }

    /// Writes the text of `id` to `dest`: a file, or a folder (the file goes in it under its
    /// own name).
    ///
    /// # Errors
    ///
    /// When `id` is unknown or the file cannot be written.
    pub fn export(&self, id: &str, dest: &Path) -> Result<PathBuf, LibraryError> {
        let entry = self
            .get(id)
            .ok_or_else(|| LibraryError::Missing(id.to_owned()))?;
        let target = if dest.is_dir() {
            let stem = id.rsplit('/').next().unwrap_or(id);
            dest.join(format!("{stem}.{EXTENSION}"))
        } else if dest
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
        {
            dest.to_path_buf()
        } else {
            dest.with_extension(EXTENSION)
        };
        crate::infra::storage::atomic_write(&target, entry.source.as_bytes())?;
        Ok(target)
    }

    /// Writes every indicator into `dest`, with the same folders. Returns how many.
    ///
    /// # Errors
    ///
    /// When a file or folder cannot be written.
    pub fn export_all(&self, dest: &Path) -> Result<usize, LibraryError> {
        let target = ScriptStore::new(dest);
        let mut count = 0;
        for entry in self.known.values().map(|k| &k.entry) {
            crate::infra::storage::atomic_write(
                &target.path_of(&entry.id),
                entry.source.as_bytes(),
            )?;
            count += 1;
        }
        Ok(count)
    }
}

/// Collects the scripts of a folder (and of the folders in it) for [`Library::import`].
fn scripts_in(dir: &Path, out: &mut Vec<PathBuf>) {
    out.extend(ScriptStore::new(dir).scan().into_iter().map(|f| f.path));
}

#[cfg(test)]
mod tests;
