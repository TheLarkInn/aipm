//! Core feature types for the unified discovery module.
//!
//! Defines the kinds of AI plugin features that can be discovered, the engine
//! they belong to, the layout shape under which they were found, and the
//! `DiscoveredFeature` struct that carries all of that together.

use std::path::PathBuf;

/// The kind of AI plugin feature discovered.
///
/// Moved here from the legacy `discovery.rs` to live alongside the other
/// foundation types. Re-exported from `crate::discovery` for backwards
/// compatibility with existing call sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureKind {
    /// A skill file (`SKILL.md` inside a `skills/` directory).
    Skill,
    /// An agent file (`*.md` inside an `agents/` directory).
    Agent,
    /// A hook file (`hooks.json` inside a `hooks/` directory).
    Hook,
    /// A plugin manifest (`aipm.toml` inside a `.ai/<plugin>/` directory).
    Plugin,
    /// A marketplace manifest (`marketplace.json` at `.ai/.claude-plugin/marketplace.json`).
    Marketplace,
    /// A plugin JSON manifest (`plugin.json` at `.ai/<plugin>/.claude-plugin/plugin.json`).
    PluginJson,
    /// An instruction file (CLAUDE.md, AGENTS.md, COPILOT.md, INSTRUCTIONS.md, GEMINI.md, or
    /// `*.instructions.md`) anywhere in the project tree.
    Instructions,
}

/// The engine root a discovered feature belongs to.
///
/// `Ai` is the marketplace root, not an authoring engine — it's distinguished
/// here so that `.ai/<plugin>/` content can be classified differently from
/// `.claude/` or `.github/` source content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// The Claude engine — features under `.claude/`.
    Claude,
    /// The GitHub Copilot engine — features under `.github/`.
    Copilot,
    /// The marketplace root — features under `.ai/`.
    Ai,
}

/// The layout shape under which a skill (or other feature) was discovered.
///
/// Distinguishes the three supported skill layouts:
/// - `Canonical`: `<root>/skills/<name>/SKILL.md` (e.g. `.claude/skills/`, `.github/skills/`).
/// - `CopilotSubroot`: `<root>/copilot/<name>/SKILL.md` (legacy aipm accommodation).
/// - `CopilotSubrootWithSkills`: `<root>/copilot/skills/<name>/SKILL.md` (issue #725 layout).
///
/// Note: `.ai/<plugin>/.claude/...` paths are classified by the innermost
/// engine root (the inner `.claude`), so they take the corresponding
/// `Canonical`/`CopilotSubroot*` shape under that inner root rather than a
/// dedicated `.ai/` plugin variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// `<root>/skills/<name>/SKILL.md` and `<root>/agents/<name>.md` shapes.
    Canonical,
    /// `<root>/copilot/<name>/SKILL.md` — aipm's legacy Copilot accommodation.
    CopilotSubroot,
    /// `<root>/copilot/skills/<name>/SKILL.md` — issue #725 layout.
    CopilotSubrootWithSkills,
}

/// A discovered AI plugin feature file along with the engine, layout, and root
/// directory context derived from its path.
///
/// This is the new shape the unified discovery module produces. Today's lint
/// callers continue to use the legacy `DiscoveredFeature` re-exported from
/// `crate::discovery_legacy` until the lint integration feature switches them
/// over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFeature {
    /// The kind of feature.
    pub kind: FeatureKind,
    /// The engine root this feature belongs to.
    pub engine: Engine,
    /// The layout under which the feature was matched.
    pub layout: Layout,
    /// The engine source root directory (e.g. `.github`, `.claude`, `.ai`).
    pub source_root: PathBuf,
    /// The directory containing the feature, when applicable (e.g. the `<name>/`
    /// directory that contains `SKILL.md`). `None` for instruction files and
    /// other features that have no enclosing feature directory.
    pub feature_dir: Option<PathBuf>,
    /// The path to the actual feature file (e.g. the `SKILL.md`, the agent
    /// `.md`, the `hooks.json`, etc.).
    pub path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn feature_kind_variants_are_distinct() {
        assert_ne!(FeatureKind::Skill, FeatureKind::Agent);
        assert_ne!(FeatureKind::Hook, FeatureKind::Plugin);
        assert_ne!(FeatureKind::Marketplace, FeatureKind::PluginJson);
        assert_ne!(FeatureKind::Instructions, FeatureKind::Skill);
    }

    #[test]
    fn feature_kind_clone_eq() {
        let kind = FeatureKind::Skill;
        let cloned = kind.clone();
        assert_eq!(kind, cloned);
    }

    #[test]
    fn engine_variants_are_distinct() {
        assert_ne!(Engine::Claude, Engine::Copilot);
        assert_ne!(Engine::Copilot, Engine::Ai);
        assert_ne!(Engine::Claude, Engine::Ai);
    }

    #[test]
    fn engine_is_copy() {
        let e = Engine::Copilot;
        let copied = e;
        assert_eq!(e, copied);
    }

    #[test]
    fn layout_variants_are_distinct() {
        assert_ne!(Layout::Canonical, Layout::CopilotSubroot);
        assert_ne!(Layout::CopilotSubroot, Layout::CopilotSubrootWithSkills);
        assert_ne!(Layout::Canonical, Layout::CopilotSubrootWithSkills);
    }

    #[test]
    fn layout_is_copy() {
        let l = Layout::CopilotSubrootWithSkills;
        let copied = l;
        assert_eq!(l, copied);
    }

    #[test]
    fn discovered_feature_construction() {
        let feat = DiscoveredFeature {
            kind: FeatureKind::Skill,
            engine: Engine::Copilot,
            layout: Layout::CopilotSubrootWithSkills,
            source_root: PathBuf::from(".github"),
            feature_dir: Some(PathBuf::from(".github/copilot/skills/skill-alpha")),
            path: PathBuf::from(".github/copilot/skills/skill-alpha/SKILL.md"),
        };
        assert_eq!(feat.kind, FeatureKind::Skill);
        assert_eq!(feat.engine, Engine::Copilot);
        assert_eq!(feat.layout, Layout::CopilotSubrootWithSkills);
        assert_eq!(feat.source_root, PathBuf::from(".github"));
        assert!(feat.feature_dir.is_some());
        assert_eq!(feat.path.file_name().and_then(|n| n.to_str()), Some("SKILL.md"));
    }

    #[test]
    fn discovered_feature_clone_and_eq() {
        let feat = DiscoveredFeature {
            kind: FeatureKind::Instructions,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".github"),
            feature_dir: None,
            path: PathBuf::from(".github/copilot/copilot-instructions.md"),
        };
        let cloned = feat.clone();
        assert_eq!(feat, cloned);
    }

    #[test]
    fn discovered_feature_with_no_feature_dir() {
        let feat = DiscoveredFeature {
            kind: FeatureKind::Instructions,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".claude"),
            feature_dir: None,
            path: PathBuf::from(".claude/CLAUDE.md"),
        };
        assert!(feat.feature_dir.is_none());
    }
}
