//! Error types for the content-addressable store.

use std::path::PathBuf;

/// Errors that can occur during store operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An I/O error occurred during a store operation.
    #[error("store I/O error at {path}: {source}")]
    Io {
        /// The path involved in the operation.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// The provided hash string is invalid (wrong length or non-hex chars).
    #[error("invalid hash: {reason}")]
    InvalidHash {
        /// Description of why the hash is invalid.
        reason: String,
    },

    /// Content not found in the store.
    #[error("content not found for hash: {hash}")]
    NotFound {
        /// The hash that was not found.
        hash: String,
    },
}
