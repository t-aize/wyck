//! The profiles of the app and the plain config file that lists them.

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::config::error::{ConfigError, Result};
use crate::config::fs_util::atomic_write;
use crate::config::paths::AppPaths;

/// A stable, opaque identifier for one `ProfileConfig`, generated once when the profile is
/// created and never reused.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(String);

impl ProfileId {
    /// Generates a new, statistically-unique profile id (UUID v4).
    pub fn new_random() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// The raw id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A configured connection profile: public connection settings, never secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub id: ProfileId,
    pub display_name: String,
    pub service: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<i64>,
}

impl ProfileConfig {
    /// Creates a new profile with a freshly generated `ProfileId`.
    pub fn new(display_name: impl Into<String>, service: impl Into<String>) -> Self {
        Self {
            id: ProfileId::new_random(),
            display_name: display_name.into(),
            service: service.into(),
            client_id: None,
            callback_port: None,
            account_id: None,
        }
    }
}

/// The plain, human-editable part of the config: which profiles exist and which one is active.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub active_profile: Option<ProfileId>,
    #[serde(default)]
    pub profiles: Vec<ProfileConfig>,
}

impl AppConfig {
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
        toml::from_str(&text).map_err(|source| {
            warn!(path = %path.display(), error = %source, "the config file could not be parsed");
            ConfigError::Parse {
                path,
                source: Box::new(source),
            }
        })
    }

    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        let text = toml::to_string_pretty(self).map_err(ConfigError::Serialize)?;
        atomic_write(&paths.config_file(), text.as_bytes())
    }

    pub fn profile(&self, id: &ProfileId) -> Option<&ProfileConfig> {
        self.profiles.iter().find(|profile| &profile.id == id)
    }

    /// The active profile, if one is set and it still exists.
    pub fn active_profile(&self) -> Option<&ProfileConfig> {
        self.active_profile.as_ref().and_then(|id| self.profile(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_ids_are_unique() {
        let a = ProfileId::new_random();
        let b = ProfileId::new_random();
        assert_ne!(a, b);
    }

    #[test]
    fn profile_config_round_trips_through_toml() {
        let mut profile = ProfileConfig::new("Live: FTMO 100k", "ctrader-openapi");
        profile.client_id = Some("public-client-id".to_owned());
        profile.callback_port = Some(52123);
        profile.account_id = Some(12_345_678);
        let toml_text = toml::to_string(&profile).unwrap();
        let parsed: ProfileConfig = toml::from_str(&toml_text).unwrap();
        assert_eq!(profile, parsed);
    }

    #[test]
    fn settings_a_profile_does_not_use_are_left_out_of_the_file() {
        let profile = ProfileConfig::new("Minimal profile", "some-service");
        let toml_text = toml::to_string(&profile).unwrap();
        let value: toml::Value = toml::from_str(&toml_text).unwrap();
        for key in ["client_id", "callback_port", "account_id"] {
            assert!(value.get(key).is_none(), "`{key}` in:\n{toml_text}");
        }
    }

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
}
