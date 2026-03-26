//! Error types for the installer module.

/// Errors that can occur during install operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An I/O error occurred.
    #[error("installer I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Manifest parsing or editing failed.
    #[error("manifest error: {reason}")]
    Manifest {
        /// Description of the manifest error.
        reason: String,
    },

    /// Lockfile drift detected in `--locked` mode.
    #[error("lockfile drift: {reason}")]
    LockfileDrift {
        /// Description of the drift.
        reason: String,
    },

    /// Resolution failed.
    #[error("resolution error: {0}")]
    Resolution(String),
}
