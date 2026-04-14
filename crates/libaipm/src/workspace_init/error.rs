//! Error types for workspace initialization.

use std::path::PathBuf;

/// Errors specific to workspace init.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The directory already has an `aipm.toml`.
    #[error("already initialized: aipm.toml already exists in {}", .0.display())]
    WorkspaceAlreadyInitialized(PathBuf),

    /// The `.ai/` marketplace directory already exists.
    #[error(".ai/ marketplace already exists in {}", .0.display())]
    MarketplaceAlreadyExists(PathBuf),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parse error in an existing settings file.
    #[error("JSON parse error in {}: {source}", path.display())]
    JsonParse {
        /// Path to the file that failed to parse.
        path: PathBuf,
        /// The underlying `serde_json` error.
        source: serde_json::Error,
    },
}
