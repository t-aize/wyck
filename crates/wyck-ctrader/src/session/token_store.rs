//! Where [`crate::session::Session`] keeps the tokens between runs.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::auth::TokenSet;
use crate::error::Result;

/// Where the tokens are kept between runs.
///
/// The session calls [`TokenStore::save`] every time it gets a new pair, **before** using it (the
/// old refresh token stops working the moment the new pair is issued, so a pair lost to a crash is
/// a lost sign in). Implement it on top of the OS keyring or an encrypted file; never store tokens
/// in plain text.
#[async_trait]
pub trait TokenStore: Send + Sync + 'static {
    /// The stored tokens, if any.
    ///
    /// # Errors
    ///
    /// Whatever the storage reports.
    async fn load(&self) -> Result<Option<TokenSet>>;

    /// Stores the tokens, replacing what was there.
    ///
    /// # Errors
    ///
    /// Whatever the storage reports. A failure to save ends the session: continuing would risk
    /// losing the only valid refresh token.
    async fn save(&self, tokens: &TokenSet) -> Result<()>;
}

/// A [`TokenStore`] that keeps the tokens in memory only. For tests and short lived programs.
#[derive(Debug, Default)]
pub struct MemoryTokenStore {
    tokens: Mutex<Option<TokenSet>>,
}

#[async_trait]
impl TokenStore for MemoryTokenStore {
    async fn load(&self) -> Result<Option<TokenSet>> {
        Ok(self
            .tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone())
    }

    async fn save(&self, tokens: &TokenSet) -> Result<()> {
        *self
            .tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(tokens.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;
    use std::time::SystemTime;

    #[tokio::test]
    async fn the_memory_store_keeps_what_it_is_given() {
        let store = MemoryTokenStore::default();
        assert!(store.load().await.unwrap().is_none());
        let tokens = crate::auth::parse_token_response(
            br#"{"accessToken":"a","refreshToken":"r","expiresIn":10}"#,
            SystemTime::now(),
        )
        .unwrap();
        store.save(&tokens).await.unwrap();
        let loaded = store.load().await.unwrap().unwrap();
        assert_eq!(loaded.access_token.expose_secret(), "a");
    }
}
