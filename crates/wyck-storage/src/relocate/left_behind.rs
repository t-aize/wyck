//! The old folders a move left behind, and deleting one on request.

use std::fs;
use std::path::{Path, PathBuf};

use tracing::info;

use super::tree::{data_files, within};
use crate::error::{ConfigError, Result};
use crate::locations::{Kind, Locations};
use crate::paths::AppPaths;

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
