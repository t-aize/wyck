//! Reading folders as the data they hold: paths made comparable, and the files that are data.

use std::fs;
use std::path::{Path, PathBuf};

use crate::locations;

/// How deep a folder is followed. A tree deeper than this is not data of Wyck.
const MAX_DEPTH: usize = 12;

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
pub(super) fn normalized(path: &Path) -> PathBuf {
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
                _ => break path,
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
pub(super) fn within(inner: &Path, outer: &Path) -> bool {
    normalized(inner).starts_with(normalized(outer))
}

/// The files under `dir` that are data, with their sizes and their path relative to `dir`: not the
/// marker, not the leftovers of a write, not links.
pub(super) fn data_files(dir: &Path) -> Vec<(PathBuf, u64)> {
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
