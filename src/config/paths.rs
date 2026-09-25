//! OS-standard config/data directory resolution.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use tracing::debug;

use crate::config::error::{ConfigError, Result};

const QUALIFIER: &str = "sh";
const ORGANIZATION: &str = "wyck";
const APPLICATION: &str = "wyck";

/// The directories [`crate::config`] reads and writes: a config directory for
/// [`crate::config::AppConfig`]'s TOML file, and a data directory for anything a
/// [`crate::config::secret::SecretStore`] backend needs to persist on disk (currently only
/// [`crate::config::secret::EncryptedFileSecretStore`]: the default
/// [`crate::config::secret::KeyringSecretStore`] backend stores nothing here, the OS credential
/// store owns that).
///
/// On a real install, resolve via [`Self::discover`], which asks the OS for its
/// standard per-user application-data locations (`%APPDATA%\wyck` on Windows,
/// `~/Library/Application Support/sh.wyck.wyck` on macOS, `~/.config/wyck` on Linux, via
/// the `directories` crate). For tests, or a caller that wants a portable/overridden
/// location, [`Self::at`] points both directories at an arbitrary path instead.
#[derive(Debug, Clone)]
pub struct AppPaths {
    config_dir: PathBuf,
    data_dir: PathBuf,
}

impl AppPaths {
    /// Resolves the OS-standard config and data directories for `wyck`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::NoProjectDirs`] if the OS reports no home directory for
    /// the current user (e.g. running as a system service account on some platforms):
    /// see [`directories::ProjectDirs::from`].
    pub fn discover() -> Result<Self> {
        let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
            .ok_or(ConfigError::NoProjectDirs)?;
        let paths = Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
        };
        debug!(
            config_dir = %paths.config_dir.display(),
            data_dir = %paths.data_dir.display(),
            "resolved the OS-standard config/data directories"
        );
        Ok(paths)
    }

    /// Points both the config and data directories at `dir`. Intended for tests and for
    /// callers that want a portable install (e.g. config alongside the executable)
    /// instead of the OS-standard per-user location.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        Self {
            config_dir: dir.clone(),
            data_dir: dir,
        }
    }

    /// The directory [`crate::config::AppConfig`]'s TOML file lives in.
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// The full path to the config TOML file.
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// The directory an on-disk [`crate::config::secret::SecretStore`] backend may persist
    /// files in.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// The subdirectory [`crate::config::secret::EncryptedFileSecretStore`] persists its
    /// per-secret envelope files in.
    pub fn secrets_dir(&self) -> PathBuf {
        self.data_dir.join("secrets")
    }

    /// The folder the scripted indicators are read from unless the user chose another.
    pub fn indicators_dir(&self) -> PathBuf {
        self.config_dir.join("indicators")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_uses_the_same_directory_for_config_and_data() {
        let paths = AppPaths::at("/tmp/example");
        assert_eq!(paths.config_dir(), Path::new("/tmp/example"));
        assert_eq!(paths.data_dir(), Path::new("/tmp/example"));
        assert_eq!(paths.config_file(), Path::new("/tmp/example/config.toml"));
        assert_eq!(paths.secrets_dir(), Path::new("/tmp/example/secrets"));
        assert_eq!(paths.indicators_dir(), Path::new("/tmp/example/indicators"));
    }
}
