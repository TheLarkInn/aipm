//! Error types for workspace initialization.

use std::path::PathBuf;

/// Errors specific to workspace init.
#[derive(Debug, thiserror::Error)]
pub enum Error {
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

    /// An existing `aipm.toml` could not be parsed or validated.
    ///
    /// Surfaced when `aipm init` finds a pre-existing manifest that the
    /// idempotent path would otherwise log-and-reuse, but the file is
    /// malformed (TOML syntax error, missing required fields, etc.).
    #[error("existing manifest at {} is invalid: {source}", path.display())]
    ExistingManifestInvalid {
        /// Path to the malformed manifest file.
        path: PathBuf,
        /// The underlying manifest error.
        #[source]
        source: crate::manifest::error::Error,
    },

    /// An existing engine-appropriate marketplace manifest could not be
    /// parsed.
    ///
    /// Surfaced when `aipm init` finds a pre-existing
    /// `marketplace.json` file (under `.ai/.claude-plugin/`,
    /// `.ai/.github/plugin/`, etc.) that the idempotent path would
    /// otherwise log-and-reuse, but the file is malformed JSON.
    #[error("existing marketplace manifest at {} is invalid: {source}", path.display())]
    ExistingMarketplaceInvalid {
        /// Path to the malformed marketplace manifest.
        path: PathBuf,
        /// The underlying JSON parse error.
        #[source]
        source: serde_json::Error,
    },
}
