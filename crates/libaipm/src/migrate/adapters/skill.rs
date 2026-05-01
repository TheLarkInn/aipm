//! Skill adapters for the unified migrate pipeline.
//!
//! Holds both the Copilot and Claude skill adapters — both consume a
//! [`DiscoveredFeature`] with `kind == FeatureKind::Skill` and produce a
//! migration [`Artifact`]. The two adapters share most of their logic
//! (frontmatter parse, recursive file collection) and differ only in
//! engine attribution and the script-reference variable prefix(es) they
//! search for.

use crate::discovery::{DiscoveredFeature, Engine, FeatureKind};
use crate::fs::Fs;
use crate::migrate::skill_common;
use crate::migrate::{Artifact, ArtifactKind, Error};

use super::Adapter;

/// Adapter for Copilot skills (`<.github>/skills/`, `<.github>/copilot/...`).
pub struct CopilotSkillAdapter;

impl Adapter for CopilotSkillAdapter {
    fn name(&self) -> &'static str {
        "copilot-skill"
    }

    fn applies_to(&self, feat: &DiscoveredFeature) -> bool {
        feat.engine == Engine::Copilot && feat.kind == FeatureKind::Skill
    }

    fn to_artifact(&self, feat: &DiscoveredFeature, fs: &dyn Fs) -> Result<Artifact, Error> {
        let entry_dir = feat.feature_dir.clone().ok_or_else(|| Error::ConfigParse {
            path: feat.path.clone(),
            reason: "skill feature has no feature_dir".to_string(),
        })?;

        let content = fs.read_to_string(&feat.path)?;
        let metadata = skill_common::parse_frontmatter(&content, &feat.path)?;
        let files = skill_common::collect_files_recursive(&entry_dir, &entry_dir, fs)?;

        // Search for both Copilot and Claude skill dir variable references.
        let mut referenced_scripts =
            skill_common::extract_script_references(&content, "${SKILL_DIR}/");
        referenced_scripts
            .extend(skill_common::extract_script_references(&content, "${CLAUDE_SKILL_DIR}/"));

        // Name precedence: frontmatter `name` field, else the directory name.
        let dir_name =
            entry_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let name = metadata.name.clone().unwrap_or(dir_name);

        Ok(Artifact {
            kind: ArtifactKind::Skill,
            name,
            source_path: entry_dir,
            files,
            referenced_scripts,
            metadata,
        })
    }
}

/// Adapter for Claude skills (`.claude/skills/<name>/SKILL.md`).
///
/// Replaces the filesystem-walking body of
/// [`crate::migrate::skill_detector::SkillDetector`] with a per-feature
/// transformation. Differs from [`CopilotSkillAdapter`] only in that it
/// extracts only `${CLAUDE_SKILL_DIR}/` references (no `${SKILL_DIR}/`
/// variant — that's a Copilot-specific convention).
pub struct ClaudeSkillAdapter;

impl Adapter for ClaudeSkillAdapter {
    fn name(&self) -> &'static str {
        "claude-skill"
    }

    fn applies_to(&self, feat: &DiscoveredFeature) -> bool {
        feat.engine == Engine::Claude && feat.kind == FeatureKind::Skill
    }

    fn to_artifact(&self, feat: &DiscoveredFeature, fs: &dyn Fs) -> Result<Artifact, Error> {
        let entry_dir = feat.feature_dir.clone().ok_or_else(|| Error::ConfigParse {
            path: feat.path.clone(),
            reason: "skill feature has no feature_dir".to_string(),
        })?;

        let content = fs.read_to_string(&feat.path)?;
        let metadata = skill_common::parse_frontmatter(&content, &feat.path)?;
        let files = skill_common::collect_files_recursive(&entry_dir, &entry_dir, fs)?;
        let referenced_scripts =
            skill_common::extract_script_references(&content, "${CLAUDE_SKILL_DIR}/");

        let dir_name =
            entry_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let name = metadata.name.clone().unwrap_or(dir_name);

        Ok(Artifact {
            kind: ArtifactKind::Skill,
            name,
            source_path: entry_dir,
            files,
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
    fn applies_to_copilot_skill_only() {
        let adapter = CopilotSkillAdapter;
        let copilot_skill = DiscoveredFeature {
            kind: FeatureKind::Skill,
            engine: Engine::Copilot,
            layout: Layout::CopilotSubrootWithSkills,
            source_root: PathBuf::from(".github"),
            feature_dir: Some(PathBuf::from(".github/copilot/skills/x")),
            path: PathBuf::from(".github/copilot/skills/x/SKILL.md"),
        };
        assert!(adapter.applies_to(&copilot_skill));
    }

    #[test]
    fn rejects_claude_skill() {
        let adapter = CopilotSkillAdapter;
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
    fn rejects_copilot_agent() {
        let adapter = CopilotSkillAdapter;
        let copilot_agent = DiscoveredFeature {
            kind: FeatureKind::Agent,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".github"),
            feature_dir: None,
            path: PathBuf::from(".github/agents/x.md"),
        };
        assert!(!adapter.applies_to(&copilot_agent));
    }

    #[test]
    fn name_is_stable() {
        assert_eq!(CopilotSkillAdapter.name(), "copilot-skill");
    }

    #[test]
    fn to_artifact_reads_real_skill() {
        // Set up a tempdir, create a SKILL.md, and run the adapter against it.
        let tmp = tempfile::tempdir().expect("tempdir");
        let skill_dir = tmp.path().join("skill-alpha");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: skill-alpha\ndescription: First test skill\n---\n\n# alpha\n",
        )
        .expect("write SKILL.md");

        let feat = DiscoveredFeature {
            kind: FeatureKind::Skill,
            engine: Engine::Copilot,
            layout: Layout::CopilotSubrootWithSkills,
            source_root: tmp.path().to_path_buf(),
            feature_dir: Some(skill_dir.clone()),
            path: skill_dir.join("SKILL.md"),
        };
        let artifact = CopilotSkillAdapter.to_artifact(&feat, &Real).expect("artifact");
        assert_eq!(artifact.kind, ArtifactKind::Skill);
        assert_eq!(artifact.name, "skill-alpha");
        assert_eq!(artifact.source_path, skill_dir);
    }

    #[test]
    fn to_artifact_returns_err_when_feature_dir_is_none() {
        let feat = DiscoveredFeature {
            kind: FeatureKind::Skill,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: PathBuf::from("/tmp"),
            feature_dir: None,
            path: PathBuf::from("/tmp/SKILL.md"),
        };
        let result = CopilotSkillAdapter.to_artifact(&feat, &Real);
        assert!(result.is_err());
    }

    // --- ClaudeSkillAdapter ---

    #[test]
    fn claude_applies_to_claude_skill_only() {
        let adapter = ClaudeSkillAdapter;
        let claude_skill = DiscoveredFeature {
            kind: FeatureKind::Skill,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".claude"),
            feature_dir: Some(PathBuf::from(".claude/skills/x")),
            path: PathBuf::from(".claude/skills/x/SKILL.md"),
        };
        assert!(adapter.applies_to(&claude_skill));
    }

    #[test]
    fn claude_rejects_copilot_skill() {
        let adapter = ClaudeSkillAdapter;
        let copilot_skill = DiscoveredFeature {
            kind: FeatureKind::Skill,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".github"),
            feature_dir: Some(PathBuf::from(".github/skills/x")),
            path: PathBuf::from(".github/skills/x/SKILL.md"),
        };
        assert!(!adapter.applies_to(&copilot_skill));
    }

    #[test]
    fn claude_rejects_claude_agent() {
        let adapter = ClaudeSkillAdapter;
        let claude_agent = DiscoveredFeature {
            kind: FeatureKind::Agent,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".claude"),
            feature_dir: Some(PathBuf::from(".claude/agents")),
            path: PathBuf::from(".claude/agents/x.md"),
        };
        assert!(!adapter.applies_to(&claude_agent));
    }

    #[test]
    fn claude_skill_name_is_stable() {
        assert_eq!(ClaudeSkillAdapter.name(), "claude-skill");
    }

    #[test]
    fn claude_to_artifact_reads_real_skill() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let skill_dir = tmp.path().join("deploy");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: deploy\ndescription: Deploy stuff\n---\n\n# deploy\n",
        )
        .expect("write");

        let feat = DiscoveredFeature {
            kind: FeatureKind::Skill,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: tmp.path().to_path_buf(),
            feature_dir: Some(skill_dir.clone()),
            path: skill_dir.join("SKILL.md"),
        };
        let artifact = ClaudeSkillAdapter.to_artifact(&feat, &Real).expect("artifact");
        assert_eq!(artifact.kind, ArtifactKind::Skill);
        assert_eq!(artifact.name, "deploy");
        assert_eq!(artifact.source_path, skill_dir);
    }

    #[test]
    fn claude_to_artifact_extracts_claude_skill_dir_scripts() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let skill_dir = tmp.path().join("deploy");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: deploy\n---\nRun ${CLAUDE_SKILL_DIR}/scripts/run.sh\n",
        )
        .expect("write");

        let feat = DiscoveredFeature {
            kind: FeatureKind::Skill,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: tmp.path().to_path_buf(),
            feature_dir: Some(skill_dir.clone()),
            path: skill_dir.join("SKILL.md"),
        };
        let artifact = ClaudeSkillAdapter.to_artifact(&feat, &Real).expect("artifact");
        assert_eq!(artifact.referenced_scripts.len(), 1);
    }

    #[test]
    fn claude_to_artifact_returns_err_when_feature_dir_is_none() {
        let feat = DiscoveredFeature {
            kind: FeatureKind::Skill,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: PathBuf::from("/tmp"),
            feature_dir: None,
            path: PathBuf::from("/tmp/SKILL.md"),
        };
        let result = ClaudeSkillAdapter.to_artifact(&feat, &Real);
        assert!(result.is_err());
    }
}
