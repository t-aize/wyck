//! Where the app keeps what it saves, as the start of the app and the settings see it.
//!
//! The mechanics are in [`wyck_storage::locations`] (which kinds of data can be kept
//! elsewhere, and the rules for a folder) and [`wyck_storage::relocate`] (moving the
//! files). This module is what the app does with them:
//!
//! * [`app_paths`] is asked first thing at start, and its first call makes the move the user asked
//!   for and brings an old setting along (the indicators folder used to be a field of a shared
//!   document, which is wrong once that document is synced to another computer: a path means
//!   something on one computer only).
//! * [`overview`] and the `stage_*` functions are what the Folders page of the settings shows and
//!   asks for. A move waits for the next start, as an import does.
//! * [`startup_notes`] says what the start found or did, for the notices at the first frame.
//!
//! Dev and production: the standard folders differ, so do the pointers, a folder carries a marker of
//! the build that made it, and the cloud folder is `Wyck (dev)` for a development build (see
//! [`wyck_storage::locations`]).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use wyck_core::math::u64_to_f64;

use gpui::App;
use tracing::{info, warn};

use crate::prefs::indicators as indicator_prefs;
use crate::{alerts, appearance, chart_export, drawings, workspace};
use wyck_platform::build_info::BuildMode;
use wyck_storage::locations::{self, CloudFolder, Kind, Locations, Problem, Use};
use wyck_storage::relocate::{self, Action, Applied, Change, Leftover, PlanError, Step};
use wyck_storage::{AppPaths, ConfigError, DocumentStore};

/// The documents the app knows by name, to tell the copies a sync service makes of them.
const KNOWN_DOCUMENTS: [&str; 7] = [
    workspace::PREFERENCES,
    workspace::WATCHLISTS,
    alerts::DOCUMENT,
    drawings::DOCUMENT,
    appearance::DOCUMENT,
    indicator_prefs::DOCUMENT,
    chart_export::DOCUMENT,
];

/// What the start did, kept for the notices.
#[derive(Debug, Default)]
struct Startup {
    applied: Applied,
    /// The indicators folder that was taken out of the shared settings.
    legacy_indicators: Option<PathBuf>,
}

static STARTUP: OnceLock<Startup> = OnceLock::new();
static PATHS: OnceLock<Option<AppPaths>> = OnceLock::new();

/// The folders of the app, resolved once: `None` when the system gives it none. Everything that
/// needs a path of the app asks here, so there is one answer for the whole run.
///
/// The first call also makes the move of folders the user asked for (it has to be done before
/// anything reads a document), which is why the first call is the first thing `run` does.
pub fn app_paths() -> Option<&'static AppPaths> {
    PATHS.get_or_init(prepare).as_ref()
}

/// Finds the folders of the app: makes the move that waits, if one does, and brings the old
/// indicators folder setting along.
fn prepare() -> Option<AppPaths> {
    let mut paths = AppPaths::discover().ok()?;
    let mut startup = Startup::default();
    if paths.config_dir().join(relocate::PENDING).exists() {
        let stamp = chrono::DateTime::<chrono::Local>::from(wyck_clock::now())
            .format("%Y-%m-%d-%H%M%S")
            .to_string();
        startup.applied = relocate::apply_pending(&paths, &stamp);
        paths = AppPaths::discover().ok()?;
    }
    if let Some(folder) = take_legacy_indicators_folder(&paths) {
        startup.legacy_indicators = Some(folder);
        paths = AppPaths::discover().ok()?;
    }
    // `prepare` runs once, inside the `get_or_init` of `PATHS`: nothing is set before it.
    STARTUP.get_or_init(|| startup);
    Some(paths)
}

/// Writes to the log what the first call of [`app_paths`] did. It runs before the log is started
/// (the log folder is one of the folders that can move), so this is called once the log is.
pub fn log_startup() {
    let Some(startup) = STARTUP.get() else {
        return;
    };
    let applied = &startup.applied;
    if !applied.is_empty() {
        info!(
            copied = ?applied.copied,
            adopted = ?applied.adopted,
            switched = ?applied.switched,
            "moved the folders of saved data at this start"
        );
    }
    for (kind, reason) in &applied.failed {
        warn!(
            kind = kind.code(),
            reason, "a folder could not be moved at this start"
        );
    }
    if let Some(reason) = &applied.refused {
        warn!(reason, "the move of folders was dropped at this start");
    }
    if let Some(folder) = &startup.legacy_indicators {
        info!(folder = %folder.display(), "the indicators folder is now kept apart from the shared settings");
    }
    if let Some(paths) = app_paths() {
        for problem in paths.problems() {
            warn!(%problem, kind = problem.kind.code(), "a chosen folder cannot be used, the standard one is used for now");
        }
        if let Some(error) = paths.pointer_error() {
            warn!(
                error,
                "the file of chosen folders could not be read, the standard folders are used"
            );
        }
    }
}

/// The indicators folder an older version kept in the `indicators` document, moved to the choices
/// of this computer; the document no longer carries it. `None` when there was none.
///
/// A folder that is not a full path means nothing and is dropped. A choice already made on this
/// computer wins over the old field.
fn take_legacy_indicators_folder(paths: &AppPaths) -> Option<PathBuf> {
    if paths.pointer_error().is_some() {
        return None; // writing the choices would write over a file to repair
    }
    let store = paths.documents();
    let text = store.load_text(indicator_prefs::DOCUMENT).ok()??;
    let mut table: toml::Table = toml::from_str(&text).ok()?;
    let toml::Value::String(folder) = table.remove("folder")? else {
        return None;
    };
    let path = PathBuf::from(folder.trim());
    let usable = path.is_absolute();
    let mut choices = paths.locations().clone();
    let moved = usable && choices.indicators.is_none();
    if moved {
        choices.indicators = Some(path.clone());
        if let Err(error) = choices.save(paths.config_dir()) {
            warn!(%error, "could not move the indicators folder to the choices of this computer");
            return None;
        }
    } else if !usable {
        warn!(folder = %path.display(), "dropped an indicators folder that is not a full path");
    }
    match toml::to_string_pretty(&table) {
        Ok(rest) => {
            if let Err(error) = store.save_text(indicator_prefs::DOCUMENT, &rest) {
                warn!(%error, "could not take the old indicators folder out of the settings");
            }
        }
        Err(error) => warn!(%error, "could not write the indicator settings again"),
    }
    moved.then_some(path)
}

/// The paths as the disk says now: the folders of this run with the choices saved since (a folder
/// picked in the Indicators settings takes effect at once, and the page must show it).
fn fresh() -> Option<AppPaths> {
    let paths = app_paths()?;
    let locations = Locations::load(paths.config_dir()).ok()?;
    Some(paths.clone().with_locations(locations))
}

/// A notice for the first frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Whether something needs a look (a failure), or it is only news.
    pub warning: bool,
    pub title: String,
    pub text: String,
}

fn note(warning: bool, title: &str, text: impl Into<String>) -> Note {
    Note {
        warning,
        title: title.to_owned(),
        text: text.into(),
    }
}

/// What the start found or did about the folders, for the user to read.
#[must_use]
pub fn startup_notes() -> Vec<Note> {
    app_paths()
        .map(|paths| notes_for(paths, STARTUP.get()))
        .unwrap_or_default()
}

fn notes_for(paths: &AppPaths, startup: Option<&Startup>) -> Vec<Note> {
    let mut notes = Vec::new();
    if let Some(startup) = startup {
        let applied = &startup.applied;
        let moved: Vec<String> = Kind::ALL
            .into_iter()
            .filter(|kind| applied.moved(*kind))
            .map(|kind| format!("{}: {}", kind.label(), paths.dir(kind).display()))
            .collect();
        if !moved.is_empty() {
            notes.push(note(false, "Folders moved", moved.join("\n")));
        }
        for (kind, reason) in &applied.failed {
            notes.push(note(
                true,
                "A folder was not moved",
                format!("{}: {reason}. It stays where it was.", kind.label()),
            ));
        }
        if let Some(reason) = &applied.refused {
            notes.push(note(true, "The move was dropped", reason.clone()));
        }
        if let Some(folder) = &startup.legacy_indicators {
            notes.push(note(
                false,
                "Indicators folder",
                format!(
                    "{} is now remembered by this computer alone, not in the settings, so settings kept in a cloud folder never point to a folder of another computer.",
                    folder.display()
                ),
            ));
        }
    }
    if let Some(error) = paths.pointer_error() {
        notes.push(note(
            true,
            "Chosen folders not read",
            format!("Every folder is in its standard place until this is fixed: {error}"),
        ));
    }
    let unavailable: Vec<String> = paths
        .problems()
        .iter()
        .map(|problem| format!("{}: {problem}", problem.kind.label()))
        .collect();
    if !unavailable.is_empty() {
        notes.push(note(
            true,
            "A chosen folder cannot be used",
            format!(
                "{}\nThe standard folder is used until it can be. Changes made meanwhile stay on this computer.",
                unavailable.join("\n")
            ),
        ));
    }
    let copies = conflicts_in(paths).len();
    if copies > 0 {
        notes.push(note(
            true,
            "Copies made by your sync service",
            format!(
                "{copies} file(s) in your settings look like copies made when two computers changed the same file. Wyck reads only the original. Settings > Folders lists them."
            ),
        ));
    }
    notes
}

/// The copies a sync service made of a document, in the settings folder and in each account's.
fn conflicts_in(paths: &AppPaths) -> Vec<PathBuf> {
    let mut found = locations::conflict_copies(&paths.state_dir(), &KNOWN_DOCUMENTS);
    if let Ok(scopes) = DocumentStore::list_scopes(paths) {
        for scope in scopes {
            let store = paths.scope(&scope);
            found.extend(locations::conflict_copies(store.dir(), &KNOWN_DOCUMENTS));
        }
    }
    found
}

/// One kind of data and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: Kind,
    /// Where it is read from and written to now.
    pub path: PathBuf,
    /// Where it is when nothing is chosen.
    pub(crate) standard: PathBuf,
    /// Whether the user put it somewhere else.
    pub custom: bool,
    /// Why the folder that was chosen is not used, when that is so.
    pub unavailable: Option<Problem>,
}

/// A cloud folder found on this computer, and whether the data is already in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudChoice {
    pub folder: CloudFolder,
    /// Every kind the cloud preset covers is kept in this folder now, so using it again would
    /// change nothing.
    pub in_use: bool,
}

/// What the Folders page shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overview {
    pub mode: BuildMode,
    pub rows: Vec<Row>,
    pub pointer_error: Option<String>,
    /// The cloud folders found on this computer.
    pub cloud: Vec<CloudChoice>,
    /// The copies a sync service made of the user's documents.
    pub conflicts: Vec<PathBuf>,
    /// The old folders earlier moves left behind.
    pub leftovers: Vec<Leftover>,
    /// What a move that waits for the next start will do, in words.
    pub waiting: Option<Vec<String>>,
}

/// The folders as they are now. `None` when the system gives the app no folder.
#[must_use]
pub fn overview() -> Option<Overview> {
    let paths = fresh().or_else(|| app_paths().cloned())?;
    Some(overview_of(&paths, locations::cloud_folders()))
}

/// Whether the folder of every kind the cloud preset covers is the one `root` would give it.
fn uses_cloud(paths: &AppPaths, root: &Path) -> bool {
    let same = |a: &Path, b: &Path| {
        let words = |path: &Path| -> Vec<String> {
            path.components()
                .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
                .collect()
        };
        words(a) == words(b)
    };
    let layout = locations::cloud_layout(root, paths.mode());
    !layout.is_empty()
        && layout
            .iter()
            .all(|(kind, dir)| paths.is_custom(*kind) && same(&paths.dir(*kind), dir))
}

fn overview_of(paths: &AppPaths, cloud: Vec<CloudFolder>) -> Overview {
    let cloud = cloud
        .into_iter()
        .map(|folder| CloudChoice {
            in_use: uses_cloud(paths, &folder.path),
            folder,
        })
        .collect();
    let rows = Kind::ALL
        .into_iter()
        .map(|kind| Row {
            kind,
            path: paths.dir(kind),
            standard: paths.standard_dir(kind).to_path_buf(),
            custom: paths.is_custom(kind),
            unavailable: paths
                .problems()
                .iter()
                .find(|problem| problem.kind == kind)
                .cloned(),
        })
        .collect();
    let waiting = relocate::pending(paths).and_then(|changes| {
        relocate::plan(paths, &changes)
            .ok()
            .map(|steps| describe(&steps))
    });
    Overview {
        mode: paths.mode(),
        rows,
        pointer_error: paths.pointer_error().map(str::to_owned),
        cloud,
        conflicts: conflicts_in(paths),
        leftovers: relocate::leftovers(paths),
        waiting,
    }
}

/// Puts a move of one kind aside for the next start: to `picked` (or to a folder named after the
/// kind inside it when it holds other things), or with `None` back to the standard folder.
///
/// # Errors
///
/// Why the move cannot be made.
pub fn stage_folder(kind: Kind, picked: Option<&Path>) -> Result<Vec<Step>, PlanError> {
    let paths = fresh().ok_or(PlanError::Storage(ConfigError::NoProjectDirs))?;
    stage_folder_in(&paths, kind, picked)
}

fn stage_folder_in(
    paths: &AppPaths,
    kind: Kind,
    picked: Option<&Path>,
) -> Result<Vec<Step>, PlanError> {
    let to = picked.map(|dir| locations::target_in(dir, kind, paths.mode()));
    relocate::stage(paths, &[Change { kind, to }])
}

/// Puts the move of everything the cloud folder `root` can hold aside for the next start: a
/// `Wyck` folder (`Wyck (dev)` for a development build) with one folder per kind inside. The logs
/// stay where they are.
///
/// # Errors
///
/// Why the move cannot be made.
pub fn stage_cloud(root: &Path) -> Result<Vec<Step>, PlanError> {
    let paths = fresh().ok_or(PlanError::Storage(ConfigError::NoProjectDirs))?;
    stage_cloud_in(&paths, root)
}

fn stage_cloud_in(paths: &AppPaths, root: &Path) -> Result<Vec<Step>, PlanError> {
    let changes: Vec<Change> = locations::cloud_layout(root, paths.mode())
        .into_iter()
        .map(|(kind, dir)| Change {
            kind,
            to: Some(dir),
        })
        .collect();
    relocate::stage(paths, &changes)
}

/// Forgets the move that waits.
pub fn cancel_staged() {
    if let Some(paths) = app_paths()
        && let Err(error) = relocate::cancel(paths)
    {
        warn!(%error, "could not forget the move that waits");
    }
}

/// Deletes a folder an earlier move left behind.
pub fn delete_leftover(path: &Path) -> Result<(), ConfigError> {
    let paths = fresh().ok_or(ConfigError::NoProjectDirs)?;
    relocate::delete_leftover(&paths, path)
}

/// Why the indicators folder could not be changed.
#[derive(Debug, thiserror::Error)]
pub enum FolderError {
    #[error("{0}")]
    Folder(#[from] Problem),
    #[error(transparent)]
    Storage(#[from] ConfigError),
}

/// Saves the folder the indicators are read from, for this computer: `None` is the standard
/// folder. The folder may be one the user already keeps their scripts in; it is read in place,
/// nothing is copied. Returns the folder to read from now.
///
/// # Errors
///
/// A folder that cannot be used, or the choices that cannot be saved.
pub(crate) fn set_indicators_folder(dir: Option<&Path>) -> Result<PathBuf, FolderError> {
    let paths = fresh().ok_or(ConfigError::NoProjectDirs)?;
    set_indicators_folder_in(&paths, dir)
}

fn set_indicators_folder_in(paths: &AppPaths, dir: Option<&Path>) -> Result<PathBuf, FolderError> {
    if let Some(dir) = dir {
        locations::check(dir, Kind::Indicators, paths.mode(), Use::Own)?;
    }
    let mut choices = paths.locations().clone();
    choices.set(Kind::Indicators, dir.map(Path::to_path_buf));
    choices.save(paths.config_dir())?;
    Ok(dir.map_or_else(
        || paths.standard_dir(Kind::Indicators).to_path_buf(),
        Path::to_path_buf,
    ))
}

/// Shows the folder of `kind` in the file explorer of the system, made first so that it opens even
/// when nothing is in it yet.
pub fn open_folder(kind: Kind, cx: &mut App) {
    let Some(paths) = fresh() else {
        return;
    };
    let dir = paths.dir(kind);
    if let Err(error) = std::fs::create_dir_all(&dir) {
        warn!(%error, dir = %dir.display(), "could not create the folder to show it");
        return;
    }
    cx.reveal_path(&dir);
}

/// Shows `path` in the file explorer of the system.
pub fn reveal(path: &Path, cx: &mut App) {
    cx.reveal_path(path);
}

/// A size in bytes as a few characters.
fn size(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.0} KB", u64_to_f64(bytes) / 1024.0),
        _ => format!("{:.1} MB", u64_to_f64(bytes) / 1_048_576.0),
    }
}

/// What a move will do, one line per kind, in words a person reads.
#[must_use]
pub fn describe(steps: &[Step]) -> Vec<String> {
    steps
        .iter()
        .map(|step| {
            let (label, to) = (step.kind.label(), step.to.display());
            match &step.action {
                Action::Copy { files, bytes } if step.to_standard => format!(
                    "{label}: back to the standard folder {to}. {files} file(s), {}, are copied, and the folder they come from is kept aside until you delete it.",
                    size(*bytes)
                ),
                Action::Copy { files, bytes } => format!(
                    "{label}: {files} file(s), {}, are copied to {to}. The folder they come from is kept aside until you delete it.",
                    size(*bytes)
                ),
                Action::Adopt { files } => format!(
                    "{label}: {to} already holds data of Wyck ({files} file(s)). It is used as it is, and what this computer has now is kept aside until you delete it."
                ),
                Action::Switch if step.to_standard => {
                    format!("{label}: back to the standard folder {to}.")
                }
                Action::Switch => format!("{label}: {to} is used from now on."),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(dir: &Path) -> AppPaths {
        AppPaths::at_for(BuildMode::Production, dir)
    }

    fn temp() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let (base, cloud) = (temp.path().join("config"), temp.path().join("cloud"));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&cloud).unwrap();
        (temp, base, cloud)
    }

    fn reload(base: &Path) -> AppPaths {
        paths(base).with_locations(Locations::load(base).unwrap())
    }

    #[test]
    fn the_old_indicators_folder_leaves_the_shared_document_for_the_choices_of_this_computer() {
        let (_temp, base, _cloud) = temp();
        let current = paths(&base);
        let scripts = base.join("my scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        let text = format!(
            "auto_reload = false\nfolder = {:?}\nfavorites = [\"a\"]\n",
            scripts.to_str().unwrap()
        );
        current.documents().save_text("indicators", &text).unwrap();

        let taken = take_legacy_indicators_folder(&current);

        assert_eq!(taken, Some(scripts.clone()));
        let after = reload(&base);
        assert_eq!(after.dir(Kind::Indicators), scripts);
        let rest = after.documents().load_text("indicators").unwrap().unwrap();
        assert!(!rest.contains("folder"), "{rest}");
        assert!(
            rest.contains("auto_reload = false") && rest.contains("favorites"),
            "the rest is kept: {rest}"
        );
        assert_eq!(take_legacy_indicators_folder(&after), None, "done once");
    }

    #[test]
    fn a_choice_made_on_this_computer_wins_and_a_relative_folder_is_dropped() {
        let (_temp, base, cloud) = temp();
        let current = paths(&base);
        let mut choices = Locations::default();
        choices.set(Kind::Indicators, Some(cloud.clone()));
        choices.save(&base).unwrap();
        let current = current.with_locations(Locations::load(&base).unwrap());
        current
            .documents()
            .save_text("indicators", "folder = \"C:\\\\old\"\n")
            .unwrap();
        // A path on a Windows machine is not a full path on another system: it only counts where it is.
        let dropped = take_legacy_indicators_folder(&current);
        assert_eq!(dropped, None, "the choice already made stays");
        assert_eq!(Locations::load(&base).unwrap().indicators, Some(cloud));

        current
            .documents()
            .save_text("indicators", "folder = \"relative/dir\"\n")
            .unwrap();
        assert_eq!(take_legacy_indicators_folder(&current), None);
        let rest = current
            .documents()
            .load_text("indicators")
            .unwrap()
            .unwrap();
        assert!(!rest.contains("folder"), "{rest}");
    }

    #[test]
    fn a_damaged_pointer_is_never_written_over_by_the_old_setting() {
        let (_temp, base, _cloud) = temp();
        std::fs::write(base.join(locations::FILE), "settings = [[[").unwrap();
        paths(&base)
            .documents()
            .save_text("indicators", "folder = \"/x\"\n")
            .unwrap();
        // Read as the start does: the pointer is damaged.
        let discovered = AppPaths::discover_for(BuildMode::Production, |name| {
            (name == "WYCK_CONFIG_DIR").then(|| base.clone().into_os_string())
        })
        .unwrap();
        assert!(discovered.pointer_error().is_some());
        assert_eq!(take_legacy_indicators_folder(&discovered), None);
        assert_eq!(
            std::fs::read_to_string(base.join(locations::FILE)).unwrap(),
            "settings = [[["
        );
    }

    #[test]
    fn one_kind_is_staged_into_the_folder_picked_or_a_folder_named_after_it() {
        let (_temp, base, cloud) = temp();
        let current = paths(&base);
        let steps = stage_folder_in(&current, Kind::Backups, Some(&cloud)).unwrap();
        assert_eq!(steps[0].to, cloud, "an empty folder is used as it is");
        relocate::cancel(&current).unwrap();

        std::fs::write(cloud.join("holiday.jpg"), "x").unwrap();
        let steps = stage_folder_in(&current, Kind::Backups, Some(&cloud)).unwrap();
        assert_eq!(
            steps[0].to,
            cloud.join("backups"),
            "files that are not ours are left alone"
        );
        assert!(relocate::pending(&current).is_some());
    }

    #[test]
    fn the_cloud_preset_moves_four_kinds_into_a_folder_of_this_build() {
        let (_temp, base, cloud) = temp();
        let current = paths(&base);
        let steps = stage_cloud_in(&current, &cloud).unwrap();
        let kinds: Vec<_> = steps.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            [
                Kind::Settings,
                Kind::Accounts,
                Kind::Indicators,
                Kind::Backups
            ]
        );
        assert!(steps.iter().all(|s| s.to.starts_with(cloud.join("Wyck"))));

        let development = AppPaths::at_for(BuildMode::Development, &base);
        relocate::cancel(&current).unwrap();
        let steps = stage_cloud_in(&development, &cloud).unwrap();
        assert!(
            steps
                .iter()
                .all(|s| s.to.starts_with(cloud.join("Wyck (dev)"))),
            "a development build has a folder of its own"
        );
    }

    #[test]
    fn the_overview_says_which_cloud_folder_the_data_is_already_in() {
        let (temp, base, cloud) = temp();
        let other = temp.path().join("other cloud");
        std::fs::create_dir_all(&other).unwrap();
        let folders = |dirs: &[&Path]| -> Vec<CloudFolder> {
            dirs.iter()
                .map(|path| CloudFolder {
                    name: "cloud".to_owned(),
                    path: path.to_path_buf(),
                })
                .collect()
        };
        let in_use = |paths: &AppPaths| -> Vec<bool> {
            overview_of(paths, folders(&[&cloud, &other]))
                .cloud
                .iter()
                .map(|choice| choice.in_use)
                .collect()
        };

        let current = paths(&base);
        assert_eq!(in_use(&current), [false, false], "nothing is moved yet");

        stage_cloud_in(&current, &cloud).unwrap();
        assert_eq!(
            in_use(&current),
            [false, false],
            "a move that waits is not done"
        );
        relocate::apply_pending(&current, "t");
        let after = reload(&base);
        assert_eq!(in_use(&after), [true, false]);

        let development = AppPaths::at_for(BuildMode::Development, &base)
            .with_locations(Locations::load(&base).unwrap());
        assert_eq!(
            overview_of(&development, folders(&[&cloud]))
                .cloud
                .iter()
                .map(|choice| choice.in_use)
                .collect::<Vec<_>>(),
            [false],
            "the folder of the installed app is not the one of a development build"
        );

        let mut choices = Locations::load(&base).unwrap();
        choices.set(Kind::Backups, None);
        choices.save(&base).unwrap();
        assert_eq!(
            in_use(&reload(&base)),
            [false, false],
            "one kind back in its standard folder: using the cloud folder again finishes the job"
        );
    }

    #[test]
    fn the_development_build_cannot_take_the_folder_of_the_installed_app() {
        let (_temp, base, cloud) = temp();
        let installed = paths(&base);
        stage_cloud_in(&installed, &cloud).unwrap();
        relocate::apply_pending(&installed, "t");

        // Another config folder, a development build, asked to use the installed app's folder.
        let (_other, dev_base, _) = temp();
        let development = AppPaths::at_for(BuildMode::Development, &dev_base);
        let target = cloud.join("Wyck").join("settings");
        let error = relocate::stage(
            &development,
            &[Change {
                kind: Kind::Settings,
                to: Some(target),
            }],
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("belongs to the installed app"),
            "{error}"
        );
    }

    #[test]
    fn the_indicators_folder_is_changed_for_this_computer_with_no_move_and_no_marker() {
        let (_temp, base, cloud) = temp();
        let current = paths(&base);
        std::fs::write(cloud.join("my_rsi.rhai"), "plot(\"a\", close);").unwrap();

        let used = set_indicators_folder_in(&current, Some(&cloud)).unwrap();

        assert_eq!(used, cloud);
        assert_eq!(reload(&base).dir(Kind::Indicators), cloud);
        assert!(
            !cloud.join(locations::MARKER).exists(),
            "their own folder is not marked"
        );
        assert!(
            cloud.join("my_rsi.rhai").is_file(),
            "nothing was copied or moved"
        );

        let used = set_indicators_folder_in(&reload(&base), None).unwrap();
        assert_eq!(used, base.join("indicators"));
        assert!(!reload(&base).is_custom(Kind::Indicators));

        assert!(matches!(
            set_indicators_folder_in(&current, Some(Path::new("relative"))),
            Err(FolderError::Folder(Problem {
                reason: locations::Reason::NotAbsolute,
                ..
            }))
        ));
    }

    #[test]
    fn the_overview_says_where_each_kind_is_and_what_is_wrong() {
        let (_temp, base, cloud) = temp();
        let mut choices = Locations::default();
        choices.set(Kind::Settings, Some(cloud.join("gone")));
        choices.save(&base).unwrap();
        let current = reload(&base);
        std::fs::create_dir_all(current.state_dir()).unwrap();
        std::fs::write(current.state_dir().join("preferences-LAPTOP.toml"), "x").unwrap();

        let overview = overview_of(&current, Vec::new());

        assert_eq!(overview.rows.len(), 5);
        let settings = &overview.rows[0];
        assert!(
            !settings.custom,
            "the folder that was chosen is not there, so the standard one is used"
        );
        assert_eq!(settings.path, settings.standard);
        assert_eq!(
            settings.unavailable.as_ref().unwrap().reason,
            locations::Reason::Missing
        );
        assert!(overview.rows[1].unavailable.is_none());
        assert_eq!(overview.conflicts.len(), 1);
        assert_eq!(overview.mode, BuildMode::Production);
        assert!(overview.waiting.is_none());

        let notes = notes_for(&current, None);
        let titles: Vec<&str> = notes.iter().map(|n| n.title.as_str()).collect();
        assert!(
            titles.contains(&"A chosen folder cannot be used"),
            "{titles:?}"
        );
        assert!(
            titles.contains(&"Copies made by your sync service"),
            "{titles:?}"
        );
        assert!(notes.iter().all(|n| n.warning));
    }

    #[test]
    fn a_move_that_waits_is_described_and_the_start_tells_what_it_did() {
        let (_temp, base, cloud) = temp();
        let start = paths(&base);
        start
            .documents()
            .save_text("preferences", "magnet = true\n")
            .unwrap();
        let steps = stage_folder_in(&start, Kind::Settings, Some(&cloud.join("settings"))).unwrap();
        let lines = describe(&steps);
        assert!(
            lines[0].starts_with("Settings: 1 file(s), 14 B, are copied to"),
            "{lines:?}"
        );
        assert!(overview_of(&start, Vec::new()).waiting.is_some());

        let applied = relocate::apply_pending(&start, "t");
        let after = reload(&base);
        let startup = Startup {
            applied,
            legacy_indicators: None,
        };
        let notes = notes_for(&after, Some(&startup));
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "Folders moved");
        assert!(notes[0].text.contains("Settings:"), "{}", notes[0].text);
        assert!(!notes[0].warning);
    }

    #[test]
    fn sizes_are_a_few_characters() {
        assert_eq!(size(12), "12 B");
        assert_eq!(size(2048), "2 KB");
        assert_eq!(size(3 * 1_048_576), "3.0 MB");
    }
}
