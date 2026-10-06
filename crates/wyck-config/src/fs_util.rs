//! The atomic, durable write every file of the app goes through.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use tracing::trace;

use crate::error::{ConfigError, Result};

/// The marker in the name of a file that is being written, before it takes the place of the real
/// one: `.{name}.tmp-{random}`.
const TEMP_MARKER: &str = ".tmp-";

/// Writes `contents` to `path` so that a reader, or the next start after a crash or a power cut,
/// finds either the old complete file or the new complete file, never half of one.
///
/// The steps are the ones a database takes:
///
/// 1. The parent directory is created when it is missing (readable by the owner only on Unix).
/// 2. The contents go to a uniquely named temporary file in the same directory, created with `0600`
///    permissions on Unix before anything is written to it, since some callers write ciphertext
///    or references to secrets.
/// 3. The temporary file is flushed to the disk (`fsync`), not only to the cache of the system:
///    without it, a rename that survives a crash can point at a file with no contents.
/// 4. It is renamed over the target, which is atomic on the same file system on every platform
///    this crate targets.
/// 5. The directory is flushed too (Unix), so the rename itself survives a power cut.
///
/// A failure removes the temporary file and leaves the target as it was. Every file the app
/// writes goes through this function. A caller that returns `io::Result` can use `?` on it
/// directly (see the `From<ConfigError>` impl of `std::io::Error`).
///
/// # Errors
///
/// [`ConfigError::Write`], with the path.
pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    write_atomically(path, contents).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn write_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path has no parent directory",
        )
    })?;
    create_private_dir_all(dir)?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("wyck");
    let tmp_path = dir.join(format!(".{file_name}{TEMP_MARKER}{}", uuid::Uuid::new_v4()));

    write_synced(&tmp_path, contents)
        .and_then(|()| fs::rename(&tmp_path, path))
        .inspect_err(|_| {
            let _ = fs::remove_file(&tmp_path);
        })?;
    sync_dir(dir);
    trace!(path = %path.display(), bytes = contents.len(), "wrote a file atomically");
    Ok(())
}

/// The temporary files [`atomic_write`] left in `dir` because the program stopped in the
/// middle of a write. They are harmless (nothing reads them) and safe to delete.
#[must_use]
pub fn stale_temp_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with('.') && name.contains(TEMP_MARKER)
        })
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
}

/// Creates `dir` and the folders above it that are missing. On Unix the ones created are for the
/// owner only (`0700`); the ones that exist keep their permissions.
pub(crate) fn create_private_dir_all(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir)
    }
}

#[cfg(unix)]
fn write_synced(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents)?;
    file.sync_all()
}

// Windows has no direct equivalent of Unix mode bits; the file inherits the ACLs of the parent
// directory, which for `AppPaths::discover`'s per-user AppData location already restrict access
// to the owning user account. Callers who need a stronger guarantee than that should prefer
// `KeyringSecretStore` (Windows Credential Manager) over `EncryptedFileSecretStore`.
#[cfg(not(unix))]
fn write_synced(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(contents)?;
    file.sync_all()
}

/// Flushes a directory so a rename in it survives a power cut. Best effort: a directory cannot be
/// opened for this on Windows, and a failure here does not undo a write that succeeded.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = fs::File::open(dir) {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_parent_dirs_and_writes_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("nested").join("file.toml");

        atomic_write(&path, b"hello = 1\n").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello = 1\n");
    }

    #[test]
    fn atomic_write_overwrites_existing_content_and_leaves_no_temp_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("file.toml");

        atomic_write(&path, b"first\n").unwrap();
        atomic_write(&path, b"second\n").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second\n");
        let leftover_temp_files = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count();
        assert_eq!(leftover_temp_files, 0);
    }

    #[test]
    fn a_failed_write_leaves_the_old_file_and_no_temporary_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("kept.toml");
        atomic_write(&path, b"old\n").unwrap();
        // The target is a directory now: the rename cannot replace it.
        let blocked = temp_dir.path().join("blocked");
        fs::create_dir(&blocked).unwrap();

        assert!(atomic_write(&blocked, b"new\n").is_err());

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old\n");
        assert!(stale_temp_files(temp_dir.path()).is_empty());
    }

    #[test]
    fn a_temporary_file_left_by_a_crash_is_found_and_ignored_by_readers() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("config.toml");
        atomic_write(&path, b"real\n").unwrap();
        let leftover = temp_dir.path().join(".config.toml.tmp-1234");
        fs::write(&leftover, b"half a wri").unwrap();

        assert_eq!(stale_temp_files(temp_dir.path()), vec![leftover]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "real\n");
    }

    #[test]
    fn writers_racing_on_one_file_never_leave_it_torn() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = std::sync::Arc::new(temp_dir.path().join("race.toml"));
        let threads: Vec<_> = (0..8)
            .map(|n| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for round in 0..25 {
                        let text = format!("writer = {n}\nround = {round}\n{}\n", "x".repeat(4096));
                        atomic_write(&path, text.as_bytes()).unwrap();
                        let seen = std::fs::read_to_string(&*path).unwrap();
                        assert!(
                            seen.starts_with("writer = ") && seen.ends_with("x\n"),
                            "torn: {}",
                            &seen[..seen.len().min(40)]
                        );
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert!(stale_temp_files(temp_dir.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn folders_made_by_a_write_are_for_the_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("a").join("b").join("file.toml");
        atomic_write(&path, b"x").unwrap();

        for dir in [
            path.parent().unwrap(),
            path.parent().unwrap().parent().unwrap(),
        ] {
            let mode = std::fs::metadata(dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{}", dir.display());
        }
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_restricts_permissions_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("secret.toml");
        atomic_write(&path, b"secret\n").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
