//! Profiles, documents and credential storage (OS keyring or encrypted files).

mod documents;
mod error;
mod fs_util;
mod paths;
mod profile;
mod secrets;
mod tokens;

pub use documents::DocumentStore;
pub use error::{ConfigError, Result};
pub use fs_util::stale_temp_files;
pub use paths::{AppPaths, CONFIG_DIR_ENV, DATA_DIR_ENV};
pub use profile::{AppConfig, ProfileConfig, ProfileId};
pub use secrets::{EncryptedFileSecretStore, KeyringSecretStore, SecretKey, SecretStore};
pub use tokens::{OpenApiTokenStorage, OpenApiTokens};

use std::sync::Arc;

use secrecy::SecretString;
use tracing::{debug, info, warn};

/// The name under which a profile's Open API application secret is stored.
pub const CLIENT_SECRET: &str = "client-secret";
const OAUTH_TOKENS: &str = "oauth-token-set";

fn profile_secret_key(id: &ProfileId, name: &str) -> SecretKey {
    SecretKey::new("profile-field", &format!("{}:{name}", id.as_str()))
}

/// The folders, the profiles and the credential store, held for the life of the app.
pub struct WyckConfig {
    paths: AppPaths,
    app_config: AppConfig,
    secrets: Arc<dyn SecretStore>,
}

impl WyckConfig {
    /// Loads the config at `paths`. Credentials go to the OS keyring, or to encrypted files
    /// locked by `passphrase` when one is given.
    pub fn open(paths: AppPaths, passphrase: Option<SecretString>) -> Result<Self> {
        let secrets: Box<dyn SecretStore> = match passphrase {
            Some(passphrase) => Box::new(EncryptedFileSecretStore::new(
                paths.secrets_dir(),
                passphrase,
            )),
            None => Box::new(KeyringSecretStore::default()),
        };
        Self::load(paths, secrets)
    }

    /// Loads (or, on first run, initializes) the config at `paths` with the given credential store.
    pub fn load(paths: AppPaths, secrets: Box<dyn SecretStore>) -> Result<Self> {
        let app_config = AppConfig::load(&paths)?;
        info!(
            profiles = app_config.profiles.len(),
            active = app_config.active_profile.is_some(),
            "wyck config loaded"
        );
        Ok(Self {
            paths,
            app_config,
            secrets: Arc::from(secrets),
        })
    }

    /// Applies `change` to a copy of the config, saves the copy, and only then keeps it.
    fn commit<T>(&mut self, change: impl FnOnce(&mut AppConfig) -> Result<T>) -> Result<T> {
        let mut next = self.app_config.clone();
        let result = change(&mut next)?;
        next.save(&self.paths)?;
        self.app_config = next;
        Ok(result)
    }

    fn require_profile(&self, id: &ProfileId) -> Result<()> {
        if self.app_config.profile(id).is_some() {
            Ok(())
        } else {
            Err(ConfigError::UnknownProfile(id.to_string()))
        }
    }

    pub fn profiles(&self) -> &[ProfileConfig] {
        &self.app_config.profiles
    }

    pub fn active_profile(&self) -> Option<&ProfileConfig> {
        self.app_config.active_profile()
    }

    /// Adds a new, empty profile and saves the config.
    pub fn add_profile(
        &mut self,
        display_name: impl Into<String>,
        service: impl Into<String>,
    ) -> Result<ProfileId> {
        let profile = ProfileConfig::new(display_name, service);
        let id = profile.id.clone();
        self.commit(|config| {
            config.profiles.push(profile);
            Ok(())
        })?;
        info!(%id, "added a profile");
        Ok(id)
    }

    /// Deletes the profile's stored credentials, then removes it from the config.
    pub fn remove_profile(&mut self, id: &ProfileId) -> Result<()> {
        self.require_profile(id)?;
        for name in [CLIENT_SECRET, OAUTH_TOKENS] {
            self.secrets.delete(&profile_secret_key(id, name))?;
        }
        self.commit(|config| {
            config.profiles.retain(|profile| &profile.id != id);
            if config.active_profile.as_ref() == Some(id) {
                config.active_profile = None;
            }
            Ok(())
        })?;
        info!(%id, "removed a profile");
        Ok(())
    }

    /// Stores a named credential of a profile, such as `CLIENT_SECRET`.
    pub fn set_profile_secret(
        &self,
        id: &ProfileId,
        name: &str,
        secret: &SecretString,
    ) -> Result<()> {
        self.require_profile(id)?;
        self.secrets.store(&profile_secret_key(id, name), secret)
    }

    pub fn profile_secret(&self, id: &ProfileId, name: &str) -> Result<Option<SecretString>> {
        self.require_profile(id)?;
        self.secrets.retrieve(&profile_secret_key(id, name))
    }

    /// Where a profile's OAuth token pair is kept (both halves in one write, so a rotated refresh
    /// token is never saved apart from its access token).
    pub fn openapi_token_storage(&self, id: &ProfileId) -> OpenApiTokenStorage {
        OpenApiTokenStorage {
            secrets: Arc::clone(&self.secrets),
            key: profile_secret_key(id, OAUTH_TOKENS),
        }
    }

    /// Records the public Open API settings of a profile after the account was picked.
    pub fn set_openapi_profile(
        &mut self,
        id: &ProfileId,
        client_id: String,
        callback_port: u16,
        account_id: i64,
    ) -> Result<()> {
        self.commit(|config| {
            let profile = config
                .profiles
                .iter_mut()
                .find(|profile| &profile.id == id)
                .ok_or_else(|| ConfigError::UnknownProfile(id.to_string()))?;
            profile.client_id = Some(client_id);
            profile.callback_port = Some(callback_port);
            profile.account_id = Some(account_id);
            Ok(())
        })?;
        debug!(%id, callback_port, account_id, "recorded the Open API profile settings");
        Ok(())
    }

    /// Sets (or, with `None`, clears) the active profile.
    pub fn set_active_profile(&mut self, id: Option<ProfileId>) -> Result<()> {
        if let Some(id) = &id
            && self.app_config.profile(id).is_none()
        {
            warn!(%id, "cannot activate an unknown profile");
            return Err(ConfigError::UnknownProfile(id.to_string()));
        }
        self.commit(|config| {
            config.active_profile = id.clone();
            Ok(())
        })?;
        info!(active = ?id, "changed the active profile");
        Ok(())
    }

    /// The documents of one scope, such as an account (`demo-45970491`).
    pub fn scope(&self, scope: &str) -> DocumentStore {
        DocumentStore::scoped(&self.paths, scope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;
    use std::time::{Duration, UNIX_EPOCH};

    fn config_in(dir: &std::path::Path) -> WyckConfig {
        let passphrase = SecretString::from("test-passphrase".to_owned());
        WyckConfig::open(AppPaths::at(dir), Some(passphrase)).unwrap()
    }

    fn secret(text: &str) -> SecretString {
        SecretString::from(text.to_owned())
    }

    #[test]
    fn a_profile_and_its_secret_persist_across_a_restart() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = config_in(temp_dir.path());

        let id = config.add_profile("Demo", "ctrader-openapi").unwrap();
        config
            .set_profile_secret(&id, CLIENT_SECRET, &secret("token-123"))
            .unwrap();

        assert_eq!(config.profiles().len(), 1);
        assert_eq!(config.profiles()[0].display_name, "Demo");

        // Re-load fresh from disk to prove persistence, not just in-memory state.
        let reloaded = config_in(temp_dir.path());
        assert_eq!(reloaded.profiles().len(), 1);
        assert_eq!(
            reloaded
                .profile_secret(&id, CLIENT_SECRET)
                .unwrap()
                .unwrap()
                .expose_secret(),
            "token-123"
        );
    }

    #[test]
    fn a_new_profile_has_no_secret() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = config_in(temp_dir.path());
        let id = config
            .add_profile("Local desktop", "ctrader-local")
            .unwrap();
        assert!(config.profile_secret(&id, CLIENT_SECRET).unwrap().is_none());
        assert!(config.openapi_token_storage(&id).load().unwrap().is_none());
    }

    #[test]
    fn remove_profile_deletes_config_entry_secrets_and_active_pointer() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = config_in(temp_dir.path());
        let id = config.add_profile("Demo", "ctrader-openapi").unwrap();
        config.set_active_profile(Some(id.clone())).unwrap();
        config
            .set_profile_secret(&id, CLIENT_SECRET, &secret("app-secret"))
            .unwrap();
        config
            .set_profile_secret(&id, OAUTH_TOKENS, &secret("refresh"))
            .unwrap();

        config.remove_profile(&id).unwrap();

        assert!(config.profiles().is_empty());
        assert!(config.active_profile().is_none());
        for name in [CLIENT_SECRET, OAUTH_TOKENS] {
            assert!(
                config
                    .secrets
                    .retrieve(&profile_secret_key(&id, name))
                    .unwrap()
                    .is_none(),
                "{name} is gone"
            );
        }
        // And it stays removed after a restart.
        assert!(config_in(temp_dir.path()).profiles().is_empty());
    }

    #[test]
    fn openapi_profile_settings_and_secrets_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = config_in(dir.path());
        let id = config.add_profile("Demo", "ctrader-openapi").unwrap();
        config
            .set_openapi_profile(&id, "client-id".into(), 8765, 42)
            .unwrap();
        config
            .set_profile_secret(&id, CLIENT_SECRET, &secret("secret"))
            .unwrap();
        config
            .openapi_token_storage(&id)
            .save(&OpenApiTokens {
                access_token: secret("access"),
                refresh_token: secret("refresh"),
                expires_at: Some(UNIX_EPOCH + Duration::from_secs(1_800_000_000)),
            })
            .unwrap();
        drop(config);

        let config = config_in(dir.path());
        let profile = &config.profiles()[0];
        assert_eq!(profile.client_id.as_deref(), Some("client-id"));
        assert_eq!(profile.callback_port, Some(8765));
        assert_eq!(profile.account_id, Some(42));
        assert_eq!(
            config
                .profile_secret(&id, CLIENT_SECRET)
                .unwrap()
                .unwrap()
                .expose_secret(),
            "secret"
        );
        let tokens = config.openapi_token_storage(&id).load().unwrap().unwrap();
        assert_eq!(tokens.access_token.expose_secret(), "access");
        assert_eq!(tokens.refresh_token.expose_secret(), "refresh");
        assert_eq!(
            tokens.expires_at,
            Some(UNIX_EPOCH + Duration::from_secs(1_800_000_000))
        );
        assert!(
            !std::fs::read_to_string(AppPaths::at(dir.path()).config_file())
                .unwrap()
                .contains("secret")
        );
    }

    #[test]
    fn unknown_profiles_are_rejected_everywhere() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = config_in(temp_dir.path());
        let ghost = ProfileId::new_random();

        assert!(matches!(
            config.remove_profile(&ghost),
            Err(ConfigError::UnknownProfile(_))
        ));
        assert!(matches!(
            config.set_active_profile(Some(ghost.clone())),
            Err(ConfigError::UnknownProfile(_))
        ));
        assert!(matches!(
            config.set_openapi_profile(&ghost, "c".into(), 1, 2),
            Err(ConfigError::UnknownProfile(_))
        ));
        assert!(matches!(
            config.set_profile_secret(&ghost, CLIENT_SECRET, &secret("x")),
            Err(ConfigError::UnknownProfile(_))
        ));
        assert!(matches!(
            config.profile_secret(&ghost, CLIENT_SECRET),
            Err(ConfigError::UnknownProfile(_))
        ));
    }

    /// A store that fails on demand, to see what a failure leaves behind.
    struct Flaky {
        inner: EncryptedFileSecretStore,
        fail_deletes: std::sync::atomic::AtomicBool,
    }

    impl SecretStore for Flaky {
        fn store(&self, key: &SecretKey, secret: &SecretString) -> Result<()> {
            self.inner.store(key, secret)
        }
        fn retrieve(&self, key: &SecretKey) -> Result<Option<SecretString>> {
            self.inner.retrieve(key)
        }
        fn delete(&self, key: &SecretKey) -> Result<()> {
            if self.fail_deletes.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(ConfigError::SecretStore {
                    key: key.to_string(),
                    message: "the keyring is locked".into(),
                });
            }
            self.inner.delete(key)
        }
    }

    fn flaky_config(dir: &std::path::Path) -> (WyckConfig, Arc<Flaky>) {
        let paths = AppPaths::at(dir);
        let flaky = Arc::new(Flaky {
            inner: EncryptedFileSecretStore::new(paths.secrets_dir(), secret("pw")),
            fail_deletes: std::sync::atomic::AtomicBool::new(false),
        });
        struct Shared(Arc<Flaky>);
        impl SecretStore for Shared {
            fn store(&self, k: &SecretKey, s: &SecretString) -> Result<()> {
                self.0.store(k, s)
            }
            fn retrieve(&self, k: &SecretKey) -> Result<Option<SecretString>> {
                self.0.retrieve(k)
            }
            fn delete(&self, k: &SecretKey) -> Result<()> {
                self.0.delete(k)
            }
        }
        let config = WyckConfig::load(paths, Box::new(Shared(flaky.clone()))).unwrap();
        (config, flaky)
    }

    #[test]
    fn a_failed_credential_delete_keeps_the_profile_so_the_removal_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let (mut config, flaky) = flaky_config(dir.path());
        let id = config.add_profile("Demo", "s").unwrap();
        config
            .set_profile_secret(&id, CLIENT_SECRET, &secret("token"))
            .unwrap();
        flaky
            .fail_deletes
            .store(true, std::sync::atomic::Ordering::SeqCst);

        assert!(config.remove_profile(&id).is_err());
        assert_eq!(config.profiles().len(), 1, "the profile is still there");
        assert_eq!(config_in_flaky_reload(dir.path()).profiles().len(), 1);

        flaky
            .fail_deletes
            .store(false, std::sync::atomic::Ordering::SeqCst);
        config.remove_profile(&id).unwrap();
        assert!(config.profiles().is_empty());
    }

    fn config_in_flaky_reload(dir: &std::path::Path) -> WyckConfig {
        WyckConfig::load(
            AppPaths::at(dir),
            Box::new(EncryptedFileSecretStore::new(
                AppPaths::at(dir).secrets_dir(),
                secret("pw"),
            )),
        )
        .unwrap()
    }

    #[test]
    fn a_failed_save_leaves_the_memory_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = config_in(dir.path());
        let kept = config.add_profile("Kept", "s").unwrap();

        // The config file can no longer be replaced: a directory sits where it goes.
        let file = AppPaths::at(dir.path()).config_file();
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();

        assert!(config.add_profile("Lost", "s").is_err());
        assert!(config.set_active_profile(Some(kept.clone())).is_err());
        assert!(config.set_openapi_profile(&kept, "c".into(), 1, 2).is_err());

        assert_eq!(config.profiles().len(), 1, "no profile was kept in memory");
        assert!(
            config.active_profile().is_none(),
            "no change was kept in memory"
        );
        assert_eq!(config.profiles()[0].client_id, None);
    }
}
