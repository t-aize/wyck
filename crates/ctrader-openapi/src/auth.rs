//! Signing a user in with OAuth 2, and keeping the tokens fresh.
//!
//! The Open API does not take a user name and password. The user grants an **application**
//! access to their cTrader ID in the browser; the application then holds tokens. The steps:
//!
//! 1. The application is registered on the Open API portal, which gives it a **client id**, a
//!    **client secret** and lets it register **redirect URIs** (see [`crate::config::ClientCredentials`]).
//! 2. [`authorization_url`] builds the address of the consent page. The user opens it, picks the
//!    accounts and the [`Scope`], and is sent back to the redirect URI with a `code` in the query.
//!    [`crate::callback::CallbackListener`] catches that on `localhost`.
//! 3. [`OAuthClient::exchange_code`] trades the code (valid **one minute**) for a [`TokenSet`]: an
//!    access token (about 30 days) and a refresh token.
//! 4. Before the access token expires, [`OAuthClient::refresh`] trades the refresh token for a new
//!    pair. **The old refresh token stops working**: store the new pair before using it.
//! 5. The access token goes to the connection: [`crate::Client::accounts`], then
//!    [`crate::Client::authorize_account`].
//!
//! Errors are told apart on purpose: a refusal by the server (a bad code, a revoked refresh token) is
//! [`OpenApiError::Auth`] and will not get better by trying again, while a failure to reach the endpoint
//! is [`OpenApiError::Transport`] and may. The session relies on that difference.
//!
//! The token endpoint is a plain HTTPS `GET` with the secret in the query string. That is how the
//! server wants it, and it is why errors from the HTTP layer are stripped of their URL here: a
//! message that included it would put the client secret and the code in a log.

use std::time::{Duration, SystemTime};

use reqwest::Url;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

use crate::config::ClientCredentials;
use crate::error::{OpenApiError, Result};
use crate::model::flex;

/// The consent page, where the user grants access.
pub const AUTHORIZE_URL: &str = "https://id.ctrader.com/my/settings/openapi/grantingaccess/";

/// The token endpoint.
pub const TOKEN_URL: &str = "https://openapi.ctrader.com/apps/token";

/// How much the user grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// View only: account data and market data, no orders. Enough for charts.
    Accounts,
    /// Full access, orders included.
    Trading,
}

impl Scope {
    /// The value of the `scope` parameter.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accounts => "accounts",
            Self::Trading => "trading",
        }
    }
}

/// A fresh random value for the `state` parameter, which ties the consent page's answer to the
/// request that started it.
#[must_use]
pub fn new_state() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// The address of the consent page for an application, a redirect URI and a scope.
///
/// `redirect_uri` must be one of the URIs registered for the application, character for
/// character. `state` is echoed back on the redirect (see [`new_state`]).
#[must_use]
pub fn authorization_url(client_id: &str, redirect_uri: &str, scope: Scope, state: &str) -> String {
    let mut url = Url::parse(AUTHORIZE_URL).expect("the consent page address is a valid URL");
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", scope.as_str())
        .append_pair("product", "web")
        .append_pair("state", state);
    url.into()
}

/// The tokens of a signed in user.
///
/// Both tokens are secrets: `Debug` shows them redacted, and they are wiped from memory when
/// dropped. Store them in the OS keyring or an encrypted file, never in plain text.
#[derive(Debug, Clone)]
pub struct TokenSet {
    /// The token that opens the connection to an account.
    pub access_token: SecretString,
    /// The token that gets a new pair.
    pub refresh_token: SecretString,
    /// The token type the server reported (`bearer`).
    pub token_type: Option<String>,
    /// How long the access token lasts, from `obtained_at`.
    pub expires_in: Option<Duration>,
    /// When the pair was received.
    pub obtained_at: SystemTime,
}

impl TokenSet {
    /// When the access token expires, if the server said how long it lasts.
    #[must_use]
    pub fn expires_at(&self) -> Option<SystemTime> {
        self.expires_in
            .and_then(|d| self.obtained_at.checked_add(d))
    }

    /// Whether the access token is expired, or will be within `margin`. Refresh a token that
    /// answers `true`. Without a known lifetime it is never reported expired: rely on the server's
    /// `OA_AUTH_TOKEN_EXPIRED` then.
    #[must_use]
    pub fn expires_within(&self, now: SystemTime, margin: Duration) -> bool {
        self.expires_at()
            .is_some_and(|at| at <= now.checked_add(margin).unwrap_or(now))
    }
}

/// The token endpoint's answer. It reports failures inside a normal answer, with `errorCode`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default, deserialize_with = "flex::opt")]
    expires_in: Option<i64>,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// Reads the token endpoint's answer into a [`TokenSet`].
///
/// # Errors
///
/// [`OpenApiError::Auth`] when the answer holds an error, or lacks a token. The text names the
/// server's error code and description, never a token.
pub fn parse_token_response(body: &[u8], now: SystemTime) -> Result<TokenSet> {
    let response: TokenResponse = serde_json::from_slice(body)
        .map_err(|_| OpenApiError::Auth("the token endpoint sent an unreadable answer".into()))?;
    if let Some(code) = response.error_code.filter(|c| !c.is_empty()) {
        let detail = response.description.filter(|d| !d.is_empty());
        return Err(OpenApiError::Auth(match detail {
            Some(detail) => format!("{code}: {detail}"),
            None => code,
        }));
    }
    let (Some(access), Some(refresh)) = (response.access_token, response.refresh_token) else {
        return Err(OpenApiError::Auth(
            "the token endpoint answered without tokens".into(),
        ));
    };
    if access.is_empty() || refresh.is_empty() {
        return Err(OpenApiError::Auth(
            "the token endpoint sent an empty token".into(),
        ));
    }
    Ok(TokenSet {
        access_token: SecretString::from(access),
        refresh_token: SecretString::from(refresh),
        token_type: response.token_type,
        expires_in: response
            .expires_in
            .and_then(|s| u64::try_from(s).ok())
            .map(Duration::from_secs),
        obtained_at: now,
    })
}

/// Talks to the token endpoint for one application.
#[derive(Debug, Clone)]
pub struct OAuthClient {
    http: reqwest::Client,
    credentials: ClientCredentials,
    token_url: String,
}

impl OAuthClient {
    /// A client for `credentials`, using the real token endpoint.
    ///
    /// # Errors
    ///
    /// [`OpenApiError::Config`] when the HTTP client cannot be built (no TLS backend, for one).
    pub fn new(credentials: ClientCredentials) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                OpenApiError::Config(format!("cannot build the HTTP client: {}", e.without_url()))
            })?;
        Ok(Self {
            http,
            credentials,
            token_url: TOKEN_URL.to_owned(),
        })
    }

    /// Uses another token endpoint, for a test server.
    #[must_use]
    pub fn with_token_url(mut self, url: impl Into<String>) -> Self {
        self.token_url = url.into();
        self
    }

    /// Trades an authorization code for tokens. The code lives one minute and works once.
    ///
    /// `redirect_uri` must be the one the consent page was opened with.
    ///
    /// # Errors
    ///
    /// [`OpenApiError::Auth`] for a refused, expired or reused code, or when the endpoint cannot
    /// be reached.
    pub async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<TokenSet> {
        self.request_tokens(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ])
        .await
    }

    /// Trades a refresh token for a new pair. The old refresh token stops working, so store the
    /// result before anything else.
    ///
    /// # Errors
    ///
    /// [`OpenApiError::Auth`] when the refresh token is unknown, already used, or revoked: the user
    /// has to sign in again.
    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenSet> {
        self.request_tokens(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ])
        .await
    }

    async fn request_tokens(&self, params: &[(&str, &str)]) -> Result<TokenSet> {
        let mut url = Url::parse(&self.token_url)
            .map_err(|_| OpenApiError::Config("the token endpoint is not a valid URL".into()))?;
        {
            let mut query = url.query_pairs_mut();
            for (key, value) in params {
                query.append_pair(key, value);
            }
            query
                .append_pair("client_id", &self.credentials.client_id)
                .append_pair(
                    "client_secret",
                    self.credentials.client_secret.expose_secret(),
                );
        }
        // `without_url` matters: the URL holds the secret and the code.
        let response = self.http.get(url).send().await.map_err(|e| {
            OpenApiError::Transport(format!(
                "the token endpoint could not be reached: {}",
                e.without_url()
            ))
        })?;
        let status = response.status();
        let body = response.bytes().await.map_err(|e| {
            OpenApiError::Transport(format!(
                "the token answer could not be read: {}",
                e.without_url()
            ))
        })?;
        match parse_token_response(&body, SystemTime::now()) {
            Ok(tokens) if status.is_success() => Ok(tokens),
            Ok(_) => Err(OpenApiError::Auth(format!(
                "the token endpoint answered {status}"
            ))),
            // An error body is more useful than the bare status.
            Err(error) if status.is_success() => Err(error),
            Err(OpenApiError::Auth(text)) if !text.starts_with("the token endpoint") => {
                Err(OpenApiError::Auth(format!("{text} ({status})")))
            }
            Err(_) => Err(OpenApiError::Auth(format!(
                "the token endpoint answered {status}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: SystemTime = SystemTime::UNIX_EPOCH;

    #[test]
    fn the_consent_url_carries_every_parameter_encoded() {
        let url = authorization_url(
            "id 1",
            "http://localhost:8765/cb?x=1",
            Scope::Accounts,
            "st4te",
        );
        assert!(url.starts_with(AUTHORIZE_URL));
        let parsed = Url::parse(&url).unwrap();
        let pairs: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(pairs["client_id"], "id 1");
        assert_eq!(pairs["redirect_uri"], "http://localhost:8765/cb?x=1");
        assert_eq!(pairs["scope"], "accounts");
        assert_eq!(pairs["product"], "web");
        assert_eq!(pairs["state"], "st4te");
        assert!(!url.contains(' '), "spaces are encoded");
    }

    #[test]
    fn scopes_have_their_parameter_values() {
        assert_eq!(Scope::Accounts.as_str(), "accounts");
        assert_eq!(Scope::Trading.as_str(), "trading");
    }

    #[test]
    fn states_are_unique_and_url_safe() {
        let (a, b) = (new_state(), new_state());
        assert_ne!(a, b);
        assert!(a.len() >= 32 && a.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn a_good_answer_gives_tokens_with_a_lifetime() {
        let body = br#"{"accessToken":"AT","refreshToken":"RT","tokenType":"bearer","expiresIn":2628000,"errorCode":null,"description":null}"#;
        let tokens = parse_token_response(body, NOW).unwrap();
        assert_eq!(tokens.access_token.expose_secret(), "AT");
        assert_eq!(tokens.refresh_token.expose_secret(), "RT");
        assert_eq!(tokens.expires_in, Some(Duration::from_secs(2_628_000)));
        assert_eq!(tokens.token_type.as_deref(), Some("bearer"));
    }

    #[test]
    fn an_error_inside_a_normal_answer_is_an_error() {
        let body = br#"{"accessToken":null,"errorCode":"ACCESS_DENIED","description":"Invalid authorization code"}"#;
        let error = parse_token_response(body, NOW).unwrap_err();
        assert_eq!(
            error.to_string(),
            "sign in failed: ACCESS_DENIED: Invalid authorization code"
        );
    }

    #[test]
    fn an_answer_without_tokens_or_with_empty_ones_is_refused() {
        assert!(parse_token_response(br#"{}"#, NOW).is_err());
        assert!(parse_token_response(br#"{"accessToken":"","refreshToken":"x"}"#, NOW).is_err());
        assert!(parse_token_response(b"<html>", NOW).is_err());
    }

    #[test]
    fn expiry_is_computed_from_when_the_pair_was_received() {
        let tokens = parse_token_response(
            br#"{"accessToken":"a","refreshToken":"r","expiresIn":1000}"#,
            NOW,
        )
        .unwrap();
        assert_eq!(tokens.expires_at(), Some(NOW + Duration::from_secs(1000)));
        let margin = Duration::from_secs(100);
        assert!(!tokens.expires_within(NOW + Duration::from_secs(800), margin));
        assert!(tokens.expires_within(NOW + Duration::from_secs(950), margin));
        assert!(tokens.expires_within(NOW + Duration::from_secs(2000), margin));
    }

    #[test]
    fn a_token_without_a_known_lifetime_is_not_reported_expired() {
        let tokens =
            parse_token_response(br#"{"accessToken":"a","refreshToken":"r"}"#, NOW).unwrap();
        assert_eq!(tokens.expires_at(), None);
        assert!(!tokens.expires_within(NOW + Duration::from_secs(10_000_000), Duration::ZERO));
    }

    #[test]
    fn the_tokens_never_show_in_debug() {
        let tokens = parse_token_response(
            br#"{"accessToken":"very-secret-access","refreshToken":"very-secret-refresh"}"#,
            NOW,
        )
        .unwrap();
        let shown = format!("{tokens:?}");
        assert!(!shown.contains("very-secret"), "{shown}");
    }
}
