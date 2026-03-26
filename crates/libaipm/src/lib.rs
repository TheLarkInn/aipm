//! Core library for AIPM — AI Plugin Manager.
//!
//! This crate contains the shared logic used by both the `aipm` consumer binary
//! and the `aipm-pack` author binary: manifest parsing, dependency resolution,
//! content-addressable store, lockfile management, and linking.

pub mod fs;
pub mod init;
pub mod lockfile;
pub mod manifest;
pub mod migrate;
pub mod registry;
pub mod resolver;
pub mod store;
pub mod version;
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
