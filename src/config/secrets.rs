//! Credential storage: the OS keyring, or encrypted files when there is none.

use std::fs;
use std::path::PathBuf;

use keyring::Entry;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tracing::{debug, trace, warn};

use self::crypto::{CryptoError, KdfParams, Sealed};
use crate::config::error::{ConfigError, Result};
use crate::config::fs_util::atomic_write;

/// A structured identifier for one secret: `namespace:name`, e.g.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretKey(String);

impl SecretKey {
    /// Builds a namespaced key: `"{namespace}:{name}"`.
    pub fn new(namespace: &str, name: &str) -> Self {
        Self(format!("{namespace}:{name}"))
    }

    /// The raw key string, as passed to the backend (e.g. as the keyring "username" field, or
    /// hashed into a filename).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A backend capable of storing, retrieving, and deleting secrets by `SecretKey`.
pub trait SecretStore: Send + Sync {
    /// Stores `secret` under `key`, overwriting any existing value.
    fn store(&self, key: &SecretKey, secret: &SecretString) -> Result<()>;

    /// Retrieves the secret stored under `key`, or `Ok(None)` if nothing is stored there yet
    /// (this is the normal "not configured" case, not an error).
    fn retrieve(&self, key: &SecretKey) -> Result<Option<SecretString>>;

    /// Deletes the secret stored under `key`.
    fn delete(&self, key: &SecretKey) -> Result<()>;
}

/// Stores secrets in the OS-native credential store: Windows Credential Manager, macOS Keychain,
/// or (on Linux) the Secret Service D-Bus API via a pure-Rust `zbus` client: whichever backend
/// the `keyring` crate resolves for the current platform.
pub struct KeyringSecretStore {
    service: String,
}

impl KeyringSecretStore {
    /// Creates a store that namespaces every credential under `service` in the OS credential
    /// manager (e.g. so `wyck`'s entries are visibly grouped together in Windows Credential
    /// Manager / macOS Keychain Access rather than mixed in with every other app's entries).
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
}

impl Default for KeyringSecretStore {
    /// Namespaces credentials under the service name `"wyck"`.
    fn default() -> Self {
        Self::new("wyck")
    }
}

impl SecretStore for KeyringSecretStore {
    fn store(&self, key: &SecretKey, secret: &SecretString) -> Result<()> {
        let entry = entry_for(&self.service, key)?;
        entry
            .set_password(secret.expose_secret())
            .inspect(|_| debug!(service = %self.service, %key, "stored a secret in the OS keyring"))
            .map_err(|source| {
                warn!(service = %self.service, %key, error = %source, "could not store a secret in the OS keyring");
                to_config_error(key, source)
            })
    }

    fn retrieve(&self, key: &SecretKey) -> Result<Option<SecretString>> {
        let entry = entry_for(&self.service, key)?;
        match entry.get_password() {
            Ok(password) => {
                trace!(service = %self.service, %key, "retrieved a secret from the OS keyring");
                Ok(Some(SecretString::from(password)))
            }
            Err(keyring::Error::NoEntry) => {
                trace!(service = %self.service, %key, "no secret in the OS keyring for this key");
                Ok(None)
            }
            Err(source) => {
                warn!(service = %self.service, %key, error = %source, "could not retrieve a secret from the OS keyring");
                Err(to_config_error(key, source))
            }
        }
    }

    fn delete(&self, key: &SecretKey) -> Result<()> {
        let entry = entry_for(&self.service, key)?;
        match entry.delete_credential() {
            Ok(()) => {
                debug!(service = %self.service, %key, "deleted a secret from the OS keyring");
                Ok(())
            }
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(source) => {
                warn!(service = %self.service, %key, error = %source, "could not delete a secret from the OS keyring");
                Err(to_config_error(key, source))
            }
        }
    }
}

fn entry_for(service: &str, key: &SecretKey) -> Result<Entry> {
    Entry::new(service, key.as_str()).map_err(|source| to_config_error(key, source))
}

fn to_config_error(key: &SecretKey, source: keyring::Error) -> ConfigError {
    ConfigError::SecretStore {
        key: key.to_string(),
        message: source.to_string(),
    }
}

/// The version of the envelope layout: the only one written and the only one read.
const ENVELOPE_VERSION: u8 = 1;

/// A secret encrypted at rest with ChaCha20-Poly1305, one file per `SecretKey`, for use where no
/// OS credential store is available: headless Linux boxes, some containers/CI environments.
pub struct EncryptedFileSecretStore {
    dir: PathBuf,
    passphrase: SecretString,
}

impl EncryptedFileSecretStore {
    /// Creates a store rooted at `dir` (typically `crate::config::AppPaths::secrets_dir`),
    /// encrypting/decrypting under `passphrase`.
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

/// The on-disk envelope for one encrypted secret.
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

/// Maps a `SecretKey`'s raw string to a readable, file-system-safe stem: every character outside
/// `[A-Za-z0-9._-]` becomes `_`.
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
/// fingerprint of the whole key.
fn file_stem(key: &SecretKey) -> String {
    let readable: String = sanitize_filename(key.as_str()).chars().take(80).collect();
    format!("{readable}-{}", fingerprint(key.as_str()))
}

/// A stable 64-bit fingerprint (FNV-1a), as 16 hex digits.
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

mod crypto {
    use argon2::{Algorithm, Argon2, Params, Version};
    use chacha20poly1305::aead::{Aead, Payload};
    use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
    use secrecy::{ExposeSecret, SecretString};
    use zeroize::Zeroizing;

    pub(crate) const SALT_LEN: usize = 16;
    pub(crate) const NONCE_LEN: usize = 12;
    const KEY_LEN: usize = 32;

    /// What can go wrong, before it is put in the words of the caller.
    #[derive(Debug)]
    pub(crate) enum CryptoError {
        Kdf(String),
        /// The system could not give random bytes.
        Random(getrandom::Error),
        Encrypt,
        Decrypt,
        Shape(String),
    }

    /// The cost of turning a passphrase into a key.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct KdfParams {
        pub memory_kib: u32,
        pub iterations: u32,
        pub parallelism: u32,
    }

    impl KdfParams {
        /// What data is sealed with: 19 MiB, 2 passes, 1 lane.
        pub(crate) const CURRENT: Self = Self {
            memory_kib: 19_456,
            iterations: 2,
            parallelism: 1,
        };

        const MAX_MEMORY_KIB: u32 = 256 * 1024;
        const MAX_ITERATIONS: u32 = 10;
        const MAX_PARALLELISM: u32 = 8;

        /// These parameters, if they are within what this crate will run: at least what Argon2 itself
        /// needs, at most a memory and time a hostile file cannot abuse.
        pub(crate) fn validated(self) -> Result<Self, CryptoError> {
            let ok = self.parallelism >= 1
                && self.parallelism <= Self::MAX_PARALLELISM
                && self.iterations >= 1
                && self.iterations <= Self::MAX_ITERATIONS
                && self.memory_kib >= 8 * self.parallelism
                && self.memory_kib <= Self::MAX_MEMORY_KIB;
            if ok {
                Ok(self)
            } else {
                Err(CryptoError::Shape(format!(
                    "key derivation cost out of bounds (memory {} KiB, {} passes, {} lanes)",
                    self.memory_kib, self.iterations, self.parallelism
                )))
            }
        }
    }

    /// Encrypted data with what is needed to open it again (given the passphrase and the associated
    /// data): never the passphrase or the key.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct Sealed {
        pub params: KdfParams,
        pub salt: [u8; SALT_LEN],
        pub nonce: [u8; NONCE_LEN],
        pub ciphertext: Vec<u8>,
    }

    fn derive(
        passphrase: &SecretString,
        salt: &[u8],
        params: KdfParams,
    ) -> Result<Zeroizing<[u8; KEY_LEN]>, CryptoError> {
        let params = params.validated()?;
        let argon = Params::new(
            params.memory_kib,
            params.iterations,
            params.parallelism,
            Some(KEY_LEN),
        )
        .map_err(|error| CryptoError::Kdf(error.to_string()))?;
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, argon)
            .hash_password_into(passphrase.expose_secret().as_bytes(), salt, key.as_mut())
            .map_err(|error| CryptoError::Kdf(error.to_string()))?;
        Ok(key)
    }

    /// Encrypts `plaintext` under `passphrase`, bound to `aad`.
    pub(crate) fn seal(
        passphrase: &SecretString,
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Sealed, CryptoError> {
        let mut salt = [0u8; SALT_LEN];
        getrandom::fill(&mut salt).map_err(CryptoError::Random)?;
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(CryptoError::Random)?;
        let params = KdfParams::CURRENT;

        let key = derive(passphrase, &salt, params)?;
        let cipher = ChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|_| CryptoError::Shape("the key has the wrong size".to_owned()))?;
        let ciphertext = cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| CryptoError::Encrypt)?;
        Ok(Sealed {
            params,
            salt,
            nonce,
            ciphertext,
        })
    }

    /// Decrypts `sealed` under `passphrase`, checking it against `aad`.
    pub(crate) fn open(
        passphrase: &SecretString,
        aad: &[u8],
        sealed: &Sealed,
    ) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        let key = derive(passphrase, &sealed.salt, sealed.params)?;
        let cipher = ChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|_| CryptoError::Shape("the key has the wrong size".to_owned()))?;
        cipher
            .decrypt(
                &Nonce::from(sealed.nonce),
                Payload {
                    msg: &sealed.ciphertext,
                    aad,
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| CryptoError::Decrypt)
    }

    /// Lowercase hexadecimal.
    pub(crate) fn hex_encode(bytes: &[u8]) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut text = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            text.push(char::from(DIGITS[usize::from(byte >> 4)]));
            text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
        }
        text
    }

    /// The bytes of a hexadecimal text (either case).
    pub(crate) fn hex_decode(text: &str) -> Result<Vec<u8>, String> {
        let bytes = text.as_bytes();
        if !bytes.len().is_multiple_of(2) {
            return Err("hex string has odd length".to_owned());
        }
        let nibble = |b: u8| match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            b'A'..=b'F' => Ok(b - b'A' + 10),
            _ => Err("hex string holds a character that is not a hex digit".to_owned()),
        };
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| Ok(nibble(pair[0])? << 4 | nibble(pair[1])?))
            .collect()
    }

    /// A fixed-size array out of decoded bytes.
    pub(crate) fn exact<const N: usize>(bytes: &[u8], what: &str) -> Result<[u8; N], CryptoError> {
        bytes
            .try_into()
            .map_err(|_| CryptoError::Shape(format!("{what} is not exactly {N} bytes")))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use proptest::prelude::*;

        fn pass(text: &str) -> SecretString {
            SecretString::from(text.to_owned())
        }

        #[test]
        fn what_is_sealed_opens_with_the_same_passphrase_and_data() {
            let sealed = seal(&pass("horse"), b"label", b"the text").unwrap();
            let opened = open(&pass("horse"), b"label", &sealed).unwrap();
            assert_eq!(opened.as_slice(), b"the text");
        }

        #[test]
        fn a_wrong_passphrase_or_other_associated_data_or_a_changed_byte_does_not_open() {
            let sealed = seal(&pass("horse"), b"label", b"the text").unwrap();
            assert!(matches!(
                open(&pass("horsf"), b"label", &sealed),
                Err(CryptoError::Decrypt)
            ));
            assert!(matches!(
                open(&pass("horse"), b"other", &sealed),
                Err(CryptoError::Decrypt)
            ));
            let mut changed = sealed.clone();
            changed.ciphertext[0] ^= 1;
            assert!(matches!(
                open(&pass("horse"), b"label", &changed),
                Err(CryptoError::Decrypt)
            ));
        }

        #[test]
        fn two_seals_of_the_same_thing_share_nothing() {
            let a = seal(&pass("p"), b"", b"same").unwrap();
            let b = seal(&pass("p"), b"", b"same").unwrap();
            assert_ne!(a.salt, b.salt);
            assert_ne!(a.nonce, b.nonce);
            assert_ne!(a.ciphertext, b.ciphertext);
        }

        #[test]
        fn a_hostile_cost_is_refused_before_anything_is_allocated() {
            for params in [
                KdfParams {
                    memory_kib: u32::MAX,
                    ..KdfParams::CURRENT
                },
                KdfParams {
                    iterations: 0,
                    ..KdfParams::CURRENT
                },
                KdfParams {
                    iterations: 1_000,
                    ..KdfParams::CURRENT
                },
                KdfParams {
                    parallelism: 0,
                    ..KdfParams::CURRENT
                },
                KdfParams {
                    memory_kib: 1,
                    ..KdfParams::CURRENT
                },
            ] {
                assert!(params.validated().is_err(), "{params:?}");
            }
            assert!(KdfParams::CURRENT.validated().is_ok());
        }

        #[test]
        fn hex_refuses_what_is_not_hex_without_panicking() {
            assert_eq!(hex_decode("00ff10").unwrap(), [0, 255, 16]);
            assert_eq!(hex_decode("ABcd").unwrap(), [0xab, 0xcd]);
            assert!(hex_decode("abc").is_err());
            assert!(hex_decode("zz").is_err());
            // Two bytes long, but the second character is in the middle of a multi-byte one: slicing
            // the text there is a panic.
            assert!(hex_decode("a\u{e9}").is_err());
            assert!(hex_decode("\u{1F600}\u{1F600}").is_err());
        }

        proptest! {
            #[test]
            fn hex_round_trips(bytes in proptest::collection::vec(any::<u8>(), 0..200)) {
                prop_assert_eq!(hex_decode(&hex_encode(&bytes)).unwrap(), bytes);
            }

            #[test]
            fn hex_decode_never_panics(text in ".{0,64}") {
                let _ = hex_decode(&text);
            }
        }
    }
}
