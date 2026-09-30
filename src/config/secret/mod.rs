//! Credential storage: the `SecretStore` trait and its two implementations.

mod file_store;
mod keyring_store;

pub use file_store::EncryptedFileSecretStore;
pub use keyring_store::KeyringSecretStore;

use secrecy::SecretString;

use crate::config::error::Result;

/// A structured identifier for one secret: `namespace:name`, e.g.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretKey(String);

impl SecretKey {
    /// Builds a namespaced key: `"{namespace}:{name}"`.
    pub fn new(namespace: &str, name: &str) -> Self {
        Self(format!("{namespace}:{name}"))
    }

    /// The raw key string, as passed to the backend (e.g. as the keyring "username" field, or
    /// hashed into a filename).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A backend capable of storing, retrieving, and deleting secrets by `SecretKey`.
pub trait SecretStore: Send + Sync {
    /// Stores `secret` under `key`, overwriting any existing value.
    fn store(&self, key: &SecretKey, secret: &SecretString) -> Result<()>;

    /// Retrieves the secret stored under `key`, or `Ok(None)` if nothing is stored there yet
    /// (this is the normal "not configured" case, not an error).
    fn retrieve(&self, key: &SecretKey) -> Result<Option<SecretString>>;

    /// Deletes the secret stored under `key`.
    fn delete(&self, key: &SecretKey) -> Result<()>;
}
