//! Error types for registry operations.

/// Errors that can occur during registry operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The requested package was not found in the registry.
    #[error("package not found: {name}")]
    PackageNotFound {
        /// The package name that was not found.
        name: String,
    },

    /// The requested version was not found for a package.
    #[error("version {version} not found for package {name}")]
    VersionNotFound {
        /// The package name.
        name: String,
        /// The version that was not found.
        version: String,
    },

    /// An I/O or network error occurred.
    #[error("registry I/O error: {reason}")]
    Io {
        /// Description of the I/O error.
        reason: String,
    },

    /// Index data could not be parsed.
    #[error("registry index parse error: {reason}")]
    IndexParse {
        /// Description of the parse error.
        reason: String,
    },

    /// Checksum verification failed.
    #[error("checksum mismatch for {name}@{version}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// The package name.
        name: String,
        /// The package version.
        version: String,
        /// The expected checksum.
        expected: String,
        /// The actual computed checksum.
        actual: String,
    },
}
