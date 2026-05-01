//! Unified feature discovery for the `aipm migrate` and `aipm lint` pipelines.
//!
//! This module is being built incrementally per the spec at
//! `specs/2026-05-01-unified-discovery-and-copilot-skill-detection.md`. In this
//! initial step it provides:
//!
//! - The new foundation types (`Engine`, `Layout`, the new `DiscoveredFeature`,
//!   `DiscoveredSet`, `ScanCounts`, `SkipReason`) in submodules `types` and
//!   `scan_report`. The `types` module name follows the existing codebase
//!   convention (e.g. `manifest/types.rs`) and avoids the
//!   `clippy::module_name_repetitions` trigger.
//! - Re-exports of the legacy types and free functions still in use by `lint`
//!   and `migrate` (`Error`, the legacy `DiscoveredFeature`, `SourceContext`,
//!   `DiscoveredSource`, `discover_features`, `discover_source_dirs`,
//!   `discover_claude_dirs`) so that today's call sites continue to compile
//!   unchanged.
//!
//! Callers needing the new shape should import via
//! `crate::discovery::types::DiscoveredFeature`. The legacy struct exposed at
//! `crate::discovery::DiscoveredFeature` will be removed once the lint and
//! migrate pipelines are switched over in later spec features.

use std::path::Path;

pub mod classify;
pub mod instruction;
pub mod layout;
pub mod legacy_compat;
pub mod scan_report;
pub mod source;
pub mod types;
pub mod walker;

#[cfg(test)]
pub(crate) mod test_env;

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
/// `discover_features` behavior.
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

// `DiscoveredFeature` now resolves to the new shape — lint has been switched
// over (feature 10). Migrate's remaining direct uses go through
// `crate::discovery_legacy::DiscoveredFeature` until feature 14 swaps it.
pub use types::{DiscoveredFeature, FeatureKind};

// Re-exports from the legacy module for the still-legacy migrate pipeline
// (`Error`, `SourceContext`, `DiscoveredSource`, the legacy free functions).
pub use crate::discovery_legacy::{
    discover_claude_dirs, discover_features, discover_source_dirs, DiscoveredSource, Error,
    SourceContext,
};

/// The environment variable that opts into the unified discovery path.
///
/// When set to `"1"`, [`discover`] uses the new walker + classifier pipeline.
/// Any other value (or the variable being unset) falls back to the legacy
/// `discover_features` adapter, preserving today's behavior. The flag is
/// intended for one release of soak time per the spec rollout plan; it will
/// be removed and the unified path made unconditional in a later feature.
pub const UNIFIED_DISCOVERY_ENV: &str = "AIPM_UNIFIED_DISCOVERY";

/// Run a single discovery walk under `project_root` and return the
/// classified [`DiscoveredSet`].
///
/// Dispatches between the unified pipeline (walker + classifier) and the
/// legacy adapter based on the `AIPM_UNIFIED_DISCOVERY` environment
/// variable. See [`UNIFIED_DISCOVERY_ENV`] for the rollout policy.
///
/// `opts.source_filter`, when set, is applied post-classification: only
/// features whose [`DiscoveredFeature::source_root`] `file_name` matches the
/// filter string are retained.
///
/// # Errors
///
/// Returns the underlying [`Error`] if the walker or legacy adapter fails.
///
/// [`DiscoveredFeature::source_root`]: crate::discovery::types::DiscoveredFeature::source_root
pub fn discover(
    project_root: &Path,
    opts: &DiscoverOptions,
    fs: &dyn crate::fs::Fs,
) -> Result<DiscoveredSet, Error> {
    if unified_enabled() {
        unified_discover(project_root, opts, fs)
    } else {
        legacy_compat::discover_features_compat(project_root, opts, fs)
    }
}

/// `true` when [`UNIFIED_DISCOVERY_ENV`] is set to exactly `"1"`.
fn unified_enabled() -> bool {
    std::env::var(UNIFIED_DISCOVERY_ENV).is_ok_and(|v| v == "1")
}

/// Unified pipeline: walk the tree, classify each file, apply source filter.
fn unified_discover(
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

    fn with_env_var<F: FnOnce()>(value: Option<&str>, body: F) {
        super::test_env::with_unified_discovery_env(value, body);
    }

    #[test]
    fn unified_enabled_returns_true_only_for_exact_one() {
        with_env_var(Some("1"), || assert!(unified_enabled()));
        with_env_var(Some("0"), || assert!(!unified_enabled()));
        with_env_var(Some("true"), || assert!(!unified_enabled()));
        with_env_var(Some(""), || assert!(!unified_enabled()));
        with_env_var(None, || assert!(!unified_enabled()));
    }

    #[test]
    fn discover_unified_finds_issue_725_tree() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        for name in ["skill-alpha", "skill-beta", "skill-gamma"] {
            touch(&root.join(format!(".github/copilot/skills/{name}/SKILL.md")));
        }
        touch(&root.join(".github/copilot/copilot-instructions.md"));
        with_env_var(Some("1"), || {
            let set = discover(root, &DiscoverOptions::default(), &Real)
                .expect("discover should succeed");
            let counts = set.counts();
            assert_eq!(counts.skills, 3, "expected 3 skills, got: {counts:?}");
            assert_eq!(counts.instructions, 1, "expected 1 instruction, got: {counts:?}");
            assert_eq!(counts.total(), 4);
            assert!(!set.scanned_dirs.is_empty());
        });
    }

    #[test]
    fn discover_legacy_finds_canonical_skills() {
        // With the flag off, the adapter routes through discover_features.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join(".claude/skills/my-skill/SKILL.md"));
        touch(&root.join(".github/skills/my-other-skill/SKILL.md"));
        with_env_var(None, || {
            let set = discover(root, &DiscoverOptions::default(), &Real)
                .expect("discover should succeed");
            let counts = set.counts();
            assert_eq!(counts.skills, 2, "legacy adapter expected 2 skills, got: {counts:?}");
        });
    }

    #[test]
    fn discover_legacy_finds_issue_725_skill_files() {
        // Subtle: today's `discover_features` (the lint walker) already finds
        // SKILL.md at .github/copilot/skills/<x>/SKILL.md via the
        // `grandparent_name == "skills"` branch in classify_feature_kind. The
        // actual #725 silent drop on the LINT side is the
        // `copilot-instructions.md` filename, not the skill files. (The
        // migrate side has the skill drop bug, but migrate uses
        // CopilotSkillDetector — a different code path entirely.)
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join(".github/copilot/skills/skill-alpha/SKILL.md"));
        with_env_var(Some("0"), || {
            let set = discover(root, &DiscoverOptions::default(), &Real)
                .expect("discover should succeed");
            assert_eq!(set.counts().skills, 1);
        });
    }

    #[test]
    fn discover_legacy_drops_copilot_instructions_md_unified_finds_it() {
        // The actual #725 lint-side silent drop: legacy misses
        // `copilot-instructions.md`; unified picks it up via the
        // <engine>-instructions.md regex shape.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join(".github/copilot/copilot-instructions.md"));

        with_env_var(Some("0"), || {
            let legacy =
                discover(root, &DiscoverOptions::default(), &Real).expect("legacy should succeed");
            assert_eq!(
                legacy.counts().instructions,
                0,
                "legacy must drop copilot-instructions.md (the bug)"
            );
        });
        with_env_var(Some("1"), || {
            let unified =
                discover(root, &DiscoverOptions::default(), &Real).expect("unified should succeed");
            assert_eq!(
                unified.counts().instructions,
                1,
                "unified must find copilot-instructions.md (the fix)"
            );
        });
    }

    #[test]
    fn discover_unified_carries_walker_skipped_dirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join("node_modules/foo/index.js"));
        touch(&root.join(".claude/skills/x/SKILL.md"));
        with_env_var(Some("1"), || {
            let set = discover(root, &DiscoverOptions::default(), &Real)
                .expect("discover should succeed");
            let names: Vec<&str> = set
                .skipped
                .iter()
                .filter_map(|r| match r {
                    SkipReason::SkipDirByName { name, .. } => Some(name.as_str()),
                    _ => None,
                })
                .collect();
            assert!(names.contains(&"node_modules"));
        });
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
        with_env_var(Some("1"), || {
            let set = discover(root, &opts, &Real).expect("discover should succeed");
            let counts = set.counts();
            assert_eq!(counts.skills, 1, "expected only the .github skill: {counts:?}");
            // The retained feature must be the .github one.
            assert!(set.features.iter().all(|f| f.engine == Engine::Copilot));
        });
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
        with_env_var(Some("1"), || {
            let set = discover(root, &opts, &Real).expect("discover should succeed");
            assert_eq!(set.counts().total(), 0);
        });
    }

    #[test]
    fn discover_empty_root_returns_empty_set() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        with_env_var(Some("1"), || {
            let set = discover(root, &DiscoverOptions::default(), &Real)
                .expect("discover should succeed");
            assert!(set.is_empty());
        });
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
