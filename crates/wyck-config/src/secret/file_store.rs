//! The fallback credential backend: an encrypted file per secret.

use std::fs;
use std::path::PathBuf;

use argon2::Argon2;
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::error::{ConfigError, Result};
use crate::fs_util::atomic_write;
use crate::secret::{SecretKey, SecretStore};

const ENVELOPE_VERSION: u8 = 1;
const SALT_LEN: usize = 16;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

/// A secret encrypted at rest with ChaCha20-Poly1305, one file per [`SecretKey`], for
/// use where no OS credential store is available — headless Linux boxes, some
/// containers/CI environments, or `wyck`'s own planned "always-on box" headless mode
/// (see the project README's roadmap). Prefer [`crate::secret::KeyringSecretStore`]
/// whenever an OS keyring is actually available; this backend exists specifically for
/// when it isn't.
///
/// # Design
///
/// Each call to [`Self::store`] generates a fresh random 16-byte salt and derives a
/// fresh 32-byte ChaCha20-Poly1305 key from `passphrase` via Argon2id
/// (`Argon2::default()`'s parameters — the algorithm's current recommended default
/// work factor). A fresh random 12-byte nonce is generated per encryption. Salt, nonce,
/// and ciphertext are hex-encoded into a small versioned TOML envelope and written
/// atomically (see the crate-internal `atomic_write` helper) to `{dir}/{sanitized key}.toml`,
/// with `0600` permissions on Unix.
///
/// Because a fresh salt (and therefore a fresh derived key) is used per secret, two
/// secrets stored under the same passphrase never share key material, even though they
/// share a passphrase — so nonce reuse under the same key, the one catastrophic failure
/// mode for an AEAD cipher, cannot happen across secrets. Within one secret, exactly one
/// nonce is ever generated per [`Self::store`] call (each call re-derives a new
/// salt+key+nonce triple from scratch, it never reuses a previous encryption's nonce
/// under the same key).
///
/// On [`Self::retrieve`], a failed AEAD authentication (wrong passphrase, or a tampered
/// file) surfaces as [`ConfigError::Crypto`] with a message that says so — it is
/// deliberately NOT reported as a generic I/O or parse failure, since "wrong passphrase"
/// is the overwhelmingly common real-world cause and the caller should be able to
/// present that specific message to the user.
pub struct EncryptedFileSecretStore {
    dir: PathBuf,
    passphrase: SecretString,
}

impl EncryptedFileSecretStore {
    /// Creates a store rooted at `dir` (typically [`crate::AppPaths::secrets_dir`]),
    /// encrypting/decrypting under `passphrase`.
    ///
    /// This crate does not prompt for the passphrase itself — reading one from a
    /// terminal, an OS secure-prompt dialog, or an environment variable is a UI/app
    /// concern, deliberately kept out of this crate so it stays usable from a TUI, a
    /// future GUI, and a headless engine alike.
    pub fn new(dir: impl Into<PathBuf>, passphrase: SecretString) -> Self {
        Self {
            dir: dir.into(),
            passphrase,
        }
    }

    fn envelope_path(&self, key: &SecretKey) -> PathBuf {
        self.dir
            .join(format!("{}.toml", sanitize_filename(key.as_str())))
    }

    fn derive_key(&self, salt: &[u8]) -> Result<[u8; KEY_LEN]> {
        let mut key_bytes = [0u8; KEY_LEN];
        Argon2::default()
            .hash_password_into(
                self.passphrase.expose_secret().as_bytes(),
                salt,
                &mut key_bytes,
            )
            .map_err(|source| ConfigError::KeyDerivation(source.to_string()))?;
        Ok(key_bytes)
    }
}

impl SecretStore for EncryptedFileSecretStore {
    fn store(&self, key: &SecretKey, secret: &SecretString) -> Result<()> {
        let mut salt = [0u8; SALT_LEN];
        getrandom::fill(&mut salt)?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce_bytes)?;

        let key_bytes = self.derive_key(&salt)?;
        let cipher = ChaCha20Poly1305::new(&Key::from(key_bytes));
        let nonce = Nonce::from(nonce_bytes);
        let ciphertext = cipher
            .encrypt(&nonce, secret.expose_secret().as_bytes())
            .map_err(|source| ConfigError::Crypto {
                key: key.to_string(),
                message: source.to_string(),
            })?;

        let envelope = Envelope {
            version: ENVELOPE_VERSION,
            salt: hex_encode(&salt),
            nonce: hex_encode(&nonce_bytes),
            ciphertext: hex_encode(&ciphertext),
        };
        let toml_text = toml::to_string(&envelope).map_err(ConfigError::Serialize)?;
        atomic_write(&self.envelope_path(key), toml_text.as_bytes())
    }

    fn retrieve(&self, key: &SecretKey) -> Result<Option<SecretString>> {
        let path = self.envelope_path(key);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(ConfigError::Read { path, source }),
        };

        let envelope: Envelope = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source: Box::new(source),
        })?;
        if envelope.version != ENVELOPE_VERSION {
            return Err(ConfigError::MalformedEnvelope {
                key: key.to_string(),
                reason: format!(
                    "unsupported envelope version {} (this build understands version {ENVELOPE_VERSION})",
                    envelope.version
                ),
            });
        }

        let malformed = |reason: String| ConfigError::MalformedEnvelope {
            key: key.to_string(),
            reason,
        };
        let salt = hex_decode(&envelope.salt).map_err(malformed)?;
        let nonce_bytes = hex_decode(&envelope.nonce).map_err(malformed)?;
        let ciphertext = hex_decode(&envelope.ciphertext).map_err(malformed)?;

        let key_bytes = self.derive_key(&salt)?;
        let cipher = ChaCha20Poly1305::new(&Key::from(key_bytes));
        let nonce = Nonce::try_from(nonce_bytes.as_slice())
            .map_err(|_| malformed("nonce is not exactly 12 bytes".to_owned()))?;
        let plaintext = cipher.decrypt(&nonce, ciphertext.as_ref()).map_err(|_source| ConfigError::Crypto {
            key: key.to_string(),
            message: "decryption failed — this almost always means the passphrase is wrong (or the file was tampered with)".to_owned(),
        })?;

        let plaintext = String::from_utf8(plaintext)
            .map_err(|_| malformed("decrypted payload was not valid UTF-8".to_owned()))?;
        Ok(Some(SecretString::from(plaintext)))
    }

    fn delete(&self, key: &SecretKey) -> Result<()> {
        let path = self.envelope_path(key);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(ConfigError::Write { path, source }),
        }
    }
}

/// The on-disk envelope for one encrypted secret. All three byte fields are hex-encoded
/// so the file stays plain ASCII TOML (consistent with [`crate::AppConfig`]'s format,
/// easy to `cat`/diff for debugging without special tooling — only the plaintext they
/// decode to is sensitive, and that never touches disk).
#[derive(Debug, Serialize, Deserialize)]
struct Envelope {
    version: u8,
    salt: String,
    nonce: String,
    ciphertext: String,
}

/// Maps a [`SecretKey`]'s raw string to a filesystem-safe file stem by replacing every
/// character outside `[A-Za-z0-9._-]` with `_`. [`SecretKey`] values are always built by
/// this crate's own code (namespace:name pairs — see [`SecretKey::new`]), never from
/// unsanitized external input, so this only needs to guarantee a valid, collision-free-
/// in-practice filename, not defend against adversarial input.
fn sanitize_filename(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_decode(text: &str) -> std::result::Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err("hex string has odd length".to_owned());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|source| source.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn store_at(dir: &Path, passphrase: &str) -> EncryptedFileSecretStore {
        EncryptedFileSecretStore::new(dir, SecretString::from(passphrase.to_owned()))
    }

    #[test]
    fn round_trips_a_secret() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = store_at(temp_dir.path(), "correct horse battery staple");
        let key = SecretKey::new("test", "profile-1");

        store
            .store(&key, &SecretString::from("super-secret-token".to_owned()))
            .unwrap();
        let retrieved = store.retrieve(&key).unwrap().unwrap();

        assert_eq!(retrieved.expose_secret(), "super-secret-token");
    }

    #[test]
    fn retrieve_returns_none_for_unknown_key() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = store_at(temp_dir.path(), "passphrase");
        let key = SecretKey::new("test", "does-not-exist");

        assert!(store.retrieve(&key).unwrap().is_none());
    }

    #[test]
    fn wrong_passphrase_fails_to_decrypt() {
        let temp_dir = tempfile::tempdir().unwrap();
        let key = SecretKey::new("test", "profile-1");
        store_at(temp_dir.path(), "right passphrase")
            .store(&key, &SecretString::from("token".to_owned()))
            .unwrap();

        let result = store_at(temp_dir.path(), "wrong passphrase").retrieve(&key);

        assert!(matches!(result, Err(ConfigError::Crypto { .. })));
    }

    #[test]
    fn delete_is_idempotent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = store_at(temp_dir.path(), "passphrase");
        let key = SecretKey::new("test", "profile-1");

        store
            .store(&key, &SecretString::from("token".to_owned()))
            .unwrap();
        store.delete(&key).unwrap();
        store.delete(&key).unwrap(); // deleting again must not error
        assert!(store.retrieve(&key).unwrap().is_none());
    }

    #[test]
    fn overwriting_a_secret_uses_a_fresh_salt_and_nonce() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = store_at(temp_dir.path(), "passphrase");
        let key = SecretKey::new("test", "profile-1");

        store
            .store(&key, &SecretString::from("first".to_owned()))
            .unwrap();
        let path = store.envelope_path(&key);
        let first_envelope: Envelope = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();

        store
            .store(&key, &SecretString::from("second".to_owned()))
            .unwrap();
        let second_envelope: Envelope =
            toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();

        assert_ne!(first_envelope.salt, second_envelope.salt);
        assert_ne!(first_envelope.nonce, second_envelope.nonce);
        assert_eq!(
            store.retrieve(&key).unwrap().unwrap().expose_secret(),
            "second"
        );
    }

    #[test]
    fn hex_round_trip() {
        let bytes = [0u8, 1, 15, 16, 255];
        assert_eq!(hex_decode(&hex_encode(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn sanitize_filename_strips_unsafe_characters() {
        assert_eq!(
            sanitize_filename("ctrader-remote:profile:abc/def"),
            "ctrader-remote_profile_abc_def"
        );
    }
}
