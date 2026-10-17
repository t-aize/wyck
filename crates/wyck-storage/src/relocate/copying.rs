//! Copying the files of a folder to another and reading the copy back.

use std::fs;
use std::path::{Path, PathBuf};

use super::tree::data_files;
use crate::error::{ConfigError, Result};
use crate::fs_util::{remove_stray, retry_when_busy};

/// The largest file whose content is compared byte by byte after a copy; a larger one is compared
/// by its size.
const COMPARED_BYTES: u64 = 8 * 1024 * 1024;

/// Copies the data files of `from` into `to`, each through a temporary name. `made` gets every
/// file written.
pub(super) fn copy_tree(from: &Path, to: &Path, made: &mut Vec<PathBuf>) -> std::io::Result<()> {
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
            remove_stray(&temporary);
            return Err(error);
        }
        made.push(target);
    }
    Ok(())
}

/// Reads the copy back: the same files, of the same size, and the same bytes up to
/// [`COMPARED_BYTES`].
pub(super) fn verify(from: &Path, to: &Path) -> Result<()> {
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
