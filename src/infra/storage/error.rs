//! The crate's error type.

use std::path::PathBuf;

/// The crate-wide result alias.
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Errors produced by [`crate::infra::storage::AppPaths`], [`crate::infra::storage::AppConfig`], and the
/// [`crate::infra::storage::secret`] backends.
///
/// Every variant that touches a file carries the path; every variant that touches a
/// secret carries the [`crate::infra::storage::SecretKey`] it was operating on: never the secret value
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

    /// Reading a config or secret file failed.
    #[error("failed to read `{path}`: {source}")]
    Read {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Writing a config or secret file failed.
    #[error("failed to write `{path}`: {source}")]
    Write {
        /// The file that could not be written.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A config file's content was not valid TOML.
    #[error("failed to parse `{path}` as TOML: {source}")]
    Parse {
        /// The file that failed to parse.
        path: PathBuf,
        /// The underlying parse error.
        #[source]
        source: Box<toml::de::Error>,
    },

    /// [`AppConfig`](crate::infra::storage::AppConfig) could not be encoded to TOML.
    #[error("failed to serialize config to TOML: {0}")]
    Serialize(#[from] toml::ser::Error),

    /// A [`crate::infra::storage::secret::SecretStore`] backend failed. `message` is the backend's own
    /// error text (e.g. from `keyring::Error`'s `Display` impl): never the secret
    /// value, which the backend never has a reason to put in an error message.
    #[error("credential store error for key `{key}`: {message}")]
    SecretStore {
        /// The key the failing operation was for.
        key: String,
        /// The backend's own error text.
        message: String,
    },

    /// The requested secret does not exist in the store (distinct from a backend
    /// failure: this is the normal "not set yet" case, returned as `Ok(None)` from
    /// [`crate::infra::storage::secret::SecretStore::retrieve`] rather than this variant in most call
    /// paths; this variant exists for operations that require the secret to already
    /// exist, e.g. an explicit `delete`).
    #[error("no credential is stored for key `{0}`")]
    SecretNotFound(String),

    /// Key derivation (passphrase -> encryption key) failed in
    /// [`crate::infra::storage::secret::EncryptedFileSecretStore`]. Does not happen for well-formed
    /// inputs under normal operation; see that type's docs for the salt/output-length
    /// invariants that would need to be violated to trigger this.
    #[error("key derivation failed: {0}")]
    KeyDerivation(String),

    /// AEAD encryption or decryption failed in
    /// [`crate::infra::storage::secret::EncryptedFileSecretStore`]. On decrypt, this most commonly
    /// means the supplied passphrase does not match the one the secret was encrypted
    /// with (the AEAD authentication tag will not verify): surface this to the user as
    /// "wrong passphrase", not as file corruption.
    #[error("encryption/decryption failed for key `{key}`: {message}")]
    Crypto {
        /// The key the failing operation was for.
        key: String,
        /// The backend's own error text.
        message: String,
    },

    /// The stored secret envelope's `version` field is newer than this crate build
    /// understands, or the file is not a valid envelope at all.
    #[error("unreadable secret envelope for key `{key}`: {reason}")]
    MalformedEnvelope {
        /// The key whose envelope could not be read.
        key: String,
        /// Why the envelope was rejected.
        reason: String,
    },

    /// [`crate::infra::storage::WyckConfig::set_active_profile`] (or similar) was given a
    /// [`crate::infra::storage::ProfileId`] that isn't in [`crate::infra::storage::AppConfig::profiles`].
    #[error("no profile with id `{0}` is configured")]
    UnknownProfile(String),

    /// A name (of a document, a scope or a named credential) cannot be used as given: it is empty,
    /// too long, or holds characters that could point outside the folder it belongs in. See
    /// [`crate::infra::storage::names`].
    #[error("`{name}` cannot be used as a name: {reason}")]
    InvalidName {
        /// The name that was refused.
        name: String,
        /// Why.
        reason: &'static str,
    },

    /// The config file was written by a newer version of the app than this one, so it is left
    /// alone instead of being read wrong and written back without what this version does not
    /// know.
    #[error(
        "`{path}` uses config format {found}, but this version only understands up to {supported}"
    )]
    UnsupportedSchema {
        /// The file.
        path: PathBuf,
        /// The version it says it has.
        found: u32,
        /// The newest version this build understands.
        supported: u32,
    },

    /// Text that should be a sealed (passphrase-encrypted) document is not one, or is damaged, or
    /// was made by a newer version.
    #[error("not a readable sealed document: {0}")]
    Sealed(String),

    /// A sealed document did not open: the passphrase does not match, or the document was
    /// changed after it was sealed. The two cannot be told apart, on purpose.
    #[error("the passphrase is wrong, or the sealed document was changed")]
    WrongPassphrase,

    /// The text is not a backup of this app (see [`crate::infra::storage::backup`]).
    #[error("This file is not a wyck backup.")]
    NotABackup,

    /// A backup made by a newer version than this one knows: it is refused, not misread.
    #[error("This backup was made by a newer version of wyck (format {found}).")]
    BackupTooNew {
        /// The version it says it has.
        found: u32,
        /// The newest version this build understands.
        supported: u32,
    },

    /// A part of a backup is not what it should be, with what is wrong.
    #[error("This backup is damaged: {0}.")]
    BackupDamaged(String),

    /// A backup is sealed with a passphrase and none was given.
    #[error("This backup is locked with a passphrase.")]
    PassphraseRequired,

    /// No backup is kept under this name (see [`crate::infra::storage::backup::BackupStore`]).
    #[error("There is no saved backup called `{0}`.")]
    BackupNotFound(String),

    /// Secure random byte generation failed (extremely rare: indicates a broken or
    /// exhausted OS entropy source).
    #[error("failed to generate random bytes: {0}")]
    Random(#[from] getrandom::Error),
}

impl From<ConfigError> for std::io::Error {
    /// A config error as an I/O error, for code that returns `io::Result` and writes or reads
    /// through this crate: the kind of the underlying I/O error is kept when there is one.
    fn from(error: ConfigError) -> Self {
        let kind = match &error {
            ConfigError::Read { source, .. } | ConfigError::Write { source, .. } => source.kind(),
            ConfigError::Parse { .. }
            | ConfigError::InvalidName { .. }
            | ConfigError::NotABackup
            | ConfigError::BackupTooNew { .. }
            | ConfigError::BackupDamaged(_)
            | ConfigError::UnsupportedSchema { .. } => std::io::ErrorKind::InvalidData,
            ConfigError::BackupNotFound(_) => std::io::ErrorKind::NotFound,
            _ => std::io::ErrorKind::Other,
        };
        Self::new(kind, error)
    }
}
