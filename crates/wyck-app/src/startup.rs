//! The startup flow: from settings to a [`ConnectRequest`].
//!
//! Two sources, decided by [`ConnectionChoice`]:
//!
//! - **The active profile** of `wyck-config`: its service, endpoint and display name, with the
//!   token read from the OS keyring. This is the normal way.
//! - **The environment**: service, endpoint and token given directly. For development and demo
//!   accounts.
//!
//! A missing profile is not a crash: it is a state the front end explains (`NoProfile`), because
//! a first run has no profile and must say what to do.

use wyck_config::{AppPaths, KeyringSecretStore, WyckConfig};
use wyck_engine::EngineError;
use wyck_engine::broker::ConnectRequest;

use crate::settings::ConnectionChoice;

/// Why no connection could be built.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    /// No profile is configured or none is marked active. Normal on a first run.
    #[error("no account profile is set up yet")]
    NoProfile,
    /// The configuration files or the credential store could not be read.
    #[error("could not read the configuration: {0}")]
    Config(String),
    /// The profile exists but cannot be turned into a connection (unknown service, missing token).
    #[error(transparent)]
    Engine(#[from] EngineError),
}

/// Builds the connection request for `choice`.
///
/// `open_config` is only called when the active profile is needed, so an environment
/// connection never touches the keyring. Tests pass their own loader.
///
/// # Errors
///
/// [`StartupError::NoProfile`] when there is no active profile, [`StartupError::Config`] when the
/// configuration cannot be read, [`StartupError::Engine`] when the profile cannot be used.
pub fn connect_request(
    choice: ConnectionChoice,
    open_config: impl FnOnce() -> Result<WyckConfig, StartupError>,
) -> Result<ConnectRequest, StartupError> {
    match choice {
        ConnectionChoice::Environment {
            service,
            endpoint,
            token,
        } => {
            let mut request = ConnectRequest::new(service, endpoint, token);
            request.label = "environment".to_owned();
            Ok(request)
        }
        ConnectionChoice::ActiveProfile => {
            let config = open_config()?;
            let profile = config.active_profile().ok_or(StartupError::NoProfile)?;
            Ok(ConnectRequest::from_profile(&config, &profile.id)?)
        }
    }
}

/// Opens the user's real configuration: the OS-standard directories, and the OS keyring for tokens.
///
/// # Errors
///
/// [`StartupError::Config`] when the directories cannot be resolved or the file cannot be read.
pub fn open_user_config() -> Result<WyckConfig, StartupError> {
    let paths = AppPaths::discover().map_err(|e| StartupError::Config(e.to_string()))?;
    WyckConfig::load(paths, Box::new(KeyringSecretStore::default()))
        .map_err(|e| StartupError::Config(e.to_string()))
}

#[cfg(test)]
mod tests {
    use secrecy::{ExposeSecret, SecretString};
    use wyck_config::EncryptedFileSecretStore;
    use wyck_engine::broker::ServiceKind;

    use super::*;

    fn config_in(dir: &std::path::Path) -> WyckConfig {
        let paths = AppPaths::at(dir);
        let store = EncryptedFileSecretStore::new(
            paths.secrets_dir(),
            SecretString::from("test-passphrase"),
        );
        WyckConfig::load(paths, Box::new(store)).unwrap()
    }

    #[test]
    fn an_environment_connection_never_opens_the_configuration() {
        let request = connect_request(
            ConnectionChoice::Environment {
                service: ServiceKind::CtraderLocal,
                endpoint: "http://127.0.0.1:9876/mcp/".to_owned(),
                token: None,
            },
            || panic!("the configuration must not be opened"),
        )
        .unwrap();
        assert_eq!(request.service, ServiceKind::CtraderLocal);
        assert_eq!(request.endpoint, "http://127.0.0.1:9876/mcp/");
    }

    #[test]
    fn the_token_of_an_environment_connection_is_passed_on_and_not_printed() {
        let request = connect_request(
            ConnectionChoice::Environment {
                service: ServiceKind::CtraderRemote,
                endpoint: "https://example.invalid/mcp".to_owned(),
                token: Some(SecretString::from("super-secret-token")),
            },
            || panic!("unused"),
        )
        .unwrap();
        assert_eq!(
            request.token.as_ref().unwrap().expose_secret(),
            "super-secret-token"
        );
        assert!(!format!("{request:?}").contains("super-secret-token"));
    }

    #[test]
    fn a_first_run_has_no_profile_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let error = connect_request(ConnectionChoice::ActiveProfile, || {
            Ok(config_in(dir.path()))
        })
        .unwrap_err();
        assert!(matches!(error, StartupError::NoProfile), "{error:?}");
    }

    #[test]
    fn the_active_profile_becomes_a_request_with_its_token() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = config_in(dir.path());
        let id = config
            .add_profile(
                "Demo",
                "ctrader-remote",
                None,
                Some(SecretString::from("tok")),
            )
            .unwrap();
        config.set_active_profile(Some(id)).unwrap();
        drop(config);

        let request = connect_request(ConnectionChoice::ActiveProfile, || {
            Ok(config_in(dir.path()))
        })
        .unwrap();
        assert_eq!(request.service, ServiceKind::CtraderRemote);
        assert_eq!(request.label, "Demo");
        assert_eq!(request.token.as_ref().unwrap().expose_secret(), "tok");
    }

    #[test]
    fn a_broken_loader_is_reported_as_a_configuration_problem() {
        let error = connect_request(ConnectionChoice::ActiveProfile, || {
            Err(StartupError::Config("disk".into()))
        })
        .unwrap_err();
        assert!(error.to_string().contains("disk"));
    }
}
