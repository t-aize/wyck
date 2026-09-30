//! The default credential backend: the OS-native store.

use keyring::Entry;
use secrecy::{ExposeSecret, SecretString};
use tracing::{debug, trace, warn};

use crate::config::error::{ConfigError, Result};
use crate::config::secret::{SecretKey, SecretStore};

/// Stores secrets in the OS-native credential store: Windows Credential Manager, macOS
/// Keychain, or (on Linux) the Secret Service D-Bus API via a pure-Rust `zbus` client:
/// whichever backend the [`keyring`] crate resolves for the current platform. This is
/// the recommended default: the OS owns key management entirely, so there is no
/// passphrase to prompt for or key file to protect.
///
/// # Why this crate pins `keyring`'s default features and never disables them
///
/// Versions of the `keyring` crate before its v4 rearchitecture required the caller to
/// opt into a platform-native backend via a Cargo feature (`windows-native`,
/// `apple-native`, `sync-secret-service`, ...); if a project's `Cargo.toml` forgot one,
/// `keyring` silently fell back to a non-persistent in-memory *mock* store on that
/// platform: no compile error, no runtime error, just credentials that vanish on
/// restart. This bit multiple real projects (see the `keyring-rs` issue tracker for
/// reports of exactly this: session tokens that "never persist" because the build was
/// missing a platform feature). As of `keyring` 4.x this crate depends on, the `v1`
/// feature, which bundles the Windows, Apple, and Secret-Service-via-`zbus` backends:
/// is enabled by `default-features`, so a plain `keyring = "4.2"` dependency already
/// gets real native backends on every platform `wyck` targets. **Never set
/// `default-features = false` on the `keyring` dependency** in this crate's `Cargo.toml`
/// without re-verifying which backend is actually selected: that footgun is exactly
/// what silently reintroduces the mock-store failure mode above.
pub struct KeyringSecretStore {
    /// The "service" field under which every [`SecretKey`] is namespaced in the OS
    /// store (distinct from [`SecretKey`]'s own namespace convention: this is the
    /// outer namespace the OS credential manager itself groups entries by).
    service: String,
}

impl KeyringSecretStore {
    /// Creates a store that namespaces every credential under `service` in the OS
    /// credential manager (e.g. so `wyck`'s entries are visibly grouped together in
    /// Windows Credential Manager / macOS Keychain Access rather than mixed in with
    /// every other app's entries).
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
}

impl Default for KeyringSecretStore {
    /// Namespaces credentials under the service name `"wyck"`.
    fn default() -> Self {
        Self::new("wyck")
    }
}

impl SecretStore for KeyringSecretStore {
    fn store(&self, key: &SecretKey, secret: &SecretString) -> Result<()> {
        let entry = entry_for(&self.service, key)?;
        entry
            .set_password(secret.expose_secret())
            .inspect(|_| debug!(service = %self.service, %key, "stored a secret in the OS keyring"))
            .map_err(|source| {
                warn!(service = %self.service, %key, error = %source, "could not store a secret in the OS keyring");
                to_config_error(key, source)
            })
    }

    fn retrieve(&self, key: &SecretKey) -> Result<Option<SecretString>> {
        let entry = entry_for(&self.service, key)?;
        match entry.get_password() {
            Ok(password) => {
                trace!(service = %self.service, %key, "retrieved a secret from the OS keyring");
                Ok(Some(SecretString::from(password)))
            }
            Err(keyring::Error::NoEntry) => {
                trace!(service = %self.service, %key, "no secret in the OS keyring for this key");
                Ok(None)
            }
            Err(source) => {
                warn!(service = %self.service, %key, error = %source, "could not retrieve a secret from the OS keyring");
                Err(to_config_error(key, source))
            }
        }
    }

    fn delete(&self, key: &SecretKey) -> Result<()> {
        let entry = entry_for(&self.service, key)?;
        match entry.delete_credential() {
            Ok(()) => {
                debug!(service = %self.service, %key, "deleted a secret from the OS keyring");
                Ok(())
            }
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(source) => {
                warn!(service = %self.service, %key, error = %source, "could not delete a secret from the OS keyring");
                Err(to_config_error(key, source))
            }
        }
    }
}

fn entry_for(service: &str, key: &SecretKey) -> Result<Entry> {
    Entry::new(service, key.as_str()).map_err(|source| to_config_error(key, source))
}

fn to_config_error(key: &SecretKey, source: keyring::Error) -> ConfigError {
    ConfigError::SecretStore {
        key: key.to_string(),
        message: source.to_string(),
    }
}
