//! Legacy compatibility adapter for the unified discovery module.
//!
//! Wraps today's [`crate::discovery_legacy::discover_features`] so that
//! `discover()` callers can run with the new `DiscoveredSet` return shape
//! while the `AIPM_UNIFIED_DISCOVERY` env var is OFF (the default during the
//! one-release soak window per the spec rollout plan).
//!
//! Delete this module in the cleanup feature once the unified path is the
//! default and the env-var check is removed.

use std::path::Path;

use crate::discovery_legacy::{discover_features, Error};
use crate::fs::Fs;

use super::scan_report::DiscoveredSet;
use super::source;
use super::types::{DiscoveredFeature, Engine, Layout};
use super::DiscoverOptions;

/// Run today's `discover_features` and adapt its output into a
/// [`DiscoveredSet`].
///
/// Each legacy `DiscoveredFeature` is mapped to the new shape:
///
/// - `kind` is preserved.
/// - `engine` is inferred from the file path via
///   [`source::infer_engine_root`]. If no engine ancestor is found, the
///   feature is dropped (the new shape has no fallback engine variant; the
///   unified path's classifier has the same "no engine, no feature"
///   behavior, so the two paths agree on the dropped set).
/// - `layout` is set to [`Layout::Canonical`] — the legacy classifier does
///   not distinguish layouts, so we use the conservative default. The
///   unified path's `pick_layout_for_skill` provides richer labels when
///   the flag is on.
/// - `source_root` is the engine ancestor returned by
///   [`source::infer_engine_root`].
/// - `feature_dir` is `path.parent()`.
/// - `path` is the legacy `file_path`.
///
/// `scanned_dirs` and `skipped` are empty — the legacy walker does not
/// track them.
///
/// `opts.source_filter`, when set, is applied post-classification. The
/// `_fs` parameter is ignored (the legacy path uses its own filesystem
/// access internally) but accepted for API symmetry with the unified
/// pipeline.
///
/// # Errors
///
/// Returns the underlying [`Error`] if the legacy walker fails.
pub fn discover_features_compat(
    project_root: &Path,
    opts: &DiscoverOptions,
    _fs: &dyn Fs,
) -> Result<DiscoveredSet, Error> {
    let legacy = discover_features(project_root, opts.max_depth)?;
    let mut features: Vec<DiscoveredFeature> = Vec::with_capacity(legacy.len());
    for f in legacy {
        // For features without an engine ancestor (e.g. project-root
        // CLAUDE.md), synthesize a (Engine::Ai, project_root) context.
        // This matches `classify::classify`'s fallback so the unified and
        // legacy paths agree on root-level instruction files.
        let (engine, source_root) = source::infer_engine_root(&f.file_path, project_root)
            .unwrap_or_else(|| (Engine::Ai, project_root.to_path_buf()));
        features.push(DiscoveredFeature {
            kind: f.kind,
            engine,
            layout: Layout::Canonical,
            source_root,
            feature_dir: f.file_path.parent().map(Path::to_path_buf),
            path: f.file_path,
        });
    }
    if let Some(filter) = opts.source_filter.as_deref() {
        features
            .retain(|f| f.source_root.file_name().is_some_and(|n| n.to_string_lossy() == *filter));
    }
    Ok(DiscoveredSet { features, scanned_dirs: Vec::new(), skipped: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::types::Engine;
    use crate::fs::Real;
    use std::fs;

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, "").expect("touch file");
    }

    #[test]
    fn empty_root_returns_empty_set() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let set = discover_features_compat(tmp.path(), &DiscoverOptions::default(), &Real)
            .expect("compat should succeed");
        assert!(set.is_empty());
        assert!(set.scanned_dirs.is_empty());
        assert!(set.skipped.is_empty());
    }

    #[test]
    fn maps_canonical_skills_into_new_shape() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join(".claude/skills/my-skill/SKILL.md"));
        touch(&root.join(".github/skills/other-skill/SKILL.md"));

        let set = discover_features_compat(root, &DiscoverOptions::default(), &Real)
            .expect("compat should succeed");
        assert_eq!(set.counts().skills, 2);

        let claude =
            set.features.iter().find(|f| f.engine == Engine::Claude).expect("claude skill in set");
        assert_eq!(claude.layout, Layout::Canonical);
        assert!(claude.source_root.ends_with(".claude"));
        assert!(claude.path.ends_with("my-skill/SKILL.md"));
        assert_eq!(
            claude.feature_dir.as_ref().expect("feature_dir"),
            &claude.path.parent().expect("parent").to_path_buf()
        );

        let github =
            set.features.iter().find(|f| f.engine == Engine::Copilot).expect("github skill in set");
        assert_eq!(github.layout, Layout::Canonical);
        assert!(github.source_root.ends_with(".github"));
    }

    #[test]
    fn issue_725_skill_files_visible_via_legacy_grandparent_branch() {
        // Today's classify_feature_kind already finds these via the
        // grandparent_name == "skills" branch. The compat layer preserves
        // that.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        for name in ["skill-alpha", "skill-beta", "skill-gamma"] {
            touch(&root.join(format!(".github/copilot/skills/{name}/SKILL.md")));
        }
        let set = discover_features_compat(root, &DiscoverOptions::default(), &Real)
            .expect("compat should succeed");
        assert_eq!(set.counts().skills, 3);
        assert!(set.features.iter().all(|f| f.engine == Engine::Copilot));
        assert!(set.features.iter().all(|f| f.layout == Layout::Canonical));
    }

    #[test]
    fn copilot_instructions_md_dropped_by_legacy() {
        // The actual #725 lint-side silent drop. Compat preserves today's
        // bug — the unified path is what fixes it.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join(".github/copilot/copilot-instructions.md"));
        let set = discover_features_compat(root, &DiscoverOptions::default(), &Real)
            .expect("compat should succeed");
        assert_eq!(set.counts().instructions, 0);
    }

    #[test]
    fn project_root_instructions_kept_with_ai_fallback() {
        // CLAUDE.md at the project root has no .claude/.github/.ai ancestor.
        // Today's `discover_features` classifies it as Instructions; the
        // compat layer uses an `(Engine::Ai, project_root)` fallback so we
        // don't drop the file (matches classify::classify's fallback for
        // the unified path).
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join("CLAUDE.md"));
        let set = discover_features_compat(root, &DiscoverOptions::default(), &Real)
            .expect("compat should succeed");
        assert_eq!(set.counts().instructions, 1);
        let inst = set.features.first().expect("at least one feature");
        assert_eq!(inst.engine, Engine::Ai);
        assert_eq!(inst.source_root, root);
    }

    #[test]
    fn source_filter_retains_only_matching() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join(".claude/skills/x/SKILL.md"));
        touch(&root.join(".github/skills/y/SKILL.md"));
        let opts = DiscoverOptions {
            source_filter: Some(".github".to_string()),
            ..DiscoverOptions::default()
        };
        let set = discover_features_compat(root, &opts, &Real).expect("compat should succeed");
        assert_eq!(set.counts().skills, 1);
        assert!(set.features.iter().all(|f| f.engine == Engine::Copilot));
    }

    #[test]
    fn max_depth_propagated_to_legacy_walker() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join(".claude/skills/x/SKILL.md")); // depth 3
        touch(&root.join("a/b/.claude/skills/x/SKILL.md")); // depth 5
        let opts = DiscoverOptions { max_depth: Some(3), ..DiscoverOptions::default() };
        let set = discover_features_compat(root, &opts, &Real).expect("compat should succeed");
        // Only the shallow .claude/skills/... should be visible at depth 3.
        // (depth 5 tree exceeds the depth limit — legacy walker prunes it.)
        assert!(set.features.iter().all(|f| !f.path.to_string_lossy().contains("/a/b/")));
    }

    #[test]
    fn marketplace_and_plugin_json_under_ai_round_trip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join(".ai/.claude-plugin/marketplace.json"));
        touch(&root.join(".ai/my-plugin/.claude-plugin/plugin.json"));
        touch(&root.join(".ai/my-plugin/aipm.toml"));
        let set = discover_features_compat(root, &DiscoverOptions::default(), &Real)
            .expect("compat should succeed");
        let counts = set.counts();
        assert_eq!(counts.marketplaces, 1);
        assert_eq!(counts.plugin_jsons, 1);
        assert_eq!(counts.plugins, 1);
        assert!(set.features.iter().all(|f| f.engine == Engine::Ai));
    }
}
