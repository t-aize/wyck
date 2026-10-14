//! Moving a kind of data to another folder, safely.
//!
//! The user asks for a move in the settings ([`stage`]); nothing is moved while the app runs,
//! because the documents are held in memory and saved to the folder they were read from. The next
//! start applies it ([`apply_pending`]), before anything reads a document, as a restore does.
//!
//! # What happens to the files
//!
//! * **The new folder is empty** (or does not exist): the files are copied there one by one, each
//!   through a temporary name, then every file is read back and compared. Only when all of them
//!   match is the new folder marked as Wyck's (see [`crate::infra::storage::locations`]) and the
//!   choice written in `locations.toml`. A copy that is cut short never looks like a folder with
//!   data in it, because the marker is the last thing written.
//! * **The new folder already holds data of Wyck** (a second computer joining a cloud folder the
//!   first one filled): it is used as it is, and nothing from this computer is copied over it.
//! * **The old folder is kept.** After a copy or a switch to data that was already there, the old
//!   folder is renamed `<name>.moved-<date>` beside where it was and listed in `locations.toml`,
//!   so nothing is lost if the user changes their mind; [`delete_leftover`] removes one on request.
//! * **The logs are not carried**: a log says what happened on one computer, in one place.
//!
//! # What is refused
//!
//! A folder that is the folder of another kind or inside it (one kind of data must not be mixed with
//! another), one that holds the config or the credentials or is inside the credentials folder (a
//! synced folder holds nothing that signs in), and every folder [`locations::check`] refuses: one
//! that is not Wyck's and is not empty, one made by the other build, one that is not there.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::infra::storage::error::{ConfigError, Result};
use crate::infra::storage::fs_util::{atomic_write, read_text, retry_when_busy};
use crate::infra::storage::locations::{self, Kind, Locations, Problem, Use};
use crate::infra::storage::paths::AppPaths;

/// The name of the file a move waits in, in the config folder.
pub const PENDING: &str = "pending-relocation.toml";

/// The largest file whose content is compared byte by byte after a copy; a larger one is compared
/// by its size.
const COMPARED_BYTES: u64 = 8 * 1024 * 1024;

/// How deep a folder is followed. A tree deeper than this is not data of Wyck.
const MAX_DEPTH: usize = 12;

/// Moves `kind` to `to`, or with `None` back to its standard folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub kind: Kind,
    pub to: Option<PathBuf>,
}

/// What a move does to the files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// The new folder is taken as it is: there is nothing to carry.
    Switch,
    /// The files of the old folder are copied to the new one.
    Copy { files: usize, bytes: u64 },
    /// The new folder already holds data (`files` of them): it is used, and the files of this
    /// computer are kept aside.
    Adopt { files: usize },
}

/// One kind to move, with where from, where to and what happens to its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub kind: Kind,
    pub from: PathBuf,
    pub to: PathBuf,
    /// Whether `to` is the standard folder, so the choice is forgotten rather than saved.
    pub to_standard: bool,
    pub action: Action,
}

/// Why a move cannot be made.
#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("{0}")]
    Folder(#[from] Problem),
    #[error("{a} and {b} cannot be kept in the same folder, or one inside the other")]
    Mixed { a: Kind, b: Kind },
    #[error(
        "`{0}` holds the sign-in or the list of accounts of this computer, and nothing like that goes into a folder you move"
    )]
    Sensitive(PathBuf),
    #[error("`{0}` cannot be saved: its name is not valid text")]
    NotText(PathBuf),
    #[error(transparent)]
    Storage(#[from] ConfigError),
}

/// `path` with its `.` and `..` worked out by reading it, without asking the disk: a `..` of a
/// folder that does not exist yet still has to cancel the folder before it.
fn without_dots(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// A path made comparable: the real path when the place exists (symbolic links and `..` gone),
/// else the nearest existing parent with the rest after it; without the `\\?\` prefix Windows
/// adds, and in lower case on Windows, whose file names do not tell capitals apart.
fn normalized(path: &Path) -> PathBuf {
    let path = without_dots(path);
    let mut rest = Vec::new();
    let mut base = path.clone();
    let real = loop {
        match fs::canonicalize(&base) {
            Ok(real) => break real,
            Err(_) => match (base.file_name().map(ToOwned::to_owned), base.parent()) {
                (Some(name), Some(parent)) => {
                    rest.push(name);
                    base = parent.to_path_buf();
                }
                _ => break path.clone(),
            },
        }
    };
    let mut full = real;
    full.extend(rest.iter().rev());
    if cfg!(windows) {
        let text = full.to_string_lossy();
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
        PathBuf::from(text.to_lowercase())
    } else {
        full
    }
}

/// Whether `inner` is `outer` or inside it.
fn within(inner: &Path, outer: &Path) -> bool {
    normalized(inner).starts_with(normalized(outer))
}

/// The files under `dir` that are data, with their sizes and their path relative to `dir`: not the
/// marker, not the leftovers of a write, not links.
fn data_files(dir: &Path) -> Vec<(PathBuf, u64)> {
    fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<(PathBuf, u64)>) {
        if depth > MAX_DEPTH {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == locations::MARKER
                || name.contains(".tmp-")
                || name.starts_with(".wyck-probe")
            {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                walk(root, &path, depth + 1, out);
            } else if kind.is_file() {
                let size = entry.metadata().map_or(0, |m| m.len());
                if let Ok(relative) = path.strip_prefix(root) {
                    out.push((relative.to_path_buf(), size));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, 0, &mut out);
    out.sort();
    out
}

fn refuse_sensitive(paths: &AppPaths, to: &Path) -> std::result::Result<(), PlanError> {
    let secrets = paths.secrets_dir();
    let holds = |protected: &Path| within(protected, to);
    if holds(paths.config_dir()) || holds(paths.data_dir()) || within(to, &secrets) {
        return Err(PlanError::Sensitive(to.to_path_buf()));
    }
    Ok(())
}

/// What the moves in `changes` would do, checked against the folders as they are now. A change that
/// leaves a kind where it already is is left out.
///
/// # Errors
///
/// The [`PlanError`] of the first change that cannot be made.
pub fn plan(paths: &AppPaths, changes: &[Change]) -> std::result::Result<Vec<Step>, PlanError> {
    let mut steps = Vec::new();
    // Where every kind will be once the changes are made, to see that no two are mixed.
    let mut after: Vec<(Kind, PathBuf)> = Kind::ALL
        .into_iter()
        .map(|kind| (kind, paths.dir(kind)))
        .collect();

    for change in changes {
        let standard = paths.standard_dir(change.kind);
        let (to, to_standard) = match &change.to {
            Some(dir) if normalized(dir) != normalized(standard) => (dir.clone(), false),
            _ => (standard.to_path_buf(), true),
        };
        let from = paths.dir(change.kind);
        if normalized(&from) == normalized(&to) {
            continue;
        }
        if to.to_str().is_none() {
            return Err(PlanError::NotText(to));
        }
        let has_data = if to_standard {
            locations::holds_files(&to)
        } else {
            refuse_sensitive(paths, &to)?;
            locations::check(&to, change.kind, paths.mode(), Use::Chosen)?.has_data
        };
        if let Some(slot) = after.iter_mut().find(|(kind, _)| *kind == change.kind) {
            slot.1 = to.clone();
        }

        let carried = data_files(&from);
        let action = if !change.kind.carries_files() || carried.is_empty() {
            Action::Switch
        } else if has_data {
            Action::Adopt {
                files: data_files(&to).len(),
            }
        } else {
            Action::Copy {
                files: carried.len(),
                bytes: carried.iter().map(|(_, size)| size).sum(),
            }
        };
        steps.push(Step {
            kind: change.kind,
            from,
            to,
            to_standard,
            action,
        });
    }

    for (index, (a, dir_a)) in after.iter().enumerate() {
        for (b, dir_b) in &after[index + 1..] {
            if within(dir_a, dir_b) || within(dir_b, dir_a) {
                return Err(PlanError::Mixed { a: *a, b: *b });
            }
        }
    }
    Ok(steps)
}

#[derive(Debug, Serialize, Deserialize)]
struct StagedChange {
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    to: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Staged {
    #[serde(default)]
    schema_version: u32,
    changes: Vec<StagedChange>,
}

/// Checks the moves in `changes` and puts them aside to be made at the next start. Returns what
/// they will do. Nothing is written when there is nothing to do. A move that waited is replaced:
/// the last thing asked for wins.
///
/// # Errors
///
/// As [`plan`], and [`ConfigError::Serialize`] and [`ConfigError::Write`].
pub fn stage(paths: &AppPaths, changes: &[Change]) -> std::result::Result<Vec<Step>, PlanError> {
    let steps = plan(paths, changes)?;
    if steps.is_empty() {
        return Ok(steps);
    }
    let staged = Staged {
        schema_version: 1,
        changes: steps
            .iter()
            .map(|step| StagedChange {
                kind: step.kind.code().to_owned(),
                to: (!step.to_standard).then(|| step.to.clone()),
            })
            .collect(),
    };
    let text = toml::to_string_pretty(&staged).map_err(ConfigError::Serialize)?;
    atomic_write(&paths.config_dir().join(PENDING), text.as_bytes())?;
    info!(
        count = steps.len(),
        "staged a move of saved data for the next start"
    );
    Ok(steps)
}

/// What waits for the next start, if anything does.
#[must_use]
pub fn pending(paths: &AppPaths) -> Option<Vec<Change>> {
    let text = read_text(&paths.config_dir().join(PENDING)).ok()?;
    parse_staged(&text)
}

fn parse_staged(text: &str) -> Option<Vec<Change>> {
    let staged: Staged = toml::from_str(text).ok()?;
    staged
        .changes
        .into_iter()
        .map(|change| {
            Some(Change {
                kind: Kind::from_code(&change.kind)?,
                to: change.to,
            })
        })
        .collect()
}

/// Forgets a move that waits. Nothing waiting is fine.
///
/// # Errors
///
/// [`ConfigError::Write`] when the file exists and cannot go.
pub fn cancel(paths: &AppPaths) -> Result<()> {
    remove_file(&paths.config_dir().join(PENDING))
}

fn remove_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
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
    /// The kinds whose files were copied to the new folder.
    pub copied: Vec<Kind>,
    /// The kinds that now use data that was already in the new folder.
    pub adopted: Vec<Kind>,
    /// The kinds moved with nothing to carry.
    pub switched: Vec<Kind>,
    /// The kinds that could not be moved, with why. Each is where it was.
    pub failed: Vec<(Kind, String)>,
    /// Why the whole move was set aside, when it was.
    pub refused: Option<String>,
}

impl Applied {
    /// Whether nothing happened.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether the folder of `kind` is not the same as before.
    #[must_use]
    pub fn moved(&self, kind: Kind) -> bool {
        [&self.copied, &self.adopted, &self.switched]
            .iter()
            .any(|kinds| kinds.contains(&kind))
    }
}

/// Makes the moves that wait, if any: call it once at the start of the app, before the folders are
/// read, then read the paths again (see [`AppPaths::discover`]). `stamp` names the folders that
/// are kept aside.
///
/// Each kind is moved on its own and the choice is saved only when its files are in place and
/// checked, so a failure leaves that kind where it was and the others done. The file that waits is
/// removed at the end whatever happened: a move that failed is not tried again at every start.
pub fn apply_pending(paths: &AppPaths, stamp: &str) -> Applied {
    let mut applied = Applied::default();
    let file = paths.config_dir().join(PENDING);
    let text = match read_text(&file) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return applied,
        Err(error) => {
            warn!(%error, "could not read the move that waits");
            applied.refused = Some(format!("the file of the move could not be read: {error}"));
            return applied;
        }
    };
    let Some(changes) = parse_staged(&text) else {
        warn!("the move that waits cannot be read, setting it aside");
        let _ = fs::rename(&file, paths.config_dir().join(format!("{PENDING}.bad")));
        applied.refused = Some("the file of the move could not be understood".to_owned());
        return applied;
    };
    if let Some(error) = paths.pointer_error() {
        // Saving the choices would write over a file the user may want to repair.
        warn!(
            error,
            "the file of chosen folders is damaged, the move is dropped"
        );
        applied.refused = Some(format!(
            "the file of chosen folders cannot be read: {error}"
        ));
        let _ = remove_file(&file);
        return applied;
    }
    let steps = match plan(paths, &changes) {
        Ok(steps) => steps,
        Err(error) => {
            warn!(%error, "the move that waits cannot be made");
            applied.refused = Some(error.to_string());
            let _ = remove_file(&file);
            return applied;
        }
    };

    let mut locations = paths.locations().clone();
    for step in steps {
        match carry_out(paths, &step) {
            Ok(()) => {
                locations.set(step.kind, (!step.to_standard).then(|| step.to.clone()));
                if let Err(error) = locations.save(paths.config_dir()) {
                    warn!(%error, kind = step.kind.code(), "the new folder is ready but the choice could not be saved");
                    applied.failed.push((step.kind, error.to_string()));
                    continue;
                }
                info!(kind = step.kind.code(), from = %step.from.display(), to = %step.to.display(), action = ?step.action, "moved saved data");
                match step.action {
                    Action::Copy { .. } => applied.copied.push(step.kind),
                    Action::Adopt { .. } => applied.adopted.push(step.kind),
                    Action::Switch => applied.switched.push(step.kind),
                }
                if matches!(step.action, Action::Copy { .. } | Action::Adopt { .. })
                    && let Some(kept) = set_aside(&step.from, stamp)
                {
                    locations.leftovers.push(kept);
                    let _ = locations.save(paths.config_dir());
                }
            }
            Err(error) => {
                warn!(%error, kind = step.kind.code(), "could not move saved data, it stays where it was");
                applied.failed.push((step.kind, error.to_string()));
            }
        }
    }
    let _ = remove_file(&file);
    applied
}

/// Puts the files of a step in the new folder and checks them. The choice is not saved here.
fn carry_out(paths: &AppPaths, step: &Step) -> Result<()> {
    let write_error = |source| ConfigError::Write {
        path: step.to.clone(),
        source,
    };
    fs::create_dir_all(&step.to).map_err(write_error)?;
    if matches!(step.action, Action::Copy { .. }) {
        let mut made = Vec::new();
        let copied = copy_tree(&step.from, &step.to, &mut made)
            .map_err(write_error)
            .and_then(|()| verify(&step.from, &step.to));
        if let Err(error) = copied {
            // What was made here is ours (the folder was empty): take it back.
            for file in made.iter().rev() {
                let _ = fs::remove_file(file);
            }
            return Err(error);
        }
    }
    if !step.to_standard {
        locations::write_marker(&step.to, step.kind, paths.mode())?;
    }
    Ok(())
}

/// Copies the data files of `from` into `to`, each through a temporary name. `made` gets every
/// file written.
fn copy_tree(from: &Path, to: &Path, made: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for (relative, _) in data_files(from) {
        let (source, target) = (from.join(&relative), to.join(&relative));
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let temporary = target.with_file_name(format!(".{name}.tmp-copy"));
        let result = retry_when_busy(|| fs::copy(&source, &temporary))
            .and_then(|_| retry_when_busy(|| fs::rename(&temporary, &target)));
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        made.push(target);
    }
    Ok(())
}

/// Reads the copy back: the same files, of the same size, and the same bytes up to
/// [`COMPARED_BYTES`].
fn verify(from: &Path, to: &Path) -> Result<()> {
    let mismatch = |reason| ConfigError::Relocate {
        path: to.to_path_buf(),
        reason,
    };
    for (relative, size) in data_files(from) {
        let copy = to.join(&relative);
        let copied =
            fs::metadata(&copy).map_err(|_| mismatch("a file is missing from the copy"))?;
        if copied.len() != size {
            return Err(mismatch("a file in the copy has another size"));
        }
        if size <= COMPARED_BYTES {
            let (a, b) = (fs::read(from.join(&relative)), fs::read(&copy));
            match (a, b) {
                (Ok(a), Ok(b)) if a == b => {}
                _ => return Err(mismatch("a file in the copy differs from the original")),
            }
        }
    }
    Ok(())
}

/// Renames `dir` to `<name>.moved-<stamp>` beside itself and returns the new name; `None` when
/// there is no such folder or it cannot be renamed (it then stays, under its own name).
fn set_aside(dir: &Path, stamp: &str) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    let name = dir.file_name()?.to_string_lossy().into_owned();
    let stamp: String = stamp
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let mut target = dir.with_file_name(format!("{name}.moved-{stamp}"));
    let mut number = 2;
    while target.exists() {
        target = dir.with_file_name(format!("{name}.moved-{stamp}-{number}"));
        number += 1;
    }
    match retry_when_busy(|| fs::rename(dir, &target)) {
        Ok(()) => Some(target),
        Err(error) => {
            warn!(%error, dir = %dir.display(), "could not set the old folder aside, it stays where it is");
            None
        }
    }
}

/// An old folder a move left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leftover {
    pub path: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

/// The folders earlier moves left behind that are still there, in the order they were made.
#[must_use]
pub fn leftovers(paths: &AppPaths) -> Vec<Leftover> {
    let listed = Locations::load(paths.config_dir())
        .unwrap_or_default()
        .leftovers;
    listed
        .into_iter()
        .filter(|path| path.is_dir())
        .map(|path| {
            let files = data_files(&path);
            Leftover {
                bytes: files.iter().map(|(_, size)| size).sum(),
                files: files.len(),
                path,
            }
        })
        .collect()
}

/// Deletes a folder a move left behind. Only a folder in the list of [`leftovers`] is deleted, and
/// never one that holds a folder in use.
///
/// # Errors
///
/// [`ConfigError::Relocate`] for any other folder, [`ConfigError::Write`] when it cannot be
/// deleted, and the errors of reading and saving the list.
pub fn delete_leftover(paths: &AppPaths, path: &Path) -> Result<()> {
    let mut locations = Locations::load(paths.config_dir())?;
    let Some(at) = locations.leftovers.iter().position(|kept| kept == path) else {
        return Err(ConfigError::Relocate {
            path: path.to_path_buf(),
            reason: "it is not a folder that a move left behind",
        });
    };
    if Kind::ALL
        .into_iter()
        .any(|kind| within(&paths.dir(kind), path))
    {
        return Err(ConfigError::Relocate {
            path: path.to_path_buf(),
            reason: "a folder in use is inside it",
        });
    }
    match fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(ConfigError::Write {
                path: path.to_path_buf(),
                source,
            });
        }
    }
    locations.leftovers.remove(at);
    locations.save(paths.config_dir())?;
    info!(dir = %path.display(), "deleted a folder an earlier move left behind");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::storage::DocumentStore;
    use wyck_platform::build_info::BuildMode;

    const MODE: BuildMode = BuildMode::Production;

    struct Setup {
        _temp: tempfile::TempDir,
        base: PathBuf,
        cloud: PathBuf,
    }

    fn setup() -> Setup {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("config");
        let cloud = temp.path().join("cloud");
        fs::create_dir_all(&base).unwrap();
        fs::create_dir_all(&cloud).unwrap();
        Setup {
            _temp: temp,
            base,
            cloud,
        }
    }

    /// The paths as the next start finds them: the standard ones with the saved choices.
    fn paths(setup: &Setup) -> AppPaths {
        AppPaths::at_for(MODE, &setup.base).with_locations(Locations::load(&setup.base).unwrap())
    }

    fn move_to(kind: Kind, dir: &Path) -> Change {
        Change {
            kind,
            to: Some(dir.to_path_buf()),
        }
    }

    fn save(store: &DocumentStore, name: &str, text: &str) {
        store.save_text(name, text).unwrap();
    }

    #[test]
    fn the_files_are_copied_checked_and_the_old_folder_is_kept_aside() {
        let setup = setup();
        let before = paths(&setup);
        save(&before.documents(), "preferences", "magnet = true\n");
        save(&before.scope("demo-1"), "alerts", "next_id = 4\n");
        let target = setup.cloud.join("settings");

        let steps = stage(&before, &[move_to(Kind::Settings, &target)]).unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(
            steps[0].action,
            Action::Copy {
                files: 1,
                bytes: 14
            }
        );
        assert!(pending(&before).is_some());

        let applied = apply_pending(&before, "2026-10-12");
        assert_eq!(applied.copied, [Kind::Settings]);
        assert!(applied.failed.is_empty() && applied.refused.is_none());
        assert!(pending(&before).is_none(), "applied once");

        let after = paths(&setup);
        assert_eq!(after.dir(Kind::Settings), target);
        assert!(after.is_custom(Kind::Settings) && !after.is_custom(Kind::Accounts));
        assert_eq!(
            after
                .documents()
                .load_text("preferences")
                .unwrap()
                .as_deref(),
            Some("magnet = true\n")
        );
        assert!(target.join(locations::MARKER).is_file(), "marked as Wyck's");
        assert_eq!(
            after
                .scope("demo-1")
                .load_text("alerts")
                .unwrap()
                .as_deref(),
            Some("next_id = 4\n"),
            "the kinds not moved are where they were"
        );

        let left = leftovers(&after);
        assert_eq!(left.len(), 1);
        assert!(
            left[0].path.ends_with("state.moved-2026-10-12"),
            "{:?}",
            left[0].path
        );
        assert_eq!(left[0].files, 1);
        assert!(
            !before.standard_dir(Kind::Settings).exists(),
            "the old name is free"
        );
    }

    #[test]
    fn a_second_computer_uses_what_the_first_one_put_in_the_cloud() {
        let first = setup();
        let target = first.cloud.join("settings");
        let one = paths(&first);
        save(&one.documents(), "preferences", "magnet = true\n");
        stage(&one, &[move_to(Kind::Settings, &target)]).unwrap();
        apply_pending(&one, "a");

        // Another computer: its own settings, and the same cloud folder.
        let second = setup();
        let two = paths(&second);
        save(&two.documents(), "preferences", "magnet = false\n");
        save(&two.documents(), "extra", "x = 1\n");
        let steps = stage(&two, &[move_to(Kind::Settings, &target)]).unwrap();
        assert_eq!(steps[0].action, Action::Adopt { files: 1 });
        let applied = apply_pending(&two, "b");
        assert_eq!(applied.adopted, [Kind::Settings]);

        let after = paths(&second);
        assert_eq!(
            after
                .documents()
                .load_text("preferences")
                .unwrap()
                .as_deref(),
            Some("magnet = true\n"),
            "the data in the cloud wins"
        );
        assert_eq!(after.documents().load_text("extra").unwrap(), None);
        assert_eq!(
            fs::read_to_string(target.join("preferences.toml")).unwrap(),
            "magnet = true\n",
            "nothing of this computer was copied over it"
        );
        let kept = leftovers(&after);
        assert_eq!(kept.len(), 1);
        assert_eq!(
            fs::read_to_string(kept[0].path.join("preferences.toml")).unwrap(),
            "magnet = false\n",
            "and what this computer had is kept"
        );
    }

    #[test]
    fn nothing_to_carry_is_a_switch_and_the_logs_are_never_carried() {
        let setup = setup();
        let before = paths(&setup);
        let logs = setup.cloud.join("logs");
        fs::create_dir_all(before.logs_dir()).unwrap();
        fs::write(before.logs_dir().join("wyck.2026-10-12.log"), "old line").unwrap();

        let steps = stage(
            &before,
            &[
                move_to(Kind::Indicators, &setup.cloud.join("indicators")),
                move_to(Kind::Logs, &logs),
            ],
        )
        .unwrap();
        assert!(
            steps.iter().all(|step| step.action == Action::Switch),
            "{steps:?}"
        );
        let applied = apply_pending(&before, "s");
        assert_eq!(applied.switched.len(), 2);
        assert!(leftovers(&paths(&setup)).is_empty());
        assert!(
            before.logs_dir().join("wyck.2026-10-12.log").is_file(),
            "the old log stays where it was written"
        );
        assert_eq!(paths(&setup).logs_dir(), logs);
    }

    #[test]
    fn going_back_to_the_standard_folder_brings_the_files_home() {
        let setup = setup();
        let target = setup.cloud.join("settings");
        let start = paths(&setup);
        save(&start.documents(), "preferences", "magnet = true\n");
        stage(&start, &[move_to(Kind::Settings, &target)]).unwrap();
        apply_pending(&start, "one");

        let away = paths(&setup);
        save(&away.documents(), "later", "n = 2\n");
        let steps = stage(
            &away,
            &[Change {
                kind: Kind::Settings,
                to: None,
            }],
        )
        .unwrap();
        assert!(steps[0].to_standard);
        let applied = apply_pending(&away, "two");
        assert_eq!(applied.copied, [Kind::Settings]);

        let home = paths(&setup);
        assert!(!home.is_custom(Kind::Settings));
        assert!(
            home.locations().get(Kind::Settings).is_none(),
            "the choice is forgotten"
        );
        assert_eq!(
            home.documents().load_text("later").unwrap().as_deref(),
            Some("n = 2\n")
        );
        assert_eq!(
            home.documents()
                .load_text("preferences")
                .unwrap()
                .as_deref(),
            Some("magnet = true\n")
        );
    }

    #[test]
    fn what_is_not_ours_is_never_mixed_with_and_the_other_build_is_refused() {
        let setup = setup();
        let current = paths(&setup);
        let foreign = setup.cloud.join("holidays");
        fs::create_dir_all(&foreign).unwrap();
        fs::write(foreign.join("photo.jpg"), "x").unwrap();
        assert!(matches!(
            plan(&current, &[move_to(Kind::Settings, &foreign)]),
            Err(PlanError::Folder(Problem {
                reason: locations::Reason::NotEmpty,
                ..
            }))
        ));

        let installed = setup.cloud.join("installed");
        fs::create_dir_all(&installed).unwrap();
        locations::write_marker(&installed, Kind::Settings, BuildMode::Production).unwrap();
        let development = AppPaths::at_for(BuildMode::Development, &setup.base);
        let error = plan(&development, &[move_to(Kind::Settings, &installed)]).unwrap_err();
        assert!(
            error.to_string().contains("belongs to the installed app"),
            "{error}"
        );
        assert!(plan(&current, &[move_to(Kind::Settings, &installed)]).is_ok());
        assert!(
            matches!(
                plan(&current, &[move_to(Kind::Accounts, &installed)]),
                Err(PlanError::Folder(Problem {
                    reason: locations::Reason::OtherKind { .. },
                    ..
                }))
            ),
            "a folder of one kind is not another's"
        );
    }

    #[test]
    fn the_credentials_and_the_accounts_list_never_go_into_a_folder_you_move() {
        let setup = setup();
        let current = paths(&setup);
        for bad in [
            setup.base.clone(),
            setup.base.parent().unwrap().to_path_buf(),
            current.secrets_dir().join("inner"),
        ] {
            fs::create_dir_all(&bad).unwrap();
            let result = plan(&current, &[move_to(Kind::Backups, &bad)]);
            assert!(
                matches!(
                    result,
                    Err(PlanError::Sensitive(_)) | Err(PlanError::Folder(_))
                ),
                "{}: {result:?}",
                bad.display()
            );
        }
        let beside = setup.cloud.join("fine");
        assert!(plan(&current, &[move_to(Kind::Backups, &beside)]).is_ok());
    }

    #[test]
    fn two_kinds_cannot_share_a_folder_or_hold_each_other() {
        let setup = setup();
        let current = paths(&setup);
        let shared = setup.cloud.join("everything");
        let both = [
            move_to(Kind::Settings, &shared),
            move_to(Kind::Accounts, &shared),
        ];
        assert!(matches!(
            plan(&current, &both),
            Err(PlanError::Mixed {
                a: Kind::Settings,
                b: Kind::Accounts
            })
        ));
        fs::create_dir_all(&shared).unwrap();
        let inside = [
            move_to(Kind::Settings, &shared),
            move_to(Kind::Accounts, &shared.join("accounts")),
        ];
        assert!(matches!(
            plan(&current, &inside),
            Err(PlanError::Mixed { .. })
        ));

        let apart = [
            move_to(Kind::Settings, &setup.cloud.join("a")),
            move_to(Kind::Accounts, &setup.cloud.join("b")),
        ];
        assert_eq!(plan(&current, &apart).unwrap().len(), 2);
    }

    #[test]
    fn a_kind_already_there_is_left_out_and_nothing_is_staged_for_it() {
        let setup = setup();
        let current = paths(&setup);
        let standard = current.standard_dir(Kind::Settings).to_path_buf();
        assert!(
            plan(&current, &[move_to(Kind::Settings, &standard)])
                .unwrap()
                .is_empty()
        );
        let nothing = stage(
            &current,
            &[Change {
                kind: Kind::Backups,
                to: None,
            }],
        )
        .unwrap();
        assert!(nothing.is_empty());
        assert!(pending(&current).is_none(), "no file for nothing to do");
    }

    #[test]
    fn a_move_that_cannot_be_read_is_set_aside_and_one_that_cannot_be_made_is_dropped() {
        let setup = setup();
        let current = paths(&setup);
        fs::write(setup.base.join(PENDING), "changes = [[[").unwrap();
        let applied = apply_pending(&current, "x");
        assert!(applied.refused.is_some());
        assert!(setup.base.join(format!("{PENDING}.bad")).is_file());
        assert!(!setup.base.join(PENDING).exists());

        // The folder was made by the other build between the request and the start.
        let installed = setup.cloud.join("other");
        fs::create_dir_all(&installed).unwrap();
        locations::write_marker(&installed, Kind::Settings, BuildMode::Development).unwrap();
        let staged = Staged {
            schema_version: 1,
            changes: vec![StagedChange {
                kind: "settings".into(),
                to: Some(installed.clone()),
            }],
        };
        fs::write(setup.base.join(PENDING), toml::to_string(&staged).unwrap()).unwrap();
        let applied = apply_pending(&current, "y");
        assert!(
            applied
                .refused
                .as_deref()
                .unwrap()
                .contains("belongs to a development build")
        );
        assert!(
            Locations::load(&setup.base).unwrap().settings.is_none(),
            "nothing switched"
        );
        assert!(!setup.base.join(PENDING).exists());
        assert!(
            apply_pending(&current, "z").is_empty(),
            "nothing waits any more"
        );
    }

    #[test]
    fn a_copy_skips_what_is_not_data_and_is_read_back() {
        let setup = setup();
        let from = setup.base.join("state");
        fs::create_dir_all(from.join("sounds")).unwrap();
        fs::write(from.join("a.toml"), "a = 1").unwrap();
        fs::write(from.join("sounds").join("bell.wav"), vec![7_u8; 3000]).unwrap();
        fs::write(from.join(".a.toml.tmp-9"), "half a write").unwrap();
        fs::write(from.join(locations::MARKER), "x").unwrap();
        let to = setup.cloud.join("copy");
        fs::create_dir_all(&to).unwrap();

        let mut made = Vec::new();
        copy_tree(&from, &to, &mut made).unwrap();
        verify(&from, &to).unwrap();

        assert_eq!(made.len(), 2);
        assert!(to.join("sounds").join("bell.wav").is_file());
        assert!(!to.join(".a.toml.tmp-9").exists() && !to.join(locations::MARKER).exists());

        fs::write(to.join("a.toml"), "a = 2").unwrap();
        assert!(
            matches!(verify(&from, &to), Err(ConfigError::Relocate { .. })),
            "same size, other bytes"
        );
        fs::write(to.join("a.toml"), "longer").unwrap();
        assert!(
            matches!(verify(&from, &to), Err(ConfigError::Relocate { .. })),
            "other size"
        );
        fs::remove_file(to.join("a.toml")).unwrap();
        assert!(
            matches!(verify(&from, &to), Err(ConfigError::Relocate { .. })),
            "missing"
        );
    }

    #[test]
    fn only_a_folder_a_move_left_behind_can_be_deleted_and_never_one_in_use() {
        let setup = setup();
        let start = paths(&setup);
        save(&start.documents(), "preferences", "magnet = true\n");
        stage(
            &start,
            &[move_to(Kind::Settings, &setup.cloud.join("settings"))],
        )
        .unwrap();
        apply_pending(&start, "d");
        let now = paths(&setup);
        let left = leftovers(&now).remove(0).path;

        let stranger = setup.cloud.join("keep-me");
        fs::create_dir_all(&stranger).unwrap();
        assert!(matches!(
            delete_leftover(&now, &stranger),
            Err(ConfigError::Relocate { .. })
        ));
        assert!(stranger.is_dir());

        delete_leftover(&now, &left).unwrap();
        assert!(!left.exists());
        assert!(leftovers(&paths(&setup)).is_empty());
        assert!(Locations::load(&setup.base).unwrap().leftovers.is_empty());
        assert!(
            paths(&setup)
                .dir(Kind::Settings)
                .join("preferences.toml")
                .is_file(),
            "the data in use is untouched"
        );
    }

    #[test]
    fn a_reset_empties_a_moved_folder_but_keeps_the_folder_and_its_marker() {
        use crate::infra::storage::backup::staged;
        use crate::infra::storage::backup::store::BackupKind;

        let setup = setup();
        let start = paths(&setup);
        save(&start.documents(), "preferences", "magnet = true\n");
        let target = setup.cloud.join("settings");
        stage(&start, &[move_to(Kind::Settings, &target)]).unwrap();
        apply_pending(&start, "m");

        let now = paths(&setup);
        staged::stage_reset(&now).unwrap();
        let applied = staged::apply_pending(&now, "2026-10-12").unwrap();

        assert!(applied.reset);
        assert!(target.is_dir(), "the folder a service syncs is not removed");
        assert!(
            target.join(locations::MARKER).is_file(),
            "and is still marked as Wyck's"
        );
        assert!(!target.join("preferences.toml").exists());
        let kinds: Vec<_> = now
            .backups()
            .list()
            .unwrap()
            .iter()
            .map(|e| e.kind)
            .collect();
        assert!(kinds.contains(&BackupKind::BeforeReset), "{kinds:?}");
    }

    #[test]
    fn a_path_is_compared_by_the_place_it_names() {
        let setup = setup();
        let real = setup.cloud.join("real");
        fs::create_dir_all(&real).unwrap();
        assert!(
            within(&real.join("a").join("b"), &real),
            "even where nothing exists yet"
        );
        assert!(within(&real, &real));
        assert!(!within(&real, &real.join("a")));
        assert!(
            within(&real.join("x").join("..").join("y"), &real),
            "a `..` cancels a folder that is not there yet"
        );
        assert!(
            !within(&real.join("..").join("elsewhere"), &real),
            "and one that leads out of the folder is out"
        );
        if cfg!(windows) {
            let upper = PathBuf::from(real.to_string_lossy().to_uppercase());
            assert!(
                within(&upper, &real),
                "Windows does not tell capitals apart"
            );
        }
    }
}
