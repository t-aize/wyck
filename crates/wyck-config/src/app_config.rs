//! The non-secret application config: which profiles exist and which one is active.

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::error::{ConfigError, Result};
use crate::fs_util::atomic_write;
use crate::paths::AppPaths;
use crate::profile::{ProfileConfig, ProfileId};

const CURRENT_SCHEMA_VERSION: u32 = 1;

/// The plaintext, human-editable part of `wyck`'s configuration: which
/// [`ProfileConfig`]s exist and which one is active. Never contains a token: see
/// [`crate::secret`] for where those live instead.
///
/// Round-trips through TOML at [`crate::AppPaths::config_file`]. `schema_version` is
/// bumped whenever a breaking change to this shape ships, so a future version of this
/// crate can detect and migrate an older config file instead of failing to parse it
/// silently wrong.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    /// Bumped whenever a breaking change to this shape ships, to detect and migrate an
    /// older config file instead of failing to parse it silently wrong.
    #[serde(default = "current_schema_version")]
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

fn current_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
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
    /// shape.
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
        let config = toml::from_str(&text).map_err(|source| {
            warn!(path = %path.display(), error = %source, "the config file could not be parsed");
            ConfigError::Parse {
                path: path.clone(),
                source: Box::new(source),
            }
        })?;
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
        let profile = ProfileConfig::new(
            "Demo",
            "ctrader-remote",
            Some("https://mcp.ctrader.com/trading/mcp".to_owned()),
        );
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
}
