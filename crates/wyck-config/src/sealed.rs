//! Text encrypted with a passphrase, to carry a backup somewhere it should not be readable.
//!
//! [`seal_text`] turns any text (a backup file, an export) into a small TOML document that holds
//! the text only in encrypted form, and [`open_text`] turns it back. The passphrase is never
//! stored; the cipher and the key derivation are described in the crate-private `crypto` module
//! (Argon2id and ChaCha20-Poly1305, a new salt and nonce every time).
//!
//! ```toml
//! format = "wyck-sealed"
//! version = 1
//! label = "wyck-backup"
//! kdf = "argon2id"
//! memory_kib = 19456
//! iterations = 2
//! parallelism = 1
//! salt = "..."
//! nonce = "..."
//! ciphertext = "..."
//! ```
//!
//! The `label` says what the text is (`"wyck-backup"`). It is part of what the cipher signs, so a
//! sealed backup cannot be passed off as another kind of document: opening with a different label
//! fails. Only the label, the cost and the sizes are readable without the passphrase.
//!
//! ```
//! use secrecy::SecretString;
//! use wyck_config::sealed;
//!
//! let passphrase = SecretString::from("correct horse battery staple".to_owned());
//! let sealed = sealed::seal_text(&passphrase, "wyck-backup", "answer = 42\n")?;
//! assert!(sealed::is_sealed(&sealed));
//! assert!(!sealed.contains("answer"));
//!
//! assert_eq!(sealed::open_text(&passphrase, "wyck-backup", &sealed)?, "answer = 42\n");
//!
//! let wrong = SecretString::from("Tr0ub4dor&3".to_owned());
//! assert!(sealed::open_text(&wrong, "wyck-backup", &sealed).is_err());
//! # Ok::<(), wyck_config::ConfigError>(())
//! ```

use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use crate::crypto::{self, CryptoError, KdfParams, Sealed};
use crate::error::{ConfigError, Result};

/// The value of `format` in a sealed document.
pub const FORMAT: &str = "wyck-sealed";
/// The version of the layout this build writes, and the newest it reads.
pub const VERSION: u32 = 1;
/// The largest sealed text [`open_text`] looks at.
pub const MAX_SEALED_BYTES: usize = 128 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Document {
    format: String,
    version: u32,
    label: String,
    kdf: String,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: String,
    nonce: String,
    ciphertext: String,
}

fn associated_data(label: &str) -> Vec<u8> {
    format!("{FORMAT}:v{VERSION}:{label}").into_bytes()
}

fn bad(reason: impl Into<String>) -> ConfigError {
    ConfigError::Sealed(reason.into())
}

fn from_crypto(error: CryptoError) -> ConfigError {
    match error {
        CryptoError::Decrypt => ConfigError::WrongPassphrase,
        CryptoError::Random(source) => ConfigError::Random(source),
        CryptoError::Kdf(message) => ConfigError::KeyDerivation(message),
        CryptoError::Encrypt => bad("the text could not be encrypted"),
        CryptoError::Shape(reason) => bad(reason),
    }
}

/// Encrypts `plaintext` under `passphrase`, as the text of a sealed document for `label`.
///
/// # Errors
///
/// [`ConfigError::Random`] or [`ConfigError::KeyDerivation`] when the system or Argon2 fails,
/// which does not happen in normal use.
pub fn seal_text(passphrase: &SecretString, label: &str, plaintext: &str) -> Result<String> {
    let sealed = crypto::seal(passphrase, &associated_data(label), plaintext.as_bytes())
        .map_err(from_crypto)?;
    let document = Document {
        format: FORMAT.to_owned(),
        version: VERSION,
        label: label.to_owned(),
        kdf: "argon2id".to_owned(),
        memory_kib: sealed.params.memory_kib,
        iterations: sealed.params.iterations,
        parallelism: sealed.params.parallelism,
        salt: crypto::hex_encode(&sealed.salt),
        nonce: crypto::hex_encode(&sealed.nonce),
        ciphertext: crypto::hex_encode(&sealed.ciphertext),
    };
    toml::to_string_pretty(&document).map_err(ConfigError::Serialize)
}

/// Whether `text` looks like a sealed document (without opening it), so a caller can ask for a
/// passphrase only when one is needed.
#[must_use]
pub fn is_sealed(text: &str) -> bool {
    text.len() <= MAX_SEALED_BYTES
        && toml::from_str::<toml::Table>(text)
            .ok()
            .and_then(|table| {
                table
                    .get("format")
                    .and_then(|v| v.as_str().map(str::to_owned))
            })
            .is_some_and(|format| format == FORMAT)
}

/// Decrypts the sealed document `text` made for `label`.
///
/// # Errors
///
/// * [`ConfigError::Sealed`]: not a sealed document, damaged, made for another label, or made
///   by a newer version.
/// * [`ConfigError::WrongPassphrase`]: the passphrase does not match, or the document was
///   changed after it was sealed.
pub fn open_text(passphrase: &SecretString, label: &str, text: &str) -> Result<String> {
    if text.len() > MAX_SEALED_BYTES {
        return Err(bad("it is far too large"));
    }
    let document: Document =
        toml::from_str(text).map_err(|_| bad("it is not a sealed document"))?;
    if document.format != FORMAT {
        return Err(bad("it is not a sealed document"));
    }
    if document.version > VERSION {
        return Err(bad(format!(
            "it was sealed by a newer version of wyck (format {})",
            document.version
        )));
    }
    if document.kdf != "argon2id" {
        return Err(bad(format!("unknown key derivation `{}`", document.kdf)));
    }
    if document.label != label {
        return Err(bad(format!("it holds `{}`, not `{label}`", document.label)));
    }
    let decode = |what: &str, hex: &str| {
        crypto::hex_decode(hex).map_err(|reason| bad(format!("{what}: {reason}")))
    };
    let sealed = Sealed {
        params: KdfParams {
            memory_kib: document.memory_kib,
            iterations: document.iterations,
            parallelism: document.parallelism,
        },
        salt: crypto::exact(&decode("salt", &document.salt)?, "the salt").map_err(from_crypto)?,
        nonce: crypto::exact(&decode("nonce", &document.nonce)?, "the nonce")
            .map_err(from_crypto)?,
        ciphertext: decode("ciphertext", &document.ciphertext)?,
    };
    let plaintext =
        crypto::open(passphrase, &associated_data(label), &sealed).map_err(from_crypto)?;
    String::from_utf8(plaintext.to_vec()).map_err(|_| bad("what it holds is not text"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass(text: &str) -> SecretString {
        SecretString::from(text.to_owned())
    }

    #[test]
    fn a_sealed_text_hides_the_text_and_opens_with_the_passphrase() {
        let plain = "secret = \"hunter2\"\n[nested]\nlist = [1, 2, 3]\n";
        let sealed = seal_text(&pass("pw"), "wyck-backup", plain).unwrap();
        assert!(is_sealed(&sealed));
        assert!(!sealed.contains("hunter2") && !sealed.contains("nested"));
        assert_eq!(
            open_text(&pass("pw"), "wyck-backup", &sealed).unwrap(),
            plain
        );
    }

    #[test]
    fn plain_text_is_not_a_sealed_document() {
        assert!(!is_sealed("format = \"wyck-backup\"\n"));
        assert!(!is_sealed("not toml ["));
        assert!(matches!(
            open_text(&pass("pw"), "wyck-backup", "format = \"wyck-backup\"\n"),
            Err(ConfigError::Sealed(_))
        ));
    }

    #[test]
    fn the_wrong_passphrase_is_told_apart_from_a_damaged_document() {
        let sealed = seal_text(&pass("right"), "wyck-backup", "x = 1\n").unwrap();
        assert!(matches!(
            open_text(&pass("wrong"), "wyck-backup", &sealed),
            Err(ConfigError::WrongPassphrase)
        ));
        let damaged = sealed.replace("ciphertext = \"", "ciphertext = \"zz");
        assert!(matches!(
            open_text(&pass("right"), "wyck-backup", &damaged),
            Err(ConfigError::Sealed(_))
        ));
    }

    #[test]
    fn a_sealed_document_cannot_be_passed_off_as_another_kind() {
        let sealed = seal_text(&pass("pw"), "wyck-backup", "x = 1\n").unwrap();
        assert!(matches!(
            open_text(&pass("pw"), "something-else", &sealed),
            Err(ConfigError::Sealed(_))
        ));
        // Changing the label inside the file is caught by the cipher too.
        let relabeled = sealed.replace("label = \"wyck-backup\"", "label = \"something-else\"");
        assert!(matches!(
            open_text(&pass("pw"), "something-else", &relabeled),
            Err(ConfigError::WrongPassphrase)
        ));
    }

    #[test]
    fn a_newer_version_and_a_hostile_cost_are_refused() {
        let sealed = seal_text(&pass("pw"), "l", "x = 1\n").unwrap();
        let newer = sealed.replace("version = 1", "version = 2");
        assert!(matches!(
            open_text(&pass("pw"), "l", &newer),
            Err(ConfigError::Sealed(reason)) if reason.contains("newer")
        ));
        let hostile = sealed.replace("memory_kib = 19456", "memory_kib = 4294967295");
        assert!(matches!(
            open_text(&pass("pw"), "l", &hostile),
            Err(ConfigError::Sealed(_))
        ));
    }

    #[test]
    fn unicode_and_empty_texts_survive() {
        for plain in ["", "caf\u{e9} \u{1F4C8}\n"] {
            let sealed = seal_text(&pass("p\u{e4}ss"), "l", plain).unwrap();
            assert_eq!(open_text(&pass("p\u{e4}ss"), "l", &sealed).unwrap(), plain);
        }
    }
}
