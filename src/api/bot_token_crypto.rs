//! AES-256-GCM encryption for bot bearer tokens.
//!
//! Tokens are encrypted at submission time and decrypted only when the harness
//! needs to make an outbound call to the bot's `/debate` endpoint. Output
//! layout: `[12-byte nonce][ciphertext][16-byte auth tag]`.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use thiserror::Error;

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

/// Fixed-size 256-bit key.
pub type BotTokenKey = [u8; 32];

/// Encrypt a plaintext string with a random nonce. Output is
/// `nonce || ciphertext_with_tag`.
pub fn encrypt(key: &BotTokenKey, plaintext: &str) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
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
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let plain = cipher
        .decrypt(nonce, rest)
        .map_err(|_| CryptoError::Decrypt)?;
    String::from_utf8(plain).map_err(|_| CryptoError::Decrypt)
}

/// Parse a 64-character hex string into a 32-byte key.
pub fn parse_key_hex(s: &str) -> Result<BotTokenKey, CryptoError> {
    let bytes = hex::decode(s).map_err(|_| CryptoError::Malformed)?;
    if bytes.len() != 32 {
        return Err(CryptoError::Malformed);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> BotTokenKey {
        [7u8; 32]
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
        let wrong = [9u8; 32];
        assert!(matches!(decrypt(&wrong, &c), Err(CryptoError::Decrypt)));
    }

    #[test]
    fn short_ciphertext_is_malformed() {
        assert!(matches!(decrypt(&test_key(), &[0u8; 5]), Err(CryptoError::Malformed)));
    }

    #[test]
    fn parse_key_hex_happy() {
        let s = "0".repeat(64);
        let k = parse_key_hex(&s).unwrap();
        assert_eq!(k, [0u8; 32]);
    }

    #[test]
    fn parse_key_hex_wrong_length() {
        assert!(parse_key_hex("abcd").is_err());
    }
}
