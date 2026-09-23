//! Keeps a session's OAuth tokens in the app's credential store, so they survive a restart and a
//! token the session renews by itself is saved before it is used.

use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use wyck::config::{OpenApiTokenStorage, OpenApiTokens};
use wyck::openapi::OpenApiError;
use wyck::openapi::auth::TokenSet;
use wyck::openapi::session::TokenStore;

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
    async fn load(&self) -> wyck::openapi::Result<Option<TokenSet>> {
        self.storage
            .load()
            .map(|stored| stored.map(to_token_set))
            .map_err(|error| OpenApiError::Config(format!("could not read the tokens: {error}")))
    }

    async fn save(&self, tokens: &TokenSet) -> wyck::openapi::Result<()> {
        self.storage
            .save(&to_stored(tokens))
            .map_err(|error| OpenApiError::Config(format!("could not save the tokens: {error}")))
    }
}
