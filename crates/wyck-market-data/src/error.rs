//! The crate's error type.

use std::path::PathBuf;

/// The crate-wide result alias.
pub type Result<T> = std::result::Result<T, MarketDataError>;

/// Errors produced while reading or writing the local historical data catalog.
#[derive(Debug, thiserror::Error)]
pub enum MarketDataError {
    /// Creating a directory the catalog needs failed.
    #[error("failed to create directory `{path}`: {source}")]
    CreateDir {
        /// The directory that could not be created.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Reading a bar/tick chunk file failed.
    #[error("failed to read `{path}`: {source}")]
    Read {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Writing a bar/tick chunk file failed.
    #[error("failed to write `{path}`: {source}")]
    Write {
        /// The file that could not be written.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A chunk file's length is not a whole number of records, so it is corrupt or was
    /// truncated mid-write.
    #[error("`{path}` has a truncated record: {len} bytes is not a multiple of {record_len}")]
    TruncatedChunk {
        /// The malformed chunk file.
        path: PathBuf,
        /// The file's length in bytes.
        len: usize,
        /// The expected fixed record length.
        record_len: usize,
    },

    /// The manifest (SQLite) database could not be opened or migrated.
    #[error("failed to open the catalog manifest at `{path}`: {source}")]
    Manifest {
        /// The manifest file.
        path: PathBuf,
        /// The underlying SQLite error.
        #[source]
        source: rusqlite::Error,
    },

    /// A manifest query or write failed.
    #[error("catalog manifest query failed: {0}")]
    ManifestQuery(#[source] rusqlite::Error),

    /// The upstream historical data source (the broker, or a test fake) returned an
    /// error. Only its display text is kept, never its concrete type, so this crate
    /// never has to depend on the upstream's own error type.
    #[error("upstream historical data request failed: {0}")]
    Upstream(String),
}
