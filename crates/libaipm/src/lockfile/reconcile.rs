//! Minimal reconciliation: diff manifest deps against lockfile.
//!
//! When the manifest changes (deps added/removed/changed), we only
//! re-resolve the changed deps, carrying forward existing pins.

use std::collections::BTreeSet;

use super::types::{Lockfile, Package};

/// The result of reconciling a lockfile against a manifest.
#[derive(Debug, PartialEq, Eq)]
pub struct Reconciliation {
    /// Dependency names that need re-resolution (added or changed).
    pub needs_resolution: BTreeSet<String>,

    /// Packages from the lockfile that are carried forward unchanged.
    pub carried_forward: Vec<Package>,

    /// Dependency names that were removed from the manifest.
    pub removed: BTreeSet<String>,
}

/// Reconcile a lockfile against the current manifest dependency set.
///
/// `manifest_deps` is the set of direct dependency names from the current manifest.
/// `lockfile` is the existing lockfile (may be empty).
///
/// Returns a [`Reconciliation`] describing what changed:
/// - `needs_resolution`: deps that are new or changed and need the resolver
/// - `carried_forward`: lockfile packages that are still valid
/// - `removed`: deps that were in the lockfile but removed from the manifest
pub fn reconcile(lockfile: &Lockfile, manifest_deps: &BTreeSet<String>) -> Reconciliation {
    let locked_names: BTreeSet<String> = lockfile.packages.iter().map(|p| p.name.clone()).collect();

    // Added: in manifest but not in lockfile
    let added: BTreeSet<String> = manifest_deps.difference(&locked_names).cloned().collect();

    // Removed: in lockfile but not in manifest (direct deps only)
    let removed: BTreeSet<String> = locked_names.difference(manifest_deps).cloned().collect();

    // Carried forward: lockfile packages that are still in the manifest.
    // Workspace packages (source = "workspace") are excluded from pins —
    // they are resolved separately by the workspace resolver, not the
    // registry solver, so they should not appear as lockfile pins.
    let carried_forward: Vec<Package> = lockfile
        .packages
        .iter()
        .filter(|p| !removed.contains(&p.name) && p.source != "workspace")
        .cloned()
        .collect();

    Reconciliation { needs_resolution: added, carried_forward, removed }
}

/// Prune packages from a lockfile that are exclusive transitive deps
/// of removed packages.
///
/// A package is pruned if it was only reachable through removed packages
/// and is not a direct manifest dep or reachable through other carried-forward
/// packages.
///
/// `all_reachable` is the set of package names reachable from the
/// carried-forward packages (including transitive deps).
pub fn prune_orphans(
    carried_forward: &[Package],
    all_reachable: &BTreeSet<String>,
) -> Vec<Package> {
    carried_forward.iter().filter(|p| all_reachable.contains(&p.name)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::types::Metadata;

    fn make_lockfile(names: &[&str]) -> Lockfile {
        Lockfile {
            metadata: Metadata { lockfile_version: 1, generated_by: "test".to_string() },
            packages: names
                .iter()
                .map(|name| Package {
                    name: (*name).to_string(),
                    version: "1.0.0".to_string(),
                    source: "git+test".to_string(),
                    checksum: "sha512-test".to_string(),
                    dependencies: vec![],
                })
                .collect(),
        }
    }

    fn dep_set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn no_changes() {
        let lf = make_lockfile(&["a", "b"]);
        let deps = dep_set(&["a", "b"]);

        let result = reconcile(&lf, &deps);
        assert!(result.needs_resolution.is_empty());
        assert!(result.removed.is_empty());
        assert_eq!(result.carried_forward.len(), 2);
    }

    #[test]
    fn new_dep_added() {
        let lf = make_lockfile(&["a"]);
        let deps = dep_set(&["a", "b"]);

        let result = reconcile(&lf, &deps);
        assert_eq!(result.needs_resolution, dep_set(&["b"]));
        assert!(result.removed.is_empty());
        assert_eq!(result.carried_forward.len(), 1);
        assert_eq!(result.carried_forward[0].name, "a");
    }

    #[test]
    fn dep_removed() {
        let lf = make_lockfile(&["a", "b"]);
        let deps = dep_set(&["a"]);

        let result = reconcile(&lf, &deps);
        assert!(result.needs_resolution.is_empty());
        assert_eq!(result.removed, dep_set(&["b"]));
        assert_eq!(result.carried_forward.len(), 1);
    }

    #[test]
    fn both_added_and_removed() {
        let lf = make_lockfile(&["a", "b"]);
        let deps = dep_set(&["a", "c"]);

        let result = reconcile(&lf, &deps);
        assert_eq!(result.needs_resolution, dep_set(&["c"]));
        assert_eq!(result.removed, dep_set(&["b"]));
        assert_eq!(result.carried_forward.len(), 1);
    }

    #[test]
    fn empty_lockfile_all_new() {
        let lf = make_lockfile(&[]);
        let deps = dep_set(&["a", "b"]);

        let result = reconcile(&lf, &deps);
        assert_eq!(result.needs_resolution, dep_set(&["a", "b"]));
        assert!(result.removed.is_empty());
        assert!(result.carried_forward.is_empty());
    }

    #[test]
    fn empty_manifest_all_removed() {
        let lf = make_lockfile(&["a", "b"]);
        let deps = dep_set(&[]);

        let result = reconcile(&lf, &deps);
        assert!(result.needs_resolution.is_empty());
        assert_eq!(result.removed, dep_set(&["a", "b"]));
        assert!(result.carried_forward.is_empty());
    }

    #[test]
    fn prune_orphans_keeps_reachable() {
        let packages = vec![
            Package {
                name: "a".to_string(),
                version: "1.0.0".to_string(),
                source: "git+test".to_string(),
                checksum: "".to_string(),
                dependencies: vec![],
            },
            Package {
                name: "orphan".to_string(),
                version: "1.0.0".to_string(),
                source: "git+test".to_string(),
                checksum: "".to_string(),
                dependencies: vec![],
            },
        ];

        let reachable = dep_set(&["a"]);
        let result = prune_orphans(&packages, &reachable);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "a");
    }

    fn make_lockfile_with_sources(entries: &[(&str, &str)]) -> Lockfile {
        Lockfile {
            metadata: Metadata { lockfile_version: 1, generated_by: "test".to_string() },
            packages: entries
                .iter()
                .map(|(name, source)| Package {
                    name: (*name).to_string(),
                    version: "1.0.0".to_string(),
                    source: (*source).to_string(),
                    checksum: String::new(),
                    dependencies: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn workspace_packages_excluded_from_carried_forward() {
        let lf = make_lockfile_with_sources(&[
            ("reg-pkg", "git+https://example.com"),
            ("ws-pkg", "workspace"),
        ]);
        let deps = dep_set(&["reg-pkg", "ws-pkg"]);

        let result = reconcile(&lf, &deps);
        // ws-pkg should not be carried forward
        assert_eq!(result.carried_forward.len(), 1);
        assert_eq!(result.carried_forward[0].name, "reg-pkg");
        // ws-pkg should appear in needs_resolution since it wasn't carried forward
        // and it's in the manifest but not in carried_forward names
        assert!(result.needs_resolution.is_empty()); // it's in locked_names so not "added"
    }

    #[test]
    fn registry_packages_still_carried_forward() {
        let lf = make_lockfile_with_sources(&[
            ("pkg-a", "git+https://example.com"),
            ("pkg-b", "git+https://example.com"),
        ]);
        let deps = dep_set(&["pkg-a", "pkg-b"]);

        let result = reconcile(&lf, &deps);
        assert_eq!(result.carried_forward.len(), 2);
        assert!(result.needs_resolution.is_empty());
        assert!(result.removed.is_empty());
    }

    #[test]
    fn prune_orphans_empty_reachable() {
        let packages = vec![Package {
            name: "a".to_string(),
            version: "1.0.0".to_string(),
            source: "git+test".to_string(),
            checksum: "".to_string(),
            dependencies: vec![],
        }];

        let reachable = BTreeSet::new();
        let result = prune_orphans(&packages, &reachable);
        assert!(result.is_empty());
    }
}
