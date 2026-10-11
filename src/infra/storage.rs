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
//! crash left behind) and returns a [`Report`].
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

/// Reads one credential of a profile; see [`WyckConfig::profile_secret_reader`].
pub struct ProfileSecretReader {
    secrets: Arc<dyn SecretStore>,
    key: SecretKey,
}

impl ProfileSecretReader {
    /// The credential, or `None` when nothing is stored.
    ///
    /// # Errors
    ///
    /// Any error of the credential store.
    pub fn read(&self) -> Result<Option<SecretString>> {
        self.secrets.retrieve(&self.key)
    }
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

    /// A handle on a named credential of a profile that can move to another thread, to read it
    /// without blocking the one that owns the config (the OS keyring can take a while).
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownProfile`] or [`ConfigError::InvalidName`].
    pub fn profile_secret_reader(&self, id: &ProfileId, name: &str) -> Result<ProfileSecretReader> {
        self.require_profile(id)?;
        names::validate_name(name)?;
        Ok(ProfileSecretReader {
            secrets: Arc::clone(&self.secrets),
            key: profile_secret_key(id, name),
        })
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
#[path = "../../tests/unit/infra/storage.rs"]
mod tests;
