//! Registry abstraction for package sources.
//!
//! Defines the [`Registry`] trait and supporting types. The initial
//! implementation is `GitRegistry` (git-based index + tarball downloads).
//! A future `HttpRegistry` can implement the same trait.

pub mod config;
pub mod error;
pub mod index;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::version::Version;
use error::Error;

/// A single published version entry from the registry index.
///
/// Each line in the index file is one JSON object corresponding to
/// one published version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionEntry {
    /// Package name (including `@scope/` if scoped).
    pub name: String,

    /// Exact semver version string.
    pub vers: String,

    /// Dependencies of this version.
    #[serde(default)]
    pub deps: Vec<DepEntry>,

    /// SHA-512 checksum of the `.aipm` tarball.
    pub cksum: String,

    /// Feature flag definitions.
    #[serde(default)]
    pub features: BTreeMap<String, Vec<String>>,

    /// Whether this version has been yanked.
    #[serde(default)]
    pub yanked: bool,
}

/// A dependency entry within a [`VersionEntry`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepEntry {
    /// Dependency package name.
    pub name: String,

    /// Version requirement (e.g. `^1.0`).
    pub req: String,

    /// Features to activate on the dependency.
    #[serde(default)]
    pub features: Vec<String>,

    /// Whether this is an optional dependency.
    #[serde(default)]
    pub optional: bool,

    /// Whether to enable default features of the dependency.
    #[serde(default = "default_true")]
    pub default_features: bool,
}

const fn default_true() -> bool {
    true
}

/// All published versions for a package.
#[derive(Debug)]
pub struct PackageMetadata {
    /// The package name.
    pub name: String,

    /// All versions available in the registry.
    pub versions: Vec<VersionEntry>,
}

/// Abstraction over registry backends.
///
/// Implement this trait for git-based, HTTP API, or local registries.
pub trait Registry: Send + Sync {
    /// Fetch metadata for a package (all available versions).
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if the package is not found or a network/I/O error occurs.
    fn get_metadata(&self, name: &str) -> Result<PackageMetadata, Error>;

    /// Download the `.aipm` tarball for a specific package version.
    ///
    /// Returns the raw bytes of the tarball.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if the version is not found, download fails,
    /// or checksum verification fails.
    fn download(&self, name: &str, version: &Version) -> Result<Vec<u8>, Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_entry_deserialize_json() {
        let json = r#"{
            "name": "code-review",
            "vers": "1.2.0",
            "deps": [
                {
                    "name": "lint-skill",
                    "req": "^1.0",
                    "features": [],
                    "optional": false,
                    "default_features": true
                }
            ],
            "cksum": "sha512-abc123def456",
            "features": {
                "default": ["basic"],
                "basic": [],
                "deep": ["dep:heavy-analyzer"]
            },
            "yanked": false
        }"#;

        let entry: VersionEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.name, "code-review");
        assert_eq!(entry.vers, "1.2.0");
        assert_eq!(entry.deps.len(), 1);
        assert_eq!(entry.deps[0].name, "lint-skill");
        assert_eq!(entry.deps[0].req, "^1.0");
        assert!(entry.deps[0].default_features);
        assert!(!entry.deps[0].optional);
        assert_eq!(entry.cksum, "sha512-abc123def456");
        assert_eq!(entry.features.len(), 3);
        assert!(!entry.yanked);
    }

    #[test]
    fn version_entry_deserialize_minimal() {
        let json = r#"{"name":"simple","vers":"0.1.0","cksum":"sha512-test"}"#;

        let entry: VersionEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.name, "simple");
        assert!(entry.deps.is_empty());
        assert!(entry.features.is_empty());
        assert!(!entry.yanked);
    }

    #[test]
    fn version_entry_round_trip_json() {
        let entry = VersionEntry {
            name: "test-pkg".to_string(),
            vers: "2.0.0".to_string(),
            deps: vec![DepEntry {
                name: "dep-a".to_string(),
                req: ">=1.0".to_string(),
                features: vec!["json".to_string()],
                optional: true,
                default_features: false,
            }],
            cksum: "sha512-hash".to_string(),
            features: BTreeMap::new(),
            yanked: true,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: VersionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, parsed);
    }

    #[test]
    fn dep_entry_default_features_defaults_true() {
        let json = r#"{"name":"dep","req":"^1.0"}"#;
        let entry: DepEntry = serde_json::from_str(json).unwrap();
        assert!(entry.default_features);
        assert!(!entry.optional);
        assert!(entry.features.is_empty());
    }

    #[test]
    fn version_entry_json_lines_parsing() {
        let lines = "\
            {\"name\":\"pkg\",\"vers\":\"1.0.0\",\"cksum\":\"sha512-a\"}\n\
            {\"name\":\"pkg\",\"vers\":\"1.1.0\",\"cksum\":\"sha512-b\"}\n\
            {\"name\":\"pkg\",\"vers\":\"2.0.0\",\"cksum\":\"sha512-c\",\"yanked\":true}\n";

        let entries: Vec<VersionEntry> = lines
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].vers, "1.0.0");
        assert_eq!(entries[1].vers, "1.1.0");
        assert_eq!(entries[2].vers, "2.0.0");
        assert!(entries[2].yanked);
    }

    #[test]
    fn package_metadata_construction() {
        let meta = PackageMetadata { name: "test".to_string(), versions: vec![] };
        assert_eq!(meta.name, "test");
        assert!(meta.versions.is_empty());
    }
}
