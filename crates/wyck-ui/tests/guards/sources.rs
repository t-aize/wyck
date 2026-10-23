//! Reading the `.rs` files of a crate, named from the `crates` folder.

use std::fs;
use std::path::{Path, PathBuf};

/// The `crates` folder of the workspace.
pub(crate) fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[expect(
    clippy::panic,
    reason = "a folder that moved must fail the guard, not make it pass on nothing"
)]
pub(crate) fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The path of a file from the `crates` folder, with `/` separators: `wyck-ui/src/kit/menu.rs`.
pub(crate) fn name_of(file: &Path) -> String {
    let root = crates_dir();
    file.strip_prefix(&root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}
