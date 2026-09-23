//! Shared atomic-write helper used by [`crate::config::AppConfig::save`] and
//! [`crate::config::secret::EncryptedFileSecretStore`].

use std::fs;
use std::path::Path;

use tracing::trace;

use crate::config::error::{ConfigError, Result};

/// Writes `contents` to `path` atomically: write to a uniquely-named temp file in the
/// same directory, then rename over the target. Rename-over-existing-file is atomic on
/// the same filesystem on every platform this crate targets, so a crash or power loss
/// mid-write can never leave a half-written config or secret-envelope file behind:
/// readers only ever see the old complete file or the new complete file, never a
/// partial one.
///
/// On Unix, the temp file is created with `0600` permissions (owner read/write only)
/// before any content is written, since every caller of this function writes either a
/// [`crate::config::SecretKey`] reference (in the config file) or ciphertext (in an encrypted
/// envelope): neither should be world-readable.
pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let dir = path.parent().ok_or_else(|| ConfigError::Write {
        path: path.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path has no parent directory",
        ),
    })?;
    fs::create_dir_all(dir).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("wyck-config");
    let tmp_path = dir.join(format!(".{file_name}.tmp-{}", uuid::Uuid::new_v4()));

    write_with_restricted_permissions(&tmp_path, contents).map_err(|source| {
        ConfigError::Write {
            path: tmp_path.clone(),
            source,
        }
    })?;

    fs::rename(&tmp_path, path).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    trace!(path = %path.display(), bytes = contents.len(), "wrote a file atomically");
    Ok(())
}

#[cfg(unix)]
fn write_with_restricted_permissions(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents)
}

// Windows has no direct equivalent of Unix mode bits; the file inherits the parent
// directory's ACLs, which for `AppPaths::discover`'s per-user AppData location already
// restrict access to the owning user account. Callers on Windows who need a stronger
// guarantee than that should prefer `KeyringSecretStore` (Windows Credential Manager)
// over `EncryptedFileSecretStore` for anything secret-shaped.
#[cfg(not(unix))]
fn write_with_restricted_permissions(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    fs::write(path, contents)
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
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count();
        assert_eq!(leftover_temp_files, 0);
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
