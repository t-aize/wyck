//! OS-standard config/data directory resolution.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use tracing::debug;

use crate::error::{ConfigError, Result};

/// The environment variable that moves the config directory (and the data directory too, unless
/// [`DATA_DIR_ENV`] says otherwise): for a portable install, a test, or a second profile of the
/// app on the same machine.
pub const CONFIG_DIR_ENV: &str = "WYCK_CONFIG_DIR";
/// The environment variable that moves the data directory alone.
pub const DATA_DIR_ENV: &str = "WYCK_DATA_DIR";

const QUALIFIER: &str = "sh";
const ORGANIZATION: &str = "wyck";
const APPLICATION: &str = "wyck";

/// The directories this crate reads and writes: a config directory for
/// [`crate::AppConfig`]'s TOML file, and a data directory for anything a
/// [`crate::secret::SecretStore`] backend needs to persist on disk (currently only
/// [`crate::secret::EncryptedFileSecretStore`]: the default
/// [`crate::secret::KeyringSecretStore`] backend stores nothing here, the OS credential
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
    /// Resolves the config and data directories for `wyck`: the ones the environment names
    /// ([`CONFIG_DIR_ENV`], [`DATA_DIR_ENV`]) if it names any, the OS-standard ones otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::NoProjectDirs`] if the environment names nothing and the OS
    /// reports no home directory for the current user (e.g. running as a system service account
    /// on some platforms): see [`directories::ProjectDirs::from`].
    pub fn discover() -> Result<Self> {
        Self::discover_with(|name| std::env::var_os(name))
    }

    /// [`Self::discover`], reading the environment through `var` (so a test does not have to
    /// change the real one). A variable that is empty counts as not set.
    ///
    /// # Errors
    ///
    /// As [`Self::discover`].
    pub fn discover_with(var: impl Fn(&str) -> Option<OsString>) -> Result<Self> {
        let set = |name: &str| {
            var(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        let (config_override, data_override) = (set(CONFIG_DIR_ENV), set(DATA_DIR_ENV));
        let paths = match (config_override, data_override) {
            (Some(config_dir), data_dir) => Self {
                data_dir: data_dir.unwrap_or_else(|| config_dir.clone()),
                config_dir,
            },
            (None, data_dir) => {
                let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
                    .ok_or(ConfigError::NoProjectDirs)?;
                Self {
                    config_dir: dirs.config_dir().to_path_buf(),
                    data_dir: data_dir.unwrap_or_else(|| dirs.data_dir().to_path_buf()),
                }
            }
        };
        debug!(
            config_dir = %paths.config_dir.display(),
            data_dir = %paths.data_dir.display(),
            "resolved the config/data directories"
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

    /// The directory [`crate::AppConfig`]'s TOML file lives in.
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// The full path to the config TOML file.
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// The directory an on-disk [`crate::secret::SecretStore`] backend may persist
    /// files in.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// The subdirectory [`crate::secret::EncryptedFileSecretStore`] persists its
    /// per-secret envelope files in.
    pub fn secrets_dir(&self) -> PathBuf {
        self.data_dir.join("secrets")
    }

    /// The folder of the documents shared by every account (see [`crate::DocumentStore::global`]).
    pub fn state_dir(&self) -> PathBuf {
        self.config_dir.join("state")
    }

    /// The folder that holds one folder of documents per scope (see
    /// [`crate::DocumentStore::scoped`]).
    pub fn scopes_dir(&self) -> PathBuf {
        self.config_dir.join("scopes")
    }

    /// The folder the scripted indicators are read from unless the user chose another.
    pub fn indicators_dir(&self) -> PathBuf {
        self.config_dir.join("indicators")
    }

    /// The user's pictures folder, or the home folder when the system has none: where the app puts
    /// a picture it saves. `None` only when the system knows no home folder either.
    pub fn pictures_dir() -> Option<PathBuf> {
        let dirs = directories::UserDirs::new()?;
        Some(
            dirs.picture_dir()
                .unwrap_or_else(|| dirs.home_dir())
                .to_path_buf(),
        )
    }

    /// The user's documents folder, where a file dialog starts: the temporary folder when the
    /// system has none.
    pub fn documents_dir() -> PathBuf {
        directories::UserDirs::new()
            .and_then(|dirs| dirs.document_dir().map(Path::to_path_buf))
            .unwrap_or_else(std::env::temp_dir)
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
        assert_eq!(paths.state_dir(), Path::new("/tmp/example/state"));
        assert_eq!(paths.scopes_dir(), Path::new("/tmp/example/scopes"));
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
    fn the_environment_moves_the_directories() {
        let paths = AppPaths::discover_with(env(&[(CONFIG_DIR_ENV, "/portable/cfg")])).unwrap();
        assert_eq!(paths.config_dir(), Path::new("/portable/cfg"));
        assert_eq!(
            paths.data_dir(),
            Path::new("/portable/cfg"),
            "data follows config"
        );

        let split = AppPaths::discover_with(env(&[
            (CONFIG_DIR_ENV, "/portable/cfg"),
            (DATA_DIR_ENV, "/portable/data"),
        ]))
        .unwrap();
        assert_eq!(split.config_dir(), Path::new("/portable/cfg"));
        assert_eq!(split.data_dir(), Path::new("/portable/data"));
        assert_eq!(split.secrets_dir(), Path::new("/portable/data/secrets"));
    }

    #[test]
    fn an_empty_variable_is_the_same_as_none() {
        let with_empty = AppPaths::discover_with(env(&[(CONFIG_DIR_ENV, "")]));
        let without = AppPaths::discover_with(env(&[]));
        match (with_empty, without) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.config_dir(), b.config_dir());
                assert_eq!(a.data_dir(), b.data_dir());
            }
            (Err(_), Err(_)) => {} // a machine with no home folder
            other => panic!("an empty variable changed the outcome: {other:?}"),
        }
    }

    #[test]
    fn the_os_directories_are_used_when_nothing_is_set() {
        if let Ok(paths) = AppPaths::discover_with(env(&[])) {
            assert!(
                paths.config_dir().ends_with("wyck")
                    || paths.config_dir().to_string_lossy().contains("wyck")
            );
        }
    }
}
