//! Copilot agent adapter: turns a [`DiscoveredFeature`] of kind
//! `Agent`/engine `Copilot` into an [`Artifact`].
//!
//! Replaces the filesystem-walking body of
//! [`crate::migrate::copilot_agent_detector::CopilotAgentDetector`] with a
//! per-feature transformation. Note: the legacy detector dedupes
//! `foo.md`/`foo.agent.md` to one artifact. The unified pipeline produces
//! one [`DiscoveredFeature`] per file, so the orchestrator (or a future
//! discovery-level dedup) is responsible for handling that case. This
//! adapter operates on a single feature at a time.

use std::path::Path;

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
}
