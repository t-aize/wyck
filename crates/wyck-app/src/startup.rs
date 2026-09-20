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

use secrecy::SecretString;
use wyck_config::{AppPaths, KeyringSecretStore, WyckConfig};
use wyck_engine::EngineError;
use wyck_engine::broker::{ConnectRequest, ServiceKind};

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

/// The name of the saved account for a service.
fn profile_name(service: ServiceKind) -> &'static str {
    match service {
        ServiceKind::CtraderLocal => "cTrader Desktop (local)",
        _ => "cTrader Remote",
    }
}

/// Saves an account the user has just connected to and makes it the active one, so the next
/// start connects by itself. The token goes to the credential store, never to the config file.
///
/// There is one saved account per service: a previous one with the same name is replaced.
/// `open_config` is injected like in [`connect_request`].
///
/// # Errors
///
/// [`StartupError::Config`] when the configuration or the credential store cannot be written.
pub fn remember_connection(
    service: ServiceKind,
    token: Option<SecretString>,
    open_config: impl FnOnce() -> Result<WyckConfig, StartupError>,
) -> Result<(), StartupError> {
    let tag = match service {
        ServiceKind::CtraderLocal => "ctrader-local",
        _ => "ctrader-remote",
    };
    let name = profile_name(service);
    let mut config = open_config()?;
    let stale: Vec<_> = config
        .profiles()
        .iter()
        .filter(|p| p.display_name == name)
        .map(|p| p.id.clone())
        .collect();
    let fail = |e: wyck_config::ConfigError| StartupError::Config(e.to_string());
    for id in stale {
        config.remove_profile(&id).map_err(fail)?;
    }
    let id = config.add_profile(name, tag, None, token).map_err(fail)?;
    config.set_active_profile(Some(id)).map_err(fail)?;
    Ok(())
}

/// The symbol remembered from the last run, if there is one. A configuration that cannot be read
/// is not worth a message here: the application then simply opens on its default symbol.
pub fn last_symbol(
    open_config: impl FnOnce() -> Result<WyckConfig, StartupError>,
) -> Option<String> {
    open_config()
        .ok()
        .and_then(|config| config.last_symbol().map(str::to_owned))
}

/// Remembers the symbol the user is on, for the next start.
///
/// # Errors
///
/// [`StartupError::Config`] when the configuration cannot be written.
pub fn remember_symbol(
    symbol: &str,
    open_config: impl FnOnce() -> Result<WyckConfig, StartupError>,
) -> Result<(), StartupError> {
    open_config()?
        .set_last_symbol(Some(symbol.to_owned()))
        .map_err(|e| StartupError::Config(e.to_string()))
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
    fn a_remembered_token_is_the_next_startup_connection() {
        let dir = tempfile::tempdir().unwrap();
        remember_connection(
            ServiceKind::CtraderRemote,
            Some(SecretString::from("tok-1")),
            || Ok(config_in(dir.path())),
        )
        .unwrap();
        let request = connect_request(ConnectionChoice::ActiveProfile, || {
            Ok(config_in(dir.path()))
        })
        .unwrap();
        assert_eq!(request.service, ServiceKind::CtraderRemote);
        assert_eq!(request.token.as_ref().unwrap().expose_secret(), "tok-1");
    }

    #[test]
    fn remembering_again_replaces_the_account_instead_of_piling_up() {
        let dir = tempfile::tempdir().unwrap();
        for token in ["old", "new"] {
            remember_connection(
                ServiceKind::CtraderRemote,
                Some(SecretString::from(token)),
                || Ok(config_in(dir.path())),
            )
            .unwrap();
        }
        let config = config_in(dir.path());
        assert_eq!(config.profiles().len(), 1);
        let id = config.active_profile().unwrap().id.clone();
        assert_eq!(
            config.token_for(&id).unwrap().unwrap().expose_secret(),
            "new"
        );
    }

    #[test]
    fn a_local_account_is_remembered_without_a_token_and_switches_the_active_one() {
        let dir = tempfile::tempdir().unwrap();
        remember_connection(
            ServiceKind::CtraderRemote,
            Some(SecretString::from("tok")),
            || Ok(config_in(dir.path())),
        )
        .unwrap();
        remember_connection(ServiceKind::CtraderLocal, None, || {
            Ok(config_in(dir.path()))
        })
        .unwrap();
        let request = connect_request(ConnectionChoice::ActiveProfile, || {
            Ok(config_in(dir.path()))
        })
        .unwrap();
        assert_eq!(request.service, ServiceKind::CtraderLocal);
        assert!(request.token.is_none());
        assert_eq!(config_in(dir.path()).profiles().len(), 2);
    }

    #[test]
    fn remembering_reports_a_configuration_that_cannot_be_opened() {
        let error = remember_connection(ServiceKind::CtraderLocal, None, || {
            Err(StartupError::Config("locked".into()))
        })
        .unwrap_err();
        assert!(error.to_string().contains("locked"));
    }

    #[test]
    fn the_last_symbol_is_remembered_between_runs() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(last_symbol(|| Ok(config_in(dir.path()))), None);
        remember_symbol("XAUUSD", || Ok(config_in(dir.path()))).unwrap();
        assert_eq!(
            last_symbol(|| Ok(config_in(dir.path()))).as_deref(),
            Some("XAUUSD")
        );
        assert_eq!(
            last_symbol(|| Err(StartupError::Config("locked".into()))),
            None,
            "an unreadable configuration is not an error here"
        );
        let error =
            remember_symbol("XAUUSD", || Err(StartupError::Config("locked".into()))).unwrap_err();
        assert!(error.to_string().contains("locked"));
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
