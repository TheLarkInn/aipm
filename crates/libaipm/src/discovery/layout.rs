//! Layout grammar for the unified discovery module.
//!
//! Translates a candidate file path into a [`DiscoveredFeature`] when the
//! path matches a recognized layout shape under a given engine source root.
//!
//! The skill grammar is the heart of the issue #725 fix: a `SKILL.md` matches
//! when (A) any ancestor between the file's parent and the engine source root
//! is literally named `skills`, OR (B) the engine is Copilot and the file's
//! grandparent is named `copilot` (the legacy `.github/copilot/<name>/SKILL.md`
//! accommodation already supported by the migrate detector).
//!
//! Also provides matchers for the non-skill feature kinds:
//! `match_agent` (any `<...>/agents/*.md`), `match_hook` (`hooks.json` under
//! `hooks/` or directly under the source root), `match_plugin`
//! (`<.ai>/<plugin>/aipm.toml`), `match_marketplace`
//! (`<.ai>/.claude-plugin/marketplace.json`), and `match_plugin_json`
//! (`<.ai>/<plugin>/.claude-plugin/plugin.json`).
//!
//! All non-skill matchers return [`Layout::Canonical`] (skill is the only
//! feature kind whose layout shape varies). The matchers do not classify
//! instruction files — that lives in `discovery::instruction` (added in a
//! later spec feature).

use std::ffi::OsString;
use std::path::Path;

use super::types::{DiscoveredFeature, Engine, FeatureKind, Layout};

/// Try to match `path` as a skill (`SKILL.md`) under the given `engine` and
/// `source_root`.
///
/// Returns `Some(DiscoveredFeature)` if the path matches one of the supported
/// layouts:
///
/// | Layout | Path shape under `source_root` |
/// |---|---|
/// | [`Layout::Canonical`] | `<root>/skills/<name>/SKILL.md` (also flat `<root>/skills/SKILL.md`) |
/// | [`Layout::CopilotSubroot`] | `<root>/copilot/<name>/SKILL.md` (Copilot only) |
/// | [`Layout::CopilotSubrootWithSkills`] | `<root>/copilot/skills/<name>/SKILL.md` (issue #725) |
/// | [`Layout::AiPlugin`] | `<root>/<plugin>/skills/<name>/SKILL.md` (Ai engine, post-migrate) |
/// | [`Layout::AiNested`] | `<root>/<plugin>/<.engine>/skills/<name>/SKILL.md` (Ai engine, nested) |
///
/// Returns `None` for paths that don't match any known shape.
#[must_use]
pub fn match_skill(path: &Path, engine: Engine, source_root: &Path) -> Option<DiscoveredFeature> {
    let ancestors = collect_ancestors(path, source_root);

    let any_skills = ancestors.iter().any(|n| n.to_string_lossy() == "skills");
    let grandparent_is_copilot = ancestors.get(1).is_some_and(|n| n.to_string_lossy() == "copilot");

    if !any_skills {
        // Case B: only Copilot's `.github/copilot/<name>/SKILL.md` accommodation
        // qualifies when there is no `skills` ancestor.
        if !(engine == Engine::Copilot && grandparent_is_copilot) {
            return None;
        }
    }

    let layout = pick_layout_for_skill(&ancestors, engine);
    Some(DiscoveredFeature {
        kind: FeatureKind::Skill,
        engine,
        layout,
        source_root: source_root.to_path_buf(),
        feature_dir: path.parent().map(Path::to_path_buf),
        path: path.to_path_buf(),
    })
}

/// Collect the `file_name()` of each ancestor between `path`'s parent (inclusive)
/// and `source_root` (exclusive), in order from closest-to-leaf to closest-to-root.
fn collect_ancestors(path: &Path, source_root: &Path) -> Vec<OsString> {
    path.ancestors()
        .skip(1) // skip the file itself
        .take_while(|a| *a != source_root)
        .filter_map(|a| a.file_name().map(std::ffi::OsStr::to_os_string))
        .collect()
}

/// Decide which [`Layout`] best describes a skill given its ancestors slice
/// and the engine.
///
/// Pattern arms are ordered most-specific to least-specific so that deeper
/// matches win over their prefixes.
fn pick_layout_for_skill(ancestors: &[OsString], engine: Engine) -> Layout {
    let names: Vec<String> = ancestors.iter().map(|n| n.to_string_lossy().into_owned()).collect();
    let slice: Vec<&str> = names.iter().map(String::as_str).collect();
    match slice.as_slice() {
        // Customer's #725 layout: <root>/copilot/skills/<name>/SKILL.md
        [_, s, c, ..] if *s == "skills" && *c == "copilot" => Layout::CopilotSubrootWithSkills,
        // <root>/copilot/<name>/SKILL.md
        [_, c, ..] if *c == "copilot" => Layout::CopilotSubroot,
        // .ai/<plugin>/<.engine>/skills/<name>/SKILL.md (nested authoring)
        [_, s, dot, _plugin, ..] if *s == "skills" && dot.starts_with('.') => Layout::AiNested,
        // .ai/<plugin>/skills/<name>/SKILL.md (post-migrate flat)
        [_, s, _plugin, ..] if *s == "skills" && engine == Engine::Ai => Layout::AiPlugin,
        // .claude/skills/<name>/SKILL.md, .github/skills/<name>/SKILL.md, flat etc.
        _ => Layout::Canonical,
    }
}

/// Try to match `path` as an agent (`*.md` inside an `agents/` directory).
///
/// Mirrors today's `discovery_legacy::classify_feature_kind` agent branch —
/// any `.md` file whose parent directory is named `agents` qualifies. Note
/// that instruction files (e.g. `agents/AGENTS.md`) are intentionally NOT
/// rejected here; the dispatcher (`discovery::classify`, in a later spec
/// feature) is responsible for ordering instruction-file detection BEFORE
/// agent detection.
#[must_use]
pub fn match_agent(path: &Path, engine: Engine, source_root: &Path) -> Option<DiscoveredFeature> {
    let file_name = path.file_name()?.to_string_lossy();
    if !file_name.ends_with(".md") {
        return None;
    }
    let parent_name = parent_name_lossy(path)?;
    if parent_name != "agents" {
        return None;
    }
    Some(DiscoveredFeature {
        kind: FeatureKind::Agent,
        engine,
        layout: Layout::Canonical,
        source_root: source_root.to_path_buf(),
        feature_dir: path.parent().map(Path::to_path_buf),
        path: path.to_path_buf(),
    })
}

/// Try to match `path` as a hook (`hooks.json`).
///
/// Accepts both shapes:
/// - `<source_root>/hooks.json` (the `CopilotHookDetector` accommodation).
/// - `<...>/hooks/hooks.json` (today's `classify_feature_kind` rule — broad
///   match wherever a `hooks/` parent appears).
#[must_use]
pub fn match_hook(path: &Path, engine: Engine, source_root: &Path) -> Option<DiscoveredFeature> {
    let file_name = path.file_name()?.to_string_lossy();
    if file_name != "hooks.json" {
        return None;
    }
    let parent = path.parent()?;
    let parent_is_root = parent == source_root;
    let parent_is_hooks = parent.file_name().is_some_and(|n| n.to_string_lossy() == "hooks");
    if !(parent_is_root || parent_is_hooks) {
        return None;
    }
    Some(DiscoveredFeature {
        kind: FeatureKind::Hook,
        engine,
        layout: Layout::Canonical,
        source_root: source_root.to_path_buf(),
        feature_dir: path.parent().map(Path::to_path_buf),
        path: path.to_path_buf(),
    })
}

/// Try to match `path` as a plugin manifest (`aipm.toml`).
///
/// Accepts `<.ai>/<plugin>/aipm.toml` — the grandparent must be named `.ai`.
/// Mirrors today's `classify_feature_kind` Plugin branch.
#[must_use]
pub fn match_plugin(path: &Path, engine: Engine, source_root: &Path) -> Option<DiscoveredFeature> {
    let file_name = path.file_name()?.to_string_lossy();
    if file_name != "aipm.toml" {
        return None;
    }
    let grandparent = ancestor_name_lossy(path, 2)?;
    if grandparent != ".ai" {
        return None;
    }
    Some(DiscoveredFeature {
        kind: FeatureKind::Plugin,
        engine,
        layout: Layout::Canonical,
        source_root: source_root.to_path_buf(),
        feature_dir: path.parent().map(Path::to_path_buf),
        path: path.to_path_buf(),
    })
}

/// Try to match `path` as a marketplace manifest (`marketplace.json`).
///
/// Accepts `<.ai>/.claude-plugin/marketplace.json` — parent must be
/// `.claude-plugin` and grandparent must be `.ai`. Mirrors today's
/// `classify_feature_kind` Marketplace branch.
#[must_use]
pub fn match_marketplace(
    path: &Path,
    engine: Engine,
    source_root: &Path,
) -> Option<DiscoveredFeature> {
    let file_name = path.file_name()?.to_string_lossy();
    if file_name != "marketplace.json" {
        return None;
    }
    let parent_name = parent_name_lossy(path)?;
    if parent_name != ".claude-plugin" {
        return None;
    }
    let grandparent = ancestor_name_lossy(path, 2)?;
    if grandparent != ".ai" {
        return None;
    }
    Some(DiscoveredFeature {
        kind: FeatureKind::Marketplace,
        engine,
        layout: Layout::Canonical,
        source_root: source_root.to_path_buf(),
        feature_dir: path.parent().map(Path::to_path_buf),
        path: path.to_path_buf(),
    })
}

/// Try to match `path` as a plugin JSON manifest (`plugin.json`).
///
/// Accepts `<.ai>/<plugin>/.claude-plugin/plugin.json` — parent must be
/// `.claude-plugin` and great-grandparent must be `.ai`. Mirrors today's
/// `classify_feature_kind` `PluginJson` branch.
#[must_use]
pub fn match_plugin_json(
    path: &Path,
    engine: Engine,
    source_root: &Path,
) -> Option<DiscoveredFeature> {
    let file_name = path.file_name()?.to_string_lossy();
    if file_name != "plugin.json" {
        return None;
    }
    let parent_name = parent_name_lossy(path)?;
    if parent_name != ".claude-plugin" {
        return None;
    }
    let great_grandparent = ancestor_name_lossy(path, 3)?;
    if great_grandparent != ".ai" {
        return None;
    }
    Some(DiscoveredFeature {
        kind: FeatureKind::PluginJson,
        engine,
        layout: Layout::Canonical,
        source_root: source_root.to_path_buf(),
        feature_dir: path.parent().map(Path::to_path_buf),
        path: path.to_path_buf(),
    })
}

/// Lossy `file_name()` of `path`'s parent directory.
fn parent_name_lossy(path: &Path) -> Option<String> {
    path.parent().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned())
}

/// Lossy `file_name()` of the n-th ancestor of `path` (1 = parent, 2 = grandparent, …).
fn ancestor_name_lossy(path: &Path, depth: usize) -> Option<String> {
    let mut current: Option<&Path> = Some(path);
    for _ in 0..depth {
        current = current.and_then(Path::parent);
    }
    current.and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- positive: each of the five layouts ---

    #[test]
    fn canonical_layout_claude() {
        let root = PathBuf::from("/repo/.claude");
        let path = PathBuf::from("/repo/.claude/skills/my-skill/SKILL.md");
        let feat = match_skill(&path, Engine::Claude, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Skill);
        assert_eq!(feat.engine, Engine::Claude);
        assert_eq!(feat.layout, Layout::Canonical);
        assert_eq!(feat.source_root, root);
        assert_eq!(feat.feature_dir, Some(PathBuf::from("/repo/.claude/skills/my-skill")));
        assert_eq!(feat.path, path);
    }

    #[test]
    fn canonical_layout_copilot() {
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from("/repo/.github/skills/my-skill/SKILL.md");
        let feat = match_skill(&path, Engine::Copilot, &root).expect("should match");
        assert_eq!(feat.engine, Engine::Copilot);
        assert_eq!(feat.layout, Layout::Canonical);
    }

    #[test]
    fn copilot_subroot_layout() {
        // .github/copilot/<name>/SKILL.md — the existing aipm accommodation.
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from("/repo/.github/copilot/my-skill/SKILL.md");
        let feat = match_skill(&path, Engine::Copilot, &root).expect("should match");
        assert_eq!(feat.engine, Engine::Copilot);
        assert_eq!(feat.layout, Layout::CopilotSubroot);
    }

    #[test]
    fn copilot_subroot_with_skills_layout_issue_725() {
        // The exact #725 customer layout: .github/copilot/skills/<name>/SKILL.md
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from("/repo/.github/copilot/skills/skill-alpha/SKILL.md");
        let feat = match_skill(&path, Engine::Copilot, &root).expect("should match");
        assert_eq!(feat.engine, Engine::Copilot);
        assert_eq!(feat.layout, Layout::CopilotSubrootWithSkills);
        assert_eq!(
            feat.feature_dir,
            Some(PathBuf::from("/repo/.github/copilot/skills/skill-alpha"))
        );
    }

    #[test]
    fn issue_725_all_three_skills_match() {
        let root = PathBuf::from("/repo/.github");
        for name in ["skill-alpha", "skill-beta", "skill-gamma"] {
            let path = PathBuf::from(format!("/repo/.github/copilot/skills/{name}/SKILL.md"));
            let feat = match_skill(&path, Engine::Copilot, &root).expect("skill should match");
            assert_eq!(feat.layout, Layout::CopilotSubrootWithSkills);
        }
    }

    #[test]
    fn ai_plugin_layout() {
        let root = PathBuf::from("/repo/.ai");
        let path = PathBuf::from("/repo/.ai/my-plugin/skills/my-skill/SKILL.md");
        let feat = match_skill(&path, Engine::Ai, &root).expect("should match");
        assert_eq!(feat.engine, Engine::Ai);
        assert_eq!(feat.layout, Layout::AiPlugin);
    }

    #[test]
    fn ai_nested_layout() {
        let root = PathBuf::from("/repo/.ai");
        let path = PathBuf::from("/repo/.ai/my-plugin/.claude/skills/my-skill/SKILL.md");
        let feat = match_skill(&path, Engine::Ai, &root).expect("should match");
        assert_eq!(feat.engine, Engine::Ai);
        assert_eq!(feat.layout, Layout::AiNested);
    }

    // --- positive: flat layout still works ---

    #[test]
    fn flat_skills_layout_canonical() {
        // .github/skills/SKILL.md — today's grandparent==skills branch (without
        // a per-skill name dir). Single ancestor: "skills".
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from("/repo/.github/skills/SKILL.md");
        let feat = match_skill(&path, Engine::Copilot, &root).expect("should match");
        assert_eq!(feat.layout, Layout::Canonical);
    }

    // --- negative: spec-listed cases ---

    #[test]
    fn skill_md_at_engine_root_no_match() {
        // .github/SKILL.md — no skills ancestor, no copilot grandparent.
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from("/repo/.github/SKILL.md");
        assert!(match_skill(&path, Engine::Copilot, &root).is_none());
    }

    #[test]
    fn skill_md_under_copilot_directly_no_match() {
        // .github/copilot/SKILL.md (no per-skill dir AND no skills ancestor) —
        // ancestors = [copilot]. ancestors.get(1) == None so Case B fails.
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from("/repo/.github/copilot/SKILL.md");
        assert!(match_skill(&path, Engine::Copilot, &root).is_none());
    }

    #[test]
    fn copilot_subroot_only_works_for_copilot_engine() {
        // Even if path looks like .github/copilot/<name>/SKILL.md, Case B
        // applies only when engine == Engine::Copilot.
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from("/repo/.github/copilot/my-skill/SKILL.md");
        // Same path with engine=Claude — must not match (Case B is engine-gated).
        assert!(match_skill(&path, Engine::Claude, &root).is_none());
    }

    #[test]
    fn skill_with_no_skills_ancestor_no_match_for_non_copilot() {
        // Path under .claude has no `skills` ancestor and no Copilot Case B
        // accommodation — must return None.
        let root = PathBuf::from("/repo/.claude");
        let path = PathBuf::from("/repo/.claude/notes/random/SKILL.md");
        assert!(match_skill(&path, Engine::Claude, &root).is_none());
    }

    // --- positive: tricky shapes ---

    #[test]
    fn deeply_nested_skills_under_copilot() {
        // .github/copilot/skills/group/<name>/SKILL.md — extra dir between
        // skills and the skill name. The ancestors slice is [name, group,
        // skills, copilot] — `s` is "group" (not "skills") at index 1, but the
        // skills ancestor still exists at index 2.
        //
        // Per Case A this is matched (any ancestor named "skills"), but the
        // pick_layout pattern arms target [_, s, c, ..] expecting s=="skills"
        // exactly at index 1. So the layout falls through to Canonical, which
        // is acceptable: the feature is discovered, the layout label is just
        // the conservative default.
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from("/repo/.github/copilot/skills/group/my-skill/SKILL.md");
        let feat = match_skill(&path, Engine::Copilot, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Skill);
        assert_eq!(feat.engine, Engine::Copilot);
    }

    #[test]
    fn ai_plugin_only_for_ai_engine() {
        // For .ai/<plugin>/skills/<name>/SKILL.md called with engine=Claude
        // (e.g. caller mis-attributed engine), the AiPlugin arm fails its
        // engine guard → falls through to Canonical.
        let root = PathBuf::from("/repo/.ai");
        let path = PathBuf::from("/repo/.ai/my-plugin/skills/my-skill/SKILL.md");
        let feat = match_skill(&path, Engine::Claude, &root).expect("should match");
        // Falls through to Canonical because the AiPlugin arm requires Ai engine.
        assert_eq!(feat.layout, Layout::Canonical);
    }

    #[test]
    fn ai_nested_does_not_require_specific_engine() {
        // The AiNested arm doesn't gate on engine — it gates on a dot-prefixed
        // ancestor. Verify: with engine=Claude (still pretty unusual), it
        // still picks AiNested if the shape matches.
        let root = PathBuf::from("/repo/.ai");
        let path = PathBuf::from("/repo/.ai/my-plugin/.claude/skills/my-skill/SKILL.md");
        let feat = match_skill(&path, Engine::Claude, &root).expect("should match");
        assert_eq!(feat.layout, Layout::AiNested);
    }

    // --- structural fields ---

    #[test]
    fn returns_correct_feature_dir() {
        let root = PathBuf::from("/repo/.claude");
        let path = PathBuf::from("/repo/.claude/skills/my-skill/SKILL.md");
        let feat = match_skill(&path, Engine::Claude, &root).expect("should match");
        assert_eq!(feat.feature_dir, Some(PathBuf::from("/repo/.claude/skills/my-skill")));
    }

    #[test]
    fn returns_correct_path_unchanged() {
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from("/repo/.github/copilot/skills/skill-alpha/SKILL.md");
        let feat = match_skill(&path, Engine::Copilot, &root).expect("should match");
        assert_eq!(feat.path, path);
    }

    // --- match_agent ---

    #[test]
    fn agent_canonical_layout() {
        let root = PathBuf::from("/repo/.claude");
        let path = PathBuf::from("/repo/.claude/agents/my-agent.md");
        let feat = match_agent(&path, Engine::Claude, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Agent);
        assert_eq!(feat.engine, Engine::Claude);
        assert_eq!(feat.layout, Layout::Canonical);
        assert_eq!(feat.source_root, root);
        assert_eq!(feat.feature_dir, Some(PathBuf::from("/repo/.claude/agents")));
        assert_eq!(feat.path, path);
    }

    #[test]
    fn agent_under_github_agents() {
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from("/repo/.github/agents/my-agent.md");
        let feat = match_agent(&path, Engine::Copilot, &root).expect("should match");
        assert_eq!(feat.engine, Engine::Copilot);
    }

    #[test]
    fn agent_non_md_extension_no_match() {
        let root = PathBuf::from("/repo/.claude");
        let path = PathBuf::from("/repo/.claude/agents/my-agent.txt");
        assert!(match_agent(&path, Engine::Claude, &root).is_none());
    }

    #[test]
    fn agent_wrong_parent_no_match() {
        let root = PathBuf::from("/repo/.claude");
        let path = PathBuf::from("/repo/.claude/skills/my-agent.md");
        assert!(match_agent(&path, Engine::Claude, &root).is_none());
    }

    #[test]
    fn agent_no_parent_no_match() {
        let root = PathBuf::from("/");
        let path = PathBuf::from("agent.md");
        assert!(match_agent(&path, Engine::Claude, &root).is_none());
    }

    // --- match_hook ---

    #[test]
    fn hook_under_hooks_subdir() {
        let root = PathBuf::from("/repo/.claude");
        let path = PathBuf::from("/repo/.claude/hooks/hooks.json");
        let feat = match_hook(&path, Engine::Claude, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Hook);
        assert_eq!(feat.engine, Engine::Claude);
        assert_eq!(feat.layout, Layout::Canonical);
    }

    #[test]
    fn hook_directly_under_root_copilot_accommodation() {
        // The CopilotHookDetector pattern: .github/hooks.json directly under root.
        let root = PathBuf::from("/repo/.github");
        let path = PathBuf::from("/repo/.github/hooks.json");
        let feat = match_hook(&path, Engine::Copilot, &root).expect("should match");
        assert_eq!(feat.engine, Engine::Copilot);
    }

    #[test]
    fn hook_wrong_filename_no_match() {
        let root = PathBuf::from("/repo/.claude");
        let path = PathBuf::from("/repo/.claude/hooks/other.json");
        assert!(match_hook(&path, Engine::Claude, &root).is_none());
    }

    #[test]
    fn hook_unrelated_parent_no_match() {
        // hooks.json sitting inside .claude/skills/ — not a hook (parent isn't
        // "hooks" and parent isn't the source_root).
        let root = PathBuf::from("/repo/.claude");
        let path = PathBuf::from("/repo/.claude/skills/hooks.json");
        assert!(match_hook(&path, Engine::Claude, &root).is_none());
    }

    // --- match_plugin (aipm.toml) ---

    #[test]
    fn plugin_under_ai_directory() {
        let root = PathBuf::from("/repo/.ai");
        let path = PathBuf::from("/repo/.ai/my-plugin/aipm.toml");
        let feat = match_plugin(&path, Engine::Ai, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Plugin);
        assert_eq!(feat.layout, Layout::Canonical);
        assert_eq!(feat.feature_dir, Some(PathBuf::from("/repo/.ai/my-plugin")));
    }

    #[test]
    fn plugin_outside_ai_directory_no_match() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/some-other-plugin/aipm.toml");
        assert!(match_plugin(&path, Engine::Ai, &root).is_none());
    }

    #[test]
    fn plugin_wrong_filename_no_match() {
        let root = PathBuf::from("/repo/.ai");
        let path = PathBuf::from("/repo/.ai/my-plugin/Cargo.toml");
        assert!(match_plugin(&path, Engine::Ai, &root).is_none());
    }

    #[test]
    fn plugin_at_ai_root_no_match() {
        // .ai/aipm.toml — grandparent is the project root, not .ai.
        let root = PathBuf::from("/repo/.ai");
        let path = PathBuf::from("/repo/.ai/aipm.toml");
        assert!(match_plugin(&path, Engine::Ai, &root).is_none());
    }

    // --- match_marketplace ---

    #[test]
    fn marketplace_at_canonical_path() {
        let root = PathBuf::from("/repo/.ai");
        let path = PathBuf::from("/repo/.ai/.claude-plugin/marketplace.json");
        let feat = match_marketplace(&path, Engine::Ai, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::Marketplace);
        assert_eq!(feat.layout, Layout::Canonical);
    }

    #[test]
    fn marketplace_wrong_parent_no_match() {
        let root = PathBuf::from("/repo/.ai");
        let path = PathBuf::from("/repo/.ai/some-other-dir/marketplace.json");
        assert!(match_marketplace(&path, Engine::Ai, &root).is_none());
    }

    #[test]
    fn marketplace_grandparent_not_ai_no_match() {
        let root = PathBuf::from("/repo");
        let path = PathBuf::from("/repo/.notai/.claude-plugin/marketplace.json");
        assert!(match_marketplace(&path, Engine::Ai, &root).is_none());
    }

    // --- match_plugin_json ---

    #[test]
    fn plugin_json_at_canonical_path() {
        let root = PathBuf::from("/repo/.ai");
        let path = PathBuf::from("/repo/.ai/my-plugin/.claude-plugin/plugin.json");
        let feat = match_plugin_json(&path, Engine::Ai, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::PluginJson);
        assert_eq!(feat.layout, Layout::Canonical);
    }

    #[test]
    fn plugin_json_wrong_parent_no_match() {
        let root = PathBuf::from("/repo/.ai");
        let path = PathBuf::from("/repo/.ai/my-plugin/elsewhere/plugin.json");
        assert!(match_plugin_json(&path, Engine::Ai, &root).is_none());
    }

    #[test]
    fn plugin_json_great_grandparent_not_ai_no_match() {
        // Without .ai as great-grandparent, this matches the marketplace's
        // plugin.json shape but not the plugin's. Today's behavior excludes it.
        let root = PathBuf::from("/repo/.ai");
        let path = PathBuf::from("/repo/.ai/.claude-plugin/plugin.json");
        // Here parent=.claude-plugin (good), grandparent=.ai (good for marketplace),
        // great-grandparent=/repo (NOT .ai). So this is rejected by match_plugin_json.
        assert!(match_plugin_json(&path, Engine::Ai, &root).is_none());
    }

    #[test]
    fn plugin_json_correct_great_grandparent() {
        let root = PathBuf::from("/host/.ai");
        let path = PathBuf::from("/host/.ai/my-plugin/.claude-plugin/plugin.json");
        let feat = match_plugin_json(&path, Engine::Ai, &root).expect("should match");
        assert_eq!(feat.kind, FeatureKind::PluginJson);
    }
}
