//! AES-256-GCM encryption for bot bearer tokens.
//!
//! Tokens are encrypted at submission time and decrypted only when the harness
//! needs to make an outbound call to the bot's `/debate` endpoint. Output
//! layout: `[12-byte nonce][ciphertext][16-byte auth tag]`.
//!
//! The master key is wrapped in [`BotTokenKey`], a non-Clone non-Copy newtype
//! that zeroises its bytes on drop via the `zeroize` crate.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// Errors from encrypt/decrypt operations.
#[derive(Debug, Error)]
pub enum CryptoError {
    /// Encryption failed (library-level error, effectively unreachable).
    #[error("encryption failed")]
    Encrypt,
    /// Decryption failed — wrong key, tampered ciphertext, or malformed input.
    #[error("decryption failed")]
    Decrypt,
    /// Ciphertext shorter than the 12-byte nonce prefix.
    #[error("ciphertext too short (expected at least 12 bytes for nonce)")]
    Malformed,
}

/// 256-bit AES key for bot token encryption. Wiped from memory on drop.
///
/// Deliberately non-`Clone` / non-`Copy` so the key material exists in exactly
/// one location at runtime (inside the shared `Arc<AppStateInner>`). Borrow
/// it via `AsRef<[u8]>` for use with `aes-gcm` APIs.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct BotTokenKey([u8; 32]);

impl BotTokenKey {
    /// Construct a key from a 32-byte array. The caller's copy is moved in;
    /// any temporary holding the original bytes should itself be zeroised.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// All-zero key. Used for the test harness and the dev-mode boot path
    /// when no real key is configured — never suitable for production data.
    pub fn zero() -> Self {
        Self([0u8; 32])
    }
}

impl AsRef<[u8]> for BotTokenKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

// Custom Debug — never render the raw bytes, even with `{:?}` or `dbg!`.
impl std::fmt::Debug for BotTokenKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BotTokenKey(***)")
    }
}

/// Encrypt a plaintext string with a random nonce. Output is
/// `nonce || ciphertext_with_tag`.
pub fn encrypt(key: &BotTokenKey, plaintext: &str) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.as_ref()));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| CryptoError::Encrypt)?;
    let mut output = Vec::with_capacity(12 + ciphertext.len());
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt a `nonce || ciphertext_with_tag` blob.
pub fn decrypt(key: &BotTokenKey, ciphertext: &[u8]) -> Result<String, CryptoError> {
    if ciphertext.len() < 12 {
        return Err(CryptoError::Malformed);
    }
    let (nonce_bytes, rest) = ciphertext.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.as_ref()));
    let plain = cipher
        .decrypt(nonce, rest)
        .map_err(|_| CryptoError::Decrypt)?;
    String::from_utf8(plain).map_err(|_| CryptoError::Decrypt)
}

/// Parse a 64-character hex string into a 32-byte key. The intermediate
/// decoded `Vec<u8>` is zeroised when it goes out of scope.
pub fn parse_key_hex(s: &str) -> Result<BotTokenKey, CryptoError> {
    let bytes = Zeroizing::new(hex::decode(s).map_err(|_| CryptoError::Malformed)?);
    if bytes.len() != 32 {
        return Err(CryptoError::Malformed);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(BotTokenKey::from_bytes(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> BotTokenKey {
        BotTokenKey::from_bytes([7u8; 32])
    }

    #[test]
    fn round_trip_preserves_plaintext() {
        let key = test_key();
        for s in ["", "short", "a bearer token with spaces and 1234567890"] {
            let c = encrypt(&key, s).unwrap();
            assert_eq!(decrypt(&key, &c).unwrap(), s);
        }
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = test_key();
        let mut c = encrypt(&key, "secret").unwrap();
        let last = c.len() - 1;
        c[last] ^= 0x01;
        assert!(matches!(decrypt(&key, &c), Err(CryptoError::Decrypt)));
    }

    #[test]
    fn wrong_key_fails() {
        let c = encrypt(&test_key(), "secret").unwrap();
        let wrong = BotTokenKey::from_bytes([9u8; 32]);
        assert!(matches!(decrypt(&wrong, &c), Err(CryptoError::Decrypt)));
    }

    #[test]
    fn short_ciphertext_is_malformed() {
        assert!(matches!(
            decrypt(&test_key(), &[0u8; 5]),
            Err(CryptoError::Malformed)
        ));
    }

    #[test]
    fn parse_key_hex_happy() {
        let s = "0".repeat(64);
        let k = parse_key_hex(&s).unwrap();
        assert_eq!(k.as_ref(), &[0u8; 32][..]);
    }

    #[test]
    fn parse_key_hex_wrong_length() {
        assert!(parse_key_hex("abcd").is_err());
    }

    #[test]
    fn zeroize_wipes_key_bytes() {
        // Explicit zeroise leaves all bytes zero. Drop-time behaviour is
        // identical (ZeroizeOnDrop calls the same Zeroize impl).
        let mut k = BotTokenKey::from_bytes([0xABu8; 32]);
        assert_eq!(k.as_ref(), &[0xABu8; 32][..]);
        k.zeroize();
        assert_eq!(k.as_ref(), &[0u8; 32][..]);
    }

    #[test]
    fn debug_does_not_leak_bytes() {
        let k = BotTokenKey::from_bytes([0xFFu8; 32]);
        let rendered = format!("{k:?}");
        assert_eq!(rendered, "BotTokenKey(***)");
        assert!(!rendered.contains("ff"));
        assert!(!rendered.contains("255"));
    }
}
