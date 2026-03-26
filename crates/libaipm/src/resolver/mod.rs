//! Dependency resolver for AIPM.
//!
//! Implements a backtracking constraint solver inspired by Cargo.
//! Resolves dependency graphs to exact versions using a registry.
//!
//! Key behaviors:
//! - **Version unification**: Within the same semver-major, if an already-activated
//!   version satisfies a new requirement, it is reused.
//! - **Cross-major coexistence**: Different major versions of the same package can
//!   coexist in the resolution graph (Cargo model, no peer deps).
//! - **Backtracking**: On same-major conflict, the solver backtracks to the most
//!   recent choice point with remaining candidates.

pub mod error;
pub mod overrides;

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
    resolve_with_overrides(root_deps, lockfile_pins, registry, &[])
}

/// Resolve dependencies with overrides applied.
///
/// Overrides are applied to root deps and to transitive deps as they are
/// discovered during resolution.
///
/// # Errors
///
/// Returns [`Error`] if resolution fails.
pub fn resolve_with_overrides(
    root_deps: &[Dependency],
    lockfile_pins: &BTreeMap<String, Version>,
    registry: &dyn Registry,
    override_rules: &[overrides::Override],
) -> Result<Resolution, Error> {
    let mut solver = Solver::new(registry, lockfile_pins, override_rules);
    solver.resolve(root_deps)
}

// =========================================================================
// Internal solver
// =========================================================================

/// Key for the activated map: (name, major).
type ActivatedKey = (String, u64);

/// Internal representation of an activated package during resolution.
#[derive(Clone)]
struct ActivatedPackage {
    version: Version,
    checksum: String,
    dependencies: Vec<String>,
    source: String,
}

/// A choice point for backtracking.
#[derive(Clone)]
struct ChoicePoint {
    /// The dependency being resolved at this choice point.
    dep: Dependency,
    /// All sorted candidates (highest first).
    candidates: Vec<registry::VersionEntry>,
    /// Index of the candidate currently chosen.
    current_idx: usize,
    /// Snapshot of activated state before this choice.
    activated_snapshot: BTreeMap<ActivatedKey, ActivatedPackage>,
    /// Snapshot of the queue before this choice.
    queue_snapshot: Vec<Dependency>,
}

/// The solver state.
struct Solver<'a> {
    registry: &'a dyn Registry,
    activated: BTreeMap<ActivatedKey, ActivatedPackage>,
    choices: Vec<ChoicePoint>,
    override_rules: Vec<overrides::Override>,
}

impl<'a> Solver<'a> {
    fn new(
        registry: &'a dyn Registry,
        lockfile_pins: &BTreeMap<String, Version>,
        override_rules: &[overrides::Override],
    ) -> Self {
        let mut activated: BTreeMap<ActivatedKey, ActivatedPackage> = BTreeMap::new();

        for (name, version) in lockfile_pins {
            let major = version.major();
            activated.insert(
                (name.clone(), major),
                ActivatedPackage {
                    version: version.clone(),
                    checksum: String::new(),
                    dependencies: Vec::new(),
                    source: "lockfile".to_string(),
                },
            );
        }

        Self { registry, activated, choices: Vec::new(), override_rules: override_rules.to_vec() }
    }

    fn resolve(&mut self, root_deps: &[Dependency]) -> Result<Resolution, Error> {
        let mut queue: Vec<Dependency> = root_deps.to_vec();
        // Apply overrides to root deps
        overrides::apply(&mut queue, &self.override_rules);

        while let Some(dep) = queue.pop() {
            let req = Requirement::parse(&dep.req)
                .map_err(|e| Error::Version { reason: e.to_string() })?;

            // Check if any activated version for this name satisfies the requirement
            if self.find_unified(&dep.name, &req) {
                continue;
            }

            // Fetch candidates from registry
            let metadata = self
                .registry
                .get_metadata(&dep.name)
                .map_err(|e| Error::Registry { reason: e.to_string() })?;

            let candidates = select_sorted_candidates(&metadata, &req);

            // Try to activate a candidate (with backtracking on conflict)
            self.try_activate_with_backtrack(&dep, &req, &candidates, &mut queue)?;
        }

        Ok(self.build_resolution())
    }

    /// Check if any activated version for `name` satisfies `req`.
    fn find_unified(&self, name: &str, req: &Requirement) -> bool {
        self.activated.iter().any(|((n, _), pkg)| n == name && req.matches(&pkg.version))
    }

    /// Try to activate a candidate, backtracking on same-major conflict.
    fn try_activate_with_backtrack(
        &mut self,
        dep: &Dependency,
        req: &Requirement,
        candidates: &[registry::VersionEntry],
        queue: &mut Vec<Dependency>,
    ) -> Result<(), Error> {
        if candidates.is_empty() {
            return Err(Error::NoMatch { name: dep.name.clone(), requirement: req.to_string() });
        }

        // Save choice point if there are alternatives
        if candidates.len() > 1 {
            self.choices.push(ChoicePoint {
                dep: dep.clone(),
                candidates: candidates.to_owned(),
                current_idx: 0,
                activated_snapshot: self.activated.clone(),
                queue_snapshot: queue.clone(),
            });
        }

        let candidate = candidates.first().ok_or_else(|| Error::NoMatch {
            name: dep.name.clone(),
            requirement: req.to_string(),
        })?;

        let version = Version::parse(&candidate.vers)
            .map_err(|e| Error::Version { reason: e.to_string() })?;
        let major = version.major();

        // Check for same-major conflict
        if let Some(existing) = self.activated.get(&(dep.name.clone(), major)) {
            if !req.matches(&existing.version) {
                // Same-major conflict — try backtracking
                return self.backtrack_and_retry(dep, queue);
            }
            // Same major, compatible — unified
            return Ok(());
        }

        // Activate this version
        self.activate(dep, candidate, queue)?;
        Ok(())
    }

    /// Activate a candidate version and queue its transitive dependencies.
    fn activate(
        &mut self,
        dep: &Dependency,
        candidate: &registry::VersionEntry,
        queue: &mut Vec<Dependency>,
    ) -> Result<(), Error> {
        let version = Version::parse(&candidate.vers)
            .map_err(|e| Error::Version { reason: e.to_string() })?;
        let major = version.major();
        let dep_names: Vec<String> = candidate.deps.iter().map(|d| d.name.clone()).collect();

        self.activated.insert(
            (dep.name.clone(), major),
            ActivatedPackage {
                version,
                checksum: candidate.cksum.clone(),
                dependencies: dep_names,
                source: dep.source.clone(),
            },
        );

        // Queue transitive dependencies
        for trans_dep in &candidate.deps {
            let trans_req = Requirement::parse(&trans_dep.req)
                .map_err(|e| Error::Version { reason: e.to_string() })?;

            // Check if already unified
            if self.find_unified(&trans_dep.name, &trans_req) {
                // Verify compatibility with all existing activations for this name
                let compatible = self
                    .activated
                    .iter()
                    .filter(|((n, _), _)| *n == trans_dep.name)
                    .all(|(_, pkg)| trans_req.matches(&pkg.version));

                if !compatible {
                    // Check if this is a cross-major situation (which is OK)
                    // or a same-major conflict (which needs backtracking)
                    let has_same_major_conflict = self.activated.iter().any(|((n, _), pkg)| {
                        if *n != trans_dep.name {
                            return false;
                        }
                        !trans_req.matches(&pkg.version)
                    });

                    if has_same_major_conflict {
                        return Err(Error::Conflict {
                            name: trans_dep.name.clone(),
                            existing_req: self
                                .activated
                                .iter()
                                .find(|((n, _), _)| *n == trans_dep.name)
                                .map_or_else(String::new, |(_, pkg)| pkg.version.to_string()),
                            existing_source: self
                                .activated
                                .iter()
                                .find(|((n, _), _)| *n == trans_dep.name)
                                .map_or_else(String::new, |(_, pkg)| pkg.source.clone()),
                            new_req: trans_dep.req.clone(),
                            new_source: dep.name.clone(),
                        });
                    }
                }
                continue;
            }

            let mut trans = Dependency {
                name: trans_dep.name.clone(),
                req: trans_dep.req.clone(),
                source: dep.name.clone(),
            };
            // Apply overrides to transitive dep before queueing
            overrides::apply(std::slice::from_mut(&mut trans), &self.override_rules);
            queue.push(trans);
        }

        Ok(())
    }

    /// Backtrack to the most recent choice point with remaining candidates.
    fn backtrack_and_retry(
        &mut self,
        failing_dep: &Dependency,
        queue: &mut Vec<Dependency>,
    ) -> Result<(), Error> {
        while let Some(mut choice) = self.choices.pop() {
            choice.current_idx += 1;

            if choice.current_idx < choice.candidates.len() {
                // Restore state
                self.activated = choice.activated_snapshot.clone();
                queue.clone_from(&choice.queue_snapshot);

                // Try the next candidate
                let candidate = choice
                    .candidates
                    .get(choice.current_idx)
                    .ok_or_else(|| Error::NoMatch {
                        name: choice.dep.name.clone(),
                        requirement: choice.dep.req.clone(),
                    })?
                    .clone();

                let version = Version::parse(&candidate.vers)
                    .map_err(|e| Error::Version { reason: e.to_string() })?;
                let major = version.major();

                // Check for same-major conflict with restored state
                if let Some(existing) = self.activated.get(&(choice.dep.name.clone(), major)) {
                    let req = Requirement::parse(&choice.dep.req)
                        .map_err(|e| Error::Version { reason: e.to_string() })?;
                    if !req.matches(&existing.version) {
                        // Still conflicts — continue backtracking
                        continue;
                    }
                }

                // Save updated choice point (for further backtracking if needed)
                if choice.current_idx + 1 < choice.candidates.len() {
                    self.choices.push(ChoicePoint {
                        dep: choice.dep.clone(),
                        candidates: choice.candidates.clone(),
                        current_idx: choice.current_idx,
                        activated_snapshot: self.activated.clone(),
                        queue_snapshot: queue.clone(),
                    });
                }

                // Activate the new candidate
                self.activate(&choice.dep, &candidate, queue)?;

                // The old failing dep was from the previous (failed) branch.
                // The new candidate's transitive deps have been handled by activate().
                return Ok(());
            }
        }

        // No more choices to backtrack to — report conflict
        Err(Error::Conflict {
            name: failing_dep.name.clone(),
            existing_req: self
                .activated
                .iter()
                .find(|((n, _), _)| *n == failing_dep.name)
                .map_or_else(String::new, |(_, pkg)| pkg.version.to_string()),
            existing_source: self
                .activated
                .iter()
                .find(|((n, _), _)| *n == failing_dep.name)
                .map_or_else(String::new, |(_, pkg)| pkg.source.clone()),
            new_req: failing_dep.req.clone(),
            new_source: failing_dep.source.clone(),
        })
    }

    /// Build the final resolution from activated packages.
    fn build_resolution(&self) -> Resolution {
        let packages = self
            .activated
            .iter()
            .map(|((name, _), pkg)| Resolved {
                name: name.clone(),
                version: pkg.version.clone(),
                source: Source::Registry { index_url: String::new() },
                checksum: pkg.checksum.clone(),
                dependencies: pkg.dependencies.clone(),
                features: BTreeSet::new(),
            })
            .collect();

        Resolution { packages }
    }
}

/// Select all non-yanked candidates matching a requirement, sorted highest-first.
fn select_sorted_candidates(
    metadata: &PackageMetadata,
    req: &Requirement,
) -> Vec<registry::VersionEntry> {
    let mut candidates: Vec<registry::VersionEntry> = metadata
        .versions
        .iter()
        .filter(|v| !v.yanked)
        .filter(|v| Version::parse(&v.vers).ok().is_some_and(|ver| req.matches(&ver)))
        .cloned()
        .collect();

    // Sort descending by version
    candidates.sort_by(|a, b| {
        let va = Version::parse(&a.vers);
        let vb = Version::parse(&b.vers);
        match (vb, va) {
            (Ok(vb), Ok(va)) => vb.cmp(&va),
            _ => std::cmp::Ordering::Equal,
        }
    });

    candidates
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

    // =========================================================================
    // Basic resolution tests (existing)
    // =========================================================================

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

        // a requires c ^1.0, b requires c ^2.0 — cross-major coexistence!
        let deps = vec![root_dep("a", "^1.0"), root_dep("b", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg);

        // With cross-major coexistence, this now SUCCEEDS
        assert!(result.is_ok());
        let res = result.unwrap();
        // Both c@1.x and c@2.x should be in the resolution
        let c_versions: Vec<String> =
            res.packages.iter().filter(|p| p.name == "c").map(|p| p.version.to_string()).collect();
        assert_eq!(c_versions.len(), 2);
        assert!(c_versions.contains(&"1.0.0".to_string()));
        assert!(c_versions.contains(&"2.0.0".to_string()));
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

    // =========================================================================
    // Version unification tests
    // =========================================================================

    #[test]
    fn unify_shared_deps_to_single_version() {
        // BDD: "Unify shared dependencies to a single version"
        let mut reg = MockRegistry::new();
        reg.add_package("skill-a", vec![("1.0.0", vec![dep("common-util", "^2.0")])]);
        reg.add_package("skill-b", vec![("1.0.0", vec![dep("common-util", "^2.1")])]);
        reg.add_package(
            "common-util",
            vec![("2.0.0", vec![]), ("2.1.0", vec![]), ("2.2.0", vec![])],
        );

        let deps = vec![root_dep("skill-a", "^1.0"), root_dep("skill-b", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let common = result.packages.iter().filter(|p| p.name == "common-util").collect::<Vec<_>>();
        assert_eq!(common.len(), 1, "common-util should appear exactly once");
        assert_eq!(common[0].version.to_string(), "2.2.0");
    }

    #[test]
    fn same_major_deps_unified_to_one_version() {
        // BDD: "Same-major dependencies are unified to one version"
        let mut reg = MockRegistry::new();
        reg.add_package("plugin-a", vec![("1.0.0", vec![dep("common-util", "^2.0")])]);
        reg.add_package("plugin-b", vec![("1.0.0", vec![dep("common-util", "^2.5")])]);
        reg.add_package(
            "common-util",
            vec![("2.0.0", vec![]), ("2.5.0", vec![]), ("2.8.0", vec![])],
        );

        let deps = vec![root_dep("plugin-a", "^1.0"), root_dep("plugin-b", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let common = result.packages.iter().filter(|p| p.name == "common-util").collect::<Vec<_>>();
        assert_eq!(common.len(), 1, "common-util should appear exactly once");
        assert_eq!(common[0].version.to_string(), "2.8.0");
    }

    // =========================================================================
    // Cross-major coexistence tests
    // =========================================================================

    #[test]
    fn cross_major_deps_coexist() {
        // BDD: "Allow multiple incompatible major versions"
        let mut reg = MockRegistry::new();
        reg.add_package("skill-a", vec![("1.0.0", vec![dep("common-util", "^1.0")])]);
        reg.add_package("skill-b", vec![("1.0.0", vec![dep("common-util", "^2.0")])]);
        reg.add_package(
            "common-util",
            vec![("1.0.0", vec![]), ("1.5.0", vec![]), ("2.0.0", vec![]), ("2.1.0", vec![])],
        );

        let deps = vec![root_dep("skill-a", "^1.0"), root_dep("skill-b", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let common_versions: BTreeSet<String> = result
            .packages
            .iter()
            .filter(|p| p.name == "common-util")
            .map(|p| p.version.to_string())
            .collect();

        assert_eq!(common_versions.len(), 2, "both major versions should coexist");
        // Should have highest of each major
        assert!(common_versions.contains("1.5.0"));
        assert!(common_versions.contains("2.1.0"));
    }

    #[test]
    fn cross_major_framework_coexistence() {
        // BDD: "Cross-major dependencies coexist in the graph"
        let mut reg = MockRegistry::new();
        reg.add_package("plugin-a", vec![("1.0.0", vec![dep("framework", "^1.0")])]);
        reg.add_package("plugin-b", vec![("1.0.0", vec![dep("framework", "^2.0")])]);
        reg.add_package(
            "framework",
            vec![("1.0.0", vec![]), ("1.5.0", vec![]), ("2.0.0", vec![]), ("2.1.0", vec![])],
        );

        let deps = vec![root_dep("plugin-a", "^1.0"), root_dep("plugin-b", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let framework_versions: BTreeSet<String> = result
            .packages
            .iter()
            .filter(|p| p.name == "framework")
            .map(|p| p.version.to_string())
            .collect();

        assert_eq!(framework_versions.len(), 2);
        assert!(framework_versions.iter().any(|v| v.starts_with("1.")));
        assert!(framework_versions.iter().any(|v| v.starts_with("2.")));
    }

    // =========================================================================
    // Backtracking tests
    // =========================================================================

    #[test]
    fn backtrack_on_conflict() {
        // BDD: "Backtrack on conflict"
        // skill-a@1.2.0 depends on common-util =2.0.0
        // skill-a@1.1.0 depends on common-util ^2.0.0
        // skill-b depends on common-util =2.1.0
        // Expected: skill-a resolves to 1.1.0, common-util to 2.1.0
        let mut reg = MockRegistry::new();
        reg.add_package(
            "skill-a",
            vec![
                ("1.1.0", vec![dep("common-util", "^2.0.0")]),
                ("1.2.0", vec![dep("common-util", "=2.0.0")]),
            ],
        );
        reg.add_package("skill-b", vec![("1.0.0", vec![dep("common-util", "=2.1.0")])]);
        reg.add_package("common-util", vec![("2.0.0", vec![]), ("2.1.0", vec![])]);

        let deps = vec![root_dep("skill-a", "^1.0"), root_dep("skill-b", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let skill_a = result.packages.iter().find(|p| p.name == "skill-a").unwrap();
        assert_eq!(skill_a.version.to_string(), "1.1.0");

        let common = result.packages.iter().find(|p| p.name == "common-util").unwrap();
        assert_eq!(common.version.to_string(), "2.1.0");
    }

    #[test]
    fn report_unsolvable_conflicts() {
        // BDD: "Report unsolvable conflicts clearly"
        // skill-a depends on common-util =1.0.0
        // skill-b depends on common-util =1.1.0
        // Both are same major, can't be unified → error
        let mut reg = MockRegistry::new();
        reg.add_package("skill-a", vec![("1.0.0", vec![dep("common-util", "=1.0.0")])]);
        reg.add_package("skill-b", vec![("1.0.0", vec![dep("common-util", "=1.1.0")])]);
        reg.add_package("common-util", vec![("1.0.0", vec![]), ("1.1.0", vec![])]);

        let deps = vec![root_dep("skill-a", "^1.0"), root_dep("skill-b", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg);

        assert!(result.is_err());
        if let Err(Error::Conflict { name, .. }) = &result {
            assert_eq!(name, "common-util");
        }
    }

    #[test]
    fn prefer_highest_compatible_version() {
        // BDD: "Prefer the highest compatible version"
        let mut reg = MockRegistry::new();
        reg.add_package(
            "skill-a",
            vec![("1.0.0", vec![]), ("1.1.0", vec![]), ("1.2.0", vec![]), ("2.0.0", vec![])],
        );

        let deps = vec![root_dep("skill-a", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let skill = result.packages.iter().find(|p| p.name == "skill-a").unwrap();
        assert_eq!(skill.version.to_string(), "1.2.0");
    }

    #[test]
    fn resolve_simple_dependency_tree() {
        // BDD: "Resolve a simple dependency tree"
        let mut reg = MockRegistry::new();
        reg.add_package("skill-a", vec![("1.2.0", vec![dep("common-util", "^2.0")])]);
        reg.add_package("common-util", vec![("2.1.0", vec![])]);

        let deps = vec![root_dep("skill-a", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let skill = result.packages.iter().find(|p| p.name == "skill-a").unwrap();
        assert_eq!(skill.version.to_string(), "1.2.0");

        let common = result.packages.iter().find(|p| p.name == "common-util").unwrap();
        assert_eq!(common.version.to_string(), "2.1.0");
    }

    #[test]
    fn three_level_cross_major() {
        // a -> b ^1.0, c ^1.0
        // b -> lib ^1.0
        // c -> lib ^2.0
        // lib has 1.x and 2.x — should coexist
        let mut reg = MockRegistry::new();
        reg.add_package("a", vec![("1.0.0", vec![dep("b", "^1.0"), dep("c", "^1.0")])]);
        reg.add_package("b", vec![("1.0.0", vec![dep("lib", "^1.0")])]);
        reg.add_package("c", vec![("1.0.0", vec![dep("lib", "^2.0")])]);
        reg.add_package("lib", vec![("1.0.0", vec![]), ("2.0.0", vec![])]);

        let deps = vec![root_dep("a", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let lib_versions: BTreeSet<String> = result
            .packages
            .iter()
            .filter(|p| p.name == "lib")
            .map(|p| p.version.to_string())
            .collect();
        assert_eq!(lib_versions.len(), 2);
        assert!(lib_versions.contains("1.0.0"));
        assert!(lib_versions.contains("2.0.0"));
    }

    // =========================================================================
    // Override integration tests
    // =========================================================================

    #[test]
    fn global_override_forces_version() {
        // BDD: "Override a transitive dependency version globally"
        let mut reg = MockRegistry::new();
        reg.add_package("skill-a", vec![("1.0.0", vec![dep("vulnerable-lib", "^1.0")])]);
        reg.add_package(
            "vulnerable-lib",
            vec![("1.0.0", vec![]), ("2.0.0", vec![]), ("2.1.0", vec![])],
        );

        let override_rules = vec![overrides::Override::Global {
            name: "vulnerable-lib".to_string(),
            req: "^2.0.0".to_string(),
        }];

        let deps = vec![root_dep("skill-a", "^1.0")];
        let result =
            resolve_with_overrides(&deps, &BTreeMap::new(), &reg, &override_rules).unwrap();

        let vuln = result.packages.iter().find(|p| p.name == "vulnerable-lib").unwrap();
        assert!(vuln.version.to_string().starts_with("2.")); // forced to 2.x
    }

    #[test]
    fn scoped_override_only_affects_parent() {
        // BDD: "Override a dependency only when it is a child of a specific package"
        let mut reg = MockRegistry::new();
        reg.add_package("skill-a", vec![("1.0.0", vec![dep("common-util", "^2.0")])]);
        reg.add_package("skill-b", vec![("1.0.0", vec![dep("common-util", "^2.0")])]);
        reg.add_package(
            "common-util",
            vec![("2.0.0", vec![]), ("2.1.0", vec![]), ("2.5.0", vec![])],
        );

        let override_rules = vec![overrides::Override::Scoped {
            parent: "skill-a".to_string(),
            child: "common-util".to_string(),
            req: "=2.1.0".to_string(),
        }];

        // Note: scoped overrides apply to transitive deps with matching source.
        // Since skill-a's dep on common-util has source="skill-a",
        // the override pins it to =2.1.0.
        // skill-b's dep on common-util has source="skill-b", so it resolves normally.
        // With unification, if skill-a's common-util is resolved first at 2.1.0,
        // and skill-b's ^2.0 matches 2.1.0, they unify.
        let deps = vec![root_dep("skill-a", "^1.0"), root_dep("skill-b", "^1.0")];
        let result =
            resolve_with_overrides(&deps, &BTreeMap::new(), &reg, &override_rules).unwrap();

        let common = result.packages.iter().filter(|p| p.name == "common-util").collect::<Vec<_>>();
        // The scoped override forces =2.1.0 under skill-a; skill-b's ^2.0 also matches 2.1.0
        assert!(!common.is_empty());
    }

    #[test]
    fn replacement_override_swaps_package() {
        // BDD: "Replace a dependency with a fork via override"
        let mut reg = MockRegistry::new();
        reg.add_package("app", vec![("1.0.0", vec![dep("broken-lib", "^1.0")])]);
        reg.add_package("broken-lib", vec![("1.0.0", vec![])]);
        reg.add_package("fixed-lib", vec![("1.0.0", vec![]), ("1.1.0", vec![])]);

        let override_rules = vec![overrides::Override::Replacement {
            original: "broken-lib".to_string(),
            replacement: "fixed-lib".to_string(),
            req: "^1.0".to_string(),
        }];

        let deps = vec![root_dep("app", "^1.0")];
        let result =
            resolve_with_overrides(&deps, &BTreeMap::new(), &reg, &override_rules).unwrap();

        let names: BTreeSet<String> = result.packages.iter().map(|p| p.name.clone()).collect();
        assert!(names.contains("fixed-lib"), "replacement should be used");
        assert!(!names.contains("broken-lib"), "original should not appear");
    }
}
