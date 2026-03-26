//! SHA-512 hashing utilities for the content-addressable store.
//!
//! Each file is hashed individually with SHA-512, enabling per-file
//! deduplication across package versions.

use sha2::{Digest, Sha512};
use std::fmt::Write as FmtWrite;

/// Compute the SHA-512 hash of the given content, returning a lowercase
/// hex-encoded string (128 characters).
pub fn sha512_hex(content: &[u8]) -> String {
    let hash = Sha512::digest(content);
    let mut hex = String::with_capacity(128);
    for byte in hash {
        // write! on a String is infallible, but we handle the result anyway
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Minimum valid hash length (SHA-512 hex = 128 chars).
pub const SHA512_HEX_LEN: usize = 128;

/// Validate that a string looks like a valid SHA-512 hex hash.
///
/// Returns `Ok(())` if the hash is exactly 128 lowercase hex characters,
/// or an error description otherwise.
pub fn validate(hash: &str) -> Result<(), String> {
    if hash.len() != SHA512_HEX_LEN {
        return Err(format!("expected {} hex characters, got {}", SHA512_HEX_LEN, hash.len()));
    }
    if !hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
        return Err("hash must contain only lowercase hex characters (0-9, a-f)".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha512_empty_input() {
        // Known SHA-512 of empty input
        let hash = sha512_hex(b"");
        assert_eq!(hash.len(), 128);
        assert_eq!(
            hash,
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
             47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
    }

    #[test]
    fn sha512_hello_world() {
        let hash = sha512_hex(b"hello world");
        assert_eq!(hash.len(), 128);
        // Known SHA-512 of "hello world"
        assert_eq!(
            hash,
            "309ecc489c12d6eb4cc40f50c902f2b4d0ed77ee511a7c7a9bcd3ca86d4cd86f\
             989dd35bc5ff499670da34255b45b0cfd830e81f605dcf7dc5542e93ae9cd76f"
        );
    }

    #[test]
    fn sha512_binary_data() {
        let data: Vec<u8> = (0..=255).collect();
        let hash = sha512_hex(&data);
        assert_eq!(hash.len(), 128);
        // All chars should be lowercase hex
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn sha512_deterministic() {
        let data = b"deterministic test input";
        let hash1 = sha512_hex(data);
        let hash2 = sha512_hex(data);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn sha512_different_inputs_produce_different_hashes() {
        let hash1 = sha512_hex(b"input A");
        let hash2 = sha512_hex(b"input B");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn validate_valid() {
        let hash = sha512_hex(b"test");
        assert!(validate(&hash).is_ok());
    }

    #[test]
    fn validate_too_short() {
        let result = validate("abcdef");
        assert!(result.is_err());
        let err = result.err().unwrap_or_default();
        assert!(err.contains("expected 128"));
    }

    #[test]
    fn validate_too_long() {
        let long = "a".repeat(129);
        let result = validate(&long);
        assert!(result.is_err());
    }

    #[test]
    fn validate_uppercase_rejected() {
        // 128 chars but with uppercase
        let hash = "A".repeat(128);
        let result = validate(&hash);
        assert!(result.is_err());
        let err = result.err().unwrap_or_default();
        assert!(err.contains("lowercase"));
    }

    #[test]
    fn validate_non_hex_rejected() {
        let mut hash = "a".repeat(127);
        hash.push('g'); // 'g' is not hex
        let result = validate(&hash);
        assert!(result.is_err());
    }

    #[test]
    fn validate_empty_string() {
        let result = validate("");
        assert!(result.is_err());
    }
}
