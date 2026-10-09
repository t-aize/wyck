//! The folder of indicator scripts: where the files are, which names they may have, how they are
//! read and written.
//!
//! An indicator is one text file with the [`EXTENSION`] extension. Its **id** is its path from the
//! folder without the extension, with `/` between folders (`trend/my average`): that is what a
//! chart saves to remember which indicator it holds, and what a backup stores it under.
//!
//! This module does not know what a script means (compiling and running it is the job of the code
//! that owns the language). It owns what has to be the same everywhere the files are touched:
//! the rule for ids ([`clean_id`]), the limits, the scan of the folder, and the atomic write. The
//! editor, the import and export of the indicator library and the backup all go through it.
//!
//! ```
//! use wyck::infra::storage::AppPaths;
//! use wyck::infra::storage::scripts::ScriptStore;
//!
//! # let dir = tempfile::tempdir().unwrap();
//! # let paths = AppPaths::at(dir.path());
//! let store = ScriptStore::in_config(&paths);
//! store.write("trend/my average", "plot(\"a\", close);")?;
//! assert_eq!(store.ids(), ["trend/my average"]);
//! assert_eq!(store.read("trend/my average")?.as_deref(), Some("plot(\"a\", close);"));
//! # Ok::<(), wyck::infra::storage::ConfigError>(())
//! ```

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::infra::storage::error::{ConfigError, Result};
use crate::infra::storage::fs_util::atomic_write;
use crate::infra::storage::paths::AppPaths;

/// The extension of an indicator file.
pub const EXTENSION: &str = "rhai";
/// The biggest script kept, in bytes.
pub const MAX_FILE_BYTES: u64 = 256 * 1024;
/// The most scripts a folder holds.
pub const MAX_SCRIPTS: usize = 500;
/// How many folders deep the scripts are looked for.
pub const MAX_DEPTH: usize = 3;
/// The folder a deleted script goes to, inside the scripts folder. Hidden from every scan.
pub const TRASH: &str = ".trash";

/// The names Windows keeps for devices; a file cannot have them whatever its extension.
const RESERVED: [&str; 12] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "lpt1", "lpt2", "lpt3", ".",
];

fn bad(name: &str, reason: &'static str) -> ConfigError {
    ConfigError::InvalidName {
        name: name.to_owned(),
        reason,
    }
}

/// One part of an id: a file or folder name. Letters, digits, spaces and `- _ . ( ) % + , &`, at
/// most 64 characters, not starting or ending with a dot, and not a name the system keeps.
///
/// # Errors
///
/// [`ConfigError::InvalidName`], saying why.
pub fn clean_part(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(bad(name, "it is empty"));
    }
    if name.chars().count() > 64 {
        return Err(bad(name, "it is longer than 64 characters"));
    }
    if name.starts_with('.') || name.ends_with('.') {
        return Err(bad(name, "it cannot start or end with a dot"));
    }
    if !name.chars().all(|c| {
        c.is_alphanumeric()
            || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')' | '%' | '+' | ',' | '&')
    }) {
        return Err(bad(
            name,
            "use letters, digits, spaces and - _ . ( ) % + , &",
        ));
    }
    let stem = name.split('.').next().unwrap_or(name).to_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        return Err(bad(name, "the system keeps that name"));
    }
    Ok(name.to_owned())
}

/// A whole id (`folder/name`), every part checked, at most [`MAX_DEPTH`] folders deep.
///
/// # Errors
///
/// [`ConfigError::InvalidName`], saying why.
pub fn clean_id(id: &str) -> Result<String> {
    let parts: Vec<&str> = id.split('/').collect();
    if parts.len() > MAX_DEPTH + 1 {
        return Err(bad(id, "the folders are nested too deep"));
    }
    let cleaned: Result<Vec<String>> = parts.into_iter().map(clean_part).collect();
    Ok(cleaned?.join("/"))
}

/// Whether `id` is already in the form [`clean_id`] gives back: what a backup must hold, since it
/// is used as a path.
#[must_use]
pub fn is_clean_id(id: &str) -> bool {
    clean_id(id).is_ok_and(|cleaned| cleaned == id)
}

/// A script file found in the folder, not read yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptFile {
    /// The path from the folder, without the extension, with `/` between folders.
    pub id: String,
    /// Where the file is.
    pub path: PathBuf,
    /// When it was last changed, when the system says.
    pub modified: Option<SystemTime>,
    /// Its size in bytes.
    pub len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptText {
    /// The id of the script.
    pub id: String,
    pub source: String,
}

/// A folder of indicator scripts. Holds only a path, so it is cheap to clone and every call goes
/// to the file system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptStore {
    dir: PathBuf,
}

impl ScriptStore {
    /// The scripts of the folder `dir`, wherever the user keeps them.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The scripts of the default folder, inside the config folder (see
    /// [`AppPaths::indicators_dir`]).
    pub fn in_config(paths: &AppPaths) -> Self {
        Self::new(paths.indicators_dir())
    }

    /// The folder the scripts are in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where the file of `id` is (or would be). The id is not checked: see [`clean_id`].
    pub fn path_of(&self, id: &str) -> PathBuf {
        let mut path = self.dir.clone();
        for part in id.split('/') {
            path.push(part);
        }
        path.set_extension(EXTENSION);
        path
    }

    /// Makes the folder when it is not there.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Write`] when it cannot be made.
    pub fn ensure_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.dir).map_err(|source| ConfigError::Write {
            path: self.dir.clone(),
            source,
        })
    }

    /// Every script file of the folder (and of the folders in it, [`MAX_DEPTH`] deep), by id. Hidden
    /// files and folders (the trash, an editor's leftovers), links and files of another kind are
    /// not scripts. A missing folder gives an empty list.
    pub fn scan(&self) -> Vec<ScriptFile> {
        let mut found = Vec::new();
        scan_into(&self.dir, &self.dir, 0, &mut found);
        found.sort_by(|a, b| a.id.cmp(&b.id));
        found
    }

    /// The ids of every script of the folder.
    pub fn ids(&self) -> Vec<String> {
        self.scan().into_iter().map(|file| file.id).collect()
    }

    /// Every script with its text, at most [`MAX_SCRIPTS`]. A file that is too big or is not text
    /// is left out: it cannot be one of the scripts the app runs, so it has no place in a copy of
    /// them either.
    pub fn read_all(&self) -> Vec<ScriptText> {
        self.scan()
            .into_iter()
            .filter(|file| file.len <= MAX_FILE_BYTES)
            .filter_map(|file| {
                let source = std::fs::read_to_string(&file.path).ok()?;
                Some(ScriptText {
                    id: file.id,
                    source,
                })
            })
            .take(MAX_SCRIPTS)
            .collect()
    }

    /// The text of the script `id`, or `None` when there is no such file.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidName`] for an id that could point outside the folder,
    /// [`ConfigError::Read`] when the file exists and cannot be read as text.
    pub fn read(&self, id: &str) -> Result<Option<String>> {
        clean_id(id)?;
        let path = self.path_of(id);
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(Some(text)),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(ConfigError::Read { path, source }),
        }
    }

    /// Writes the script `id`, atomically, making the folders it needs. Returns where it went.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidName`] for an id that is not allowed or a text over
    /// [`MAX_FILE_BYTES`], [`ConfigError::Write`] on an I/O failure.
    pub fn write(&self, id: &str, source: &str) -> Result<PathBuf> {
        clean_id(id)?;
        if source.len() as u64 > MAX_FILE_BYTES {
            return Err(bad(id, "the script is bigger than 256 KB"));
        }
        let path = self.path_of(id);
        atomic_write(&path, source.as_bytes())?;
        Ok(path)
    }

    /// Copies every script into `dest`, with the same folders, atomically. Returns how many.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Write`] when a file cannot be written.
    pub fn export_all(&self, dest: &Path) -> Result<usize> {
        let target = Self::new(dest);
        let scripts = self.read_all();
        for script in &scripts {
            target.write(&script.id, &script.source)?;
        }
        Ok(scripts.len())
    }
}

fn scan_into(root: &Path, dir: &Path, depth: usize, out: &mut Vec<ScriptFile>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for item in read.flatten() {
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        // Hidden files and folders (the trash, an editor's leftovers) are not scripts.
        if name.starts_with('.') || name.starts_with('~') {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            if depth < MAX_DEPTH {
                scan_into(root, &path, depth + 1, out);
            }
        } else if meta.is_file()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
        {
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let id = relative
                .with_extension("")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push(ScriptFile {
                id,
                path,
                modified: meta.modified().ok(),
                len: meta.len(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ScriptStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ScriptStore::new(dir.path().join("indicators"));
        (dir, store)
    }

    #[test]
    fn an_id_cannot_leave_the_folder_or_use_a_device_name() {
        for bad in [
            "../escape",
            "a/../b",
            "con",
            "C:/x",
            ".hidden",
            "",
            "a//b",
            "a/b/c/d/e",
            "trailing.",
        ] {
            assert!(clean_id(bad).is_err(), "{bad:?} was accepted");
            assert!(!is_clean_id(bad), "{bad:?}");
        }
        for good in ["Mine", "trend/My average (2)", "a/b/c/d", "rsi+macd, 50%"] {
            assert_eq!(clean_id(good).unwrap(), good);
            assert!(is_clean_id(good));
        }
        assert!(!is_clean_id(" padded "), "a padded id is not clean as is");
    }

    #[test]
    fn scripts_are_written_atomically_scanned_and_read_back() {
        let (_dir, store) = store();
        assert!(store.scan().is_empty(), "a missing folder is empty");
        store.write("Trend/Average", "plot(\"a\", close);").unwrap();
        store.write("Other", "plot(\"b\", open);").unwrap();
        // Not scripts: hidden, another kind, a leftover of an editor.
        std::fs::write(store.dir().join("notes.txt"), "x").unwrap();
        std::fs::write(store.dir().join(".hidden.rhai"), "x").unwrap();
        std::fs::write(store.dir().join("~lock.rhai"), "x").unwrap();
        std::fs::create_dir_all(store.dir().join(TRASH)).unwrap();
        std::fs::write(store.dir().join(TRASH).join("gone.rhai"), "x").unwrap();

        assert_eq!(store.ids(), ["Other", "Trend/Average"]);
        assert_eq!(
            store.read("Trend/Average").unwrap().as_deref(),
            Some("plot(\"a\", close);")
        );
        assert_eq!(store.read("Nothing").unwrap(), None);
        assert!(store.read("../x").is_err());
        assert!(
            crate::infra::storage::fs_util::stale_temp_files(store.dir()).is_empty(),
            "no temporary file is left"
        );
    }

    #[test]
    fn a_script_too_big_or_not_text_is_not_written_and_not_copied() {
        let (_dir, store) = store();
        let big = "x".repeat(MAX_FILE_BYTES as usize + 1);
        assert!(store.write("Big", &big).is_err());
        assert!(store.write("../x", "y").is_err());

        store.write("Fine", "ok").unwrap();
        std::fs::write(store.path_of("Binary"), [0xff, 0xfe, 0x00, 0x81]).unwrap();
        std::fs::write(store.path_of("Huge"), big).unwrap();
        let texts = store.read_all();
        assert_eq!(texts.len(), 1, "only the readable one is a copy: {texts:?}");
        assert_eq!(texts[0].id, "Fine");
    }

    #[test]
    fn export_all_keeps_the_folders() {
        let (dir, store) = store();
        store.write("A/B", "1").unwrap();
        store.write("C", "2").unwrap();
        let dest = dir.path().join("out");
        assert_eq!(store.export_all(&dest).unwrap(), 2);
        assert_eq!(
            std::fs::read_to_string(dest.join("A").join("B.rhai")).unwrap(),
            "1"
        );
        assert_eq!(std::fs::read_to_string(dest.join("C.rhai")).unwrap(), "2");
    }
}
