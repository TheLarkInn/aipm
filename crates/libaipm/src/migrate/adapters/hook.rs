//! Copilot hook adapter: turns a [`DiscoveredFeature`] of kind
//! `Hook`/engine `Copilot` into an [`Artifact`].
//!
//! Replaces the filesystem-walking body of
//! [`crate::migrate::copilot_hook_detector::CopilotHookDetector`] with a
//! per-feature transformation. Reuses the legacy normalization helpers
//! (`normalize_hook_events`, `extract_hook_script_references`) verbatim by
//! making them `pub(crate)`.

use crate::discovery::{DiscoveredFeature, Engine, FeatureKind};
use crate::fs::Fs;
use crate::migrate::copilot_hook_detector::{
    extract_hook_script_references, normalize_hook_events,
};
use crate::migrate::{Artifact, ArtifactKind, ArtifactMetadata, Error};

use super::Adapter;

/// Adapter for Copilot hooks (`<.github>/hooks.json` or
/// `<.github>/hooks/hooks.json`).
pub struct CopilotHookAdapter;

impl Adapter for CopilotHookAdapter {
    fn name(&self) -> &'static str {
        "copilot-hook"
    }

    fn applies_to(&self, feat: &DiscoveredFeature) -> bool {
        feat.engine == Engine::Copilot && feat.kind == FeatureKind::Hook
    }

    fn to_artifact(&self, feat: &DiscoveredFeature, fs: &dyn Fs) -> Result<Artifact, Error> {
        let content = fs.read_to_string(&feat.path)?;
        let json: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| Error::ConfigParse {
                path: feat.path.clone(),
                reason: format!("invalid JSON in hooks.json: {e}"),
            })?;

        // Normalize legacy event names to the canonical camelCase forms.
        let normalized = normalize_hook_events(&json);
        let hooks_content =
            serde_json::to_string_pretty(&normalized).unwrap_or_else(|_| "{}".to_string());

        let referenced_scripts = extract_hook_script_references(&normalized);

        Ok(Artifact {
            kind: ArtifactKind::Hook,
            name: "copilot-hooks".to_string(),
            source_path: feat.path.clone(),
            files: Vec::new(),
            referenced_scripts,
            metadata: ArtifactMetadata {
                name: Some("copilot-hooks".to_string()),
                description: Some("Hooks from Copilot CLI hooks.json".to_string()),
                raw_content: Some(hooks_content),
                ..ArtifactMetadata::default()
            },
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
    fn applies_to_copilot_hook_only() {
        let adapter = CopilotHookAdapter;
        let hook = DiscoveredFeature {
            kind: FeatureKind::Hook,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".github"),
            feature_dir: Some(PathBuf::from(".github")),
            path: PathBuf::from(".github/hooks.json"),
        };
        assert!(adapter.applies_to(&hook));
    }

    #[test]
    fn rejects_claude_hook() {
        let adapter = CopilotHookAdapter;
        let hook = DiscoveredFeature {
            kind: FeatureKind::Hook,
            engine: Engine::Claude,
            layout: Layout::Canonical,
            source_root: PathBuf::from(".claude"),
            feature_dir: Some(PathBuf::from(".claude/hooks")),
            path: PathBuf::from(".claude/hooks/hooks.json"),
        };
        assert!(!adapter.applies_to(&hook));
    }

    #[test]
    fn rejects_copilot_skill() {
        let adapter = CopilotHookAdapter;
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
        assert_eq!(CopilotHookAdapter.name(), "copilot-hook");
    }

    #[test]
    fn to_artifact_reads_and_normalizes_hooks_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let hooks_path = tmp.path().join("hooks.json");
        // Legacy event name `SessionStart` should be normalized to `sessionStart`.
        std::fs::write(&hooks_path, r#"{"SessionStart": [{"command": "echo hi"}]}"#)
            .expect("write hooks.json");

        let feat = DiscoveredFeature {
            kind: FeatureKind::Hook,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: tmp.path().to_path_buf(),
            feature_dir: Some(tmp.path().to_path_buf()),
            path: hooks_path.clone(),
        };
        let artifact = CopilotHookAdapter.to_artifact(&feat, &Real).expect("artifact");
        assert_eq!(artifact.kind, ArtifactKind::Hook);
        assert_eq!(artifact.name, "copilot-hooks");
        assert_eq!(artifact.source_path, hooks_path);
        let raw = artifact.metadata.raw_content.expect("raw_content set");
        assert!(raw.contains("sessionStart"), "expected normalized event in: {raw}");
    }

    #[test]
    fn to_artifact_returns_err_for_invalid_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let hooks_path = tmp.path().join("hooks.json");
        std::fs::write(&hooks_path, "not valid json").expect("write");

        let feat = DiscoveredFeature {
            kind: FeatureKind::Hook,
            engine: Engine::Copilot,
            layout: Layout::Canonical,
            source_root: tmp.path().to_path_buf(),
            feature_dir: Some(tmp.path().to_path_buf()),
            path: hooks_path,
        };
        let result = CopilotHookAdapter.to_artifact(&feat, &Real);
        assert!(matches!(result, Err(Error::ConfigParse { .. })));
    }
}
