//! Credential storage: the [`SecretStore`] trait and its two implementations.
//!
//! Tokens never live in [`crate::AppConfig`]'s plaintext TOML file — only a
//! [`SecretKey`] identifying *where* to look one up does. The actual secret bytes go
//! through a [`SecretStore`] backend and are held in memory as
//! [`secrecy::SecretString`] (zeroized on drop, never printed by `Debug`), never as a
//! plain `String`.
//!
//! Two backends are provided:
//!
//! - [`KeyringSecretStore`] (default, recommended) — delegates to the OS-native
//!   credential store (Windows Credential Manager, macOS Keychain, Linux Secret
//!   Service). No key management burden on this crate at all; the OS owns it.
//! - [`EncryptedFileSecretStore`] (fallback) — for environments without an OS keyring
//!   (headless Linux boxes, some CI/container environments, `wyck`'s own planned
//!   "always-on box" headless mode per the project README). Encrypts each secret with
//!   ChaCha20-Poly1305 under a key derived from a caller-supplied passphrase via
//!   Argon2id, one envelope file per [`SecretKey`].

mod file_store;
mod keyring_store;

pub use file_store::EncryptedFileSecretStore;
pub use keyring_store::KeyringSecretStore;

use secrecy::SecretString;

use crate::error::Result;

/// A structured identifier for one secret: `namespace:name`, e.g.
/// `"ctrader-remote:profile:<uuid>"`. Namespacing keeps different crates' secrets from
/// colliding in a shared OS credential store (which is keyed by a flat
/// service/username pair) without any of those crates needing to coordinate on naming
/// conventions beyond "pick a namespace".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretKey(String);

impl SecretKey {
    /// Builds a namespaced key: `"{namespace}:{name}"`.
    pub fn new(namespace: &str, name: &str) -> Self {
        Self(format!("{namespace}:{name}"))
    }

    /// The canonical key for a [`crate::ProfileConfig`]'s credential, namespaced under
    /// `"profile"` so it can never collide with a key a different call site builds
    /// directly via [`Self::new`].
    pub fn for_profile(profile_id: &crate::ProfileId) -> Self {
        Self::new("profile", profile_id.as_str())
    }

    /// The raw key string, as passed to the backend (e.g. as the keyring "username"
    /// field, or hashed into a filename).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A backend capable of storing, retrieving, and deleting secrets by [`SecretKey`].
///
/// Implementations must never let a secret value escape into an error message, a log
/// line, or any other diagnostic output — only the [`SecretKey`] (never sensitive on
/// its own) is safe to include in an [`crate::ConfigError`].
pub trait SecretStore: Send + Sync {
    /// Stores `secret` under `key`, overwriting any existing value.
    fn store(&self, key: &SecretKey, secret: &SecretString) -> Result<()>;

    /// Retrieves the secret stored under `key`, or `Ok(None)` if nothing is stored
    /// there yet (this is the normal "not configured" case, not an error).
    fn retrieve(&self, key: &SecretKey) -> Result<Option<SecretString>>;

    /// Deletes the secret stored under `key`. Succeeds (as a no-op) if nothing was
    /// stored there.
    fn delete(&self, key: &SecretKey) -> Result<()>;
}
