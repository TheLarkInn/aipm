//! Error types for workspace operations.
//!
//! Covers workspace root discovery, member discovery via glob expansion,
//! and manifest loading for workspace members.

/// Errors that can occur during workspace operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A workspace member discovery error (glob, I/O, or manifest parse failure).
    #[error("workspace discovery error: {0}")]
    Discovery(String),

    /// No workspace root was found walking up from the starting directory.
    #[error("no workspace root found — no aipm.toml with [workspace] in parent directories")]
    NoWorkspaceRoot,
}
