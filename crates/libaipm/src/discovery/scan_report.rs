//! Scan report types for the unified discovery module.
//!
//! `DiscoveredSet` is the value returned from a single discovery walk. It
//! carries the discovered features, the directories that were scanned, and the
//! reasons any candidate paths were skipped — enough information for callers
//! to render a scan summary and to drive both lint and migrate from a single
//! source of truth.

use std::path::PathBuf;

use super::types::{DiscoveredFeature, FeatureKind};

/// Aggregated counts of features discovered, broken down by `FeatureKind`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScanCounts {
    /// Number of `FeatureKind::Skill` features.
    pub skills: usize,
    /// Number of `FeatureKind::Agent` features.
    pub agents: usize,
    /// Number of `FeatureKind::Hook` features.
    pub hooks: usize,
    /// Number of `FeatureKind::Instructions` features.
    pub instructions: usize,
    /// Number of `FeatureKind::Plugin` features.
    pub plugins: usize,
    /// Number of `FeatureKind::Marketplace` features.
    pub marketplaces: usize,
    /// Number of `FeatureKind::PluginJson` features.
    pub plugin_jsons: usize,
}

impl ScanCounts {
    /// Total number of features across all kinds.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.skills
            + self.agents
            + self.hooks
            + self.instructions
            + self.plugins
            + self.marketplaces
            + self.plugin_jsons
    }
}

/// Reason a candidate path was skipped during the walk.
///
/// Recorded so the scan summary can explain "why didn't aipm find my file" —
/// the canonical answer to issue #725-style silent failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// A directory was skipped because its name matched the `SKIP_DIRS` set
    /// (`node_modules`, `target`, `.git`, etc.).
    SkipDirByName {
        /// The path of the skipped directory.
        path: PathBuf,
        /// The directory name that matched the skip list.
        name: String,
    },
}

/// The result of a single discovery walk.
#[derive(Debug, Default, Clone)]
pub struct DiscoveredSet {
    /// All discovered features, in walk order.
    pub features: Vec<DiscoveredFeature>,
    /// Every directory the walker descended into.
    pub scanned_dirs: Vec<PathBuf>,
    /// Skip reasons recorded during the walk.
    pub skipped: Vec<SkipReason>,
}

impl DiscoveredSet {
    /// Aggregate counts of discovered features by kind.
    #[must_use]
    pub fn counts(&self) -> ScanCounts {
        let mut counts = ScanCounts::default();
        for feat in &self.features {
            match feat.kind {
                FeatureKind::Skill => counts.skills += 1,
                FeatureKind::Agent => counts.agents += 1,
                FeatureKind::Hook => counts.hooks += 1,
                FeatureKind::Instructions => counts.instructions += 1,
                FeatureKind::Plugin => counts.plugins += 1,
                FeatureKind::Marketplace => counts.marketplaces += 1,
                FeatureKind::PluginJson => counts.plugin_jsons += 1,
            }
        }
        counts
    }

    /// Returns `true` if no features were discovered.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{DiscoveredFeature, Engine, Layout};
    use super::*;
    use std::path::PathBuf;

    fn make_feature(kind: FeatureKind) -> DiscoveredFeature {
        DiscoveredFeature {
            kind,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".github"),
            feature_dir: Some(PathBuf::from(".github/skills/test")),
            path: PathBuf::from(".github/skills/test/SKILL.md"),
        }
    }

    #[test]
    fn discovered_set_default_is_empty() {
        let set = DiscoveredSet::default();
        assert!(set.is_empty());
        assert_eq!(set.features.len(), 0);
        assert_eq!(set.scanned_dirs.len(), 0);
        assert_eq!(set.skipped.len(), 0);
    }

    #[test]
    fn scan_counts_default_is_zero() {
        let counts = ScanCounts::default();
        assert_eq!(counts.skills, 0);
        assert_eq!(counts.agents, 0);
        assert_eq!(counts.hooks, 0);
        assert_eq!(counts.instructions, 0);
        assert_eq!(counts.plugins, 0);
        assert_eq!(counts.marketplaces, 0);
        assert_eq!(counts.plugin_jsons, 0);
        assert_eq!(counts.total(), 0);
    }

    #[test]
    fn scan_counts_total_sums_all_kinds() {
        let counts = ScanCounts {
            skills: 3,
            agents: 2,
            hooks: 1,
            instructions: 1,
            plugins: 1,
            marketplaces: 1,
            plugin_jsons: 1,
        };
        assert_eq!(counts.total(), 10);
    }

    #[test]
    fn discovered_set_counts_aggregates_each_kind() {
        let set = DiscoveredSet {
            features: vec![
                make_feature(FeatureKind::Skill),
                make_feature(FeatureKind::Skill),
                make_feature(FeatureKind::Skill),
                make_feature(FeatureKind::Agent),
                make_feature(FeatureKind::Agent),
                make_feature(FeatureKind::Hook),
                make_feature(FeatureKind::Instructions),
                make_feature(FeatureKind::Plugin),
                make_feature(FeatureKind::Marketplace),
                make_feature(FeatureKind::PluginJson),
            ],
            scanned_dirs: Vec::new(),
            skipped: Vec::new(),
        };
        let counts = set.counts();
        assert_eq!(counts.skills, 3);
        assert_eq!(counts.agents, 2);
        assert_eq!(counts.hooks, 1);
        assert_eq!(counts.instructions, 1);
        assert_eq!(counts.plugins, 1);
        assert_eq!(counts.marketplaces, 1);
        assert_eq!(counts.plugin_jsons, 1);
        assert_eq!(counts.total(), 10);
    }

    #[test]
    fn discovered_set_is_empty_true_when_no_features() {
        let set = DiscoveredSet {
            features: Vec::new(),
            scanned_dirs: vec![PathBuf::from(".")],
            skipped: vec![SkipReason::SkipDirByName {
                path: PathBuf::from("node_modules"),
                name: "node_modules".to_string(),
            }],
        };
        assert!(set.is_empty());
    }

    #[test]
    fn discovered_set_counts_empty_set_returns_zeros() {
        let set = DiscoveredSet::default();
        let counts = set.counts();
        assert_eq!(counts.skills, 0);
        assert_eq!(counts.agents, 0);
        assert_eq!(counts.hooks, 0);
        assert_eq!(counts.instructions, 0);
        assert_eq!(counts.plugins, 0);
        assert_eq!(counts.marketplaces, 0);
        assert_eq!(counts.plugin_jsons, 0);
        assert_eq!(counts.total(), 0);
    }

    #[test]
    fn discovered_set_is_empty_false_with_features() {
        let set = DiscoveredSet {
            features: vec![make_feature(FeatureKind::Skill)],
            scanned_dirs: Vec::new(),
            skipped: Vec::new(),
        };
        assert!(!set.is_empty());
    }

    #[test]
    fn skip_reason_skip_dir_by_name() {
        let reason = SkipReason::SkipDirByName {
            path: PathBuf::from("node_modules"),
            name: "node_modules".to_string(),
        };
        assert!(matches!(&reason, SkipReason::SkipDirByName { .. }));
        if let SkipReason::SkipDirByName { name, path } = &reason {
            assert_eq!(name, "node_modules");
            assert_eq!(path, &PathBuf::from("node_modules"));
        }
    }

    #[test]
    fn skip_reason_clone_eq() {
        let r =
            SkipReason::SkipDirByName { path: PathBuf::from("target"), name: "target".to_string() };
        assert_eq!(r.clone(), r);
    }
}
