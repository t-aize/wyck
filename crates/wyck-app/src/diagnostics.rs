//! What a user needs to report a problem: where the log files are, and how many panic reports the
//! app has written. The files themselves are written by [`crate::system::logging`].

use std::path::{Path, PathBuf};

use gpui::App;

use crate::system::logging;

/// The folder of the log files and of the panic reports, `None` when the system gives the app no
/// folder.
pub fn logs_dir() -> Option<PathBuf> {
    crate::app_paths().map(|paths| paths.logs_dir().to_path_buf())
}

/// How many panic reports are in `dir`.
pub fn report_count(dir: &Path) -> usize {
    logging::reports(dir).len()
}

/// Shows the folder of the logs in the file explorer of the system. The folder is made first, so
/// it opens even when nothing has been written yet.
pub fn open_logs_folder(cx: &mut App) {
    let Some(dir) = logs_dir() else {
        return;
    };
    if let Err(error) = std::fs::create_dir_all(&dir) {
        tracing::warn!(%error, dir = %dir.display(), "could not create the log folder");
        return;
    }
    cx.reveal_path(&dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_count_is_the_number_of_panic_reports() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(report_count(dir.path()), 0);
        std::fs::write(dir.path().join("panic-1.txt"), "x").unwrap();
        std::fs::write(dir.path().join("wyck.2026-10-12.log"), "x").unwrap();
        assert_eq!(report_count(dir.path()), 1);
    }
}
