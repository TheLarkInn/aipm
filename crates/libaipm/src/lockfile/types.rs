//! Lockfile data types for `aipm.lock`.
//!
//! The lockfile records exact resolved dependency versions with integrity
//! hashes for deterministic installs.

use serde::{Deserialize, Serialize};

/// The current lockfile format version.
pub const LOCKFILE_VERSION: u32 = 1;

/// Top-level lockfile structure.
///
/// Serializes to/from the `aipm.lock` TOML format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lockfile {
    /// Lockfile metadata (version, generator).
    pub metadata: Metadata,

    /// Resolved packages with exact versions and checksums.
    #[serde(rename = "package", default)]
    pub packages: Vec<Package>,
}

/// Lockfile metadata section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Metadata {
    /// Lockfile format version (currently 1).
    pub lockfile_version: u32,

    /// The tool version that generated this lockfile.
    pub generated_by: String,
}

/// A single resolved package in the lockfile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Package {
    /// Package name (including `@scope/` if scoped).
    pub name: String,

    /// Exact resolved version.
    pub version: String,

    /// Package source identifier.
    ///
    /// Formats:
    /// - `git+{index_url}` — from a git-based registry
    /// - `http+{api_url}` — from an HTTP API registry (future)
    /// - `workspace` — workspace member
    /// - `path+{absolute_path}` — path dependency
    pub source: String,

    /// SHA-512 integrity checksum (empty for workspace/path deps).
    pub checksum: String,

    /// Direct dependency specifications (e.g. `["lint-skill ^1.0"]`).
    #[serde(default)]
    pub dependencies: Vec<String>,
}

impl Lockfile {
    /// Create a new empty lockfile with the current version.
    pub const fn new(generated_by: String) -> Self {
        Self {
            metadata: Metadata { lockfile_version: LOCKFILE_VERSION, generated_by },
            packages: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_lockfile_has_correct_version() {
        let lf = Lockfile::new("aipm 0.10.0".to_string());
        assert_eq!(lf.metadata.lockfile_version, LOCKFILE_VERSION);
        assert_eq!(lf.metadata.generated_by, "aipm 0.10.0");
        assert!(lf.packages.is_empty());
    }

    #[test]
    fn round_trip_serialization_empty() {
        let lf = Lockfile::new("aipm 0.10.0".to_string());
        let toml_str = toml::to_string(&lf).unwrap();
        let parsed: Lockfile = toml::from_str(&toml_str).unwrap();
        assert_eq!(lf, parsed);
    }

    #[test]
    fn round_trip_serialization_with_packages() {
        let lf = Lockfile {
            metadata: Metadata { lockfile_version: 1, generated_by: "aipm 0.10.0".to_string() },
            packages: vec![
                Package {
                    name: "code-review".to_string(),
                    version: "1.2.0".to_string(),
                    source: "git+https://github.com/org/aipm-registry.git".to_string(),
                    checksum: "sha512-abc123".to_string(),
                    dependencies: vec!["lint-skill ^1.0".to_string()],
                },
                Package {
                    name: "lint-skill".to_string(),
                    version: "1.5.0".to_string(),
                    source: "git+https://github.com/org/aipm-registry.git".to_string(),
                    checksum: "sha512-789ghi".to_string(),
                    dependencies: vec![],
                },
            ],
        };

        let toml_str = toml::to_string(&lf).unwrap();
        let parsed: Lockfile = toml::from_str(&toml_str).unwrap();
        assert_eq!(lf, parsed);
    }

    #[test]
    fn deserialize_from_spec_format() {
        let input = r#"
[metadata]
lockfile_version = 1
generated_by = "aipm 0.10.0"

[[package]]
name = "code-review"
version = "1.2.0"
source = "git+https://github.com/org/aipm-registry.git"
checksum = "sha512-abc123def456"
dependencies = ["lint-skill ^1.0"]

[[package]]
name = "lint-skill"
version = "1.5.0"
source = "git+https://github.com/org/aipm-registry.git"
checksum = "sha512-789ghi012jkl"
dependencies = []

[[package]]
name = "core-hooks"
version = "0.3.0"
source = "workspace"
checksum = ""
dependencies = []
"#;

        let lf: Lockfile = toml::from_str(input).unwrap();
        assert_eq!(lf.metadata.lockfile_version, 1);
        assert_eq!(lf.packages.len(), 3);
        assert_eq!(lf.packages[0].name, "code-review");
        assert_eq!(lf.packages[0].dependencies.len(), 1);
        assert_eq!(lf.packages[2].source, "workspace");
        assert!(lf.packages[2].checksum.is_empty());
    }

    #[test]
    fn deserialize_missing_dependencies_defaults_empty() {
        let input = r#"
[metadata]
lockfile_version = 1
generated_by = "aipm 0.10.0"

[[package]]
name = "simple"
version = "1.0.0"
source = "workspace"
checksum = ""
"#;

        let lf: Lockfile = toml::from_str(input).unwrap();
        assert_eq!(lf.packages.len(), 1);
        assert!(lf.packages[0].dependencies.is_empty());
    }

    #[test]
    fn deserialize_no_packages_defaults_empty() {
        let input = r#"
[metadata]
lockfile_version = 1
generated_by = "aipm 0.10.0"
"#;

        let lf: Lockfile = toml::from_str(input).unwrap();
        assert!(lf.packages.is_empty());
    }

    #[test]
    fn package_source_types() {
        let pkg_git = Package {
            name: "a".to_string(),
            version: "1.0.0".to_string(),
            source: "git+https://github.com/org/registry.git".to_string(),
            checksum: "sha512-hash".to_string(),
            dependencies: vec![],
        };
        assert!(pkg_git.source.starts_with("git+"));

        let pkg_ws = Package {
            name: "b".to_string(),
            version: "1.0.0".to_string(),
            source: "workspace".to_string(),
            checksum: String::new(),
            dependencies: vec![],
        };
        assert_eq!(pkg_ws.source, "workspace");

        let pkg_path = Package {
            name: "c".to_string(),
            version: "1.0.0".to_string(),
            source: "path+/home/user/dev/plugin".to_string(),
            checksum: String::new(),
            dependencies: vec![],
        };
        assert!(pkg_path.source.starts_with("path+"));
    }
}
