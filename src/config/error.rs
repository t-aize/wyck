//! The crate's error type.

use std::path::PathBuf;

/// The crate-wide result alias.
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Errors produced by `crate::config::AppPaths`, `crate::config::AppConfig`, and the
/// `crate::config::secret` backends.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(
        "could not resolve an OS-standard config directory (no home directory reported for the current user)"
    )]
    NoProjectDirs,

    /// Reading a config or secret file failed.
    #[error("failed to read `{path}`: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Writing a config or secret file failed.
    #[error("failed to write `{path}`: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// A config file's content was not valid TOML.
    #[error("failed to parse `{path}` as TOML: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },

    #[error("failed to serialize config to TOML: {0}")]
    Serialize(#[from] toml::ser::Error),

    /// A `crate::config::secret::SecretStore` backend failed.
    #[error("credential store error for key `{key}`: {message}")]
    SecretStore { key: String, message: String },

    #[error("no credential is stored for key `{0}`")]
    SecretNotFound(String),

    #[error("key derivation failed: {0}")]
    KeyDerivation(String),

    /// AEAD encryption or decryption failed in `crate::config::secret::EncryptedFileSecretStore`.
    #[error("encryption/decryption failed for key `{key}`: {message}")]
    Crypto { key: String, message: String },

    /// The stored secret envelope's `version` field is newer than this crate build understands,
    /// or the file is not a valid envelope at all.
    #[error("unreadable secret envelope for key `{key}`: {reason}")]
    MalformedEnvelope { key: String, reason: String },

    #[error("no profile with id `{0}` is configured")]
    UnknownProfile(String),

    /// A name (of a document, a scope or a named credential) cannot be used as given: it is
    /// empty, too long, or holds characters that could point outside the folder it belongs in.
    #[error("`{name}` cannot be used as a name: {reason}")]
    InvalidName { name: String, reason: &'static str },

    /// The config file was written by a newer version of the app than this one, so it is left
    /// alone instead of being read wrong and written back without what this version does not
    /// know.
    #[error(
        "`{path}` uses config format {found}, but this version only understands up to {supported}"
    )]
    UnsupportedSchema {
        path: PathBuf,
        found: u32,
        supported: u32,
    },

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
            | ConfigError::UnsupportedSchema { .. } => std::io::ErrorKind::InvalidData,
            _ => std::io::ErrorKind::Other,
        };
        Self::new(kind, error)
    }
}
