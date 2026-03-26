//! Error types for the linker module.

use std::path::PathBuf;

/// Errors that can occur during linking operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An I/O error occurred during a link operation.
    #[error("link I/O error at '{}': {source}", path.display())]
    Io {
        /// The path involved in the error.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// The target path already exists and is not a symlink/junction.
    #[error("target already exists and is not a link: {}", path.display())]
    TargetExists {
        /// The path that already exists.
        path: PathBuf,
    },

    /// The source path does not exist.
    #[error("source path does not exist: {}", path.display())]
    SourceMissing {
        /// The missing source path.
        path: PathBuf,
    },
}
