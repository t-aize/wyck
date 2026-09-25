//! The folder of scripted indicators, and the list of what is in it.
//!
//! Every `.rhai` file in the folder (and in the folders inside it, a few levels deep) is an
//! indicator. Its id is its path from the folder without the extension, `trend/my_average`, and
//! that id is what a chart saves to remember which indicator it holds.
//!
//! A [`Library`] reads a folder: it lists the files, compiles the ones that are new or changed,
//! and does the things a user does to files (create, save, rename, duplicate, delete, import,
//! export). What it read is published to the [`registry`], which is what the rest of the app looks
//! indicators up in.
//!
//! Nothing here is drawn or waits for a window: a library can be given to another thread, and
//! reading a folder is what the app does in the background to notice a file edited elsewhere.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::super::intern::{self, Slices};
use super::super::{InputSpec, Placement, PlotSpec, Spec};
use super::run::{Declaration, Limits, Problem, Script};

/// The extension of an indicator file.
pub const EXTENSION: &str = "rhai";

/// The biggest script kept, in bytes.
pub const MAX_FILE_BYTES: u64 = 256 * 1024;

/// The most indicators a library holds.
pub const MAX_SCRIPTS: usize = 500;

/// How many folders deep the library looks.
pub const MAX_DEPTH: usize = 3;

/// What a script may do while it is being declared (run on no bars): far less than a computation.
const DECLARE_LIMITS: Limits = Limits {
    operations: 2_000_000,
    time: Duration::from_secs(1),
};

/// The folder a deleted indicator goes to, inside the library's own.
const TRASH: &str = ".trash";

/// What is said about an indicator apart from how it computes: the words the menus show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub name: String,
    pub short: String,
    pub category: String,
    pub description: String,
    pub author: String,
    pub version: String,
}

/// One indicator file, as it was read.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The path from the folder, without the extension, with `/` between folders.
    pub id: String,
    pub path: PathBuf,
    pub source: Arc<str>,
    /// The compiled script; `None` when it does not compile or fail while declaring.
    pub script: Option<Arc<Script>>,
    pub problems: Vec<Problem>,
    /// What the panels need to show the indicator and its settings.
    pub spec: Spec,
    pub info: Info,
    /// Tells two versions of a script apart.
    pub stamp: u64,
}

impl Entry {
    pub fn is_ready(&self) -> bool {
        self.script.is_some()
    }
}

// ---- the registry ----

#[derive(Default)]
struct Registry {
    entries: BTreeMap<String, Arc<Entry>>,
    revision: u64,
}

static REGISTRY: RwLock<Option<Registry>> = RwLock::new(None);

fn with_registry<R>(f: impl FnOnce(&mut Registry) -> R) -> R {
    let mut guard = REGISTRY.write().unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(Registry::default))
}

/// What every chart looks the indicators it holds up in.
pub mod registry {
    use super::*;

    /// The indicator called `id`.
    pub fn get(id: &str) -> Option<Arc<Entry>> {
        with_registry(|r| r.entries.get(id).cloned())
    }

    /// Whether there is an indicator called `id`.
    pub fn contains(id: &str) -> bool {
        with_registry(|r| r.entries.contains_key(id))
    }

    /// Every indicator, by id.
    pub fn all() -> Vec<Arc<Entry>> {
        with_registry(|r| r.entries.values().cloned().collect())
    }

    /// A number that grows whenever what is published changes.
    #[cfg(test)]
    pub fn revision() -> u64 {
        with_registry(|r| r.revision)
    }

    /// Publishes `entries` as the whole list. Returns whether it differs from what was there.
    pub fn install(entries: Vec<Arc<Entry>>) -> bool {
        with_registry(|r| {
            let next: BTreeMap<String, Arc<Entry>> =
                entries.into_iter().map(|e| (e.id.clone(), e)).collect();
            let same = next.len() == r.entries.len()
                && next
                    .iter()
                    .all(|(id, e)| r.entries.get(id).is_some_and(|old| old.stamp == e.stamp));
            if !same {
                r.entries = next;
                r.revision += 1;
            }
            !same
        })
    }
}

// ---- names ----

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

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> LibraryError + '_ {
    move |source| LibraryError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// The names Windows keeps for devices; a file cannot have them whatever its extension.
const RESERVED: [&str; 12] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "lpt1", "lpt2", "lpt3", ".",
];

/// One part of an id: a file or folder name. Letters, digits, spaces and `- _ . ( )`.
pub fn clean_part(name: &str) -> Result<String, LibraryError> {
    let name = name.trim();
    let bad = |why| LibraryError::BadName(name.to_owned(), why);
    if name.is_empty() {
        return Err(bad("it is empty"));
    }
    if name.chars().count() > 64 {
        return Err(bad("it is longer than 64 characters"));
    }
    if name.starts_with('.') || name.ends_with('.') {
        return Err(bad("it cannot start or end with a dot"));
    }
    if !name.chars().all(|c| {
        c.is_alphanumeric()
            || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')' | '%' | '+' | ',' | '&')
    }) {
        return Err(bad("use letters, digits, spaces and - _ . ( ) % + , &"));
    }
    let stem = name.split('.').next().unwrap_or(name).to_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        return Err(bad("the system keeps that name"));
    }
    Ok(name.to_owned())
}

/// A whole id (`folder/name`), every part checked, at most [`MAX_DEPTH`] folders deep.
pub fn clean_id(id: &str) -> Result<String, LibraryError> {
    let parts: Vec<&str> = id.split('/').collect();
    if parts.len() > MAX_DEPTH + 1 {
        return Err(LibraryError::BadName(
            id.to_owned(),
            "the folders are nested too deep",
        ));
    }
    let cleaned: Result<Vec<String>, _> = parts.into_iter().map(clean_part).collect();
    Ok(cleaned?.join("/"))
}

/// A short name for the legend: the initials of several words, or the first letters of one.
pub fn short_of(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let text: String = if words.len() > 1 {
        words.iter().filter_map(|w| w.chars().next()).collect()
    } else {
        name.chars()
            .filter(|c| c.is_alphanumeric())
            .take(5)
            .collect()
    };
    let text: String = text.to_uppercase().chars().take(6).collect();
    if text.is_empty() {
        "?".to_owned()
    } else {
        text
    }
}

// ---- what an indicator says about itself ----

static INPUTS: Slices<InputSpec> = Slices::new();
static PLOTS: Slices<PlotSpec> = Slices::new();

/// The spec the panels use, from what a script declared.
pub fn spec_of(id: &str, declaration: &Declaration) -> (Spec, Info) {
    let meta = &declaration.meta;
    let stem = id.rsplit('/').next().unwrap_or(id);
    let name = meta.name.clone().unwrap_or_else(|| stem.to_owned());
    let short = meta.short.clone().unwrap_or_else(|| short_of(&name));
    let category = if !meta.category.is_empty() {
        meta.category.clone()
    } else if let Some((folder, _)) = id.rsplit_once('/') {
        folder.rsplit('/').next().unwrap_or(folder).to_owned()
    } else {
        "Custom".to_owned()
    };
    let inputs: Vec<InputSpec> = declaration
        .inputs
        .iter()
        .map(|i| InputSpec {
            key: intern::name(&i.key),
            label: intern::name(&i.label),
            kind: i.input_kind(),
            default: i.default,
            min: i.min,
            max: i.max,
            step: i.step,
        })
        .collect();
    let plots: Vec<PlotSpec> = declaration
        .plots
        .iter()
        .map(|p| PlotSpec {
            key: intern::name(&p.key),
            label: intern::name(&p.label),
            kind: p.kind,
            color: p.color,
            width: p.width,
            dash: p.dash,
        })
        .collect();
    let spec = Spec {
        label: intern::name(&name),
        short: intern::name(&short),
        placement: if meta.overlay {
            Placement::Overlay
        } else {
            Placement::Pane
        },
        format: meta.format,
        inputs: INPUTS.get(&inputs),
        plots: PLOTS.get(&plots),
        range: meta.range,
    };
    let info = Info {
        name,
        short,
        category,
        description: meta.description.clone(),
        author: meta.author.clone(),
        version: meta.version.clone(),
    };
    (spec, info)
}

/// What a chart shows for an indicator whose script is gone or does not work.
pub fn missing_spec(id: &str) -> Spec {
    let stem = id.rsplit('/').next().unwrap_or(id);
    Spec {
        label: intern::name(stem),
        short: intern::name(&short_of(stem)),
        placement: Placement::Overlay,
        format: super::super::ValueFormat::Plain(2),
        inputs: &[],
        plots: &[],
        range: None,
    }
}

fn stamp_of(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

/// Reads and compiles one file.
fn load(id: &str, path: &Path, len: u64) -> Entry {
    let broken = |source: Arc<str>, problems: Vec<Problem>| {
        let declaration = Declaration::default();
        let (spec, mut info) = spec_of(id, &declaration);
        info.category = "Broken".to_owned();
        Entry {
            id: id.to_owned(),
            path: path.to_path_buf(),
            stamp: stamp_of(&source),
            source,
            script: None,
            problems,
            spec: missing_spec(id).with(spec.label, spec.short),
            info,
        }
    };
    if len > MAX_FILE_BYTES {
        return broken(
            Arc::from(""),
            vec![Problem::error(
                0,
                0,
                format!("the file is bigger than {} KB", MAX_FILE_BYTES / 1024),
            )],
        );
    }
    let source = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            return broken(
                Arc::from(""),
                vec![Problem::error(
                    0,
                    0,
                    format!("the file cannot be read: {error}"),
                )],
            );
        }
    };
    match Script::compile_with(&source, DECLARE_LIMITS) {
        Ok(script) => {
            let (spec, info) = spec_of(id, &script.declaration);
            Entry {
                id: id.to_owned(),
                path: path.to_path_buf(),
                stamp: stamp_of(&source),
                source: Arc::from(source),
                problems: script.warnings.clone(),
                script: Some(Arc::new(script)),
                spec,
                info,
            }
        }
        Err(problems) => broken(Arc::from(source), problems),
    }
}

impl Spec {
    /// The same spec under other names.
    fn with(mut self, label: &'static str, short: &'static str) -> Self {
        self.label = label;
        self.short = short;
        self
    }
}

// ---- the folder ----

/// A file found in the folder.
struct Found {
    id: String,
    path: PathBuf,
    modified: Option<SystemTime>,
    len: u64,
}

fn scan(root: &Path) -> Vec<Found> {
    let mut out = Vec::new();
    scan_into(root, root, 0, &mut out);
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn scan_into(root: &Path, dir: &Path, depth: usize, out: &mut Vec<Found>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for item in read.flatten() {
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        // Hidden files and folders (the trash, an editor's leftovers) are not indicators.
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
            out.push(Found {
                id,
                path,
                modified: meta.modified().ok(),
                len: meta.len(),
            });
        }
    }
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
#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    /// The ids of the indicators that were added.
    pub imported: Vec<String>,
    /// The files that were not, and why.
    pub skipped: Vec<(PathBuf, String)>,
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
        let mut path = self.dir.clone();
        for part in id.split('/') {
            path.push(part);
        }
        path.set_extension(EXTENSION);
        path
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
        if source.len() as u64 > MAX_FILE_BYTES {
            return Err(LibraryError::BadName(
                id.to_owned(),
                "the script is bigger than 256 KB",
            ));
        }
        let path = self.path_of(id);
        wyck::config::atomic_write(&path, source.as_bytes()).map_err(|error| LibraryError::Io {
            path: path.clone(),
            source: std::io::Error::other(error.to_string()),
        })?;
        Ok(path)
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
                scripts_in(path, 0, &mut files);
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

    fn import_one(&mut self, path: &Path) -> Result<String, String> {
        let is_script = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION));
        if !is_script {
            return Err(format!("it is not a .{EXTENSION} file"));
        }
        let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        if meta.len() > MAX_FILE_BYTES {
            return Err(format!("it is bigger than {} KB", MAX_FILE_BYTES / 1024));
        }
        let source =
            std::fs::read_to_string(path).map_err(|_| "it is not a text file".to_owned())?;
        if self.known.len() >= MAX_SCRIPTS {
            return Err(format!("the library is full ({MAX_SCRIPTS} indicators)"));
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
        let id = self.unique_id(None, &name).map_err(|e| e.to_string())?;
        self.write(&id, &source).map_err(|e| e.to_string())?;
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
        std::fs::write(&target, entry.source.as_bytes()).map_err(io_error(&target))?;
        Ok(target)
    }

    /// Writes every indicator into `dest`, with the same folders. Returns how many.
    ///
    /// # Errors
    ///
    /// When a file or folder cannot be written.
    pub fn export_all(&self, dest: &Path) -> Result<usize, LibraryError> {
        let mut count = 0;
        for entry in self.known.values().map(|k| &k.entry) {
            let mut target = dest.to_path_buf();
            for part in entry.id.split('/') {
                target.push(part);
            }
            target.set_extension(EXTENSION);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(io_error(parent))?;
            }
            std::fs::write(&target, entry.source.as_bytes()).map_err(io_error(&target))?;
            count += 1;
        }
        Ok(count)
    }
}

/// Collects the scripts of a folder (and of the folders in it) for [`Library::import`].
fn scripts_in(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let mut found = Vec::new();
    scan_into(dir, dir, depth, &mut found);
    out.extend(found.into_iter().map(|f| f.path));
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMA: &str = "indicator(#{ name: \"Two lines\", short: \"TL\", overlay: true });\n\
        let len = input_int(\"length\", 20, #{ min: 2, max: 200 });\n\
        plot(\"fast\", sma(close, len));";

    fn library() -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path().join("indicators"));
        library.ensure_dir().unwrap();
        (dir, library)
    }

    #[test]
    fn a_file_in_the_folder_is_an_indicator_with_a_spec() {
        let (_guard, mut library) = library();
        library.create(None, "Two lines", SMA).unwrap();
        let entry = library.get("Two lines").unwrap();
        assert!(entry.is_ready());
        assert_eq!(entry.spec.label, "Two lines");
        assert_eq!(entry.spec.short, "TL");
        assert_eq!(entry.spec.placement, Placement::Overlay);
        assert_eq!(entry.spec.inputs.len(), 1);
        assert_eq!(entry.spec.inputs[0].key, "length");
        assert_eq!(entry.spec.plots.len(), 1);
    }

    #[test]
    fn a_script_with_a_mistake_is_listed_as_broken_with_its_problems() {
        let (_guard, mut library) = library();
        library.create(None, "Oops", "plot(\"a\", clos);").unwrap();
        let entry = library.get("Oops").unwrap();
        assert!(!entry.is_ready());
        assert_eq!(entry.problems.len(), 1);
        assert_eq!(entry.info.category, "Broken");
    }

    #[test]
    fn folders_make_categories_and_ids() {
        let (_guard, mut library) = library();
        let id = library.create(Some("Trend"), "Two lines", SMA).unwrap();
        assert_eq!(id, "Trend/Two lines");
        assert_eq!(library.get(&id).unwrap().info.category, "Trend");
        assert!(
            library.path_of(&id).ends_with("Trend/Two lines.rhai")
                || library.path_of(&id).ends_with("Trend\\Two lines.rhai")
        );
    }

    #[test]
    fn refreshing_tells_what_was_added_edited_and_removed_and_leaves_the_rest() {
        let (_guard, mut library) = library();
        library.create(None, "One", SMA).unwrap();
        library.create(None, "Two", SMA).unwrap();
        assert!(library.refresh().is_empty());
        let one = library.get("One").unwrap();

        let path = library.path_of("Two");
        std::fs::write(&path, format!("{SMA}\nplot(\"slow\", sma(close, 50));")).unwrap();
        std::fs::write(library.path_of("Three"), SMA).unwrap();
        std::fs::remove_file(library.path_of("One")).unwrap();
        let changes = library.refresh();
        assert_eq!(changes.added, ["Three"]);
        assert_eq!(changes.changed, ["Two"]);
        assert_eq!(changes.removed, ["One"]);
        assert!(library.get("One").is_none());
        assert_eq!(one.id, "One");
    }

    #[test]
    fn saving_the_same_text_changes_nothing_for_anyone() {
        let (_guard, mut library) = library();
        library.create(None, "One", SMA).unwrap();
        let before = library.get("One").unwrap();
        let changes = library.save("One", SMA).unwrap();
        assert!(changes.is_empty(), "{changes:?}");
        assert!(Arc::ptr_eq(&before, &library.get("One").unwrap()));
    }

    #[test]
    fn names_that_are_taken_get_a_number() {
        let (_guard, mut library) = library();
        assert_eq!(library.create(None, "A", SMA).unwrap(), "A");
        assert_eq!(library.create(None, "A", SMA).unwrap(), "A (2)");
        assert_eq!(library.duplicate("A").unwrap(), "A copy");
    }

    #[test]
    fn names_the_system_would_refuse_are_refused_here() {
        for bad in [
            "",
            "  ",
            ".hidden",
            "a/b",
            "con",
            "NUL",
            "what?",
            "x\\y",
            "tab\tname",
        ] {
            assert!(clean_part(bad).is_err(), "{bad:?}");
        }
        assert_eq!(clean_part(" My average (2) ").unwrap(), "My average (2)");
        assert!(clean_id("a/b/c/d/e/f").is_err());
        assert_eq!(clean_id("Trend/Fast MA").unwrap(), "Trend/Fast MA");
    }

    #[test]
    fn renaming_moves_the_file_and_refuses_a_taken_name() {
        let (_guard, mut library) = library();
        library.create(Some("F"), "Old", SMA).unwrap();
        library.create(Some("F"), "Other", SMA).unwrap();
        assert_eq!(library.rename("F/Old", "New").unwrap(), "F/New");
        assert!(library.get("F/Old").is_none() && library.get("F/New").is_some());
        assert!(matches!(
            library.rename("F/New", "Other"),
            Err(LibraryError::Exists(_))
        ));
        assert!(matches!(
            library.rename("nope", "X"),
            Err(LibraryError::Missing(_))
        ));
    }

    #[test]
    fn deleting_keeps_the_file_in_the_trash() {
        let (_guard, mut library) = library();
        library.create(None, "Gone", SMA).unwrap();
        library.delete("Gone").unwrap();
        assert!(library.get("Gone").is_none());
        let trash: Vec<_> = std::fs::read_dir(library.dir().join(TRASH))
            .unwrap()
            .collect();
        assert_eq!(trash.len(), 1);
        // The trash is not read as indicators.
        assert!(library.refresh().is_empty());
    }

    #[test]
    fn importing_takes_scripts_and_says_why_it_skips_the_rest() {
        let (guard, mut library) = library();
        let elsewhere = guard.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("good.rhai"), SMA).unwrap();
        std::fs::write(elsewhere.join("notes.txt"), "hello").unwrap();
        std::fs::write(elsewhere.join("binary.rhai"), [0xff, 0xfe, 0x00, 0x81]).unwrap();
        let report = library.import(&[
            elsewhere.join("good.rhai"),
            elsewhere.join("notes.txt"),
            elsewhere.join("binary.rhai"),
        ]);
        assert_eq!(report.imported, ["good"]);
        assert_eq!(report.skipped.len(), 2);
        assert!(library.get("good").unwrap().is_ready());
        // Importing it again does not replace the first.
        let again = library.import(&[elsewhere.join("good.rhai")]);
        assert_eq!(again.imported, ["good (2)"]);
    }

    #[test]
    fn exporting_writes_the_text_where_asked() {
        let (guard, mut library) = library();
        library.create(Some("F"), "Out", SMA).unwrap();
        let dest = guard.path().join("exported");
        std::fs::create_dir_all(&dest).unwrap();
        let file = library.export("F/Out", &dest).unwrap();
        assert_eq!(std::fs::read_to_string(file).unwrap(), SMA);
        let all = guard.path().join("all");
        assert_eq!(library.export_all(&all).unwrap(), 1);
        assert!(all.join("F").join("Out.rhai").exists());
    }

    #[test]
    fn a_huge_file_is_not_read() {
        let (_guard, mut library) = library();
        std::fs::write(
            library.path_of("Big"),
            vec![b'a'; (MAX_FILE_BYTES + 1) as usize],
        )
        .unwrap();
        library.refresh();
        let entry = library.get("Big").unwrap();
        assert!(!entry.is_ready());
        assert!(entry.problems[0].message.contains("bigger"));
    }

    #[test]
    fn what_is_published_is_what_charts_look_up() {
        let (_guard, mut library) = library();
        library.create(None, "Published one", SMA).unwrap();
        let before = registry::revision();
        assert!(library.publish());
        assert!(registry::revision() > before);
        assert!(registry::get("Published one").is_some());
        assert!(
            !library.publish(),
            "publishing the same thing again changes nothing"
        );
    }

    #[test]
    fn short_names_are_made_from_the_name() {
        assert_eq!(short_of("Two lines"), "TL");
        assert_eq!(short_of("Momentum"), "MOMEN");
        assert_eq!(short_of("!!!"), "?");
    }
}
