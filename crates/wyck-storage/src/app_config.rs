//! The non-secret application config: which profiles exist and which one is active.

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::error::{ConfigError, Result};
use crate::fs_util::{atomic_write, read_text};
use crate::migrate::{self, Plan};
use crate::paths::AppPaths;
use crate::profile::{ProfileConfig, ProfileId};

/// The version of the layout this build writes, and the newest it reads.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// How an older `config.toml` is brought to [`CURRENT_SCHEMA_VERSION`]. Raising the version means
/// adding a step here (`Step::unchanged` for a field that only gained a default); a test fails
/// until it is done.
const PLAN: Plan = Plan {
    document: "config",
    current: CURRENT_SCHEMA_VERSION,
    steps: &[],
};

/// Just the version of a config file, read before the rest.
#[derive(Deserialize)]
struct Head {
    schema_version: u32,
}

/// The plaintext, human-editable part of `wyck`'s configuration: which
/// [`ProfileConfig`]s exist and which one is active. Never contains a token: see
/// [`crate::secret`] for where those live instead.
///
/// Round-trips through TOML at [`crate::AppPaths::config_file`]. `schema_version` is required
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
    ///
    /// A file in an older layout is upgraded first (see [`crate::migrate`]): the
    /// old text stays next to it as `config.toml.v<old>.bak`.
    pub fn load(paths: &AppPaths) -> Result<Self> {
        Self::load_with(paths, &PLAN)
    }

    fn load_with(paths: &AppPaths, plan: &Plan) -> Result<Self> {
        let path = paths.config_file();
        let mut text = match read_text(&path) {
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
        let parse_error = |source| {
            warn!(path = %path.display(), error = %source, "the config file could not be parsed");
            ConfigError::Parse {
                path: path.clone(),
                source: Box::new(source),
            }
        };
        // The version is read first, on its own: a file of another layout may not fit the type.
        let head: Head = toml::from_str(&text).map_err(parse_error)?;
        // A file from the future is left alone: reading it with what this version knows and
        // writing it back would drop what this version does not.
        if head.schema_version > plan.current {
            warn!(
                path = %path.display(),
                found = head.schema_version,
                supported = plan.current,
                "the config file was written by a newer version"
            );
            return Err(ConfigError::UnsupportedSchema {
                path,
                found: head.schema_version,
                supported: plan.current,
            });
        }
        if head.schema_version < plan.current {
            migrate::upgrade_file(&path, plan)?;
            text = read_text(&path).map_err(|source| ConfigError::Read {
                path: path.clone(),
                source,
            })?;
        }
        let config: Self = toml::from_str(&text).map_err(parse_error)?;
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
    fn the_plan_reaches_the_version_this_build_writes() {
        assert_eq!(PLAN.current, CURRENT_SCHEMA_VERSION);
        assert!(
            PLAN.is_complete(),
            "the version was raised: add a step to PLAN (Step::unchanged if only a default is new)"
        );
    }

    fn rename_symbol(table: &mut toml::Table) -> std::result::Result<(), migrate::StepError> {
        if let Some(symbol) = table.remove("symbol") {
            table.insert("last_symbol".to_owned(), symbol);
        }
        Ok(())
    }

    const NEXT: Plan = Plan {
        document: "config",
        current: 2,
        steps: &[migrate::Step {
            from: 1,
            apply: rename_symbol,
        }],
    };

    #[test]
    fn an_older_config_is_upgraded_on_load_and_the_old_text_is_kept() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(temp_dir.path());
        let old = "schema_version = 1\nsymbol = \"EURUSD\"\n";
        std::fs::write(paths.config_file(), old).unwrap();

        let config = AppConfig::load_with(&paths, &NEXT).unwrap();

        assert_eq!(config.last_symbol.as_deref(), Some("EURUSD"));
        assert_eq!(config.schema_version, 2);
        assert!(
            std::fs::read_to_string(paths.config_file())
                .unwrap()
                .contains("schema_version = 2"),
            "the file on disk is upgraded too"
        );
        let kept = temp_dir.path().join("config.toml.v1.bak");
        assert_eq!(std::fs::read_to_string(kept).unwrap(), old);

        // The second start finds nothing to do.
        let again = AppConfig::load_with(&paths, &NEXT).unwrap();
        assert_eq!(again, config);
    }

    #[test]
    fn an_upgraded_config_that_still_does_not_fit_is_refused_and_the_old_text_is_kept() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(temp_dir.path());
        let old = "schema_version = 1\nactive_profile = 7\n";
        std::fs::write(paths.config_file(), old).unwrap();

        let result = AppConfig::load_with(&paths, &NEXT);

        assert!(
            matches!(result, Err(ConfigError::Parse { .. })),
            "{result:?}"
        );
        let kept = temp_dir.path().join("config.toml.v1.bak");
        assert_eq!(std::fs::read_to_string(kept).unwrap(), old);
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
