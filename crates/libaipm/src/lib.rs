//! Core library for AIPM — AI Plugin Manager.
//!
//! This crate contains the shared logic used by the `aipm` binary: manifest
//! parsing, dependency resolution, content-addressable store, lockfile
//! management, and linking.

pub mod acquirer;
pub mod cache;
pub mod discovery;
pub mod engine;
pub mod frontmatter;
pub mod fs;
pub mod generate;
pub mod init;
pub mod installed;
pub mod installer;
pub mod linker;
pub mod lint;
pub mod locked_file;
pub mod lockfile;
pub mod logging;
pub mod make;
pub mod manifest;
pub mod marketplace;
pub mod migrate;
pub mod path_security;
pub mod platform;
pub mod registry;
pub mod resolver;
pub mod security;
pub mod spec;
pub mod store;
pub mod version;
#[cfg(feature = "wizard")]
pub mod wizard;
pub mod workspace;
pub mod workspace_init;

/// Returns the library version.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!version().is_empty());
    }
}
