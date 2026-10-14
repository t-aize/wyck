//! Where each kind of saved data lives, and the file that says so.
//!
//! By default everything is in the standard folders of the system (see [`AppPaths`]). A user can
//! keep some of it elsewhere, such as a folder that OneDrive, Dropbox or another service syncs
//! between computers, or a data disk. Five kinds can move, each on its own ([`Kind`]):
//!
//! * the settings (layout, shortcuts, favorites, everything the look and the charts keep),
//! * the data of the accounts (drawings, alerts, watchlists),
//! * the indicator scripts,
//! * the backups,
//! * the logs.
//!
//! What never moves: the credentials (the system keyring, or the encrypted files of the data folder)
//! and `config.toml`, which lists the signed-in accounts and ties each to its credentials on this
//! computer. A synced folder holds nothing that signs in.
//!
//! # The pointer
//!
//! The choice is saved in `locations.toml`, in the standard config folder of the build (the one
//! [`AppPaths`] starts from, or where `WYCK_CONFIG_DIR` says). That file is never moved: something
//! has to be at a known place for the rest to be found, as in the profile list of a browser. A
//! development build and the installed app have different standard folders, so their pointers are
//! apart.
//!
//! # Dev and production
//!
//! Every moved folder holds a small marker, `wyck-location.toml`, that says which build made it and
//! which kind of data it is for. A development build refuses a folder marked for the installed app
//! and the other way round, and a folder of one kind is never taken for another. This is the same
//! rule as the keyring service and the default folders: testing cannot reach real data.
//!
//! # A folder that is not there
//!
//! A synced folder can be missing at start (the service has not mounted it, the drive is out).
//! The pointer is kept, that kind is read from and written to its standard folder for this run,
//! and [`AppPaths::problems`] says why, so the app can tell the user. Nothing is created in a place
//! that was not found, and nothing is written over a folder made by the other build.
//!
//! [`AppPaths`]: crate::infra::storage::AppPaths
//! [`AppPaths::problems`]: crate::infra::storage::AppPaths::problems

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::infra::storage::error::{ConfigError, Result};
use crate::infra::storage::fs_util::{atomic_write, read_text};
use crate::infra::storage::migrate::{self, Outcome, Plan};
use wyck_platform::build_info::BuildMode;

/// The name of the pointer, in the config folder.
pub const FILE: &str = "locations.toml";
/// The name of the marker, inside a moved folder.
pub const MARKER: &str = "wyck-location.toml";

const MARKER_FORMAT: &str = "wyck-location";
const SCHEMA_VERSION: u32 = 1;

/// How an older `locations.toml` is brought to the current layout.
const PLAN: Plan = Plan {
    document: "locations",
    current: SCHEMA_VERSION,
    steps: &[],
};

/// A kind of data that can be kept in a folder of the user's choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// Layout, shortcuts and every document shared by the accounts.
    Settings,
    /// Drawings, alerts and watchlists, one folder per account.
    Accounts,
    /// The scripts of the indicators the user writes.
    Indicators,
    /// The copies of everything, kept for restoring.
    Backups,
    /// The log files and the reports of a panic.
    Logs,
}

impl Kind {
    pub const ALL: [Self; 5] = [
        Self::Settings,
        Self::Accounts,
        Self::Indicators,
        Self::Backups,
        Self::Logs,
    ];

    /// The name of the kind in files: the name of a folder inside a cloud folder, and the `kind`
    /// of a marker.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::Accounts => "accounts",
            Self::Indicators => "indicators",
            Self::Backups => "backups",
            Self::Logs => "logs",
        }
    }

    /// The kind a [`Self::code`] names.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.code() == code)
    }

    /// A few words for a list.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Settings => "Settings",
            Self::Accounts => "Account data",
            Self::Indicators => "Indicator scripts",
            Self::Backups => "Backups",
            Self::Logs => "Logs",
        }
    }

    /// What is in it, for the line under the label.
    #[must_use]
    pub const fn help(self) -> &'static str {
        match self {
            Self::Settings => {
                "Look, colors, layout, shortcuts, favorites, ticket and indicator settings"
            }
            Self::Accounts => {
                "Drawings and their saved looks, watchlists and alerts, one folder per account number"
            }
            Self::Indicators => "The .rhai files of your own indicators",
            Self::Backups => "The copies kept for restoring, automatic ones included",
            Self::Logs => "What the app wrote about itself, and the report of a crash",
        }
    }

    /// Whether the cloud preset puts it in the cloud folder. The logs say what happened on one
    /// computer, so they stay where they are.
    #[must_use]
    pub const fn in_cloud_preset(self) -> bool {
        !matches!(self, Self::Logs)
    }

    /// Whether moving it carries its files along. The old logs stay where they were written.
    #[must_use]
    pub const fn carries_files(self) -> bool {
        !matches!(self, Self::Logs)
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

fn version() -> u32 {
    SCHEMA_VERSION
}

/// The folders the user chose. A kind with none is in its standard folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Locations {
    #[serde(default = "version")]
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accounts: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indicators: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backups: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logs: Option<PathBuf>,
    /// Folders a move left behind (the old place, renamed), kept until the user deletes them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leftovers: Vec<PathBuf>,
}

impl Default for Locations {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            settings: None,
            accounts: None,
            indicators: None,
            backups: None,
            logs: None,
            leftovers: Vec::new(),
        }
    }
}

impl Locations {
    /// The folder chosen for `kind`.
    #[must_use]
    pub fn get(&self, kind: Kind) -> Option<&Path> {
        match kind {
            Kind::Settings => &self.settings,
            Kind::Accounts => &self.accounts,
            Kind::Indicators => &self.indicators,
            Kind::Backups => &self.backups,
            Kind::Logs => &self.logs,
        }
        .as_deref()
    }

    /// Chooses the folder of `kind`, or with `None` goes back to the standard one.
    pub fn set(&mut self, kind: Kind, dir: Option<PathBuf>) {
        match kind {
            Kind::Settings => self.settings = dir,
            Kind::Accounts => self.accounts = dir,
            Kind::Indicators => self.indicators = dir,
            Kind::Backups => self.backups = dir,
            Kind::Logs => self.logs = dir,
        }
    }

    /// Whether nothing is chosen and nothing is left behind.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Reads the pointer in `config_dir`; nothing there is the default.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Read`], [`ConfigError::Parse`] for a file that is not TOML of this shape,
    /// and [`ConfigError::UnsupportedSchema`] for one written by a newer version. The file is left
    /// as it is.
    pub fn load(config_dir: &Path) -> Result<Self> {
        let path = config_dir.join(FILE);
        if let Outcome::Newer { found, .. } = migrate::upgrade_file(&path, &PLAN)? {
            return Err(ConfigError::UnsupportedSchema {
                path,
                found,
                supported: SCHEMA_VERSION,
            });
        }
        let text = match read_text(&path) {
            Ok(text) => text,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => return Err(ConfigError::Read { path, source }),
        };
        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path,
            source: Box::new(source),
        })
    }

    /// Writes the pointer in `config_dir`, atomically.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Serialize`] (a path that is not valid text cannot be written) and
    /// [`ConfigError::Write`].
    pub fn save(&self, config_dir: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self).map_err(ConfigError::Serialize)?;
        atomic_write(&config_dir.join(FILE), text.as_bytes())
    }
}

/// Why a folder cannot be used for a kind of data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// The path is not absolute: it would mean a different place each time.
    NotAbsolute,
    /// There is nothing there. A synced or removable drive may not be mounted.
    Missing,
    /// It is a file.
    NotAFolder,
    /// A file cannot be made in it.
    NotWritable,
    /// It holds other things and was not made by Wyck: data would be mixed with them.
    NotEmpty,
    /// It was made by the other build (`found` is `production` or `development`).
    OtherBuild { found: String },
    /// It was made for another kind of data (`found` is the code of that kind).
    OtherKind { found: String },
    /// It could not be looked into.
    Unreadable,
}

/// A folder that cannot be used, with the kind it was for and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub kind: Kind,
    pub path: PathBuf,
    pub reason: Reason,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.path.display();
        match &self.reason {
            Reason::NotAbsolute => write!(f, "`{path}` is not a full path"),
            Reason::Missing => write!(f, "`{path}` was not found"),
            Reason::NotAFolder => write!(f, "`{path}` is a file, not a folder"),
            Reason::NotWritable => write!(f, "Wyck cannot write in `{path}`"),
            Reason::NotEmpty => write!(
                f,
                "`{path}` holds other files and was not made by Wyck, so it would be mixed with them"
            ),
            Reason::OtherBuild { found } => {
                let (this, other) = if found == "production" {
                    ("a development build", "the installed app")
                } else {
                    ("the installed app", "a development build")
                };
                write!(f, "`{path}` belongs to {other}, and this is {this}")
            }
            Reason::OtherKind { found } => write!(
                f,
                "`{path}` holds `{found}` data, not `{}`",
                self.kind.code()
            ),
            Reason::Unreadable => write!(f, "`{path}` could not be looked into"),
        }
    }
}

impl std::error::Error for Problem {}

/// What a folder that can be used holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checked {
    /// Whether it already holds data: it was made by Wyck for this kind, or it is not empty.
    pub has_data: bool,
}

/// How strict [`check`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Use {
    /// A folder already in use. It must be there.
    Existing,
    /// A folder just chosen to receive data. It may not exist yet (what is above it must), and if
    /// it does it must be empty or made by Wyck.
    Chosen,
    /// A folder the user already keeps their own files in, such as the scripts they wrote, to be
    /// read in place. As [`Self::Chosen`], but other files in it are theirs and welcome.
    Own,
}

/// The content of a marker.
#[derive(Debug, Serialize, Deserialize)]
struct Marker {
    format: String,
    #[serde(default)]
    schema_version: u32,
    kind: String,
    build: String,
}

/// Writes the marker that says this folder is for `kind`, made by this build.
///
/// # Errors
///
/// [`ConfigError::Serialize`] and [`ConfigError::Write`].
pub fn write_marker(dir: &Path, kind: Kind, mode: BuildMode) -> Result<()> {
    let marker = Marker {
        format: MARKER_FORMAT.to_owned(),
        schema_version: SCHEMA_VERSION,
        kind: kind.code().to_owned(),
        build: mode.code().to_owned(),
    };
    let text = toml::to_string_pretty(&marker).map_err(ConfigError::Serialize)?;
    atomic_write(&dir.join(MARKER), text.as_bytes())
}

/// The marker of `dir`: `None` when there is none (or it is not a marker of Wyck).
fn read_marker(dir: &Path) -> std::result::Result<Option<Marker>, ()> {
    match read_text(&dir.join(MARKER)) {
        Ok(text) => match toml::from_str::<Marker>(&text) {
            Ok(marker) if marker.format == MARKER_FORMAT => Ok(Some(marker)),
            _ => Ok(None),
        },
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    }
}

/// Whether anything but the marker and the leftovers of a write is in `dir`.
pub(crate) fn holds_files(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name != MARKER && !name.contains(".tmp-") && !name.starts_with(".wyck-probe")
        })
    })
}

/// Whether a file can be made in `dir`.
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".wyck-probe-{}", std::process::id()));
    let made = fs::write(&probe, b"").is_ok();
    let _ = fs::remove_file(&probe);
    made
}

/// Whether `dir` can hold the data of `kind` for this build.
///
/// # Errors
///
/// The [`Problem`] that rules it out.
pub fn check(
    dir: &Path,
    kind: Kind,
    mode: BuildMode,
    how: Use,
) -> std::result::Result<Checked, Problem> {
    let fail = |reason| Problem {
        kind,
        path: dir.to_path_buf(),
        reason,
    };
    if !dir.is_absolute() {
        return Err(fail(Reason::NotAbsolute));
    }
    match fs::metadata(dir) {
        Ok(metadata) if !metadata.is_dir() => Err(fail(Reason::NotAFolder)),
        Ok(_) => {
            let marker = read_marker(dir).map_err(|()| fail(Reason::Unreadable))?;
            let has_data = match &marker {
                Some(marker) => {
                    if marker.build != mode.code() {
                        return Err(fail(Reason::OtherBuild {
                            found: marker.build.clone(),
                        }));
                    }
                    if marker.kind != kind.code() {
                        return Err(fail(Reason::OtherKind {
                            found: marker.kind.clone(),
                        }));
                    }
                    true
                }
                None => holds_files(dir),
            };
            if marker.is_none() && has_data && how == Use::Chosen {
                return Err(fail(Reason::NotEmpty));
            }
            if !writable(dir) {
                return Err(fail(Reason::NotWritable));
            }
            Ok(Checked { has_data })
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => match how {
            Use::Existing => Err(fail(Reason::Missing)),
            Use::Chosen | Use::Own => {
                // A folder to be made, with whatever is missing above it: the nearest folder that
                // exists has to be one, and open to writing. A drive or a service that is not there
                // has no such folder, and nothing is made on it.
                match dir.ancestors().skip(1).find(|above| above.exists()) {
                    None => Err(fail(Reason::Missing)),
                    Some(above) if !above.is_dir() => Err(fail(Reason::NotAFolder)),
                    Some(above) if !writable(above) => Err(fail(Reason::NotWritable)),
                    Some(_) => Ok(Checked { has_data: false }),
                }
            }
        },
        // Unix says so when a part of the path is a file; Windows says "not found".
        Err(source) if source.kind() == std::io::ErrorKind::NotADirectory => {
            Err(fail(Reason::NotAFolder))
        }
        Err(_) => Err(fail(Reason::Unreadable)),
    }
}

/// The folder to use when the user picked `picked` for `kind`: itself when it is empty or already
/// Wyck's, and a folder named after the kind inside it when it holds other things, so that nothing
/// is ever mixed with files that are not ours.
#[must_use]
pub fn target_in(picked: &Path, kind: Kind, mode: BuildMode) -> PathBuf {
    match check(picked, kind, mode, Use::Chosen) {
        Err(Problem {
            reason: Reason::NotEmpty,
            ..
        }) => picked.join(kind.code()),
        _ => picked.to_path_buf(),
    }
}

/// The folder for `kind` that `locations` names if it can be used, the standard one otherwise;
/// a folder that cannot be used is added to `problems`.
pub(crate) fn resolve(
    locations: &Locations,
    kind: Kind,
    standard: &Path,
    mode: BuildMode,
    problems: &mut Vec<Problem>,
) -> PathBuf {
    let Some(chosen) = locations.get(kind) else {
        return standard.to_path_buf();
    };
    match check(chosen, kind, mode, Use::Existing) {
        Ok(_) => chosen.to_path_buf(),
        Err(problem) => {
            warn!(%problem, kind = kind.code(), "a chosen folder cannot be used, the standard one is used for now");
            problems.push(problem);
            standard.to_path_buf()
        }
    }
}

/// A folder a cloud service keeps in step between computers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudFolder {
    /// How the user knows it: the name of the folder, such as `OneDrive` or `OneDrive - Contoso`.
    pub name: String,
    pub path: PathBuf,
}

/// The cloud folders of this computer, the ones the environment names and the well-known ones in
/// the home folder. Only folders that exist.
///
/// OneDrive sets `OneDrive`, `OneDriveConsumer` and `OneDriveCommercial` for the user; the other
/// services are found by the name they give their folder. The list is a help, not a rule: the user
/// can always pick any folder.
#[must_use]
pub fn cloud_folders_with(
    var: impl Fn(&str) -> Option<OsString>,
    home: Option<&Path>,
) -> Vec<CloudFolder> {
    let mut found: Vec<CloudFolder> = Vec::new();
    let mut add = |path: PathBuf| {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let same = |other: &CloudFolder| {
            other.path.to_string_lossy().to_lowercase() == path.to_string_lossy().to_lowercase()
        };
        if path.is_dir() && !found.iter().any(same) {
            found.push(CloudFolder { name, path });
        }
    };

    for name in ["OneDriveCommercial", "OneDriveConsumer", "OneDrive"] {
        if let Some(value) = var(name).filter(|value| !value.is_empty()) {
            add(PathBuf::from(value));
        }
    }
    let Some(home) = home else {
        return found;
    };
    let mut in_home: Vec<PathBuf> = fs::read_dir(home)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            name.starts_with("onedrive")
                || [
                    "dropbox",
                    "google drive",
                    "icloud drive",
                    "iclouddrive",
                    "nextcloud",
                    "pcloud drive",
                ]
                .contains(&name.as_str())
        })
        .map(|entry| entry.path())
        .collect();
    in_home.sort();
    // macOS keeps the providers under CloudStorage, and iCloud Drive in its own place.
    let storage = home.join("Library").join("CloudStorage");
    let mut in_storage: Vec<PathBuf> = fs::read_dir(storage)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.path())
        .collect();
    in_storage.sort();
    in_home.extend(in_storage);
    in_home.push(
        home.join("Library")
            .join("Mobile Documents")
            .join("com~apple~CloudDocs"),
    );
    for path in in_home {
        add(path);
    }
    found
}

/// [`cloud_folders_with`] for the real environment and the home folder of the user.
#[must_use]
pub fn cloud_folders() -> Vec<CloudFolder> {
    let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf());
    cloud_folders_with(|name| std::env::var_os(name), home.as_deref())
}

/// The name of the folder this build makes inside a cloud folder: a development build has its own,
/// so it never meets the data of the installed app.
#[must_use]
pub const fn cloud_folder_name(mode: BuildMode) -> &'static str {
    match mode {
        BuildMode::Production => "Wyck",
        BuildMode::Development => "Wyck (dev)",
    }
}

/// Where each kind goes when the data is put in the cloud folder `root`: the kinds the preset
/// covers, in a folder per kind inside `root/Wyck`.
#[must_use]
pub fn cloud_layout(root: &Path, mode: BuildMode) -> Vec<(Kind, PathBuf)> {
    let base = root.join(cloud_folder_name(mode));
    Kind::ALL
        .into_iter()
        .filter(|kind| kind.in_cloud_preset())
        .map(|kind| (kind, base.join(kind.code())))
        .collect()
}

/// The files in `dir` that look like the copy a sync service makes when two computers changed the
/// same file: a document name followed by a computer name, a number or a note, such as
/// `preferences-LAPTOP.toml`, `preferences (1).toml` or `preferences (conflicted copy 2026).toml`.
/// `known` is the names of the documents. The app reads only `<name>.toml`, so a copy like this is
/// never read: it holds changes that are lost unless the user merges them by hand.
#[must_use]
pub fn conflict_copies(dir: &Path, known: &[&str]) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter(|entry| {
            let name = entry.file_name();
            let Some(stem) = name
                .to_string_lossy()
                .strip_suffix(".toml")
                .map(str::to_owned)
            else {
                return false;
            };
            known.iter().any(|document| {
                stem.strip_prefix(document).is_some_and(|rest| {
                    rest.starts_with(['-', ' ', '_', '(', '.']) && rest.len() > 1
                })
            })
        })
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn the_pointer_is_saved_and_read_back_and_an_empty_one_is_the_default() {
        let temp = dir();
        assert_eq!(Locations::load(temp.path()).unwrap(), Locations::default());

        let mut locations = Locations::default();
        locations.set(Kind::Settings, Some(PathBuf::from("/cloud/Wyck/settings")));
        locations.set(Kind::Logs, Some(PathBuf::from("/disk/logs")));
        locations
            .leftovers
            .push(PathBuf::from("/old/state.moved-1"));
        locations.save(temp.path()).unwrap();

        let back = Locations::load(temp.path()).unwrap();
        assert_eq!(back, locations);
        assert_eq!(
            back.get(Kind::Settings),
            Some(Path::new("/cloud/Wyck/settings"))
        );
        assert_eq!(back.get(Kind::Accounts), None);
        let text = std::fs::read_to_string(temp.path().join(FILE)).unwrap();
        assert!(
            !text.contains("accounts"),
            "a kind left alone is not written: {text}"
        );
    }

    #[test]
    fn a_damaged_or_newer_pointer_is_reported_and_left_alone() {
        let temp = dir();
        std::fs::write(temp.path().join(FILE), "settings = [[[").unwrap();
        assert!(matches!(
            Locations::load(temp.path()),
            Err(ConfigError::Parse { .. })
        ));
        assert_eq!(
            std::fs::read_to_string(temp.path().join(FILE)).unwrap(),
            "settings = [[["
        );

        std::fs::write(
            temp.path().join(FILE),
            "schema_version = 9\nfrom_later = 1\n",
        )
        .unwrap();
        assert!(matches!(
            Locations::load(temp.path()),
            Err(ConfigError::UnsupportedSchema { found: 9, .. })
        ));
    }

    #[test]
    fn every_kind_has_its_own_name_and_only_the_logs_stay_out_of_the_cloud() {
        let codes: std::collections::HashSet<_> = Kind::ALL.iter().map(|k| k.code()).collect();
        assert_eq!(codes.len(), Kind::ALL.len());
        let cloud: Vec<_> = Kind::ALL.iter().filter(|k| k.in_cloud_preset()).collect();
        assert_eq!(cloud.len(), 4);
        assert!(!Kind::Logs.in_cloud_preset() && !Kind::Logs.carries_files());
        assert!(Kind::Settings.carries_files());
    }

    #[test]
    fn a_folder_must_be_a_full_path_to_a_folder_that_can_be_written() {
        let temp = dir();
        let mode = BuildMode::Production;
        let problem = |dir: &Path, how| check(dir, Kind::Settings, mode, how).unwrap_err().reason;

        assert_eq!(
            problem(Path::new("relative/dir"), Use::Existing),
            Reason::NotAbsolute
        );
        assert_eq!(
            problem(&temp.path().join("nothing"), Use::Existing),
            Reason::Missing
        );
        let file = temp.path().join("file");
        std::fs::write(&file, "x").unwrap();
        assert_eq!(problem(&file, Use::Existing), Reason::NotAFolder);
        assert_eq!(
            problem(&file.join("below"), Use::Chosen),
            Reason::NotAFolder,
            "nothing can be made under a file"
        );
        assert_eq!(
            check(
                &temp.path().join("a").join("b"),
                Kind::Settings,
                mode,
                Use::Chosen
            )
            .unwrap(),
            Checked { has_data: false },
            "a chosen folder may be several levels below one that exists"
        );

        assert_eq!(
            check(temp.path(), Kind::Settings, mode, Use::Existing).unwrap(),
            Checked { has_data: true },
            "the temporary folder holds the file made above"
        );
        assert_eq!(
            check(&temp.path().join("new"), Kind::Settings, mode, Use::Chosen).unwrap(),
            Checked { has_data: false },
            "a folder to be made is fine"
        );
    }

    #[test]
    fn a_chosen_folder_must_be_empty_or_made_by_wyck() {
        let temp = dir();
        let mode = BuildMode::Production;
        let empty = temp.path().join("empty");
        std::fs::create_dir(&empty).unwrap();
        assert_eq!(
            check(&empty, Kind::Settings, mode, Use::Chosen).unwrap(),
            Checked { has_data: false }
        );

        let foreign = temp.path().join("foreign");
        std::fs::create_dir(&foreign).unwrap();
        std::fs::write(foreign.join("holiday.jpg"), "x").unwrap();
        let problem = check(&foreign, Kind::Settings, mode, Use::Chosen).unwrap_err();
        assert_eq!(problem.reason, Reason::NotEmpty);
        assert!(problem.to_string().contains("mixed"), "{problem}");
        assert_eq!(
            target_in(&foreign, Kind::Settings, mode),
            foreign.join("settings"),
            "files that are not ours are never mixed with"
        );
        assert_eq!(target_in(&empty, Kind::Settings, mode), empty);

        write_marker(&foreign, Kind::Settings, mode).unwrap();
        assert_eq!(
            check(&foreign, Kind::Settings, mode, Use::Chosen).unwrap(),
            Checked { has_data: true },
            "a folder made by Wyck can be taken again"
        );
    }

    #[test]
    fn the_folder_of_the_other_build_or_of_another_kind_is_refused() {
        let temp = dir();
        write_marker(temp.path(), Kind::Settings, BuildMode::Production).unwrap();

        let development = check(
            temp.path(),
            Kind::Settings,
            BuildMode::Development,
            Use::Existing,
        )
        .unwrap_err();
        assert_eq!(
            development.reason,
            Reason::OtherBuild {
                found: "production".into()
            }
        );
        assert!(
            development
                .to_string()
                .contains("belongs to the installed app"),
            "{development}"
        );
        let production = {
            let other = dir();
            write_marker(other.path(), Kind::Settings, BuildMode::Development).unwrap();
            check(
                other.path(),
                Kind::Settings,
                BuildMode::Production,
                Use::Chosen,
            )
            .unwrap_err()
        };
        assert!(
            production
                .to_string()
                .contains("belongs to a development build")
        );

        let kind = check(
            temp.path(),
            Kind::Accounts,
            BuildMode::Production,
            Use::Existing,
        )
        .unwrap_err();
        assert_eq!(
            kind.reason,
            Reason::OtherKind {
                found: "settings".into()
            }
        );
        assert!(
            check(
                temp.path(),
                Kind::Settings,
                BuildMode::Production,
                Use::Existing
            )
            .is_ok(),
            "its own build and kind pass"
        );
    }

    #[test]
    fn a_folder_that_cannot_be_used_leaves_the_kind_in_its_standard_place_and_says_why() {
        let temp = dir();
        let standard = temp.path().join("standard");
        let mut locations = Locations::default();
        locations.set(Kind::Settings, Some(temp.path().join("gone")));
        let mut problems = Vec::new();

        let used = resolve(
            &locations,
            Kind::Settings,
            &standard,
            BuildMode::Production,
            &mut problems,
        );
        assert_eq!(used, standard);
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].reason, Reason::Missing);

        // A kind with no choice is not a problem.
        let used = resolve(
            &locations,
            Kind::Accounts,
            &standard,
            BuildMode::Production,
            &mut problems,
        );
        assert_eq!(used, standard);
        assert_eq!(problems.len(), 1);

        // A folder that is there is used.
        let chosen = temp.path().join("chosen");
        std::fs::create_dir(&chosen).unwrap();
        locations.set(Kind::Accounts, Some(chosen.clone()));
        let used = resolve(
            &locations,
            Kind::Accounts,
            &standard,
            BuildMode::Production,
            &mut problems,
        );
        assert_eq!(used, chosen);
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn the_cloud_folders_of_the_environment_and_of_the_home_folder_are_found_once_each() {
        let temp = dir();
        let home = temp.path().join("home");
        for name in [
            "OneDrive",
            "OneDrive - Contoso",
            "Dropbox",
            "Documents",
            "Library/CloudStorage/GoogleDrive-me",
            "Library/Mobile Documents/com~apple~CloudDocs",
        ] {
            std::fs::create_dir_all(home.join(name)).unwrap();
        }
        std::fs::write(home.join("OneDrive notes.txt"), "a file, not a folder").unwrap();
        let missing = home.join("OneDriveGhost");
        let consumer = home.join("OneDrive");
        let found = cloud_folders_with(
            env(&[
                ("OneDriveConsumer", consumer.to_str().unwrap()),
                ("OneDrive", consumer.to_str().unwrap()),
                ("OneDriveCommercial", missing.to_str().unwrap()),
            ]),
            Some(&home),
        );

        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names.iter().filter(|name| **name == "OneDrive").count(),
            1,
            "named by the environment and found in the home folder, listed once: {names:?}"
        );
        for wanted in [
            "OneDrive - Contoso",
            "Dropbox",
            "GoogleDrive-me",
            "com~apple~CloudDocs",
        ] {
            assert!(names.contains(&wanted), "{wanted} in {names:?}");
        }
        assert!(!names.contains(&"Documents") && !names.contains(&"OneDriveGhost"));
        assert!(cloud_folders_with(env(&[]), None).is_empty());
    }

    #[test]
    fn a_development_build_has_its_own_folder_in_the_cloud() {
        let root = Path::new("/cloud");
        let production = cloud_layout(root, BuildMode::Production);
        let development = cloud_layout(root, BuildMode::Development);
        assert_eq!(production.len(), 4);
        assert!(
            production
                .iter()
                .all(|(_, path)| path.starts_with("/cloud/Wyck"))
        );
        assert!(
            development
                .iter()
                .all(|(_, path)| path.starts_with("/cloud/Wyck (dev)"))
        );
        assert!(!production.iter().any(|(kind, _)| *kind == Kind::Logs));
        assert_eq!(
            production[0],
            (Kind::Settings, PathBuf::from("/cloud/Wyck/settings"))
        );
    }

    #[test]
    fn copies_a_sync_service_makes_are_found_and_the_documents_are_not() {
        let temp = dir();
        for name in [
            "preferences.toml",
            "alerts.toml",
            "preferences-LAPTOP.toml",
            "preferences (1).toml",
            "alerts (conflicted copy 2026-10-12).toml",
            "preferences.sync-conflict-20261012-101010-ABCDEFG.toml",
            "preferences.toml.v1.bak",
            "preferences.toml.bad",
            "notes.toml",
            "indicator_editor_drafts.toml",
        ] {
            std::fs::write(temp.path().join(name), "x").unwrap();
        }
        let found: Vec<String> = conflict_copies(temp.path(), &["preferences", "alerts"])
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            found,
            [
                "alerts (conflicted copy 2026-10-12).toml",
                "preferences (1).toml",
                "preferences-LAPTOP.toml",
                "preferences.sync-conflict-20261012-101010-ABCDEFG.toml",
            ]
        );
        assert!(conflict_copies(&temp.path().join("nothing"), &["preferences"]).is_empty());
    }
}
