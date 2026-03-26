//! Store directory layout utilities.
//!
//! The content-addressable store uses a 2-character prefix sharding scheme:
//! `~/.aipm/store/{first-2-chars}/{remaining-chars}`
//!
//! This prevents any single directory from accumulating too many entries.

use std::path::{Path, PathBuf};

use super::error::Error;
use super::hash::SHA512_HEX_LEN;

/// Minimum prefix length used for directory sharding.
const PREFIX_LEN: usize = 2;

/// Compute the full filesystem path for a given content hash within the store.
///
/// The hash is split into a 2-character prefix directory and the remaining
/// characters as the filename: `{store_root}/{prefix}/{rest}`.
///
/// # Errors
///
/// Returns [`Error::InvalidHash`] if the hash is not a valid SHA-512 hex string.
pub fn hash_to_path(store_root: &Path, hash: &str) -> Result<PathBuf, Error> {
    if hash.len() != SHA512_HEX_LEN {
        return Err(Error::InvalidHash {
            reason: format!("expected {SHA512_HEX_LEN} hex characters, got {}", hash.len()),
        });
    }
    if !hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
        return Err(Error::InvalidHash {
            reason: "hash must contain only lowercase hex characters (0-9, a-f)".to_string(),
        });
    }

    let (prefix, rest) = hash.split_at(PREFIX_LEN);
    Ok(store_root.join(prefix).join(rest))
}

/// Return the 2-character prefix directory for a given hash.
///
/// # Errors
///
/// Returns [`Error::InvalidHash`] if the hash is too short.
pub fn hash_prefix_dir(store_root: &Path, hash: &str) -> Result<PathBuf, Error> {
    if hash.len() < PREFIX_LEN {
        return Err(Error::InvalidHash {
            reason: format!("hash too short: need at least {PREFIX_LEN} characters"),
        });
    }
    let prefix = &hash[..PREFIX_LEN];
    Ok(store_root.join(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::hash::sha512_hex;

    #[test]
    fn hash_to_path_splits_correctly() {
        let root = Path::new("/home/user/.aipm/store");
        let hash = sha512_hex(b"test content");
        let path = hash_to_path(root, &hash).unwrap();

        let prefix = &hash[..2];
        let rest = &hash[2..];
        assert_eq!(path, root.join(prefix).join(rest));
    }

    #[test]
    fn hash_to_path_rejects_short_hash() {
        let root = Path::new("/store");
        let result = hash_to_path(root, "abcd");
        assert!(result.is_err());
    }

    #[test]
    fn hash_to_path_rejects_uppercase() {
        let root = Path::new("/store");
        let hash = "A".repeat(128);
        let result = hash_to_path(root, &hash);
        assert!(result.is_err());
    }

    #[test]
    fn hash_to_path_rejects_non_hex() {
        let root = Path::new("/store");
        let mut hash = "a".repeat(127);
        hash.push('z');
        let result = hash_to_path(root, &hash);
        assert!(result.is_err());
    }

    #[test]
    fn hash_prefix_dir_works() {
        let root = Path::new("/store");
        let hash = sha512_hex(b"data");
        let dir = hash_prefix_dir(root, &hash).unwrap();
        assert_eq!(dir, root.join(&hash[..2]));
    }

    #[test]
    fn hash_prefix_dir_rejects_too_short() {
        let root = Path::new("/store");
        let result = hash_prefix_dir(root, "a");
        assert!(result.is_err());
    }
}
