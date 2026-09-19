//! The crate's error type.

use std::path::PathBuf;

/// The crate-wide result alias.
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Errors produced by [`crate::AppPaths`], [`crate::AppConfig`], and the
/// [`crate::secret`] backends.
///
/// Every variant that touches a file carries the path; every variant that touches a
/// secret carries the [`crate::SecretKey`] it was operating on: never the secret value
/// itself, so a `{:?}`/`{}` of this error (e.g. in a log line) can never leak a token.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The OS did not report a home directory for the current user, so no
    /// OS-standard config/data directory could be resolved. See
    /// [`directories::ProjectDirs::from`].
    #[error(
        "could not resolve an OS-standard config directory (no home directory reported for the current user)"
    )]
    NoProjectDirs,

    #[error("failed to read `{path}`: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write `{path}`: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse `{path}` as TOML: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },

    #[error("failed to serialize config to TOML: {0}")]
    Serialize(#[from] toml::ser::Error),

    /// A [`crate::secret::SecretStore`] backend failed. `message` is the backend's own
    /// error text (e.g. from `keyring::Error`'s `Display` impl): never the secret
    /// value, which the backend never has a reason to put in an error message.
    #[error("credential store error for key `{key}`: {message}")]
    SecretStore { key: String, message: String },

    /// The requested secret does not exist in the store (distinct from a backend
    /// failure: this is the normal "not set yet" case, returned as `Ok(None)` from
    /// [`crate::secret::SecretStore::retrieve`] rather than this variant in most call
    /// paths; this variant exists for operations that require the secret to already
    /// exist, e.g. an explicit `delete`).
    #[error("no credential is stored for key `{0}`")]
    SecretNotFound(String),

    /// Key derivation (passphrase -> encryption key) failed in
    /// [`crate::secret::EncryptedFileSecretStore`]. Does not happen for well-formed
    /// inputs under normal operation; see that type's docs for the salt/output-length
    /// invariants that would need to be violated to trigger this.
    #[error("key derivation failed: {0}")]
    KeyDerivation(String),

    /// AEAD encryption or decryption failed in
    /// [`crate::secret::EncryptedFileSecretStore`]. On decrypt, this most commonly
    /// means the supplied passphrase does not match the one the secret was encrypted
    /// with (the AEAD authentication tag will not verify): surface this to the user as
    /// "wrong passphrase", not as file corruption.
    #[error("encryption/decryption failed for key `{key}`: {message}")]
    Crypto { key: String, message: String },

    /// The stored secret envelope's `version` field is newer than this crate build
    /// understands, or the file is not a valid envelope at all.
    #[error("unreadable secret envelope for key `{key}`: {reason}")]
    MalformedEnvelope { key: String, reason: String },

    /// [`crate::WyckConfig::set_active_profile`] (or similar) was given a
    /// [`crate::ProfileId`] that isn't in [`crate::AppConfig::profiles`].
    #[error("no profile with id `{0}` is configured")]
    UnknownProfile(String),

    /// Secure random byte generation failed (extremely rare: indicates a broken or
    /// exhausted OS entropy source).
    #[error("failed to generate random bytes: {0}")]
    Random(#[from] getrandom::Error),
}
