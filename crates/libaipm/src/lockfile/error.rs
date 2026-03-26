//! Error types for lockfile operations.

use std::path::PathBuf;

/// Errors that can occur during lockfile operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An I/O error occurred reading or writing the lockfile.
    #[error("lockfile I/O error at {path}: {source}")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// The lockfile TOML could not be parsed.
    #[error("lockfile parse error: {reason}")]
    Parse {
        /// Description of the parse error.
        reason: String,
    },

    /// The lockfile version is unsupported.
    #[error("unsupported lockfile version: {version} (expected {expected})")]
    UnsupportedVersion {
        /// The version found in the lockfile.
        version: u32,
        /// The expected version.
        expected: u32,
    },

    /// The lockfile does not match the manifest (drift detected).
    #[error("lockfile/manifest drift: {reason}")]
    Drift {
        /// Description of the drift.
        reason: String,
    },
}
