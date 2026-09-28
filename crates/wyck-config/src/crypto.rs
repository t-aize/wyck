//! The encryption both the secret files and the sealed documents are made of, in one place.
//!
//! A passphrase becomes a 256-bit key with Argon2id, and the data is encrypted with
//! ChaCha20-Poly1305, an authenticated cipher: a wrong passphrase, or a single changed bit, makes
//! opening fail instead of giving back garbage.
//!
//! * Every seal draws a fresh 16-byte salt and a fresh 12-byte nonce from the operating system, so
//!   the same passphrase never gives the same key twice and a nonce is never used twice under one
//!   key (the one mistake that breaks this kind of cipher).
//! * The Argon2id cost is written next to the data ([`KdfParams`]), so it can be raised later
//!   without making older files unreadable, and a file cannot ask for more than
//!   [`KdfParams::validated`] allows (a hostile file must not make the app allocate gigabytes).
//! * The *associated data* ties the ciphertext to what it is for: the key it was stored under, or
//!   the label of the document. Copying the file of one secret over another, or a backup over a
//!   different kind of document, then fails to open. Without it the cipher would accept the swap.
//! * Keys and decrypted bytes live in [`Zeroizing`] buffers, wiped when dropped.

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
    /// Argon2 refused its parameters or its input.
    Kdf(String),
    /// The system could not give random bytes.
    Random(getrandom::Error),
    /// The cipher failed to encrypt.
    Encrypt,
    /// The tag did not verify: wrong passphrase, or changed data.
    Decrypt,
    /// The salt, nonce or parameters have the wrong size or are out of bounds.
    Shape(String),
}

/// The cost of turning a passphrase into a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KdfParams {
    /// Memory, in KiB.
    pub memory_kib: u32,
    /// Passes over that memory.
    pub iterations: u32,
    /// Lanes computed in parallel.
    pub parallelism: u32,
}

impl KdfParams {
    /// What new data is sealed with: 19 MiB, 2 passes, 1 lane. This is the minimum the OWASP
    /// Password Storage Cheat Sheet gives for Argon2id, and what the `argon2` crate uses by
    /// default, so data sealed before the cost was written down opens the same way.
    pub(crate) const CURRENT: Self = Self {
        memory_kib: 19_456,
        iterations: 2,
        parallelism: 1,
    };

    const MAX_MEMORY_KIB: u32 = 256 * 1024;
    const MAX_ITERATIONS: u32 = 10;
    const MAX_PARALLELISM: u32 = 8;

    /// These parameters, if they are within what this crate will run: at least what Argon2
    /// itself needs, at most a memory and time a hostile file cannot abuse.
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

/// The bytes of a hexadecimal text (either case). Works on bytes, never on slices of the text, so
/// a multi-byte character in a damaged file is an error and not a panic.
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
    fn the_current_cost_is_what_the_argon2_crate_uses_by_default() {
        // Data sealed before the cost was written down used `Argon2::default()`.
        let default = Params::DEFAULT;
        assert_eq!(default.m_cost(), KdfParams::CURRENT.memory_kib);
        assert_eq!(default.t_cost(), KdfParams::CURRENT.iterations);
        assert_eq!(default.p_cost(), KdfParams::CURRENT.parallelism);
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
