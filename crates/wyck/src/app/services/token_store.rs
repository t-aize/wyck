//! Keeps a session's OAuth tokens in the app's credential store, so they survive a restart and a
//! token the session renews by itself is saved before it is used.

use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use wyck_config::{OpenApiTokenStorage, OpenApiTokens};
use wyck_openapi::OpenApiError;
use wyck_openapi::auth::TokenSet;
use wyck_openapi::session::TokenStore;

pub struct ConfigTokenStore {
    storage: OpenApiTokenStorage,
}

impl ConfigTokenStore {
    pub fn new(storage: OpenApiTokenStorage) -> Self {
        Self { storage }
    }
}

/// The session's view of a stored token pair. The store keeps the moment the access token
/// expires; the session wants how long it lasts and when it was received, so the pair is treated
/// as received now.
pub fn to_token_set(stored: OpenApiTokens) -> TokenSet {
    let now = SystemTime::now();
    TokenSet {
        access_token: stored.access_token,
        refresh_token: stored.refresh_token,
        token_type: None,
        expires_in: stored
            .expires_at
            .map(|at| at.duration_since(now).unwrap_or(Duration::ZERO)),
        obtained_at: now,
    }
}

pub fn to_stored(tokens: &TokenSet) -> OpenApiTokens {
    OpenApiTokens {
        access_token: tokens.access_token.clone(),
        refresh_token: tokens.refresh_token.clone(),
        expires_at: tokens.expires_at(),
    }
}

#[async_trait]
impl TokenStore for ConfigTokenStore {
    async fn load(&self) -> wyck_openapi::Result<Option<TokenSet>> {
        self.storage
            .load()
            .map(|stored| stored.map(to_token_set))
            .map_err(|error| OpenApiError::Config(format!("could not read the tokens: {error}")))
    }

    async fn save(&self, tokens: &TokenSet) -> wyck_openapi::Result<()> {
        self.storage
            .save(&to_stored(tokens))
            .map_err(|error| OpenApiError::Config(format!("could not save the tokens: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::{ExposeSecret, SecretString};

    #[test]
    fn a_token_pair_goes_to_the_store_and_back() {
        let now = SystemTime::now();
        let tokens = TokenSet {
            access_token: SecretString::from("access"),
            refresh_token: SecretString::from("refresh"),
            token_type: None,
            expires_in: Some(Duration::from_secs(3_600)),
            obtained_at: now,
        };
        let stored = to_stored(&tokens);
        assert_eq!(stored.access_token.expose_secret(), "access");
        let back = to_token_set(stored);
        assert_eq!(back.access_token.expose_secret(), "access");
        assert_eq!(back.refresh_token.expose_secret(), "refresh");
        // About an hour left, counted from now.
        let left = back.expires_in.unwrap().as_secs();
        assert!((3_590..=3_600).contains(&left), "{left}");
    }

    #[test]
    fn an_expired_token_has_no_time_left() {
        let stored = OpenApiTokens {
            access_token: SecretString::from("a"),
            refresh_token: SecretString::from("r"),
            expires_at: Some(SystemTime::now() - Duration::from_secs(60)),
        };
        assert_eq!(to_token_set(stored).expires_in, Some(Duration::ZERO));
    }
}
