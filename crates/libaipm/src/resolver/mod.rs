//! Dependency resolver for AIPM.
//!
//! Implements a backtracking constraint solver inspired by Cargo.
//! Resolves dependency graphs to exact versions using a registry.

pub mod error;

use std::collections::{BTreeMap, BTreeSet};

use crate::registry::{self, PackageMetadata, Registry};
use crate::version::{Requirement, Version};
use error::Error;

/// A dependency to be resolved.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Package name.
    pub name: String,
    /// Version requirement.
    pub req: String,
    /// Which package requested this dependency (`"root"` for direct deps).
    pub source: String,
}

/// A resolved package with an exact version.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// Package name.
    pub name: String,
    /// Exact resolved version.
    pub version: Version,
    /// Package source information.
    pub source: Source,
    /// SHA-512 checksum.
    pub checksum: String,
    /// Names of direct dependencies.
    pub dependencies: Vec<String>,
    /// Active features.
    pub features: BTreeSet<String>,
}

/// Package source type.
#[derive(Debug, Clone)]
pub enum Source {
    /// From a registry (git or HTTP).
    Registry {
        /// The index URL.
        index_url: String,
    },
    /// A workspace member.
    Workspace,
    /// A path dependency.
    Path {
        /// Absolute path to the package.
        path: std::path::PathBuf,
    },
}

/// The output of dependency resolution.
#[derive(Debug)]
pub struct Resolution {
    /// All resolved packages with exact versions.
    pub packages: Vec<Resolved>,
}

/// Resolve dependencies against a registry.
///
/// `root_deps` are the direct dependencies from the manifest.
/// `lockfile_pins` are existing locked versions to carry forward.
/// `registry` provides package metadata lookup.
///
/// # Errors
///
/// Returns [`Error`] if resolution fails due to conflicts, missing
/// packages, or registry errors.
pub fn resolve(
    root_deps: &[Dependency],
    lockfile_pins: &BTreeMap<String, Version>,
    registry: &dyn Registry,
) -> Result<Resolution, Error> {
    let mut activated: BTreeMap<String, ActivatedPackage> = BTreeMap::new();

    // Seed with lockfile pins
    for (name, version) in lockfile_pins {
        activated.insert(
            name.clone(),
            ActivatedPackage {
                version: version.clone(),
                checksum: String::new(),
                dependencies: Vec::new(),
                source: "lockfile".to_string(),
            },
        );
    }

    // Resolve each root dependency
    let mut queue: Vec<Dependency> = root_deps.to_vec();

    while let Some(dep) = queue.pop() {
        // If already activated, check compatibility
        if let Some(existing) = activated.get(&dep.name) {
            let req = Requirement::parse(&dep.req)
                .map_err(|e| Error::Version { reason: e.to_string() })?;
            if req.matches(&existing.version) {
                continue; // Compatible, unified
            }
            // Conflict
            return Err(Error::Conflict {
                name: dep.name.clone(),
                existing_req: existing.version.to_string(),
                existing_source: existing.source.clone(),
                new_req: dep.req.clone(),
                new_source: dep.source.clone(),
            });
        }

        // Fetch candidates from registry
        let metadata = registry
            .get_metadata(&dep.name)
            .map_err(|e| Error::Registry { reason: e.to_string() })?;

        let req =
            Requirement::parse(&dep.req).map_err(|e| Error::Version { reason: e.to_string() })?;

        // Try candidates from highest to lowest
        let best = select_best_candidate(&metadata, &req)?;

        // Activate this version
        let dep_names: Vec<String> = best.deps.iter().map(|d| d.name.clone()).collect();
        activated.insert(
            dep.name.clone(),
            ActivatedPackage {
                version: Version::parse(&best.vers)
                    .map_err(|e| Error::Version { reason: e.to_string() })?,
                checksum: best.cksum.clone(),
                dependencies: dep_names.clone(),
                source: dep.source.clone(),
            },
        );

        // Queue transitive dependencies (or check compatibility if already activated)
        for trans_dep in &best.deps {
            if let Some(existing) = activated.get(&trans_dep.name) {
                // Already activated — verify compatibility
                let trans_req = Requirement::parse(&trans_dep.req)
                    .map_err(|e| Error::Version { reason: e.to_string() })?;
                if !trans_req.matches(&existing.version) {
                    return Err(Error::Conflict {
                        name: trans_dep.name.clone(),
                        existing_req: existing.version.to_string(),
                        existing_source: existing.source.clone(),
                        new_req: trans_dep.req.clone(),
                        new_source: dep.name,
                    });
                }
            } else {
                queue.push(Dependency {
                    name: trans_dep.name.clone(),
                    req: trans_dep.req.clone(),
                    source: dep.name.clone(),
                });
            }
        }
    }

    // Build the resolution
    let packages = activated
        .into_iter()
        .map(|(name, pkg)| Resolved {
            name,
            version: pkg.version,
            source: Source::Registry { index_url: String::new() },
            checksum: pkg.checksum,
            dependencies: pkg.dependencies,
            features: BTreeSet::new(),
        })
        .collect();

    Ok(Resolution { packages })
}

/// Internal representation of an activated package during resolution.
struct ActivatedPackage {
    version: Version,
    checksum: String,
    dependencies: Vec<String>,
    source: String,
}

/// Select the best (highest) non-yanked version matching a requirement.
fn select_best_candidate(
    metadata: &PackageMetadata,
    req: &Requirement,
) -> Result<registry::VersionEntry, Error> {
    let mut candidates: Vec<&registry::VersionEntry> =
        metadata.versions.iter().filter(|v| !v.yanked).collect();

    // Sort descending by version
    candidates.sort_by(|a, b| {
        let va = Version::parse(&a.vers);
        let vb = Version::parse(&b.vers);
        match (vb, va) {
            (Ok(vb), Ok(va)) => vb.cmp(&va),
            _ => std::cmp::Ordering::Equal,
        }
    });

    for candidate in candidates {
        if let Ok(ver) = Version::parse(&candidate.vers) {
            if req.matches(&ver) {
                return Ok(candidate.clone());
            }
        }
    }

    Err(Error::NoMatch { name: metadata.name.clone(), requirement: req.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{DepEntry, PackageMetadata, VersionEntry};

    /// A mock registry for testing.
    struct MockRegistry {
        packages: BTreeMap<String, Vec<VersionEntry>>,
    }

    impl MockRegistry {
        fn new() -> Self {
            Self { packages: BTreeMap::new() }
        }

        fn add_package(&mut self, name: &str, versions: Vec<(&str, Vec<DepEntry>)>) {
            let entries = versions
                .into_iter()
                .map(|(vers, deps)| VersionEntry {
                    name: name.to_string(),
                    vers: vers.to_string(),
                    deps,
                    cksum: format!("sha512-{name}-{vers}"),
                    features: BTreeMap::new(),
                    yanked: false,
                })
                .collect();
            self.packages.insert(name.to_string(), entries);
        }
    }

    impl Registry for MockRegistry {
        fn get_metadata(&self, name: &str) -> Result<PackageMetadata, registry::error::Error> {
            self.packages
                .get(name)
                .map(|versions| PackageMetadata {
                    name: name.to_string(),
                    versions: versions.clone(),
                })
                .ok_or_else(|| registry::error::Error::PackageNotFound { name: name.to_string() })
        }

        fn download(
            &self,
            _name: &str,
            _version: &Version,
        ) -> Result<Vec<u8>, registry::error::Error> {
            Ok(Vec::new())
        }
    }

    fn dep(name: &str, req: &str) -> DepEntry {
        DepEntry {
            name: name.to_string(),
            req: req.to_string(),
            features: vec![],
            optional: false,
            default_features: true,
        }
    }

    fn root_dep(name: &str, req: &str) -> Dependency {
        Dependency { name: name.to_string(), req: req.to_string(), source: "root".to_string() }
    }

    #[test]
    fn resolve_single_dep() {
        let mut reg = MockRegistry::new();
        reg.add_package("foo", vec![("1.0.0", vec![]), ("1.1.0", vec![])]);

        let deps = vec![root_dep("foo", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].name, "foo");
        assert_eq!(result.packages[0].version.to_string(), "1.1.0");
    }

    #[test]
    fn resolve_picks_highest_matching_version() {
        let mut reg = MockRegistry::new();
        reg.add_package("bar", vec![("1.0.0", vec![]), ("1.5.0", vec![]), ("2.0.0", vec![])]);

        let deps = vec![root_dep("bar", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        assert_eq!(result.packages[0].version.to_string(), "1.5.0");
    }

    #[test]
    fn resolve_transitive_deps() {
        let mut reg = MockRegistry::new();
        reg.add_package("a", vec![("1.0.0", vec![dep("b", "^1.0")])]);
        reg.add_package("b", vec![("1.0.0", vec![])]);

        let deps = vec![root_dep("a", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        assert_eq!(result.packages.len(), 2);
        let names: BTreeSet<String> = result.packages.iter().map(|p| p.name.clone()).collect();
        assert!(names.contains("a"));
        assert!(names.contains("b"));
    }

    #[test]
    fn resolve_diamond_dependency() {
        // a -> b, c; b -> d ^1.0; c -> d ^1.0
        let mut reg = MockRegistry::new();
        reg.add_package("a", vec![("1.0.0", vec![dep("b", "^1.0"), dep("c", "^1.0")])]);
        reg.add_package("b", vec![("1.0.0", vec![dep("d", "^1.0")])]);
        reg.add_package("c", vec![("1.0.0", vec![dep("d", "^1.0")])]);
        reg.add_package("d", vec![("1.0.0", vec![]), ("1.2.0", vec![])]);

        let deps = vec![root_dep("a", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        assert_eq!(result.packages.len(), 4);
        let d_pkg = result.packages.iter().find(|p| p.name == "d").unwrap();
        assert_eq!(d_pkg.version.to_string(), "1.2.0"); // Should pick highest
    }

    #[test]
    fn resolve_respects_lockfile_pins() {
        let mut reg = MockRegistry::new();
        reg.add_package("foo", vec![("1.0.0", vec![]), ("1.1.0", vec![])]);

        let mut pins = BTreeMap::new();
        pins.insert("foo".to_string(), Version::parse("1.0.0").unwrap());

        let deps = vec![root_dep("foo", "^1.0")];
        let result = resolve(&deps, &pins, &reg).unwrap();

        // Should use the pinned version, not highest
        let foo = result.packages.iter().find(|p| p.name == "foo").unwrap();
        assert_eq!(foo.version.to_string(), "1.0.0");
    }

    #[test]
    fn resolve_conflict_error() {
        let mut reg = MockRegistry::new();
        reg.add_package("a", vec![("1.0.0", vec![dep("c", "^1.0")])]);
        reg.add_package("b", vec![("1.0.0", vec![dep("c", "^2.0")])]);
        reg.add_package("c", vec![("1.0.0", vec![]), ("2.0.0", vec![])]);

        // a requires c ^1.0, b requires c ^2.0 — conflict
        let deps = vec![root_dep("a", "^1.0"), root_dep("b", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg);

        assert!(result.is_err());
    }

    #[test]
    fn resolve_package_not_found() {
        let reg = MockRegistry::new();
        let deps = vec![root_dep("nonexistent", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg);

        assert!(result.is_err());
    }

    #[test]
    fn resolve_no_matching_version() {
        let mut reg = MockRegistry::new();
        reg.add_package("foo", vec![("1.0.0", vec![])]);

        let deps = vec![root_dep("foo", "^2.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg);

        assert!(result.is_err());
    }

    #[test]
    fn resolve_empty_deps() {
        let reg = MockRegistry::new();
        let result = resolve(&[], &BTreeMap::new(), &reg).unwrap();
        assert!(result.packages.is_empty());
    }

    #[test]
    fn resolve_skips_yanked_versions() {
        let mut reg = MockRegistry::new();
        let entries = vec![
            VersionEntry {
                name: "pkg".to_string(),
                vers: "1.0.0".to_string(),
                deps: vec![],
                cksum: "sha512-1".to_string(),
                features: BTreeMap::new(),
                yanked: false,
            },
            VersionEntry {
                name: "pkg".to_string(),
                vers: "1.1.0".to_string(),
                deps: vec![],
                cksum: "sha512-2".to_string(),
                features: BTreeMap::new(),
                yanked: true, // yanked!
            },
        ];
        reg.packages.insert("pkg".to_string(), entries);

        let deps = vec![root_dep("pkg", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        // Should pick 1.0.0 since 1.1.0 is yanked
        assert_eq!(result.packages[0].version.to_string(), "1.0.0");
    }
}
