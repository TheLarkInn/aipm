//! Error types for the migration pipeline.

use std::path::PathBuf;

/// Errors specific to migration.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The `.ai/` marketplace directory does not exist.
    #[error("marketplace directory does not exist at {0} — run `aipm init --marketplace` first")]
    MarketplaceNotFound(PathBuf),

    /// The source directory does not exist.
    #[error("source directory does not exist: {0}")]
    SourceNotFound(PathBuf),

    /// The source type is not supported.
    #[error("unsupported source type '{0}' — supported sources: .claude, .github")]
    UnsupportedSource(String),

    /// Failed to parse marketplace.json.
    #[error("failed to parse marketplace.json at {path}: {source}")]
    MarketplaceJsonParse {
        /// Path to the marketplace.json file.
        path: PathBuf,
        /// The underlying parse error.
        source: serde_json::Error,
    },

    /// Failed to parse SKILL.md frontmatter.
    #[error("failed to parse SKILL.md frontmatter in {path}: {reason}")]
    FrontmatterParse {
        /// Path to the SKILL.md file.
        path: PathBuf,
        /// Description of the parse failure.
        reason: String,
    },

    /// Failed to parse a JSON configuration file.
    #[error("failed to parse {path}: {reason}")]
    ConfigParse {
        /// Path to the configuration file.
        path: PathBuf,
        /// Description of the parse failure.
        reason: String,
    },

    /// Discovery failed during recursive directory walking.
    #[error("failed to discover source directories: {0}")]
    DiscoveryFailed(#[from] crate::discovery::Error),

    /// An I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
