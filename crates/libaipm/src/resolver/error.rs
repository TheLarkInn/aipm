//! Error types for dependency resolution.

/// Errors that can occur during dependency resolution.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No compatible version found for a dependency.
    #[error("no compatible version found for '{name}' matching {requirement}")]
    NoMatch {
        /// The package name.
        name: String,
        /// The version requirement that could not be satisfied.
        requirement: String,
    },

    /// A version conflict between two requirements for the same package.
    #[error("version conflict for '{name}': {existing_req} (from {existing_source}) vs {new_req} (from {new_source})")]
    Conflict {
        /// The package with conflicting requirements.
        name: String,
        /// The already-activated requirement.
        existing_req: String,
        /// Which package introduced the existing requirement.
        existing_source: String,
        /// The new conflicting requirement.
        new_req: String,
        /// Which package introduced the new requirement.
        new_source: String,
    },

    /// Registry lookup failed.
    #[error("registry error: {reason}")]
    Registry {
        /// Description of the registry error.
        reason: String,
    },

    /// Version parsing error.
    #[error("version error: {reason}")]
    Version {
        /// Description of the version error.
        reason: String,
    },
}
