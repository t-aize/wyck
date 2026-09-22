//! # config
//!
//! Centralized application configuration and encrypted credential storage: the config
//! and secret handling shared by every part of `wyck` that needs to persist settings
//! or hold a broker/API token.
//!
//! ## Two kinds of state, kept apart on purpose
//!
//! - **[`AppConfig`]**: plaintext, human-editable, versioned TOML at
//!   [`AppPaths::config_file`]. Holds [`ProfileConfig`]s: a display name, a free-form
//!   `service` tag, and an optional endpoint URI. Safe to `cat`, back up, or sync
//!   between machines.
//! - **[`secret::SecretStore`]**: the token for each profile, held only in memory as a
//!   [`secrecy::SecretString`] (zeroized on drop, never printed by `Debug`) and
//!   persisted through a pluggable backend: [`secret::KeyringSecretStore`] (default:
//!   delegates to the OS credential manager) or [`secret::EncryptedFileSecretStore`]
//!   (fallback: ChaCha20-Poly1305 under an Argon2id-derived key, for environments with
//!   no OS keyring). **A token never appears in [`AppConfig`]'s TOML file**: only a
//!   [`secret::SecretKey`] derived from the profile's id does, and that key identifies
//!   *where* to look the token up, not the token itself.
//!
//! [`WyckConfig`] is the facade that ties the two together: load it once at startup,
//! then use it to list/add/remove profiles and fetch a profile's token when a client
//! (e.g. [`crate::openapi`]) needs one to connect.
//!
//! ## Example
//!
//! ```no_run
//! use secrecy::SecretString;
//! use wyck::config::{AppPaths, KeyringSecretStore, WyckConfig};
//!
//! # fn main() -> wyck::config::Result<()> {
//! let paths = AppPaths::discover()?;
//! let mut config = WyckConfig::load(paths, Box::new(KeyringSecretStore::default()))?;
//!
//! let id = config.add_profile(
//!     "Live: FTMO 100k",
//!     "ctrader-openapi",
//!     None,
//!     Some(SecretString::from("the-account-token".to_string())),
//! )?;
//! config.set_active_profile(Some(id.clone()))?;
//!
//! if let Some(token) = config.token_for(&id)? {
//!     // hand `token` to e.g. wyck::openapi::ClientCredentials
//! }
//! # Ok(())
//! # }
//! ```

mod app_config;
mod error;
mod fs_util;
mod paths;
mod profile;
pub mod secret;

pub use app_config::AppConfig;
pub use error::{ConfigError, Result};
pub use paths::AppPaths;
pub use profile::{ProfileConfig, ProfileId};
pub use secret::{EncryptedFileSecretStore, KeyringSecretStore, SecretKey, SecretStore};

use secrecy::ExposeSecret;
use secrecy::SecretString;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// One OAuth token pair kept under a single credential-store key.
#[derive(Debug, Clone)]
pub struct OpenApiTokens {
    /// The bearer token used on the connection.
    pub access_token: SecretString,
    /// Exchanged for a new access token once it expires.
    pub refresh_token: SecretString,
    /// When `access_token` expires, if known.
    pub expires_at: Option<SystemTime>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredOpenApiTokens {
    access_token: String,
    refresh_token: String,
    expires_at: Option<u64>,
}

/// The top-level entry point: [`AppPaths`] plus a loaded [`AppConfig`] plus a chosen
/// [`SecretStore`] backend, combined into the single type a front end actually imports
/// and holds for the lifetime of the app.
///
/// Every mutating method here (`add_profile`, `remove_profile`, `set_active_profile`)
/// persists [`AppConfig`] to disk before returning `Ok`, so the in-memory state and the
/// on-disk state never drift: a caller never needs to remember to call an explicit
/// `save` afterward.
pub struct WyckConfig {
    paths: AppPaths,
    app_config: AppConfig,
    secrets: Box<dyn SecretStore>,
}

impl WyckConfig {
    /// Loads (or, on first run, initializes) the app config at `paths`, paired with
    /// `secrets` as the credential backend for every profile this instance manages.
    ///
    /// # Errors
    ///
    /// Propagates [`AppConfig::load`]'s errors (I/O or parse failures on an existing,
    /// but corrupt or unreadable, config file).
    pub fn load(paths: AppPaths, secrets: Box<dyn SecretStore>) -> Result<Self> {
        let app_config = AppConfig::load(&paths)?;
        Ok(Self {
            paths,
            app_config,
            secrets,
        })
    }

    /// The resolved config/data directories this instance reads and writes.
    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    /// Every configured profile.
    pub fn profiles(&self) -> &[ProfileConfig] {
        &self.app_config.profiles
    }

    /// The currently active profile, if any is set and it still exists.
    pub fn active_profile(&self) -> Option<&ProfileConfig> {
        self.app_config.active_profile()
    }

    /// Looks up a profile by id.
    pub fn profile(&self, id: &ProfileId) -> Option<&ProfileConfig> {
        self.app_config.profile(id)
    }

    /// Adds a new profile: stores `token` (if given) under the profile's derived
    /// [`SecretKey`] (see [`SecretKey::for_profile`]) in the configured
    /// [`SecretStore`], appends the non-secret [`ProfileConfig`], and persists the
    /// updated [`AppConfig`] to disk.
    ///
    /// `token` is `None` for services that don't require one, or whose credentials are
    /// stored separately through [`Self::set_profile_secret`].
    ///
    /// If the secret-store write succeeds but the subsequent disk save fails, the
    /// stored credential is left in place (harmless: it's simply not yet referenced by
    /// any profile in the config) rather than attempting a rollback; retrying
    /// `add_profile` with the same inputs after fixing the save failure is always safe,
    /// since [`SecretStore::store`] overwrites rather than erroring on an existing key.
    ///
    /// # Errors
    ///
    /// Any error the [`SecretStore`] backend or [`AppConfig::save`] returns.
    pub fn add_profile(
        &mut self,
        display_name: impl Into<String>,
        service: impl Into<String>,
        endpoint: Option<String>,
        token: Option<SecretString>,
    ) -> Result<ProfileId> {
        let profile = ProfileConfig::new(display_name, service, endpoint);
        let id = profile.id.clone();

        if let Some(token) = &token {
            self.secrets.store(&SecretKey::for_profile(&id), token)?;
        }
        self.app_config.profiles.push(profile);
        self.app_config.save(&self.paths)?;

        Ok(id)
    }

    /// Removes a profile: deletes its stored credential, removes it from the config,
    /// clears `active_profile` if it pointed at this one, and persists the change.
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownProfile`] if `id` doesn't name a configured profile;
    /// otherwise any error the [`SecretStore`] backend or [`AppConfig::save`] returns.
    pub fn remove_profile(&mut self, id: &ProfileId) -> Result<()> {
        let before = self.app_config.profiles.len();
        self.app_config.profiles.retain(|profile| &profile.id != id);
        if self.app_config.profiles.len() == before {
            return Err(ConfigError::UnknownProfile(id.to_string()));
        }

        self.secrets.delete(&SecretKey::for_profile(id))?;
        for name in ["client-secret", "oauth-token-set"] {
            self.secrets.delete(&Self::profile_secret_key(id, name))?;
        }
        if self.app_config.active_profile.as_ref() == Some(id) {
            self.app_config.active_profile = None;
        }
        self.app_config.save(&self.paths)
    }

    /// Retrieves the token for `id`, or `Ok(None)` if none is stored (e.g. the profile
    /// was created without a token, or it was deleted from the credential store
    /// directly).
    pub fn token_for(&self, id: &ProfileId) -> Result<Option<SecretString>> {
        self.secrets.retrieve(&SecretKey::for_profile(id))
    }

    /// Stores a named credential for a profile, separate from its MCP bearer token.
    /// This is used for Open API client secrets and OAuth tokens.
    pub fn set_profile_secret(
        &self,
        id: &ProfileId,
        name: &str,
        secret: &SecretString,
    ) -> Result<()> {
        if self.profile(id).is_none() {
            return Err(ConfigError::UnknownProfile(id.to_string()));
        }
        self.secrets
            .store(&Self::profile_secret_key(id, name), secret)
    }

    /// Reads a named credential for a profile.
    pub fn profile_secret(&self, id: &ProfileId, name: &str) -> Result<Option<SecretString>> {
        if self.profile(id).is_none() {
            return Err(ConfigError::UnknownProfile(id.to_string()));
        }
        self.secrets.retrieve(&Self::profile_secret_key(id, name))
    }

    /// Saves an OAuth token pair with one credential-store write. A rotated refresh
    /// token must never be persisted separately from its matching access token.
    pub fn save_openapi_tokens(&self, id: &ProfileId, tokens: &OpenApiTokens) -> Result<()> {
        let record = StoredOpenApiTokens {
            access_token: tokens.access_token.expose_secret().to_owned(),
            refresh_token: tokens.refresh_token.expose_secret().to_owned(),
            expires_at: tokens.expires_at.and_then(|time| {
                time.duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|span| span.as_secs())
            }),
        };
        let encoded = serde_json::to_string(&record).map_err(|_| ConfigError::SecretStore {
            key: Self::profile_secret_key(id, "oauth-token-set").to_string(),
            message: "could not encode the OAuth token pair".into(),
        })?;
        self.set_profile_secret(id, "oauth-token-set", &SecretString::from(encoded))
    }

    /// Loads the OAuth token pair of a profile, if it has one.
    pub fn openapi_tokens(&self, id: &ProfileId) -> Result<Option<OpenApiTokens>> {
        let Some(secret) = self.profile_secret(id, "oauth-token-set")? else {
            return Ok(None);
        };
        let record: StoredOpenApiTokens =
            serde_json::from_str(secret.expose_secret()).map_err(|_| {
                ConfigError::MalformedEnvelope {
                    key: Self::profile_secret_key(id, "oauth-token-set").to_string(),
                    reason: "invalid OAuth token record".into(),
                }
            })?;
        Ok(Some(OpenApiTokens {
            access_token: SecretString::from(record.access_token),
            refresh_token: SecretString::from(record.refresh_token),
            expires_at: record
                .expires_at
                .map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds)),
        }))
    }

    fn profile_secret_key(id: &ProfileId, name: &str) -> SecretKey {
        SecretKey::new("profile-field", &format!("{}:{name}", id.as_str()))
    }

    /// Records public Open API settings for a profile after OAuth account selection.
    pub fn set_openapi_profile(
        &mut self,
        id: &ProfileId,
        client_id: String,
        callback_port: u16,
        account_id: i64,
    ) -> Result<()> {
        let profile = self
            .app_config
            .profiles
            .iter_mut()
            .find(|profile| &profile.id == id)
            .ok_or_else(|| ConfigError::UnknownProfile(id.to_string()))?;
        profile.client_id = Some(client_id);
        profile.callback_port = Some(callback_port);
        profile.account_id = Some(account_id);
        self.app_config.save(&self.paths)
    }

    /// Sets (or, with `None`, clears) the active profile, and persists the change.
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownProfile`] if `Some(id)` doesn't name a configured profile.
    pub fn set_active_profile(&mut self, id: Option<ProfileId>) -> Result<()> {
        if let Some(id) = &id
            && self.app_config.profile(id).is_none()
        {
            return Err(ConfigError::UnknownProfile(id.to_string()));
        }
        self.app_config.active_profile = id;
        self.app_config.save(&self.paths)
    }

    /// The symbol the user was last on, if one was remembered.
    pub fn last_symbol(&self) -> Option<&str> {
        self.app_config.last_symbol.as_deref()
    }

    /// Remembers (or with `None` forgets) the symbol the user is on, and persists the change. A
    /// blank name forgets it.
    pub fn set_last_symbol(&mut self, symbol: Option<String>) -> Result<()> {
        self.app_config.last_symbol = symbol
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        self.app_config.save(&self.paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secret::EncryptedFileSecretStore;

    fn config_in(dir: &std::path::Path) -> WyckConfig {
        let paths = AppPaths::at(dir);
        let secrets = Box::new(EncryptedFileSecretStore::new(
            paths.secrets_dir(),
            SecretString::from("test-passphrase".to_owned()),
        ));
        WyckConfig::load(paths, secrets).unwrap()
    }

    #[test]
    fn the_last_symbol_survives_a_restart_and_a_config_without_one_still_loads() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = config_in(temp_dir.path());
        assert_eq!(config.last_symbol(), None, "a first run remembers nothing");

        config
            .set_last_symbol(Some("  XAUUSD ".to_owned()))
            .unwrap();
        assert_eq!(config.last_symbol(), Some("XAUUSD"));
        drop(config);
        let mut again = config_in(temp_dir.path());
        assert_eq!(again.last_symbol(), Some("XAUUSD"));

        again.set_last_symbol(Some("   ".to_owned())).unwrap();
        assert_eq!(again.last_symbol(), None, "a blank name forgets it");
        assert!(
            !std::fs::read_to_string(AppPaths::at(temp_dir.path()).config_file())
                .unwrap()
                .contains("last_symbol"),
            "nothing is written for a forgotten symbol"
        );
    }

    #[test]
    fn add_profile_persists_both_the_config_entry_and_the_secret() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = config_in(temp_dir.path());

        let id = config
            .add_profile(
                "Demo",
                "ctrader-remote",
                None,
                Some(SecretString::from("token-123".to_owned())),
            )
            .unwrap();

        assert_eq!(config.profiles().len(), 1);
        assert_eq!(config.profile(&id).unwrap().display_name, "Demo");

        // Re-load fresh from disk to prove persistence, not just in-memory state.
        let reloaded = config_in(temp_dir.path());
        assert_eq!(reloaded.profiles().len(), 1);
        use secrecy::ExposeSecret;
        assert_eq!(
            reloaded.token_for(&id).unwrap().unwrap().expose_secret(),
            "token-123"
        );
    }

    #[test]
    fn add_profile_without_a_token_stores_no_secret() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = config_in(temp_dir.path());

        let id = config
            .add_profile("Local desktop", "ctrader-local", None, None)
            .unwrap();

        assert_eq!(config.profiles().len(), 1);
        assert!(config.token_for(&id).unwrap().is_none());
    }

    #[test]
    fn remove_profile_deletes_config_entry_secret_and_active_pointer() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = config_in(temp_dir.path());
        let id = config
            .add_profile(
                "Demo",
                "ctrader-remote",
                None,
                Some(SecretString::from("token".to_owned())),
            )
            .unwrap();
        config.set_active_profile(Some(id.clone())).unwrap();
        config
            .set_profile_secret(&id, "client-secret", &SecretString::from("app-secret"))
            .unwrap();
        config
            .set_profile_secret(&id, "oauth-token-set", &SecretString::from("refresh"))
            .unwrap();

        config.remove_profile(&id).unwrap();

        assert!(config.profiles().is_empty());
        assert!(config.active_profile().is_none());
        assert!(config.token_for(&id).unwrap().is_none());
        assert!(
            config
                .secrets
                .retrieve(&WyckConfig::profile_secret_key(&id, "client-secret"))
                .unwrap()
                .is_none()
        );
        assert!(
            config
                .secrets
                .retrieve(&WyckConfig::profile_secret_key(&id, "oauth-token-set"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn openapi_profile_settings_and_secrets_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = config_in(dir.path());
        let id = config
            .add_profile("Demo", "ctrader-openapi", None, None)
            .unwrap();
        config
            .set_openapi_profile(&id, "client-id".into(), 8765, 42)
            .unwrap();
        config
            .set_profile_secret(&id, "client-secret", &SecretString::from("secret"))
            .unwrap();
        config
            .save_openapi_tokens(
                &id,
                &OpenApiTokens {
                    access_token: SecretString::from("access"),
                    refresh_token: SecretString::from("refresh"),
                    expires_at: Some(UNIX_EPOCH + Duration::from_secs(1_800_000_000)),
                },
            )
            .unwrap();
        drop(config);

        let config = config_in(dir.path());
        let profile = config.profile(&id).unwrap();
        assert_eq!(profile.client_id.as_deref(), Some("client-id"));
        assert_eq!(profile.callback_port, Some(8765));
        assert_eq!(profile.account_id, Some(42));
        use secrecy::ExposeSecret;
        assert_eq!(
            config
                .profile_secret(&id, "client-secret")
                .unwrap()
                .unwrap()
                .expose_secret(),
            "secret"
        );
        let tokens = config.openapi_tokens(&id).unwrap().unwrap();
        assert_eq!(tokens.access_token.expose_secret(), "access");
        assert_eq!(tokens.refresh_token.expose_secret(), "refresh");
        assert_eq!(
            tokens.expires_at,
            Some(UNIX_EPOCH + Duration::from_secs(1_800_000_000))
        );
        assert!(
            !std::fs::read_to_string(config.paths().config_file())
                .unwrap()
                .contains("secret")
        );
    }

    #[test]
    fn remove_profile_rejects_unknown_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = config_in(temp_dir.path());

        let result = config.remove_profile(&ProfileId::new_random());

        assert!(matches!(result, Err(ConfigError::UnknownProfile(_))));
    }

    #[test]
    fn set_active_profile_rejects_unknown_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = config_in(temp_dir.path());

        let result = config.set_active_profile(Some(ProfileId::new_random()));

        assert!(matches!(result, Err(ConfigError::UnknownProfile(_))));
    }
}
