//! Top-level classifier dispatch for the unified discovery module.
//!
//! Given a candidate file path and the project root, decides whether the
//! path is a recognized AI plugin feature and returns the corresponding
//! [`DiscoveredFeature`]. Single source of truth for the path-to-feature
//! decision used by both `aipm migrate` and `aipm lint` once the pipelines
//! are switched over.
//!
//! Order of precedence:
//! 1. Engine-root inference via [`super::source::infer_engine_root`] — if no
//!    engine ancestor (`.claude`, `.github`, `.ai`) exists, return `None`.
//! 2. Instruction-file detection via [`super::instruction::classify`] — wins
//!    over the agent rule so that files like `agents/AGENTS.md` are
//!    classified as `Instructions`, not `Agent` (today's classifier comment
//!    at `discovery_legacy.rs:259-260`).
//! 3. Filename-keyed dispatch into the layout matchers in
//!    [`super::layout`].

use std::path::Path;

use crate::fs::Fs;

use super::instruction;
use super::layout::{
    match_agent, match_hook, match_marketplace, match_plugin, match_plugin_json, match_skill,
};
use super::source;
use super::types::DiscoveredFeature;

/// Classify `path` as a discovered feature, given the `project_root`.
///
/// Returns `Some(DiscoveredFeature)` if the path matches an instruction
/// filename or a layout shape under a recognized engine source root; returns
/// `None` for any path that doesn't match any rule (the silent-skip
/// semantics the walker uses to drop unrecognized files).
///
/// The `_fs` parameter is reserved for layout matchers that may need
/// filesystem validation in later spec features. Today's matchers inspect
/// path components only, so the `Fs` trait is not used internally.
#[must_use]
pub fn classify(path: &Path, project_root: &Path, _fs: &dyn Fs) -> Option<DiscoveredFeature> {
    let file_name_os = path.file_name()?;
    let file_name = file_name_os.to_string_lossy();

    let (engine, source_root) = source::infer_engine_root(path, project_root)?;

    // Instruction files first — wins over the agent rule.
    if let Some(feat) = instruction::classify(&file_name, path, engine, &source_root) {
        return Some(feat);
    }

    match file_name.as_ref() {
        "SKILL.md" => match_skill(path, engine, &source_root),
        "hooks.json" => match_hook(path, engine, &source_root),
        "aipm.toml" => match_plugin(path, engine, &source_root),
        "marketplace.json" => match_marketplace(path, engine, &source_root),
        "plugin.json" => match_plugin_json(path, engine, &source_root),
        _ if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md")) => {
            match_agent(path, engine, &source_root)
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::types::{Engine, FeatureKind, Layout};
    use crate::fs::Real;
    use std::path::PathBuf;

    fn classify_at(path: &Path, project_root: &Path) -> Option<DiscoveredFeature> {
        classify(path, project_root, &Real)
    }

    // --- engine-root inference gating ---

    #[test]
    fn no_engine_root_returns_none() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/src/lib.rs");
        assert!(classify_at(&path, &root).is_none());
    }

    #[test]
    fn unrecognized_filename_returns_none() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.github/random.txt");
        assert!(classify_at(&path, &root).is_none());
    }

    // --- instruction precedence ---

    #[test]
    fn agents_uppercase_md_classified_as_instructions_not_agent() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.claude/agents/AGENTS.md");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Instructions);
    }

    #[test]
    fn copilot_md_classified_as_instructions() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.github/copilot.md");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Instructions);
        assert_eq!(feat.engine, Engine::Copilot);
    }

    #[test]
    fn copilot_instructions_md_classified_as_instructions() {
        // The issue #725 second silent-drop case.
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.github/copilot/copilot-instructions.md");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Instructions);
        assert_eq!(feat.engine, Engine::Copilot);
    }

    #[test]
    fn instructions_md_suffix_classified_as_instructions() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.claude/my-thing.instructions.md");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Instructions);
    }

    // --- filename dispatch matrix ---

    #[test]
    fn skill_md_dispatches_to_match_skill() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.claude/skills/my-skill/SKILL.md");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Skill);
        assert_eq!(feat.engine, Engine::Claude);
        assert_eq!(feat.layout, Layout::Canonical);
    }

    #[test]
    fn issue_725_skill_dispatches_to_match_skill() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.github/copilot/skills/skill-alpha/SKILL.md");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Skill);
        assert_eq!(feat.engine, Engine::Copilot);
        assert_eq!(feat.layout, Layout::CopilotSubrootWithSkills);
    }

    #[test]
    fn issue_725_full_tree_dispatch() {
        let root = PathBuf::from("/repo");
        for name in ["skill-alpha", "skill-beta", "skill-gamma"] {
            let path = PathBuf::from(format!("/repo/.github/copilot/skills/{name}/SKILL.md"));
            let feat = classify_at(&path, &root).expect("should match");
            assert_eq!(feat.kind, FeatureKind::Skill);
            assert_eq!(feat.layout, Layout::CopilotSubrootWithSkills);
        }
        let inst =
            classify_at(&PathBuf::from("/repo/.github/copilot/copilot-instructions.md"), &root)
                .expect("instructions should match");
        assert_eq!(inst.kind, FeatureKind::Instructions);
    }

    #[test]
    fn hooks_json_dispatches_to_match_hook() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.claude/hooks/hooks.json");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Hook);
    }

    #[test]
    fn copilot_root_hooks_json_dispatches_to_match_hook() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.github/hooks.json");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Hook);
        assert_eq!(feat.engine, Engine::Copilot);
    }

    #[test]
    fn aipm_toml_dispatches_to_match_plugin() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.ai/my-plugin/aipm.toml");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Plugin);
        assert_eq!(feat.engine, Engine::Ai);
    }

    #[test]
    fn marketplace_json_dispatches_to_match_marketplace() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.ai/.claude-plugin/marketplace.json");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Marketplace);
    }

    #[test]
    fn plugin_json_dispatches_to_match_plugin_json() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.ai/my-plugin/.claude-plugin/plugin.json");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::PluginJson);
    }

    #[test]
    fn agent_md_dispatches_to_match_agent() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.claude/agents/my-agent.md");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Agent);
        assert_eq!(feat.engine, Engine::Claude);
    }

    // --- engine attribution ---

    #[test]
    fn copilot_skill_carries_copilot_engine() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.github/skills/my-skill/SKILL.md");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.engine, Engine::Copilot);
    }

    #[test]
    fn ai_plugin_carries_ai_engine() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.ai/my-plugin/skills/my-skill/SKILL.md");
        let feat = classify_at(&path, &root).expect("should match");
        assert_eq!(feat.engine, Engine::Ai);
        assert_eq!(feat.layout, Layout::AiPlugin);
    }

    // --- negative paths that look feature-like but don't match ---

    #[test]
    fn skill_md_under_engine_root_with_no_skills_ancestor_no_match() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.github/SKILL.md");
        assert!(classify_at(&path, &root).is_none());
    }

    #[test]
    fn random_md_outside_agents_dir_no_match() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.claude/notes/random.md");
        assert!(classify_at(&path, &root).is_none());
    }

    #[test]
    fn aipm_toml_outside_ai_no_match() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.claude/some-pkg/aipm.toml");
        // No engine root that starts with .ai — engine_root will resolve to .claude,
        // and match_plugin requires grandparent == .ai. So None.
        assert!(classify_at(&path, &root).is_none());
    }
}
