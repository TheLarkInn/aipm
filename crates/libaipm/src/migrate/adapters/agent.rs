//! Agent adapters for the unified migrate pipeline.
//!
//! Holds both the Copilot and Claude agent adapters. They share most of
//! the parsing logic and differ in:
//! - Copilot: strips `.agent.md` suffix when computing default name from
//!   filename; produces empty `files` and `referenced_scripts`.
//! - Claude: relative-to-agents-dir filename in `files`; extracts script
//!   references with `${CLAUDE_AGENT_DIR}/` prefix.
//!
//! Note: the legacy `CopilotAgentDetector` dedupes `foo.md` and
//! `foo.agent.md` to a single artifact. The unified pipeline produces one
//! [`DiscoveredFeature`] per file, so the orchestrator (or a future
//! discovery-level dedup) is responsible for handling that case. These
//! adapters operate on a single feature at a time.

use std::path::{Path, PathBuf};

use crate::discovery::{DiscoveredFeature, Engine, FeatureKind};
use crate::fs::Fs;
use crate::migrate::skill_common;
use crate::migrate::{Artifact, ArtifactKind, Error};

use super::Adapter;

/// Adapter for Copilot agents (`<.github>/agents/*.md`).
pub struct CopilotAgentAdapter;

impl Adapter for CopilotAgentAdapter {
    fn name(&self) -> &'static str {
        "copilot-agent"
    }

    fn applies_to(&self, feat: &DiscoveredFeature) -> bool {
        feat.engine == Engine::Copilot && feat.kind == FeatureKind::Agent
    }

    fn to_artifact(&self, feat: &DiscoveredFeature, fs: &dyn Fs) -> Result<Artifact, Error> {
        let content = fs.read_to_string(&feat.path)?;
        let metadata = skill_common::parse_frontmatter(&content, &feat.path)?;

        // Name precedence: frontmatter `name` field, else the file stem
        // (sans the `.agent.md` suffix when present).
        let file_name =
            feat.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let stem_from_filename = if has_agent_md_suffix(&file_name) {
            file_name[..file_name.len() - ".agent.md".len()].to_string()
        } else {
            Path::new(&file_name)
                .file_stem()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
        };
        let name = metadata.name.clone().unwrap_or(stem_from_filename);

        Ok(Artifact {
            kind: ArtifactKind::Agent,
            name,
            source_path: feat.path.clone(),
            files: Vec::new(),
            referenced_scripts: Vec::new(),
            metadata,
        })
    }
}

/// `true` if `name` ends with `.agent.md` (case-insensitive).
fn has_agent_md_suffix(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".agent.md")
}

/// Adapter for Claude agents (`.claude/agents/<name>.md`).
///
/// Replaces the filesystem-walking body of
/// [`crate::migrate::agent_detector::AgentDetector`] with a per-feature
/// transformation. Differs from [`CopilotAgentAdapter`] by recording the
/// agent filename in `files` (relative to the agents directory) and by
/// extracting script references with the `${CLAUDE_AGENT_DIR}/` prefix.
pub struct ClaudeAgentAdapter;

impl Adapter for ClaudeAgentAdapter {
    fn name(&self) -> &'static str {
        "claude-agent"
    }

    fn applies_to(&self, feat: &DiscoveredFeature) -> bool {
        feat.engine == Engine::Claude && feat.kind == FeatureKind::Agent
    }

    fn to_artifact(&self, feat: &DiscoveredFeature, fs: &dyn Fs) -> Result<Artifact, Error> {
        let content = fs.read_to_string(&feat.path)?;
        let metadata = skill_common::parse_frontmatter(&content, &feat.path)?;

        let file_name =
            feat.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let stem_from_filename = Path::new(&file_name)
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        let name = metadata.name.clone().unwrap_or(stem_from_filename);

        let referenced_scripts =
            skill_common::extract_script_references(&content, "${CLAUDE_AGENT_DIR}/");

        Ok(Artifact {
            kind: ArtifactKind::Agent,
            name,
            source_path: feat.path.clone(),
            files: vec![PathBuf::from(file_name)],
            referenced_scripts,
            metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::Layout;
    use crate::fs::Real;
    use std::path::PathBuf;

    #[test]
    fn applies_to_copilot_agent_only() {
        let adapter = CopilotAgentAdapter;
        let agent = DiscoveredFeature {
            kind: FeatureKind::Agent,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".github"),
            feature_dir: Some(PathBuf::from(".github/agents")),
            path: PathBuf::from(".github/agents/my-agent.md"),
        };
        assert!(adapter.applies_to(&agent));
    }

    #[test]
    fn rejects_claude_agent() {
        let adapter = CopilotAgentAdapter;
        let agent = DiscoveredFeature {
            kind: FeatureKind::Agent,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".claude"),
            feature_dir: Some(PathBuf::from(".claude/agents")),
            path: PathBuf::from(".claude/agents/my-agent.md"),
        };
        assert!(!adapter.applies_to(&agent));
    }

    #[test]
    fn rejects_copilot_skill() {
        let adapter = CopilotAgentAdapter;
        let skill = DiscoveredFeature {
            kind: FeatureKind::Skill,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".github"),
            feature_dir: Some(PathBuf::from(".github/skills/x")),
            path: PathBuf::from(".github/skills/x/SKILL.md"),
        };
        assert!(!adapter.applies_to(&skill));
    }

    #[test]
    fn name_is_stable() {
        assert_eq!(CopilotAgentAdapter.name(), "copilot-agent");
    }

    #[test]
    fn to_artifact_reads_md_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let agents_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agents_dir).expect("create agents dir");
        let agent_path = agents_dir.join("my-agent.md");
        std::fs::write(
            &agent_path,
            "---\nname: my-agent\ndescription: A test agent\n---\n\n# my-agent\n",
        )
        .expect("write agent");

        let feat = DiscoveredFeature {
            kind: FeatureKind::Agent,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: tmp.path().to_path_buf(),
            feature_dir: Some(agents_dir),
            path: agent_path.clone(),
        };
        let artifact = CopilotAgentAdapter.to_artifact(&feat, &Real).expect("artifact");
        assert_eq!(artifact.kind, ArtifactKind::Agent);
        assert_eq!(artifact.name, "my-agent");
        assert_eq!(artifact.source_path, agent_path);
    }

    #[test]
    fn to_artifact_uses_filename_stem_when_no_frontmatter_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let agents_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agents_dir).expect("create agents dir");
        let agent_path = agents_dir.join("simple.md");
        std::fs::write(&agent_path, "# simple agent body, no frontmatter\n").expect("write");

        let feat = DiscoveredFeature {
            kind: FeatureKind::Agent,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: tmp.path().to_path_buf(),
            feature_dir: Some(agents_dir),
            path: agent_path,
        };
        let artifact = CopilotAgentAdapter.to_artifact(&feat, &Real).expect("artifact");
        assert_eq!(artifact.name, "simple");
    }

    #[test]
    fn to_artifact_strips_agent_md_suffix() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let agents_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agents_dir).expect("create agents dir");
        let agent_path = agents_dir.join("foo.agent.md");
        std::fs::write(&agent_path, "# foo agent\n").expect("write");

        let feat = DiscoveredFeature {
            kind: FeatureKind::Agent,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: tmp.path().to_path_buf(),
            feature_dir: Some(agents_dir),
            path: agent_path,
        };
        let artifact = CopilotAgentAdapter.to_artifact(&feat, &Real).expect("artifact");
        // `.agent.md` suffix stripped → name is "foo"
        assert_eq!(artifact.name, "foo");
    }

    #[test]
    fn has_agent_md_suffix_works() {
        assert!(has_agent_md_suffix("foo.agent.md"));
        assert!(has_agent_md_suffix("foo.AGENT.MD"));
        assert!(!has_agent_md_suffix("foo.md"));
        assert!(!has_agent_md_suffix("foo.agent.txt"));
    }

    // --- ClaudeAgentAdapter ---

    #[test]
    fn claude_applies_to_claude_agent_only() {
        let adapter = ClaudeAgentAdapter;
        let claude_agent = DiscoveredFeature {
            kind: FeatureKind::Agent,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".claude"),
            feature_dir: Some(PathBuf::from(".claude/agents")),
            path: PathBuf::from(".claude/agents/my-agent.md"),
        };
        assert!(adapter.applies_to(&claude_agent));
    }

    #[test]
    fn claude_rejects_copilot_agent() {
        let adapter = ClaudeAgentAdapter;
        let copilot_agent = DiscoveredFeature {
            kind: FeatureKind::Agent,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".github"),
            feature_dir: Some(PathBuf::from(".github/agents")),
            path: PathBuf::from(".github/agents/my-agent.md"),
        };
        assert!(!adapter.applies_to(&copilot_agent));
    }

    #[test]
    fn claude_rejects_claude_skill() {
        let adapter = ClaudeAgentAdapter;
        let claude_skill = DiscoveredFeature {
            kind: FeatureKind::Skill,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".claude"),
            feature_dir: Some(PathBuf::from(".claude/skills/x")),
            path: PathBuf::from(".claude/skills/x/SKILL.md"),
        };
        assert!(!adapter.applies_to(&claude_skill));
    }

    #[test]
    fn claude_agent_name_is_stable() {
        assert_eq!(ClaudeAgentAdapter.name(), "claude-agent");
    }

    #[test]
    fn claude_to_artifact_reads_md_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let agents_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agents_dir).expect("create");
        let agent_path = agents_dir.join("reviewer.md");
        std::fs::write(
            &agent_path,
            "---\nname: reviewer\ndescription: Reviews changes\n---\n\n# reviewer\n",
        )
        .expect("write");

        let feat = DiscoveredFeature {
            kind: FeatureKind::Agent,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: tmp.path().to_path_buf(),
            feature_dir: Some(agents_dir),
            path: agent_path.clone(),
        };
        let artifact = ClaudeAgentAdapter.to_artifact(&feat, &Real).expect("artifact");
        assert_eq!(artifact.kind, ArtifactKind::Agent);
        assert_eq!(artifact.name, "reviewer");
        assert_eq!(artifact.source_path, agent_path);
        // files contains the agent's own filename (relative path).
        assert_eq!(artifact.files, vec![PathBuf::from("reviewer.md")]);
    }

    #[test]
    fn claude_to_artifact_uses_filename_stem_when_no_frontmatter_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let agents_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agents_dir).expect("create");
        let agent_path = agents_dir.join("plain.md");
        std::fs::write(&agent_path, "# plain agent\n").expect("write");

        let feat = DiscoveredFeature {
            kind: FeatureKind::Agent,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: tmp.path().to_path_buf(),
            feature_dir: Some(agents_dir),
            path: agent_path,
        };
        let artifact = ClaudeAgentAdapter.to_artifact(&feat, &Real).expect("artifact");
        assert_eq!(artifact.name, "plain");
    }

    #[test]
    fn claude_to_artifact_extracts_claude_agent_dir_scripts() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let agents_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agents_dir).expect("create");
        let agent_path = agents_dir.join("scripted.md");
        std::fs::write(
            &agent_path,
            "---\nname: scripted\n---\nRun ${CLAUDE_AGENT_DIR}/scripts/setup.sh\n",
        )
        .expect("write");

        let feat = DiscoveredFeature {
            kind: FeatureKind::Agent,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: tmp.path().to_path_buf(),
            feature_dir: Some(agents_dir),
            path: agent_path,
        };
        let artifact = ClaudeAgentAdapter.to_artifact(&feat, &Real).expect("artifact");
        assert_eq!(artifact.referenced_scripts.len(), 1);
    }

    #[test]
    fn claude_does_not_strip_agent_md_suffix() {
        // Claude doesn't have the .agent.md convention — the full filename
        // stem (with .agent in it) is used.
        let tmp = tempfile::tempdir().expect("tempdir");
        let agents_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agents_dir).expect("create");
        let agent_path = agents_dir.join("foo.agent.md");
        std::fs::write(&agent_path, "# foo\n").expect("write");

        let feat = DiscoveredFeature {
            kind: FeatureKind::Agent,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: tmp.path().to_path_buf(),
            feature_dir: Some(agents_dir),
            path: agent_path,
        };
        let artifact = ClaudeAgentAdapter.to_artifact(&feat, &Real).expect("artifact");
        // file_stem of "foo.agent.md" is "foo.agent" (stem strips only the last extension).
        assert_eq!(artifact.name, "foo.agent");
    }
}
