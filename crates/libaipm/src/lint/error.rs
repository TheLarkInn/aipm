//! Error types for the lint system.

use std::path::PathBuf;

/// Errors that can occur during linting.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// I/O error during filesystem access.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parse error.
    #[error("JSON parse error in {path}: {reason}")]
    JsonParse {
        /// Path to the file that failed to parse.
        path: PathBuf,
        /// Reason for the parse failure.
        reason: String,
    },

    /// Frontmatter parse error.
    #[error("frontmatter parse error in {path}: {reason}")]
    FrontmatterParse {
        /// Path to the file that failed to parse.
        path: PathBuf,
        /// Reason for the parse failure.
        reason: String,
    },

    /// Discovery failed during recursive directory walking.
    #[error(transparent)]
    DiscoveryFailed(#[from] crate::discovery::Error),
}
