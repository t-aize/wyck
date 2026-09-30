//! The OAuth token pair of a profile, kept as one credential.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use secrecy::{ExposeSecret, SecretString};

use crate::config::error::{ConfigError, Result};
use crate::config::secret::{SecretKey, SecretStore};

/// One OAuth token pair kept under a single credential-store key.
#[derive(Debug, Clone)]
pub struct OpenApiTokens {
    pub access_token: SecretString,
    pub refresh_token: SecretString,
    pub expires_at: Option<SystemTime>,
}

/// What is written in the credential store: the pair as one JSON text.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredOpenApiTokens {
    access_token: String,
    refresh_token: String,
    expires_at: Option<u64>,
}

/// Where one profile's OAuth token pair lives in the credential store: a cheap, cloneable handle
/// from `crate::config::WyckConfig::openapi_token_storage`.
#[derive(Clone)]
pub struct OpenApiTokenStorage {
    pub(crate) secrets: Arc<dyn SecretStore>,
    pub(crate) key: SecretKey,
}

impl OpenApiTokenStorage {
    /// Saves the pair, replacing what was there.
    pub fn save(&self, tokens: &OpenApiTokens) -> Result<()> {
        let record = StoredOpenApiTokens {
            access_token: tokens.access_token.expose_secret().to_owned(),
            refresh_token: tokens.refresh_token.expose_secret().to_owned(),
            expires_at: tokens.expires_at.and_then(|time| {
                time.duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|span| span.as_secs())
            }),
        };
        let encoded = serde_json::to_string(&record).map_err(|_| ConfigError::SecretStore {
            key: self.key.to_string(),
            message: "could not encode the OAuth token pair".into(),
        })?;
        self.secrets.store(&self.key, &SecretString::from(encoded))
    }

    /// The stored pair, if there is one.
    pub fn load(&self) -> Result<Option<OpenApiTokens>> {
        let Some(secret) = self.secrets.retrieve(&self.key)? else {
            return Ok(None);
        };
        let record: StoredOpenApiTokens =
            serde_json::from_str(secret.expose_secret()).map_err(|_| {
                ConfigError::MalformedEnvelope {
                    key: self.key.to_string(),
                    reason: "invalid OAuth token record".into(),
                }
            })?;
        Ok(Some(OpenApiTokens {
            access_token: SecretString::from(record.access_token),
            refresh_token: SecretString::from(record.refresh_token),
            expires_at: record
                .expires_at
                .map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds)),
        }))
    }

    /// Deletes the stored pair.
    pub fn clear(&self) -> Result<()> {
        self.secrets.delete(&self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::secret::EncryptedFileSecretStore;

    fn storage(dir: &std::path::Path) -> OpenApiTokenStorage {
        OpenApiTokenStorage {
            secrets: Arc::new(EncryptedFileSecretStore::new(
                dir,
                SecretString::from("pw".to_owned()),
            )),
            key: SecretKey::new("profile-field", "p1:oauth-token-set"),
        }
    }

    fn pair(expires_at: Option<SystemTime>) -> OpenApiTokens {
        OpenApiTokens {
            access_token: SecretString::from("access".to_owned()),
            refresh_token: SecretString::from("refresh".to_owned()),
            expires_at,
        }
    }

    #[test]
    fn a_pair_round_trips_with_and_without_an_expiry() {
        let dir = tempfile::tempdir().unwrap();
        let storage = storage(dir.path());
        assert!(storage.load().unwrap().is_none());

        for expiry in [None, Some(UNIX_EPOCH + Duration::from_secs(1_900_000_000))] {
            storage.save(&pair(expiry)).unwrap();
            let loaded = storage.load().unwrap().unwrap();
            assert_eq!(loaded.access_token.expose_secret(), "access");
            assert_eq!(loaded.refresh_token.expose_secret(), "refresh");
            assert_eq!(loaded.expires_at, expiry);
        }
    }

    #[test]
    fn an_expiry_before_1970_is_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let storage = storage(dir.path());
        storage
            .save(&pair(Some(UNIX_EPOCH - Duration::from_secs(5))))
            .unwrap();
        assert_eq!(storage.load().unwrap().unwrap().expires_at, None);
    }

    #[test]
    fn something_else_in_the_slot_is_an_error_not_a_pair() {
        let dir = tempfile::tempdir().unwrap();
        let storage = storage(dir.path());
        storage
            .secrets
            .store(
                &storage.key,
                &SecretString::from("just a string".to_owned()),
            )
            .unwrap();
        assert!(matches!(
            storage.load(),
            Err(ConfigError::MalformedEnvelope { .. })
        ));
    }

    #[test]
    fn clearing_removes_the_pair_and_can_be_repeated() {
        let dir = tempfile::tempdir().unwrap();
        let storage = storage(dir.path());
        storage.save(&pair(None)).unwrap();
        storage.clear().unwrap();
        storage.clear().unwrap();
        assert!(storage.load().unwrap().is_none());
    }
}
