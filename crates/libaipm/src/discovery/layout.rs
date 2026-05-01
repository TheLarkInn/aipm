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
//! Subsequent features add `match_agent`, `match_hook`, `match_plugin`,
//! `match_marketplace`, and `match_plugin_json`. This file currently only
//! contains `match_skill` — keeping each step PR-sized per the spec.

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
}
