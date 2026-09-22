//! OAuth sign in for the cTrader Open API. The application opens the returned URL and
//! selects one of the accounts returned by [`OpenApiAuthorization::finish`].

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use ctrader_openapi::auth::{
    CallbackListener, OAuthClient, Scope, TokenSet, authorization_url, new_state,
};
use ctrader_openapi::session::TokenStore;
use ctrader_openapi::{ClientBuilder, ClientCredentials, Environment};
use secrecy::{ExposeSecret, SecretString};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use wyck_config::{OpenApiTokens, ProfileId, WyckConfig};

use crate::error::{EngineError, Result};

/// The default localhost port to register in the Open API portal.
pub const DEFAULT_CALLBACK_PORT: u16 = 8765;

/// One account to which the user granted access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiAccount {
    /// The cTrader account ID used for API requests.
    pub id: i64,
    /// `None` when the server did not say whether this is a live account.
    pub is_live: Option<bool>,
    /// The login number shown by cTrader.
    pub login: Option<i64>,
    /// The broker's short name.
    pub broker: Option<String>,
}

/// Credentials and tokens from a completed browser authorization.
#[derive(Debug, Clone)]
pub struct OpenApiGrant {
    /// Public application identifier.
    pub client_id: String,
    /// Application secret, to keep in the credential store.
    pub client_secret: SecretString,
    /// Access token for the selected account.
    pub access_token: SecretString,
    /// Refresh token, replaced on every renewal.
    pub refresh_token: SecretString,
    /// Access token expiry when the server supplied one.
    pub expires_at: Option<SystemTime>,
    /// Accounts offered for selection.
    pub accounts: Vec<OpenApiAccount>,
}

/// Persists refreshed Open API tokens through a profile's configured secret store.
/// The pair is written under one key before the session uses it.
pub struct ConfigTokenStore {
    config: Arc<WyckConfig>,
    profile_id: ProfileId,
}

impl ConfigTokenStore {
    /// Binds the store to one profile.
    #[must_use]
    pub fn new(config: Arc<WyckConfig>, profile_id: ProfileId) -> Self {
        Self { config, profile_id }
    }
}

#[async_trait]
impl TokenStore for ConfigTokenStore {
    async fn load(&self) -> ctrader_openapi::Result<Option<TokenSet>> {
        let config = Arc::clone(&self.config);
        let id = self.profile_id.clone();
        let stored = tokio::task::spawn_blocking(move || config.openapi_tokens(&id))
            .await
            .map_err(|_| ctrader_openapi::OpenApiError::Auth("token store task failed".into()))?
            .map_err(|_| ctrader_openapi::OpenApiError::Auth("could not load tokens".into()))?;
        Ok(stored.map(|tokens| {
            let now = SystemTime::now();
            TokenSet {
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
                token_type: Some("bearer".into()),
                expires_in: tokens
                    .expires_at
                    .map(|expiry| expiry.duration_since(now).unwrap_or(Duration::ZERO)),
                obtained_at: now,
            }
        }))
    }

    async fn save(&self, tokens: &TokenSet) -> ctrader_openapi::Result<()> {
        let config = Arc::clone(&self.config);
        let id = self.profile_id.clone();
        let stored = OpenApiTokens {
            access_token: tokens.access_token.clone(),
            refresh_token: tokens.refresh_token.clone(),
            expires_at: tokens.expires_at(),
        };
        tokio::task::spawn_blocking(move || config.save_openapi_tokens(&id, &stored))
            .await
            .map_err(|_| ctrader_openapi::OpenApiError::Auth("token store task failed".into()))?
            .map_err(|_| ctrader_openapi::OpenApiError::Auth("could not save tokens".into()))
    }
}

/// A pending sign in. Dropping it cancels the local callback listener.
pub struct OpenApiAuthorization {
    /// Consent page to open in the user's browser.
    pub url: String,
    /// Exact callback URI used by the consent page.
    pub callback_uri: String,
    listener: CallbackListener,
    state: String,
    credentials: ClientCredentials,
    runtime: Handle,
}

struct CancelOnDrop<T>(JoinHandle<T>);

impl<T> Drop for CancelOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl std::fmt::Debug for OpenApiAuthorization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenApiAuthorization")
            .field("callback_uri", &self.callback_uri)
            .finish_non_exhaustive()
    }
}

impl OpenApiAuthorization {
    /// Waits up to five minutes for the browser callback, exchanges its code and lists
    /// the accounts covered by the grant. Dropping this future cancels the attempt.
    pub async fn finish(self) -> Result<OpenApiGrant> {
        let mut done = CancelOnDrop(self.runtime.spawn(async move {
            let code = self
                .listener
                .wait(&self.state, Duration::from_secs(300))
                .await
                .map_err(auth_error)?;
            let oauth = OAuthClient::new(self.credentials.clone()).map_err(auth_error)?;
            let tokens = oauth
                .exchange_code(code.code(), &self.callback_uri)
                .await
                .map_err(auth_error)?;
            let client = ClientBuilder::new(Environment::Demo)
                .credentials(self.credentials.clone())
                .connect()
                .await
                .map_err(auth_error)?;
            let accounts = client
                .accounts(tokens.access_token.expose_secret())
                .await
                .map_err(auth_error);
            client.close().await;
            let accounts = accounts?;
            let expires_at = tokens.expires_at();
            Ok(OpenApiGrant {
                client_id: self.credentials.client_id,
                client_secret: self.credentials.client_secret,
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
                expires_at,
                accounts: accounts
                    .ctid_trader_account
                    .into_iter()
                    .map(|account| OpenApiAccount {
                        id: account.ctid_trader_account_id,
                        is_live: account.is_live,
                        login: account.trader_login,
                        broker: account.broker_title_short,
                    })
                    .collect(),
            })
        }));
        (&mut done.0).await.map_err(|_| EngineError::ShuttingDown)?
    }
}

pub(crate) async fn begin(
    runtime: Handle,
    client_id: String,
    client_secret: SecretString,
    callback_port: u16,
) -> Result<OpenApiAuthorization> {
    if client_id.trim().is_empty() || client_secret.expose_secret().trim().is_empty() {
        return Err(EngineError::Invalid(
            "Open API client ID and secret are required".into(),
        ));
    }
    if callback_port == 0 {
        return Err(EngineError::Invalid(
            "the Open API callback port must be between 1 and 65535".into(),
        ));
    }
    let listener = CallbackListener::bind(callback_port)
        .await
        .map_err(auth_error)?;
    let callback_uri = listener.redirect_uri();
    let credentials = ClientCredentials {
        client_id,
        client_secret,
    };
    let state = new_state();
    let url = authorization_url(
        &credentials.client_id,
        &callback_uri,
        Scope::Trading,
        &state,
    );
    Ok(OpenApiAuthorization {
        url,
        callback_uri,
        listener,
        state,
        credentials,
        runtime,
    })
}

fn auth_error(error: ctrader_openapi::OpenApiError) -> EngineError {
    EngineError::Invalid(format!("Open API authorization failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyck_config::{AppPaths, EncryptedFileSecretStore};

    #[tokio::test]
    async fn invalid_credentials_do_not_open_a_callback_listener() {
        let error = begin(
            Handle::current(),
            "".into(),
            SecretString::from("secret"),
            DEFAULT_CALLBACK_PORT,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, EngineError::Invalid(_)));

        let error = begin(
            Handle::current(),
            "client".into(),
            SecretString::from("secret"),
            0,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, EngineError::Invalid(_)));
    }

    #[tokio::test]
    async fn callback_is_local_and_the_consent_requests_trading() {
        let reserved = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = reserved.local_addr().unwrap().port();
        drop(reserved);
        let attempt = begin(
            Handle::current(),
            "test-client".into(),
            SecretString::from("test-secret"),
            port,
        )
        .await
        .unwrap();
        assert_eq!(attempt.callback_uri, format!("http://localhost:{port}"));
        assert!(attempt.url.contains("scope=trading"));
        assert!(!attempt.url.contains("test-secret"));
    }

    #[tokio::test]
    async fn cancelling_sign_in_releases_the_callback_port() {
        let reserved = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = reserved.local_addr().unwrap().port();
        drop(reserved);
        let attempt = begin(
            Handle::current(),
            "test-client".into(),
            SecretString::from("test-secret"),
            port,
        )
        .await
        .unwrap();
        let task = tokio::spawn(attempt.finish());
        tokio::task::yield_now().await;
        task.abort();
        let _ = task.await;
        let rebound = std::net::TcpListener::bind(("127.0.0.1", port));
        assert!(rebound.is_ok());
    }

    #[tokio::test]
    async fn rotated_tokens_are_saved_and_loaded_as_a_pair() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path());
        let secrets =
            EncryptedFileSecretStore::new(paths.secrets_dir(), SecretString::from("passphrase"));
        let mut config = WyckConfig::load(paths, Box::new(secrets)).unwrap();
        let id = config
            .add_profile("demo", "ctrader-openapi", None, None)
            .unwrap();
        let store = ConfigTokenStore::new(Arc::new(config), id);
        let tokens = TokenSet {
            access_token: SecretString::from("new-access"),
            refresh_token: SecretString::from("new-refresh"),
            token_type: Some("bearer".into()),
            expires_in: Some(Duration::from_secs(120)),
            obtained_at: SystemTime::now(),
        };
        store.save(&tokens).await.unwrap();
        let loaded = store.load().await.unwrap().unwrap();
        assert_eq!(loaded.access_token.expose_secret(), "new-access");
        assert_eq!(loaded.refresh_token.expose_secret(), "new-refresh");
    }
}
