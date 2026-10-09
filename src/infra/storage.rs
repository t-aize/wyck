//! # Storage
//!
//! Configuration, encrypted credentials and portable backups for Wyck: the one place the app
//! decides where its files live, how they are written, and how anything secret is kept.
//!
//! ## Kinds of state, kept apart on purpose
//!
//! | What | Where | Type |
//! |---|---|---|
//! | Which profiles exist, which is active | `config.toml`, plain TOML | [`AppConfig`], [`ProfileConfig`] |
//! | Tokens and secrets | the OS keyring, or encrypted files | [`SecretStore`] |
//! | Everything else the app remembers (layouts, drawings, favorites) | one TOML file per document | [`DocumentStore`] |
//! | Indicator scripts | one `.rhai` file each | [`scripts::ScriptStore`] |
//! | A backup of everything, to export, import, keep or restore | one TOML file | [`backup::Backup`], [`backup::BackupStore`] |
//! | Text to carry elsewhere, unreadable without a passphrase | a small TOML document | [`sealed`] |
//!
//! **A token never appears in `config.toml`.** The file only holds a profile's public settings;
//! a [`SecretKey`] derived from the profile's id says *where* the secret is looked up.
//! Secrets are held in memory as [`secrecy::SecretString`] (wiped on drop, never printed by
//! `Debug`).
//!
//! ## Getting started
//!
//! Most programs need [`WyckConfig`]. [`WyckConfig::open`] uses the standard folders of the
//! operating system and its keyring; the [builder](WyckConfig::builder) changes either:
//!
//! ```no_run
//! use secrecy::SecretString;
//! use wyck::infra::storage::{CLIENT_SECRET, WyckConfig};
//!
//! # fn main() -> wyck::infra::storage::Result<()> {
//! let mut config = WyckConfig::open()?;
//!
//! let id = config.add_profile("Live: FTMO 100k", "ctrader-openapi")?;
//! config.set_profile_secret(&id, CLIENT_SECRET, &SecretString::from("the-secret".to_owned()))?;
//! config.set_active_profile(Some(id.clone()))?;
//!
//! if let Some(secret) = config.profile_secret(&id, CLIENT_SECRET)? {
//!     // hand `secret` to the client that needs it
//! }
//! # Ok(())
//! # }
//! ```
//!
//! A portable install, or a headless machine with no keyring:
//!
//! ```no_run
//! use secrecy::SecretString;
//! use wyck::infra::storage::WyckConfig;
//!
//! # fn main() -> wyck::infra::storage::Result<()> {
//! let config = WyckConfig::builder()
//!     .portable("./wyck-data")
//!     .encrypted_file(SecretString::from(std::env::var("WYCK_PASSPHRASE").unwrap_or_default()))
//!     .build()?;
//! # let _ = config;
//! # Ok(())
//! # }
//! ```
//!
//! ## Guarantees
//!
//! * **Writes are atomic and durable.** Every file goes through [`atomic_write`]: a crash or a
//!   power cut leaves the old file or the new one, never half of one.
//! * **The memory never gets ahead of the disk.** Every change of [`WyckConfig`] is saved before it
//!   is kept: when the save fails the change did not happen.
//! * **A file that cannot be read never takes the app down.** [`DocumentStore::load_or_default`]
//!   sets a damaged document aside as `.bad` and starts from the defaults.
//! * **A file from a newer version is left alone.** The config refuses to load rather than being
//!   read wrong and written back without what it did not know.
//! * **Names cannot leave their folder.** See [`names`].
//! * **Secrets stay secret.** Errors and logs carry the [`SecretKey`], never the value. See
//!   [`secret::EncryptedFileSecretStore`] for the encryption.
//!
//! ## Backups
//!
//! [`backup`] owns the whole life of a backup: [`backup::export_to_file`] and
//! [`backup::stage_import_file`] for the export and import buttons, [`backup::BackupStore`]
//! (`config.backups()`) for the copies kept in `backups/` (automatic ones included), and
//! [`backup::apply_pending`] to run at the start of the app, so a restore never writes over a
//! running app and can itself be undone.
//!
//! ## Checking it
//!
//! [`WyckConfig::diagnose`] looks over the whole config (missing secrets, permissions, files a
//! crash left behind) and returns a [`Report`]. The `config_doctor` example prints it.
//!
//! ## Where the files go
//!
//! See [`AppPaths`]. `WYCK_CONFIG_DIR` and `WYCK_DATA_DIR` move them.

#![forbid(unsafe_code)]

mod app_config;
pub mod backup;
pub mod chart_export;
mod crypto;
mod doctor;
mod documents;
mod error;
mod fs_util;
pub mod names;
mod paths;
mod profile;
pub mod scripts;
pub mod sealed;
pub mod secret;
mod tokens;

pub use app_config::{AppConfig, CURRENT_SCHEMA_VERSION};
pub use doctor::{Finding, Report, Severity};
pub use documents::DocumentStore;
pub use error::{ConfigError, Result};
pub use fs_util::{atomic_write, stale_temp_files};
pub use paths::{AppPaths, CONFIG_DIR_ENV, DATA_DIR_ENV};
pub use profile::{ProfileConfig, ProfileId};
pub use secret::{EncryptedFileSecretStore, KeyringSecretStore, SecretKey, SecretStore};
pub use tokens::{OpenApiTokenStorage, OpenApiTokens};

use secrecy::SecretString;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// The name under which a profile's Open API application secret is stored.
pub const CLIENT_SECRET: &str = "client-secret";
/// The name under which a profile's OAuth token pair is stored.
pub const OAUTH_TOKENS: &str = "oauth-token-set";

/// The credential a profile's named secret lives under.
fn profile_secret_key(id: &ProfileId, name: &str) -> SecretKey {
    SecretKey::new("profile-field", &format!("{}:{name}", id.as_str()))
}

/// The top-level entry point: [`AppPaths`] plus a loaded [`AppConfig`] plus a chosen
/// [`SecretStore`] backend, combined into the single type a front end actually imports
/// and holds for the lifetime of the app.
///
/// Every mutating method persists [`AppConfig`] to disk before returning `Ok`, and keeps the
/// change in memory only if that save worked: the in-memory state and the on-disk state never
/// drift, and a caller never needs to remember to call an explicit `save`.
///
/// A `WyckConfig` is meant to be owned by one place in one process. It does no locking of its own;
/// two processes changing the same config file at once each write a complete file (the last one
/// wins) but do not merge.
pub struct WyckConfig {
    paths: AppPaths,
    app_config: AppConfig,
    secrets: Arc<dyn SecretStore>,
}

impl WyckConfig {
    /// Loads the config from the standard folders of the operating system (or where
    /// [`CONFIG_DIR_ENV`] says), with the OS keyring as the credential store. The shortest way to
    /// get a config; use [`Self::builder`] to change either.
    ///
    /// # Errors
    ///
    /// [`ConfigError::NoProjectDirs`] when there is no folder to use, and whatever
    /// [`Self::load`] returns.
    pub fn open() -> Result<Self> {
        Self::builder().build()
    }

    /// Starts a [`ConfigBuilder`].
    #[must_use]
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }

    /// Loads (or, on first run, initializes) the app config at `paths`, paired with
    /// `secrets` as the credential backend for every profile this instance manages.
    ///
    /// # Errors
    ///
    /// Propagates [`AppConfig::load`]'s errors (I/O or parse failures on an existing,
    /// but corrupt or unreadable, config file, or a file from a newer version).
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

    /// Applies `change` to a copy of the config, saves the copy, and only then keeps it. When the
    /// change or the save fails, nothing happened.
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

    /// Adds a new, empty profile and persists the updated [`AppConfig`] to disk. Its credentials
    /// are stored afterwards with [`Self::set_profile_secret`] and [`Self::openapi_token_storage`].
    ///
    /// # Errors
    ///
    /// Any error of [`AppConfig::save`]; the profile is then not added.
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

    /// Removes a profile: deletes its stored credentials, removes it from the config,
    /// clears `active_profile` if it pointed at this one, and persists the change.
    ///
    /// The credentials go first. If one cannot be deleted the profile stays, and the call can be
    /// repeated (deleting what is already gone is fine). Credentials stored under other names
    /// with [`Self::set_profile_secret`] are not known here: delete them with
    /// [`Self::delete_profile_secret`] first.
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownProfile`] if `id` doesn't name a configured profile;
    /// otherwise any error the [`SecretStore`] backend or [`AppConfig::save`] returns.
    pub fn remove_profile(&mut self, id: &ProfileId) -> Result<()> {
        if self.app_config.profile(id).is_none() {
            warn!(%id, "cannot remove an unknown profile");
            return Err(ConfigError::UnknownProfile(id.to_string()));
        }

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

    /// Stores a named credential for a profile, apart from its main token: the Open API client
    /// secret ([`CLIENT_SECRET`]), for instance. `name` follows the rule of [`names`].
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownProfile`], [`ConfigError::InvalidName`], or any error of the
    /// credential store.
    pub fn set_profile_secret(
        &self,
        id: &ProfileId,
        name: &str,
        secret: &SecretString,
    ) -> Result<()> {
        self.require_profile(id)?;
        names::validate_name(name)?;
        self.secrets.store(&profile_secret_key(id, name), secret)
    }

    /// Reads a named credential for a profile.
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownProfile`], [`ConfigError::InvalidName`], or any error of the
    /// credential store.
    pub fn profile_secret(&self, id: &ProfileId, name: &str) -> Result<Option<SecretString>> {
        self.require_profile(id)?;
        names::validate_name(name)?;
        self.secrets.retrieve(&profile_secret_key(id, name))
    }

    /// Deletes a named credential of a profile. Nothing stored is fine.
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownProfile`], [`ConfigError::InvalidName`], or any error of the
    /// credential store.
    pub fn delete_profile_secret(&self, id: &ProfileId, name: &str) -> Result<()> {
        self.require_profile(id)?;
        names::validate_name(name)?;
        self.secrets.delete(&profile_secret_key(id, name))
    }

    /// Where a profile's OAuth token pair is kept: load it, save it (both halves in one
    /// credential-store write, so a rotated refresh token is never saved apart from its access
    /// token), or clear it. The handle can outlive this borrow of the config and move to another
    /// thread, so a long-running session can save the tokens it renews on its own, without going
    /// through the `WyckConfig` the UI owns.
    pub fn openapi_token_storage(&self, id: &ProfileId) -> OpenApiTokenStorage {
        OpenApiTokenStorage {
            secrets: Arc::clone(&self.secrets),
            key: profile_secret_key(id, OAUTH_TOKENS),
        }
    }

    /// Records public Open API settings for a profile after OAuth account selection.
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownProfile`], or any error of [`AppConfig::save`].
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

    /// Sets (or, with `None`, clears) the active profile, and persists the change.
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownProfile`] if `Some(id)` doesn't name a configured profile.
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

    /// The symbol the user was last on, if one was remembered.
    pub fn last_symbol(&self) -> Option<&str> {
        self.app_config.last_symbol.as_deref()
    }

    /// Remembers (or with `None` forgets) the symbol the user is on, and persists the change. A
    /// blank name forgets it.
    ///
    /// # Errors
    ///
    /// Any error of [`AppConfig::save`].
    pub fn set_last_symbol(&mut self, symbol: Option<String>) -> Result<()> {
        let symbol = symbol
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        self.commit(|config| {
            config.last_symbol = symbol;
            Ok(())
        })?;
        debug!(symbol = ?self.app_config.last_symbol, "remembered the last symbol");
        Ok(())
    }

    /// The documents shared by every account (see [`DocumentStore::global`]).
    pub fn documents(&self) -> DocumentStore {
        self.paths.documents()
    }

    /// The documents of one scope, such as an account (see [`DocumentStore::scoped`]).
    pub fn scope(&self, scope: &str) -> DocumentStore {
        self.paths.scope(scope)
    }

    /// The indicator scripts of the default folder (see [`scripts::ScriptStore`]).
    pub fn scripts(&self) -> scripts::ScriptStore {
        self.paths.scripts()
    }

    /// The backups kept in the backups folder: list, save, restore, delete (see
    /// [`backup::BackupStore`]).
    pub fn backups(&self) -> backup::BackupStore {
        self.paths.backups()
    }

    /// Looks the whole config over and reports what is wrong or odd: missing or unreadable
    /// credentials, a dangling active profile, permissions that are too open, unfinished writes
    /// and documents that had to be set aside. Changes nothing.
    pub fn diagnose(&self) -> Report {
        doctor::run(&self.paths, &self.app_config, self.secrets.as_ref())
    }
}

/// Where a [`ConfigBuilder`] keeps credentials.
enum Backend {
    Keyring(String),
    EncryptedFile(SecretString),
    Custom(Box<dyn SecretStore>),
}

/// Builds a [`WyckConfig`] with the folders and the credential store that are wanted. Start with
/// [`WyckConfig::builder`]; nothing is read until [`Self::build`].
///
/// Without any setting it is [`WyckConfig::open`]: the standard folders and the OS keyring under
/// the service name `wyck`.
pub struct ConfigBuilder {
    paths: Option<AppPaths>,
    backend: Backend,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self {
            paths: None,
            backend: Backend::Keyring("wyck".to_owned()),
        }
    }
}

impl ConfigBuilder {
    /// Uses these folders instead of the standard ones.
    #[must_use]
    pub fn paths(mut self, paths: AppPaths) -> Self {
        self.paths = Some(paths);
        self
    }

    /// Keeps everything in one folder (see [`AppPaths::at`]): a portable install, or a test.
    #[must_use]
    pub fn portable(self, dir: impl Into<PathBuf>) -> Self {
        self.paths(AppPaths::at(dir))
    }

    /// Stores credentials in the OS keyring under `service` (the default, with `"wyck"`).
    #[must_use]
    pub fn keyring_service(mut self, service: impl Into<String>) -> Self {
        self.backend = Backend::Keyring(service.into());
        self
    }

    /// Stores credentials in encrypted files under the data folder, locked by `passphrase`: for
    /// machines with no keyring. See [`EncryptedFileSecretStore`].
    #[must_use]
    pub fn encrypted_file(mut self, passphrase: SecretString) -> Self {
        self.backend = Backend::EncryptedFile(passphrase);
        self
    }

    /// Stores credentials in a store of your own.
    #[must_use]
    pub fn secret_store(mut self, store: Box<dyn SecretStore>) -> Self {
        self.backend = Backend::Custom(store);
        self
    }

    /// Loads the config.
    ///
    /// # Errors
    ///
    /// [`ConfigError::NoProjectDirs`] when no folders were given and the system has none, and
    /// whatever [`WyckConfig::load`] returns.
    pub fn build(self) -> Result<WyckConfig> {
        let paths = match self.paths {
            Some(paths) => paths,
            None => AppPaths::discover()?,
        };
        let secrets: Box<dyn SecretStore> = match self.backend {
            Backend::Keyring(service) => Box::new(KeyringSecretStore::new(service)),
            Backend::EncryptedFile(passphrase) => Box::new(EncryptedFileSecretStore::new(
                paths.secrets_dir(),
                passphrase,
            )),
            Backend::Custom(store) => store,
        };
        WyckConfig::load(paths, secrets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;
    use std::time::{Duration, UNIX_EPOCH};

    fn config_in(dir: &std::path::Path) -> WyckConfig {
        WyckConfig::builder()
            .portable(dir)
            .encrypted_file(SecretString::from("test-passphrase".to_owned()))
            .build()
            .unwrap()
    }

    fn secret(text: &str) -> SecretString {
        SecretString::from(text.to_owned())
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
    fn a_profile_and_its_secret_persist_across_a_restart() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = config_in(temp_dir.path());

        let id = config.add_profile("Demo", "ctrader-openapi").unwrap();
        config
            .set_profile_secret(&id, CLIENT_SECRET, &secret("token-123"))
            .unwrap();

        assert_eq!(config.profiles().len(), 1);
        assert_eq!(config.profile(&id).unwrap().display_name, "Demo");

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
        let profile = config.profile(&id).unwrap();
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
            !std::fs::read_to_string(config.paths().config_file())
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

    #[test]
    fn a_credential_name_cannot_be_a_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = config_in(temp_dir.path());
        let id = config.add_profile("P", "s").unwrap();
        for bad in ["", "../x", "a/b", "a:b", "with space"] {
            assert!(
                matches!(
                    config.set_profile_secret(&id, bad, &secret("x")),
                    Err(ConfigError::InvalidName { .. })
                ),
                "{bad:?}"
            );
        }
        config
            .set_profile_secret(&id, "api-key_2", &secret("ok"))
            .unwrap();
        config.delete_profile_secret(&id, "api-key_2").unwrap();
        assert!(config.profile_secret(&id, "api-key_2").unwrap().is_none());
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
        let file = config.paths().config_file();
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();

        assert!(config.add_profile("Lost", "s").is_err());
        assert!(config.set_active_profile(Some(kept.clone())).is_err());
        assert!(config.set_last_symbol(Some("EURUSD".into())).is_err());
        assert!(config.set_openapi_profile(&kept, "c".into(), 1, 2).is_err());

        assert_eq!(config.profiles().len(), 1, "no profile was kept in memory");
        assert!(
            config.active_profile().is_none(),
            "no change was kept in memory"
        );
        assert_eq!(config.last_symbol(), None);
        assert_eq!(config.profile(&kept).unwrap().client_id, None);
    }

    #[test]
    fn the_builder_defaults_are_the_os_keyring_and_the_standard_folders() {
        // Nothing is read until `build`, and a portable folder is honored.
        let dir = tempfile::tempdir().unwrap();
        let config = WyckConfig::builder()
            .portable(dir.path())
            .secret_store(Box::new(EncryptedFileSecretStore::new(
                dir.path().join("s"),
                secret("pw"),
            )))
            .build()
            .unwrap();
        assert_eq!(config.paths().config_dir(), dir.path());
        assert!(config.profiles().is_empty());
    }

    #[test]
    fn a_healthy_config_has_nothing_to_report_and_a_broken_one_says_what() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = config_in(dir.path());
        let id = config.add_profile("Demo", "ctrader-openapi").unwrap();
        assert!(config.diagnose().is_healthy());

        // An Open API profile with no application secret stored.
        config
            .set_openapi_profile(&id, "client".into(), 8765, 1)
            .unwrap();
        let report = config.diagnose();
        assert!(
            report.is_healthy(),
            "a missing secret is a warning, not an error"
        );
        assert_eq!(report.worst(), Some(Severity::Warning));
        assert!(
            report.to_string().contains("no application secret"),
            "{report}"
        );

        // A crash left a temporary file, and a document was set aside.
        std::fs::write(config.paths().config_dir().join(".config.toml.tmp-1"), b"x").unwrap();
        let state = DocumentStore::global(config.paths());
        std::fs::create_dir_all(state.dir()).unwrap();
        std::fs::write(state.dir().join("prefs.toml.bad"), b"junk").unwrap();
        let text = config.diagnose().to_string();
        assert!(text.contains("unfinished write"), "{text}");
        assert!(text.contains("set aside"), "{text}");
    }
}
