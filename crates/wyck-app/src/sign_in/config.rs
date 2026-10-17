//! Opening the configuration when the app starts.

use crate::storage::{Severity, WyckConfig};

/// Why the settings could not be opened at start.
#[derive(Debug, thiserror::Error)]
pub enum LoadConfigError {
    /// The system gives the app no folder for its settings.
    #[error("the system gives Wyck no folder for its settings")]
    NoFolder,
    /// The folder or the files in it cannot be used.
    #[error(transparent)]
    Config(#[from] wyck_storage::ConfigError),
}

/// Loads the configuration from the standard folders, with the OS keyring for secrets, or with
/// encrypted files when the environment variable `WYCK_PASSPHRASE` holds a passphrase (a machine
/// with no keyring).
///
/// # Errors
///
/// The folders of the app cannot be found or the configuration cannot be read.
pub fn load_config() -> Result<WyckConfig, LoadConfigError> {
    let paths = crate::app_paths().ok_or(LoadConfigError::NoFolder)?.clone();
    let builder = WyckConfig::builder().paths(paths);
    let builder = match crate::storage::identity::passphrase() {
        Some(passphrase) => builder.encrypted_file(passphrase),
        None => builder,
    };
    let config = builder.build()?;
    let report = config.diagnose();
    if report.worst() >= Some(Severity::Warning) {
        tracing::warn!("the config has something to report:\n{report}");
    }
    Ok(config)
}
