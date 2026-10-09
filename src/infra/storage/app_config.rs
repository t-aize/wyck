//! The non-secret application config: which profiles exist and which one is active.

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::infra::storage::error::{ConfigError, Result};
use crate::infra::storage::fs_util::atomic_write;
use crate::infra::storage::paths::AppPaths;
use crate::infra::storage::profile::{ProfileConfig, ProfileId};

/// The version of the layout this build writes, and the newest it reads.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// The plaintext, human-editable part of `wyck`'s configuration: which
/// [`ProfileConfig`]s exist and which one is active. Never contains a token: see
/// [`crate::infra::storage::secret`] for where those live instead.
///
/// Round-trips through TOML at [`crate::infra::storage::AppPaths::config_file`]. `schema_version` is required
/// and is bumped whenever a breaking change to this shape ships; a file with a version this build
/// does not know is refused (see [`AppConfig::load`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    /// The version of this layout. Required: a file without it is not a config file of this app.
    pub schema_version: u32,
    /// The profile to connect with by default, if one is set and still exists.
    #[serde(default)]
    pub active_profile: Option<ProfileId>,
    /// Every configured profile.
    #[serde(default)]
    pub profiles: Vec<ProfileConfig>,
    /// The symbol the user was on when the application last ran, so the next start opens on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_symbol: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            active_profile: None,
            profiles: Vec::new(),
            last_symbol: None,
        }
    }
}

impl AppConfig {
    /// Loads the config from `paths.config_file()`. Returns [`AppConfig::default`]
    /// (empty, no profiles) if the file doesn't exist yet: a fresh install is not an
    /// error condition.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Read`] on any I/O failure other than "file not found";
    /// [`ConfigError::Parse`] if the file exists but isn't valid TOML matching this
    /// shape; [`ConfigError::UnsupportedSchema`] if it was written by a newer version.
    pub fn load(paths: &AppPaths) -> Result<Self> {
        let path = paths.config_file();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                debug!(path = %path.display(), "no config file yet, starting from defaults");
                return Ok(Self::default());
            }
            Err(source) => {
                warn!(path = %path.display(), error = %source, "could not read the config file");
                return Err(ConfigError::Read { path, source });
            }
        };
        let config: Self = toml::from_str(&text).map_err(|source| {
            warn!(path = %path.display(), error = %source, "the config file could not be parsed");
            ConfigError::Parse {
                path: path.clone(),
                source: Box::new(source),
            }
        })?;
        // A file from the future is left alone: reading it with what this version knows and
        // writing it back would drop what this version does not.
        if config.schema_version > CURRENT_SCHEMA_VERSION {
            warn!(
                path = %path.display(),
                found = config.schema_version,
                supported = CURRENT_SCHEMA_VERSION,
                "the config file was written by a newer version"
            );
            return Err(ConfigError::UnsupportedSchema {
                path,
                found: config.schema_version,
                supported: CURRENT_SCHEMA_VERSION,
            });
        }
        debug!(path = %path.display(), "loaded the config file");
        Ok(config)
    }

    /// Serializes and writes the config to `paths.config_file()`, atomically (see
    /// `atomic_write`).
    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        let toml_text = toml::to_string_pretty(self).map_err(ConfigError::Serialize)?;
        let path = paths.config_file();
        atomic_write(&path, toml_text.as_bytes())?;
        debug!(
            path = %path.display(),
            profiles = self.profiles.len(),
            "saved the config file"
        );
        Ok(())
    }

    /// Looks up a profile by id.
    pub fn profile(&self, id: &ProfileId) -> Option<&ProfileConfig> {
        self.profiles.iter().find(|profile| &profile.id == id)
    }

    /// The active profile, resolved from `active_profile`, if any is set and it still
    /// exists in `profiles`.
    pub fn active_profile(&self) -> Option<&ProfileConfig> {
        self.active_profile.as_ref().and_then(|id| self.profile(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_default_when_no_file_exists_yet() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(temp_dir.path());

        let config = AppConfig::load(&paths).unwrap();

        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(temp_dir.path());
        let mut config = AppConfig::default();
        let profile = ProfileConfig::new("Demo", "ctrader-openapi");
        config.active_profile = Some(profile.id.clone());
        config.profiles.push(profile);

        config.save(&paths).unwrap();
        let reloaded = AppConfig::load(&paths).unwrap();

        assert_eq!(reloaded, config);
    }

    #[test]
    fn active_profile_resolves_to_none_if_the_id_was_removed() {
        let config = AppConfig {
            active_profile: Some(ProfileId::new_random()),
            ..AppConfig::default()
        };

        assert!(config.active_profile().is_none());
    }

    #[test]
    fn rejects_malformed_toml_with_a_parse_error() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(temp_dir.path());
        std::fs::write(paths.config_file(), b"not = [valid").unwrap();

        let result = AppConfig::load(&paths);

        assert!(matches!(result, Err(ConfigError::Parse { .. })));
    }

    #[test]
    fn a_config_from_a_newer_version_is_refused_and_left_untouched() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(temp_dir.path());
        let future = "schema_version = 99\nfrom_the_future = true\n";
        std::fs::write(paths.config_file(), future).unwrap();

        let result = AppConfig::load(&paths);

        assert!(matches!(
            result,
            Err(ConfigError::UnsupportedSchema {
                found: 99,
                supported: 1,
                ..
            })
        ));
        assert_eq!(
            std::fs::read_to_string(paths.config_file()).unwrap(),
            future
        );
    }

    #[test]
    fn a_config_without_a_version_is_refused() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(temp_dir.path());
        std::fs::write(paths.config_file(), "last_symbol = \"EURUSD\"\n").unwrap();
        assert!(matches!(
            AppConfig::load(&paths),
            Err(ConfigError::Parse { .. })
        ));
    }
}
