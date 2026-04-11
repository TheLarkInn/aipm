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
    /// Features requested by the consumer.
    pub features: Vec<String>,
    /// Whether default features are enabled (default: `true`).
    pub default_features: bool,
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Active features for this package.
    features: BTreeSet<String>,
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
                    features: BTreeSet::new(),
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

        // Compute active features
        let active_features = compute_active_features(dep, &candidate.features);

        // Determine which optional deps are activated by features
        let activated_optional_deps = collect_feature_deps(&active_features, &candidate.features);

        // Non-optional deps + feature-activated optional deps
        let effective_deps: Vec<&registry::DepEntry> = candidate
            .deps
            .iter()
            .filter(|d| !d.optional || activated_optional_deps.contains(&d.name))
            .collect();

        let dep_names: Vec<String> = effective_deps.iter().map(|d| d.name.clone()).collect();

        self.activated.insert(
            (dep.name.clone(), major),
            ActivatedPackage {
                version,
                checksum: candidate.cksum.clone(),
                dependencies: dep_names,
                source: dep.source.clone(),
                features: active_features,
            },
        );

        // Queue transitive dependencies (only effective ones)
        for trans_dep in &effective_deps {
            self.queue_transitive_dep(dep, trans_dep, queue)?;
        }

        Ok(())
    }

    /// Queue a single transitive dependency, checking for unification and conflicts.
    fn queue_transitive_dep(
        &self,
        parent_dep: &Dependency,
        trans_dep: &registry::DepEntry,
        queue: &mut Vec<Dependency>,
    ) -> Result<(), Error> {
        let trans_req = Requirement::parse(&trans_dep.req)
            .map_err(|e| Error::Version { reason: e.to_string() })?;

        // Check if already unified
        if self.find_unified(&trans_dep.name, &trans_req) {
            // Verify compatibility with all existing activations for this name
            let has_same_major_conflict = self.activated.iter().any(|((n, _), pkg)| {
                if *n != trans_dep.name {
                    return false;
                }
                !trans_req.matches(&pkg.version)
            });

            if has_same_major_conflict {
                return Err(Error::Conflict(Box::new(error::ConflictDetail {
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
                    new_source: parent_dep.name.clone(),
                    existing_chain: vec![],
                    new_chain: vec![],
                })));
            }

            return Ok(());
        }

        let mut trans = Dependency {
            name: trans_dep.name.clone(),
            req: trans_dep.req.clone(),
            source: parent_dep.name.clone(),
            features: trans_dep.features.clone(),
            default_features: trans_dep.default_features,
        };
        overrides::apply(std::slice::from_mut(&mut trans), &self.override_rules);
        queue.push(trans);
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
        Err(Error::Conflict(Box::new(error::ConflictDetail {
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
            existing_chain: vec![],
            new_chain: vec![],
        })))
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
                features: pkg.features.clone(),
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

/// Compute the active features for a package based on the dependency request.
///
/// If `default_features` is true, the `default` feature (and its transitive features)
/// are included. Requested features are always included.
fn compute_active_features(
    dep: &Dependency,
    feature_defs: &BTreeMap<String, Vec<String>>,
) -> BTreeSet<String> {
    let mut active = BTreeSet::new();

    // Add default features if requested
    if dep.default_features && feature_defs.contains_key("default") {
        activate_feature_recursive("default", feature_defs, &mut active);
    }

    // Add explicitly requested features
    for f in &dep.features {
        activate_feature_recursive(f, feature_defs, &mut active);
    }

    active
}

/// Recursively activate a feature and all its transitive feature dependencies.
fn activate_feature_recursive(
    feature: &str,
    feature_defs: &BTreeMap<String, Vec<String>>,
    active: &mut BTreeSet<String>,
) {
    if !active.insert(feature.to_string()) {
        return; // Already active, avoid infinite recursion
    }

    if let Some(sub_features) = feature_defs.get(feature) {
        for sub in sub_features {
            // `dep:xxx` references are not feature names — they activate optional deps
            if !sub.starts_with("dep:") {
                activate_feature_recursive(sub, feature_defs, active);
            }
        }
    }
}

/// Collect optional dependency names that are activated by the active feature set.
///
/// Scans feature definitions for `dep:xxx` entries where the feature is active.
fn collect_feature_deps(
    active_features: &BTreeSet<String>,
    feature_defs: &BTreeMap<String, Vec<String>>,
) -> BTreeSet<String> {
    let mut activated_deps = BTreeSet::new();

    for feature in active_features {
        if let Some(deps) = feature_defs.get(feature.as_str()) {
            for dep_ref in deps {
                if let Some(dep_name) = dep_ref.strip_prefix("dep:") {
                    activated_deps.insert(dep_name.to_string());
                }
            }
        }
    }

    activated_deps
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
        Dependency {
            name: name.to_string(),
            req: req.to_string(),
            source: "root".to_string(),
            features: vec![],
            default_features: true,
        }
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
    fn resolve_invalid_requirement_returns_version_error() {
        // Covers the `Requirement::parse(...)?` error branch (line 188):
        // when a root dependency carries an unparseable semver requirement string,
        // the resolver must return `Error::Version` without panicking.
        let reg = MockRegistry::new();
        let deps = vec![root_dep("foo", "not-valid-semver!!!")];
        let result = resolve(&deps, &BTreeMap::new(), &reg);
        assert!(result.is_err());
        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err_msg.contains("version error"), "expected version error, got: {err_msg}");
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
        if let Err(Error::Conflict(detail)) = &result {
            assert_eq!(detail.name, "common-util");
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

    // =========================================================================
    // Feature system tests
    // =========================================================================

    fn root_dep_with_features(name: &str, req: &str, features: Vec<&str>) -> Dependency {
        Dependency {
            name: name.to_string(),
            req: req.to_string(),
            source: "root".to_string(),
            features: features.into_iter().map(String::from).collect(),
            default_features: true,
        }
    }

    fn root_dep_no_defaults(name: &str, req: &str) -> Dependency {
        Dependency {
            name: name.to_string(),
            req: req.to_string(),
            source: "root".to_string(),
            features: vec![],
            default_features: false,
        }
    }

    #[test]
    fn default_features_enabled() {
        // BDD: "Declare default features"
        let mut reg = MockRegistry::new();
        let mut features = BTreeMap::new();
        features.insert("default".to_string(), vec!["json-output".to_string()]);
        features.insert("json-output".to_string(), vec![]);
        features.insert("xml-output".to_string(), vec![]);

        reg.packages.insert(
            "my-plugin".to_string(),
            vec![VersionEntry {
                name: "my-plugin".to_string(),
                vers: "1.0.0".to_string(),
                deps: vec![],
                cksum: "sha512-test".to_string(),
                features,
                yanked: false,
            }],
        );

        let deps = vec![root_dep("my-plugin", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let pkg = result.packages.iter().find(|p| p.name == "my-plugin").unwrap();
        assert!(pkg.features.contains("json-output"), "default feature should be enabled");
        assert!(!pkg.features.contains("xml-output"), "non-default feature should not be enabled");
    }

    #[test]
    fn opt_out_of_default_features() {
        // BDD: "Opt out of default features"
        let mut reg = MockRegistry::new();
        let mut features = BTreeMap::new();
        features.insert("default".to_string(), vec!["json-output".to_string()]);
        features.insert("json-output".to_string(), vec![]);

        reg.packages.insert(
            "my-plugin".to_string(),
            vec![VersionEntry {
                name: "my-plugin".to_string(),
                vers: "1.0.0".to_string(),
                deps: vec![],
                cksum: "sha512-test".to_string(),
                features,
                yanked: false,
            }],
        );

        let deps = vec![root_dep_no_defaults("my-plugin", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let pkg = result.packages.iter().find(|p| p.name == "my-plugin").unwrap();
        assert!(
            !pkg.features.contains("json-output"),
            "default feature should not be enabled when opted out"
        );
    }

    #[test]
    fn enable_specific_feature() {
        // BDD: "Enable a specific optional feature"
        let mut reg = MockRegistry::new();
        let mut features = BTreeMap::new();
        features.insert("default".to_string(), vec!["json-output".to_string()]);
        features.insert("json-output".to_string(), vec![]);
        features.insert("xml-output".to_string(), vec![]);

        reg.packages.insert(
            "my-plugin".to_string(),
            vec![VersionEntry {
                name: "my-plugin".to_string(),
                vers: "1.0.0".to_string(),
                deps: vec![],
                cksum: "sha512-test".to_string(),
                features,
                yanked: false,
            }],
        );

        let deps = vec![root_dep_with_features("my-plugin", "^1.0", vec!["xml-output"])];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let pkg = result.packages.iter().find(|p| p.name == "my-plugin").unwrap();
        assert!(pkg.features.contains("xml-output"), "requested feature should be enabled");
        assert!(pkg.features.contains("json-output"), "default feature should also be enabled");
    }

    #[test]
    fn optional_dep_not_included_without_feature() {
        // BDD: "Optional dependency activated by feature"
        let mut reg = MockRegistry::new();
        let mut features = BTreeMap::new();
        features.insert("deep-analysis".to_string(), vec!["dep:heavy-analyzer".to_string()]);

        reg.packages.insert(
            "my-plugin".to_string(),
            vec![VersionEntry {
                name: "my-plugin".to_string(),
                vers: "1.0.0".to_string(),
                deps: vec![DepEntry {
                    name: "heavy-analyzer".to_string(),
                    req: "^1.0".to_string(),
                    features: vec![],
                    optional: true,
                    default_features: true,
                }],
                cksum: "sha512-test".to_string(),
                features,
                yanked: false,
            }],
        );
        reg.add_package("heavy-analyzer", vec![("1.0.0", vec![])]);

        // Install without deep-analysis feature
        let deps = vec![root_dep("my-plugin", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let names: BTreeSet<String> = result.packages.iter().map(|p| p.name.clone()).collect();
        assert!(!names.contains("heavy-analyzer"), "optional dep should not be included");
    }

    #[test]
    fn optional_dep_included_with_feature() {
        // Feature-activated optional dep
        let mut reg = MockRegistry::new();
        let mut features = BTreeMap::new();
        features.insert("deep-analysis".to_string(), vec!["dep:heavy-analyzer".to_string()]);

        reg.packages.insert(
            "my-plugin".to_string(),
            vec![VersionEntry {
                name: "my-plugin".to_string(),
                vers: "1.0.0".to_string(),
                deps: vec![DepEntry {
                    name: "heavy-analyzer".to_string(),
                    req: "^1.0".to_string(),
                    features: vec![],
                    optional: true,
                    default_features: true,
                }],
                cksum: "sha512-test".to_string(),
                features,
                yanked: false,
            }],
        );
        reg.add_package("heavy-analyzer", vec![("1.0.0", vec![])]);

        // Install WITH deep-analysis feature
        let deps = vec![root_dep_with_features("my-plugin", "^1.0", vec!["deep-analysis"])];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let names: BTreeSet<String> = result.packages.iter().map(|p| p.name.clone()).collect();
        assert!(names.contains("heavy-analyzer"), "optional dep should be included with feature");
    }

    #[test]
    fn feature_computation_basics() {
        // Test the compute_active_features function directly
        let mut feature_defs = BTreeMap::new();
        feature_defs.insert("default".to_string(), vec!["json".to_string()]);
        feature_defs.insert("json".to_string(), vec![]);
        feature_defs.insert("xml".to_string(), vec![]);
        feature_defs.insert("all".to_string(), vec!["json".to_string(), "xml".to_string()]);

        // Default features
        let dep = root_dep("pkg", "^1.0");
        let active = compute_active_features(&dep, &feature_defs);
        assert!(active.contains("default"));
        assert!(active.contains("json"));
        assert!(!active.contains("xml"));

        // Explicit features
        let dep = root_dep_with_features("pkg", "^1.0", vec!["all"]);
        let active = compute_active_features(&dep, &feature_defs);
        assert!(active.contains("all"));
        assert!(active.contains("json"));
        assert!(active.contains("xml"));
    }

    #[test]
    fn collect_dep_features() {
        let mut feature_defs = BTreeMap::new();
        feature_defs.insert(
            "deep-analysis".to_string(),
            vec!["dep:heavy-analyzer".to_string(), "dep:extra-lib".to_string()],
        );
        feature_defs.insert("json".to_string(), vec![]);

        let mut active = BTreeSet::new();
        active.insert("deep-analysis".to_string());
        active.insert("json".to_string());

        let dep_names = collect_feature_deps(&active, &feature_defs);
        assert!(dep_names.contains("heavy-analyzer"));
        assert!(dep_names.contains("extra-lib"));
        assert_eq!(dep_names.len(), 2);
    }

    // =========================================================================
    // Additional branch coverage tests
    // =========================================================================

    #[test]
    fn activate_feature_recursive_already_active_no_infinite_loop() {
        // Feature that includes itself transitively (cycle) — should not loop infinitely
        let mut feature_defs = BTreeMap::new();
        feature_defs.insert("a".to_string(), vec!["b".to_string()]);
        feature_defs.insert("b".to_string(), vec!["a".to_string()]); // cycle

        let mut active = BTreeSet::new();
        activate_feature_recursive("a", &feature_defs, &mut active);

        assert!(active.contains("a"));
        assert!(active.contains("b"));
        // No infinite loop means the test completes
    }

    #[test]
    fn activate_feature_recursive_with_dep_prefix_skipped() {
        // Sub-features starting with "dep:" should not be recursed into as feature names
        let mut feature_defs = BTreeMap::new();
        feature_defs.insert("my-feat".to_string(), vec!["dep:optional-lib".to_string()]);

        let mut active = BTreeSet::new();
        activate_feature_recursive("my-feat", &feature_defs, &mut active);

        assert!(active.contains("my-feat"));
        // "dep:optional-lib" should not appear as a feature name
        assert!(!active.contains("dep:optional-lib"));
    }

    #[test]
    fn collect_feature_deps_active_feature_not_in_defs() {
        // Active feature that has no entry in feature_defs — should be skipped gracefully
        let feature_defs: BTreeMap<String, Vec<String>> = BTreeMap::new();

        let mut active = BTreeSet::new();
        active.insert("unknown-feature".to_string());

        let dep_names = collect_feature_deps(&active, &feature_defs);
        assert!(dep_names.is_empty());
    }

    #[test]
    fn select_sorted_candidates_excludes_yanked() {
        let mut reg = MockRegistry::new();
        let entries = vec![
            VersionEntry {
                name: "pkg".to_string(),
                vers: "1.0.0".to_string(),
                deps: vec![],
                cksum: "sha512-1".to_string(),
                features: BTreeMap::new(),
                yanked: true, // yanked — should be excluded
            },
            VersionEntry {
                name: "pkg".to_string(),
                vers: "1.1.0".to_string(),
                deps: vec![],
                cksum: "sha512-2".to_string(),
                features: BTreeMap::new(),
                yanked: false,
            },
        ];
        reg.packages.insert("pkg".to_string(), entries);

        let deps = vec![root_dep("pkg", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();
        assert_eq!(result.packages[0].version.to_string(), "1.1.0");
    }

    #[test]
    fn transitive_conflict_same_major_reported() {
        // Two packages that both depend on the same package with incompatible same-major reqs
        // This tests the queue_transitive_dep same-major conflict branch
        let mut reg = MockRegistry::new();
        reg.add_package("a", vec![("1.0.0", vec![dep("shared", "=1.0.0")])]);
        reg.add_package("b", vec![("1.0.0", vec![dep("shared", "=1.1.0")])]);
        reg.add_package("shared", vec![("1.0.0", vec![]), ("1.1.0", vec![])]);

        // Both a and b need shared at incompatible same-major versions
        let deps = vec![root_dep("a", "^1.0"), root_dep("b", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_with_lockfile_pins_unified() {
        // When a lockfile pin already satisfies the requirement, find_unified returns true
        let mut reg = MockRegistry::new();
        reg.add_package("foo", vec![("1.0.0", vec![]), ("1.1.0", vec![])]);

        let mut pins = BTreeMap::new();
        pins.insert("foo".to_string(), Version::parse("1.0.0").unwrap());

        // ^1.0 matches pinned 1.0.0 — should unify without querying registry for new version
        let deps = vec![root_dep("foo", "^1.0")];
        let result = resolve(&deps, &pins, &reg).unwrap();

        let foo = result.packages.iter().find(|p| p.name == "foo").unwrap();
        assert_eq!(foo.version.to_string(), "1.0.0");
    }

    #[test]
    fn compute_active_features_no_default_feature_defined() {
        // dep.default_features=true but "default" key missing from feature_defs — should not crash
        let feature_defs: BTreeMap<String, Vec<String>> = BTreeMap::new(); // no "default" key

        let dep = root_dep("pkg", "^1.0");
        let active = compute_active_features(&dep, &feature_defs);
        // No "default" in defs, so nothing should be activated
        assert!(active.is_empty());
    }

    #[test]
    fn resolve_with_unknown_feature() {
        // A dependency requests feature "extra" that does not exist in the package's
        // feature definitions. This exercises the `None` branch at line 518 where
        // `feature_defs.get(feature)` returns `None`.
        let mut reg = MockRegistry::new();

        // Package has a "json" feature defined but NOT "extra"
        let mut features = BTreeMap::new();
        features.insert("json".to_string(), vec![]);

        reg.packages.insert(
            "my-plugin".to_string(),
            vec![VersionEntry {
                name: "my-plugin".to_string(),
                vers: "1.0.0".to_string(),
                deps: vec![],
                cksum: "sha512-test".to_string(),
                features,
                yanked: false,
            }],
        );

        // Request "extra" which is not in the feature defs
        let deps = vec![root_dep_with_features("my-plugin", "^1.0", vec!["extra"])];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let pkg = result.packages.iter().find(|p| p.name == "my-plugin").unwrap();
        // "extra" is inserted into active set even though it has no sub-features
        assert!(pkg.features.contains("extra"));
    }

    #[test]
    fn resolve_unifies_same_major_from_two_parents() {
        // Two root deps both depend on the same transitive dep at the same major.
        // The second encounter should hit the "already activated" unification path
        // inside `try_activate_with_backtrack` (line 248-254).
        //
        // We set up: root -> x ^1.0, root -> y ^1.0
        //   x@1.0.0 -> shared ^2.0
        //   y@1.0.0 -> shared ^2.1
        //   shared has 2.0.0, 2.1.0, 2.2.0
        //
        // Resolution order: x is resolved first, activates shared@2.2.0 (highest ^2.0).
        // Then y is resolved, its transitive dep shared ^2.1 goes to find_unified →
        // finds 2.2.0 matches ^2.1 → unified. This tests the queue_transitive_dep path.
        let mut reg = MockRegistry::new();
        reg.add_package("x", vec![("1.0.0", vec![dep("shared", "^2.0")])]);
        reg.add_package("y", vec![("1.0.0", vec![dep("shared", "^2.1")])]);
        reg.add_package("shared", vec![("2.0.0", vec![]), ("2.1.0", vec![]), ("2.2.0", vec![])]);

        let deps = vec![root_dep("x", "^1.0"), root_dep("y", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        // shared should appear exactly once, at the highest compatible version
        let shared: Vec<_> = result.packages.iter().filter(|p| p.name == "shared").collect();
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0].version.to_string(), "2.2.0");
    }

    #[test]
    fn transitive_dep_conflict_when_both_major_versions_already_activated() {
        // Covers the `has_same_major_conflict` True branch in `queue_transitive_dep`.
        //
        // Setup:
        //   root -> a@^1.0, b@^1.0
        //   b@1.0 -> c@^2.0
        //   a@1.0 -> [d@^1.0, c@^1.0]  (d first so c is pushed last → popped first, LIFO)
        //   d@1.0 -> c@^1.0
        //   c: 1.0.0, 2.0.0
        //
        // Resolution order (LIFO queue):
        //   1. activate b@1.0 → enqueue c@^2.0
        //   2. activate c@2.0 (from queue)
        //   3. activate a@1.0 → enqueue d@^1.0 then c@^1.0 (c is last pushed)
        //   4. activate c@1.0 (c@^1.0 popped first, LIFO)
        //   5. activate d@1.0 → `queue_transitive_dep` for c@^1.0:
        //      - find_unified(c, ^1.0) = true  (c@1.0 is activated)
        //      - c@2.0 is also activated and ^1.0 does NOT match 2.0
        //      → has_same_major_conflict = true → returns Err(Conflict)
        let mut reg = MockRegistry::new();
        reg.add_package("b", vec![("1.0.0", vec![dep("c", "^2.0")])]);
        reg.add_package("a", vec![("1.0.0", vec![dep("d", "^1.0"), dep("c", "^1.0")])]);
        reg.add_package("d", vec![("1.0.0", vec![dep("c", "^1.0")])]);
        reg.add_package("c", vec![("1.0.0", vec![]), ("2.0.0", vec![])]);

        let deps = vec![root_dep("a", "^1.0"), root_dep("b", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg);

        assert!(result.is_err());
        assert!(
            matches!(result, Err(Error::Conflict(_))),
            "expected Conflict error when same-major transitive dep conflicts with cross-major activation"
        );
    }

    #[test]
    fn backtrack_saves_choice_point_when_more_candidates_remain() {
        // Exercises the `true` branch of `choice.current_idx + 1 < choice.candidates.len()`
        // in `backtrack_and_retry`. When a package has three candidates and the first two
        // each produce a conflict, the resolver backtracks twice:
        //   1st backtrack: idx=0→1, candidates.len()=3 → 1+1=2 < 3 → saves choice point
        //   2nd backtrack: idx=1→2, candidates.len()=3 → 2+1=3 < 3 → does not save
        //
        // Setup: root -> lib =1.0.0, root -> app ^1.0
        //   lib: [1.0.0]
        //   app: [1.2.0 → lib=1.3.0, 1.1.0 → lib=1.2.0, 1.0.0 → (no deps)]
        //
        // lib =1.0.0 activates lib@1.0.0 first. app@1.2.0 wants lib=1.3.0 → conflict.
        // 1st backtrack: saves choice(idx=1), activates app@1.1.0 → lib=1.2.0 → conflict.
        // 2nd backtrack: no save, activates app@1.0.0 → succeeds.
        let mut reg = MockRegistry::new();
        reg.add_package(
            "app",
            vec![
                ("1.2.0", vec![dep("lib", "=1.3.0")]),
                ("1.1.0", vec![dep("lib", "=1.2.0")]),
                ("1.0.0", vec![]),
            ],
        );
        reg.add_package("lib", vec![("1.0.0", vec![]), ("1.2.0", vec![]), ("1.3.0", vec![])]);

        let deps = vec![root_dep("lib", "=1.0.0"), root_dep("app", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg).unwrap();

        let app = result.packages.iter().find(|p| p.name == "app").unwrap();
        assert_eq!(app.version.to_string(), "1.0.0");
        let lib = result.packages.iter().find(|p| p.name == "lib").unwrap();
        assert_eq!(lib.version.to_string(), "1.0.0");
    }

    #[test]
    fn backtrack_exhausts_all_candidates() {
        // All candidates for a package fail due to same-major conflict, exercising the
        // `false` branch at line 371 where `choice.current_idx >= choice.candidates.len()`.
        //
        // Setup: root -> a ^1.0, root -> b ^1.0
        //   a@1.0.0 -> shared =1.0.0
        //   b has two versions: b@1.1.0 -> shared =1.2.0, b@1.0.0 -> shared =1.3.0
        //   shared has 1.0.0, 1.2.0, 1.3.0
        //
        // a activates shared@1.0.0. Then b@1.1.0 wants shared =1.2.0 → conflict → backtrack
        // to b@1.0.0 which wants shared =1.3.0 → still conflicts → exhausted → error.
        let mut reg = MockRegistry::new();
        reg.add_package("a", vec![("1.0.0", vec![dep("shared", "=1.0.0")])]);
        reg.add_package(
            "b",
            vec![
                ("1.0.0", vec![dep("shared", "=1.3.0")]),
                ("1.1.0", vec![dep("shared", "=1.2.0")]),
            ],
        );
        reg.add_package("shared", vec![("1.0.0", vec![]), ("1.2.0", vec![]), ("1.3.0", vec![])]);

        let deps = vec![root_dep("a", "^1.0"), root_dep("b", "^1.0")];
        let result = resolve(&deps, &BTreeMap::new(), &reg);

        assert!(result.is_err());
    }

    #[test]
    fn backtrack_detects_conflict_with_lockfile_pin() {
        // Covers the `if let Some(existing)` True branch in `backtrack_and_retry` (L391):
        // all backtracking candidates share the same major as a lockfile pin that does
        // not satisfy the requirement, so every candidate triggers the conflict check
        // and the resolver exhausts all options.
        let mut reg = MockRegistry::new();
        reg.add_package("skill-a", vec![("1.2.0", vec![]), ("1.1.0", vec![])]);

        // Lockfile pins skill-a at 1.0.0, which does not satisfy ^1.1.
        let mut pins = BTreeMap::new();
        pins.insert("skill-a".to_string(), Version::parse("1.0.0").unwrap());

        let deps = vec![root_dep("skill-a", "^1.1")];
        let result = resolve(&deps, &pins, &reg);

        // Candidates 1.2.0 and 1.1.0 both share major=1 with the pin (1.0.0).
        // Each backtrack iteration hits L391 True (existing pin found), checks whether
        // the pin satisfies ^1.1 (it doesn't), and continues — exhausting all choices.
        assert!(matches!(result, Err(Error::Conflict(_))));
    }
}
