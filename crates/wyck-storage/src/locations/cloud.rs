//! Cloud folders: where the folders a service keeps in step between computers are, and what the
//! data of a build does inside one.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use super::Kind;
use crate::build_info::BuildMode;

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
/// `OneDrive` sets `OneDrive`, `OneDriveConsumer` and `OneDriveCommercial` for the user; the other
/// services are found by the name they give their folder. The list is a help, not a rule: the user
/// can always pick any folder.
#[must_use]
pub(crate) fn cloud_folders_with(
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

/// `cloud_folders_with` for the real environment and the home folder of the user.
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
