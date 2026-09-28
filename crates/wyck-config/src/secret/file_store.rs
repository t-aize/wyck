//! The fallback credential backend: an encrypted file per secret.

use std::fs;
use std::path::PathBuf;

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use tracing::{debug, trace, warn};

use crate::crypto::{self, CryptoError, KdfParams, Sealed};
use crate::error::{ConfigError, Result};
use crate::fs_util::atomic_write;
use crate::secret::{SecretKey, SecretStore};

/// The version of the envelope layout: the only one written and the only one read.
const ENVELOPE_VERSION: u8 = 1;

/// A secret encrypted at rest with ChaCha20-Poly1305, one file per [`SecretKey`], for
/// use where no OS credential store is available: headless Linux boxes, some
/// containers/CI environments. Prefer [`crate::secret::KeyringSecretStore`] whenever an
/// OS keyring is actually available; this backend exists specifically for when it isn't.
///
/// # Design
///
/// Each call to [`Self::store`] draws a fresh random salt and nonce, derives a 32-byte key from
/// the passphrase with Argon2id (see the crate-private `crypto` module for the cost and the
/// reasons), and encrypts the secret. The salt, the nonce, the ciphertext and the cost are
/// hex-encoded into a small versioned TOML envelope, written atomically to
/// `{dir}/{key}-{fingerprint}.toml` with `0600` permissions on Unix.
///
/// The key of the secret is part of what the cipher signs (its *associated data*): copying the
/// envelope of one secret over another's file makes opening fail, instead of quietly giving the
/// first secret's value for the second key.
///
/// A failed authentication (wrong passphrase, or a tampered file) surfaces as
/// [`ConfigError::Crypto`] with a message that says so, and not as a generic I/O or parse error,
/// since "wrong passphrase" is the overwhelmingly common real-world cause and the caller can
/// present that specific message.
///
/// A store is meant to be owned by one process at a time. Two processes storing the same key at
/// once both write a complete envelope (the last rename wins), but nothing coordinates them.
pub struct EncryptedFileSecretStore {
    dir: PathBuf,
    passphrase: SecretString,
}

impl EncryptedFileSecretStore {
    /// Creates a store rooted at `dir` (typically [`crate::AppPaths::secrets_dir`]),
    /// encrypting/decrypting under `passphrase`.
    ///
    /// This crate does not prompt for the passphrase itself: reading one from a
    /// terminal, an OS secure-prompt dialog, or an environment variable is a UI/app
    /// concern, deliberately kept out of this crate so it stays usable from a GUI or
    /// a headless engine alike.
    pub fn new(dir: impl Into<PathBuf>, passphrase: SecretString) -> Self {
        Self {
            dir: dir.into(),
            passphrase,
        }
    }

    /// The file of a key: its readable part, and a short hash of the whole key so two keys that
    /// only differ in characters a file name cannot hold (`a/b` and `a_b`) never share a file.
    fn envelope_path(&self, key: &SecretKey) -> PathBuf {
        self.dir.join(format!("{}.toml", file_stem(key)))
    }

    /// What the cipher signs for a key.
    fn associated_data(key: &SecretKey) -> Vec<u8> {
        format!("wyck-secret:v{ENVELOPE_VERSION}:{}", key.as_str()).into_bytes()
    }

    fn crypto_error(key: &SecretKey, error: CryptoError) -> ConfigError {
        match error {
            CryptoError::Random(source) => ConfigError::Random(source),
            CryptoError::Kdf(message) => ConfigError::KeyDerivation(message),
            CryptoError::Encrypt => ConfigError::Crypto {
                key: key.to_string(),
                message: "the secret could not be encrypted".to_owned(),
            },
            CryptoError::Decrypt => {
                warn!(%key, "decryption failed: wrong passphrase, or the file was tampered with");
                ConfigError::Crypto {
                    key: key.to_string(),
                    message: "decryption failed: this almost always means the passphrase is wrong (or the file was tampered with)".to_owned(),
                }
            }
            CryptoError::Shape(reason) => ConfigError::MalformedEnvelope {
                key: key.to_string(),
                reason,
            },
        }
    }
}

impl SecretStore for EncryptedFileSecretStore {
    fn store(&self, key: &SecretKey, secret: &SecretString) -> Result<()> {
        use secrecy::ExposeSecret;

        let sealed = crypto::seal(
            &self.passphrase,
            &Self::associated_data(key),
            secret.expose_secret().as_bytes(),
        )
        .map_err(|error| Self::crypto_error(key, error))?;

        let envelope = Envelope {
            version: ENVELOPE_VERSION,
            memory_kib: sealed.params.memory_kib,
            iterations: sealed.params.iterations,
            parallelism: sealed.params.parallelism,
            salt: crypto::hex_encode(&sealed.salt),
            nonce: crypto::hex_encode(&sealed.nonce),
            ciphertext: crypto::hex_encode(&sealed.ciphertext),
        };
        let toml_text = toml::to_string(&envelope).map_err(ConfigError::Serialize)?;
        let path = self.envelope_path(key);
        atomic_write(&path, toml_text.as_bytes())?;
        debug!(%key, path = %path.display(), "encrypted and stored a secret");
        Ok(())
    }

    fn retrieve(&self, key: &SecretKey) -> Result<Option<SecretString>> {
        let path = self.envelope_path(key);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                trace!(%key, path = %path.display(), "no envelope on disk for this key");
                return Ok(None);
            }
            Err(source) => {
                warn!(%key, path = %path.display(), error = %source, "could not read the envelope");
                return Err(ConfigError::Read { path, source });
            }
        };

        let envelope: Envelope = toml::from_str(&text).map_err(|source| {
            warn!(%key, path = %path.display(), error = %source, "the envelope could not be parsed");
            ConfigError::Parse {
                path: path.clone(),
                source: Box::new(source),
            }
        })?;
        let malformed = |reason: String| ConfigError::MalformedEnvelope {
            key: key.to_string(),
            reason,
        };
        if envelope.version != ENVELOPE_VERSION {
            warn!(%key, found = envelope.version, expected = ENVELOPE_VERSION, "unsupported envelope version");
            return Err(malformed(format!(
                "unsupported envelope version {} (this build understands version {ENVELOPE_VERSION})",
                envelope.version
            )));
        }
        let params = KdfParams {
            memory_kib: envelope.memory_kib,
            iterations: envelope.iterations,
            parallelism: envelope.parallelism,
        };
        let aad = Self::associated_data(key);

        let decode = |what: &str, hex: &str| {
            crypto::hex_decode(hex).map_err(|reason| malformed(format!("{what}: {reason}")))
        };
        let sealed = Sealed {
            params,
            salt: crypto::exact(&decode("salt", &envelope.salt)?, "the salt")
                .map_err(|error| Self::crypto_error(key, error))?,
            nonce: crypto::exact(&decode("nonce", &envelope.nonce)?, "the nonce")
                .map_err(|error| Self::crypto_error(key, error))?,
            ciphertext: decode("ciphertext", &envelope.ciphertext)?,
        };
        let plaintext = crypto::open(&self.passphrase, &aad, &sealed)
            .map_err(|error| Self::crypto_error(key, error))?;

        let plaintext = String::from_utf8(plaintext.to_vec())
            .map_err(|_| malformed("decrypted payload was not valid UTF-8".to_owned()))?;
        trace!(%key, path = %path.display(), "decrypted and retrieved a secret");
        Ok(Some(SecretString::from(plaintext)))
    }

    fn delete(&self, key: &SecretKey) -> Result<()> {
        let path = self.envelope_path(key);
        match fs::remove_file(&path) {
            Ok(()) => {
                debug!(%key, path = %path.display(), "deleted a secret envelope");
                Ok(())
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => {
                warn!(%key, path = %path.display(), error = %source, "could not delete the secret envelope");
                Err(ConfigError::Write { path, source })
            }
        }
    }
}

/// The on-disk envelope for one encrypted secret. All byte fields are hex-encoded so the file
/// stays plain ASCII TOML (easy to `cat` and diff without special tooling: only the plaintext
/// they decode to is sensitive, and that never touches disk). Every field is required.
#[derive(Debug, Serialize, Deserialize)]
struct Envelope {
    version: u8,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: String,
    nonce: String,
    ciphertext: String,
}

/// Maps a [`SecretKey`]'s raw string to a readable, file-system-safe stem: every character
/// outside `[A-Za-z0-9._-]` becomes `_`.
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

/// The stem of the file of a key: the readable part (kept short), then 16 hex digits of a
/// fingerprint of the whole key. Different keys give different stems even when their
/// readable parts are the same.
fn file_stem(key: &SecretKey) -> String {
    let readable: String = sanitize_filename(key.as_str()).chars().take(80).collect();
    format!("{readable}-{}", fingerprint(key.as_str()))
}

/// A stable 64-bit fingerprint (FNV-1a), as 16 hex digits. It only has to tell apart the few keys
/// of one app on one machine; it protects nothing, so a fast non-cryptographic hash is right.
fn fingerprint(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use secrecy::ExposeSecret;

    use super::*;

    fn store_at(dir: &Path, passphrase: &str) -> EncryptedFileSecretStore {
        EncryptedFileSecretStore::new(dir, SecretString::from(passphrase.to_owned()))
    }

    fn secret(text: &str) -> SecretString {
        SecretString::from(text.to_owned())
    }

    fn envelope_of(store: &EncryptedFileSecretStore, key: &SecretKey) -> Envelope {
        toml::from_str(&fs::read_to_string(store.envelope_path(key)).unwrap()).unwrap()
    }

    #[test]
    fn round_trips_a_secret() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = store_at(temp_dir.path(), "correct horse battery staple");
        let key = SecretKey::new("test", "profile-1");

        store.store(&key, &secret("super-secret-token")).unwrap();
        let retrieved = store.retrieve(&key).unwrap().unwrap();

        assert_eq!(retrieved.expose_secret(), "super-secret-token");
        let text = fs::read_to_string(store.envelope_path(&key)).unwrap();
        assert!(!text.contains("super-secret-token"));
        assert_eq!(envelope_of(&store, &key).version, 1);
    }

    #[test]
    fn retrieve_returns_none_for_unknown_key() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = store_at(temp_dir.path(), "passphrase");
        assert!(
            store
                .retrieve(&SecretKey::new("test", "does-not-exist"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn wrong_passphrase_fails_to_decrypt() {
        let temp_dir = tempfile::tempdir().unwrap();
        let key = SecretKey::new("test", "profile-1");
        store_at(temp_dir.path(), "right passphrase")
            .store(&key, &secret("token"))
            .unwrap();

        let result = store_at(temp_dir.path(), "wrong passphrase").retrieve(&key);

        assert!(matches!(result, Err(ConfigError::Crypto { .. })));
    }

    #[test]
    fn delete_is_idempotent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = store_at(temp_dir.path(), "passphrase");
        let key = SecretKey::new("test", "profile-1");

        store.store(&key, &secret("token")).unwrap();
        store.delete(&key).unwrap();
        store.delete(&key).unwrap(); // deleting again must not error
        assert!(store.retrieve(&key).unwrap().is_none());
    }

    #[test]
    fn overwriting_a_secret_uses_a_fresh_salt_and_nonce() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = store_at(temp_dir.path(), "passphrase");
        let key = SecretKey::new("test", "profile-1");

        store.store(&key, &secret("first")).unwrap();
        let first = envelope_of(&store, &key);
        store.store(&key, &secret("second")).unwrap();
        let second = envelope_of(&store, &key);

        assert_ne!(first.salt, second.salt);
        assert_ne!(first.nonce, second.nonce);
        assert_eq!(
            store.retrieve(&key).unwrap().unwrap().expose_secret(),
            "second"
        );
    }

    #[test]
    fn the_envelope_of_one_secret_cannot_stand_in_for_another() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = store_at(temp_dir.path(), "passphrase");
        let (cheap, precious) = (
            SecretKey::new("test", "cheap"),
            SecretKey::new("test", "precious"),
        );
        store.store(&cheap, &secret("low value")).unwrap();
        store.store(&precious, &secret("high value")).unwrap();

        // Someone with write access to the folder copies one envelope over the other.
        fs::copy(store.envelope_path(&cheap), store.envelope_path(&precious)).unwrap();

        assert!(matches!(
            store.retrieve(&precious),
            Err(ConfigError::Crypto { .. })
        ));
        assert_eq!(
            store.retrieve(&cheap).unwrap().unwrap().expose_secret(),
            "low value"
        );
    }

    #[test]
    fn keys_that_only_differ_in_unsafe_characters_do_not_share_a_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = store_at(temp_dir.path(), "passphrase");
        let (a, b) = (SecretKey::new("ns", "a/b"), SecretKey::new("ns", "a_b"));
        assert_ne!(store.envelope_path(&a), store.envelope_path(&b));

        store.store(&a, &secret("first")).unwrap();
        store.store(&b, &secret("second")).unwrap();

        assert_eq!(
            store.retrieve(&a).unwrap().unwrap().expose_secret(),
            "first"
        );
        assert_eq!(
            store.retrieve(&b).unwrap().unwrap().expose_secret(),
            "second"
        );
    }

    #[test]
    fn a_damaged_envelope_is_an_error_and_never_a_panic() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = store_at(temp_dir.path(), "p");
        let key = SecretKey::new("ns", "damaged");
        store.store(&key, &secret("v")).unwrap();
        let path = store.envelope_path(&key);
        let good = fs::read_to_string(&path).unwrap();
        let envelope = envelope_of(&store, &key);

        for (name, bad) in [
            (
                "multi-byte character in the salt",
                good.replace(&envelope.salt, "\u{e9}\u{e9}\u{e9}\u{e9}"),
            ),
            ("salt too short", good.replace(&envelope.salt, "00ff")),
            (
                "nonce not hex",
                good.replace(&envelope.nonce, &"zz".repeat(12)),
            ),
            (
                "another version",
                good.replace("version = 1", "version = 2"),
            ),
            ("cost left out", good.replace("memory_kib = 19456\n", "")),
            (
                "no cost at all",
                "version = 1\nsalt = \"00\"\nnonce = \"00\"\nciphertext = \"00\"\n".to_owned(),
            ),
            (
                "hostile cost",
                good.replace("memory_kib = 19456", "memory_kib = 4000000000"),
            ),
            ("not toml", "]]] not toml".to_owned()),
        ] {
            fs::write(&path, bad).unwrap();
            let result = store.retrieve(&key);
            assert!(result.is_err(), "{name} should be refused, got {result:?}");
        }
    }

    #[test]
    fn hex_round_trip() {
        let bytes = [0u8, 1, 15, 16, 255];
        assert_eq!(
            crypto::hex_decode(&crypto::hex_encode(&bytes)).unwrap(),
            bytes
        );
    }

    #[test]
    fn the_readable_part_of_a_file_name_holds_only_safe_characters() {
        assert_eq!(
            sanitize_filename("ctrader-remote:profile:abc/def"),
            "ctrader-remote_profile_abc_def"
        );
        let stem = file_stem(&SecretKey::new("ns", "../../etc/passwd"));
        assert!(
            stem.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        );
        assert!(!stem.contains('/'));
    }

    #[test]
    fn the_fingerprint_is_stable() {
        // Envelope files are found by it: changing it would lose every stored secret.
        assert_eq!(fingerprint(""), "cbf29ce484222325");
        assert_eq!(fingerprint("a"), "af63dc4c8601ec8c");
        assert_eq!(fingerprint("profile:abc"), fingerprint("profile:abc"));
    }
}
