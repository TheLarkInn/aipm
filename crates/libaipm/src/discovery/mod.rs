//! Unified feature discovery for the `aipm migrate` and `aipm lint` pipelines.
//!
//! This module is the single source of feature classification used by both
//! `migrate` and `lint`. It is unconditionally on — the previous opt-in
//! `AIPM_UNIFIED_DISCOVERY` env var was retired in this alpha release. See
//! `specs/2026-05-01-unified-discovery-and-copilot-skill-detection.md`.
//!
//! The legacy free functions (`discover_source_dirs`, `discover_claude_dirs`)
//! and the `DiscoveredSource` shape are still exported here for the migrate
//! orchestrator's per-source-dir hybrid (legacy detectors fill in deferred
//! kinds the unified classifier doesn't yet cover). The legacy
//! `discover_features` walker was deleted alongside `legacy_compat.rs`.

use std::path::Path;

pub mod classify;
pub mod instruction;
pub mod layout;
pub mod scan_report;
pub mod source;
pub mod types;
pub mod walker;

// New foundation types — accessible through both the submodule and the
// `types::` / `scan_report::` paths and re-exported here for convenience.
pub use classify::classify as classify_path;
pub use instruction::classify as classify_instruction;
pub use layout::{
    match_agent, match_hook, match_marketplace, match_plugin, match_plugin_json, match_skill,
};
pub use scan_report::{DiscoveredSet, ScanCounts, SkipReason};
pub use source::infer_engine_root;
pub use types::{Engine, Layout};
pub use walker::{walk, WalkResult};

/// Options controlling a single discovery walk.
///
/// Consumed by [`walker::walk`] and [`discover`]. The defaults — no depth
/// limit, no source filter, don't follow symlinks — match today's
/// classifier behavior.
#[derive(Debug, Default, Clone)]
pub struct DiscoverOptions {
    /// Maximum walk depth from the project root. `None` means unlimited.
    pub max_depth: Option<usize>,
    /// Optional filter on the engine source root (e.g. `".github"`). Applied
    /// post-classification by [`discover`]; the walker itself walks the full
    /// project tree.
    pub source_filter: Option<String>,
    /// When `true`, the walker follows symlinks. Defaults to `false`.
    pub follow_symlinks: bool,
}

pub use types::{DiscoveredFeature, FeatureKind};

// Re-exports from the legacy module that the migrate hybrid still uses
// (`Error`, `DiscoveredSource`, `discover_source_dirs`, `discover_claude_dirs`).
// `discover_features` and `SourceContext` were retired with `legacy_compat.rs`.
pub use crate::discovery_legacy::{
    discover_claude_dirs, discover_source_dirs, DiscoveredSource, Error,
};

/// Run a single discovery walk under `project_root` and return the
/// classified [`DiscoveredSet`].
///
/// Walks the tree once via [`walker::walk`], classifies every visited file
/// with [`classify::classify`], and (when `opts.source_filter` is set)
/// retains only features whose [`DiscoveredFeature::source_root`]
/// `file_name` matches the filter string.
///
/// # Errors
///
/// Returns the underlying [`Error`] if the walker fails.
///
/// [`DiscoveredFeature::source_root`]: crate::discovery::types::DiscoveredFeature::source_root
pub fn discover(
    project_root: &Path,
    opts: &DiscoverOptions,
    fs: &dyn crate::fs::Fs,
) -> Result<DiscoveredSet, Error> {
    let walked = walker::walk(project_root, opts)?;
    let mut features = Vec::with_capacity(walked.files.len() / 4);
    for path in &walked.files {
        if let Some(feat) = classify::classify(path, project_root, fs) {
            tracing::trace!(
                path = %path.display(),
                kind = ?feat.kind,
                engine = ?feat.engine,
                layout = ?feat.layout,
                "classified"
            );
            features.push(feat);
        } else {
            tracing::debug!(path = %path.display(), "skipped: no classification");
        }
    }
    apply_source_filter(&mut features, opts.source_filter.as_deref());
    Ok(DiscoveredSet { features, scanned_dirs: walked.scanned_dirs, skipped: walked.skipped })
}

/// Retain only features whose `source_root` `file_name` matches `filter`.
fn apply_source_filter(features: &mut Vec<types::DiscoveredFeature>, filter: Option<&str>) {
    let Some(filter) = filter else {
        return;
    };
    features.retain(|f| f.source_root.file_name().is_some_and(|n| n.to_string_lossy() == *filter));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Real;
    use std::fs;

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, "").expect("touch file");
    }

    #[test]
    fn discover_unified_finds_issue_725_tree() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        for name in ["skill-alpha", "skill-beta", "skill-gamma"] {
            touch(&root.join(format!(".github/copilot/skills/{name}/SKILL.md")));
        }
        touch(&root.join(".github/copilot/copilot-instructions.md"));
        let set =
            discover(root, &DiscoverOptions::default(), &Real).expect("discover should succeed");
        let counts = set.counts();
        assert_eq!(counts.skills, 3, "expected 3 skills, got: {counts:?}");
        assert_eq!(counts.instructions, 1, "expected 1 instruction, got: {counts:?}");
        assert_eq!(counts.total(), 4);
        assert!(!set.scanned_dirs.is_empty());
    }

    #[test]
    fn discover_canonical_claude_and_github_skills() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join(".claude/skills/my-skill/SKILL.md"));
        touch(&root.join(".github/skills/my-other-skill/SKILL.md"));
        let set =
            discover(root, &DiscoverOptions::default(), &Real).expect("discover should succeed");
        assert_eq!(set.counts().skills, 2);
    }

    #[test]
    fn discover_finds_copilot_instructions_md() {
        // The actual #725 lint-side fix: unified picks up
        // `copilot-instructions.md` via the `<engine>-instructions.md`
        // regex shape — the legacy walker silently dropped it.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join(".github/copilot/copilot-instructions.md"));
        let set =
            discover(root, &DiscoverOptions::default(), &Real).expect("discover should succeed");
        assert_eq!(
            set.counts().instructions,
            1,
            "unified must find copilot-instructions.md (the fix)"
        );
    }

    #[test]
    fn discover_carries_walker_skipped_dirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join("node_modules/foo/index.js"));
        touch(&root.join(".claude/skills/x/SKILL.md"));
        let set =
            discover(root, &DiscoverOptions::default(), &Real).expect("discover should succeed");
        let names: Vec<&str> = set
            .skipped
            .iter()
            .filter_map(|r| match r {
                SkipReason::SkipDirByName { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert!(names.contains(&"node_modules"));
    }

    #[test]
    fn source_filter_retains_only_matching_features() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join(".claude/skills/x/SKILL.md"));
        touch(&root.join(".github/skills/y/SKILL.md"));
        let opts = DiscoverOptions {
            source_filter: Some(".github".to_string()),
            ..DiscoverOptions::default()
        };
        let set = discover(root, &opts, &Real).expect("discover should succeed");
        let counts = set.counts();
        assert_eq!(counts.skills, 1, "expected only the .github skill: {counts:?}");
        assert!(set.features.iter().all(|f| f.engine == Engine::Copilot));
    }

    #[test]
    fn source_filter_no_match_returns_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join(".claude/skills/x/SKILL.md"));
        let opts = DiscoverOptions {
            source_filter: Some(".bogus".to_string()),
            ..DiscoverOptions::default()
        };
        let set = discover(root, &opts, &Real).expect("discover should succeed");
        assert_eq!(set.counts().total(), 0);
    }

    #[test]
    fn discover_empty_root_returns_empty_set() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let set =
            discover(root, &DiscoverOptions::default(), &Real).expect("discover should succeed");
        assert!(set.is_empty());
    }

    #[test]
    fn apply_source_filter_no_filter_keeps_all() {
        let mut features = vec![
            types::DiscoveredFeature {
                kind: FeatureKind::Skill,
                engine: Engine::Claude,
                layout: Layout::Canonical,
                source_root: ".claude".into(),
                feature_dir: None,
                path: "/tmp/SKILL.md".into(),
            },
            types::DiscoveredFeature {
                kind: FeatureKind::Skill,
                engine: Engine::Copilot,
                layout: Layout::Canonical,
                source_root: ".github".into(),
                feature_dir: None,
                path: "/tmp/SKILL.md".into(),
            },
        ];
        apply_source_filter(&mut features, None);
        assert_eq!(features.len(), 2);
    }
}
