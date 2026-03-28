//! Detector trait for scanning source directories for artifacts.

use std::path::Path;

use crate::fs::Fs;

use super::{Artifact, Error};

/// Trait for scanning a source directory for artifacts of a specific kind.
///
/// Each detector is responsible for one artifact type within one source folder.
/// The orchestrator calls `detect()` and collects all returned artifacts.
pub trait Detector {
    /// Human-readable name for this detector (e.g., "skill", "command").
    fn name(&self) -> &'static str;

    /// Scan `source_dir` and return all discovered artifacts.
    /// `source_dir` is the resolved path (e.g., `/project/.claude/`).
    fn detect(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Artifact>, Error>;
}

/// Returns the default set of detectors for `.claude/` source.
pub fn claude_detectors() -> Vec<Box<dyn Detector>> {
    vec![
        Box::new(super::skill_detector::SkillDetector),
        Box::new(super::command_detector::CommandDetector),
        Box::new(super::agent_detector::AgentDetector),
        Box::new(super::mcp_detector::McpDetector),
        Box::new(super::hook_detector::HookDetector),
        Box::new(super::output_style_detector::OutputStyleDetector),
    ]
}

/// Returns the default set of detectors for `.github/` (Copilot CLI) source.
pub fn copilot_detectors() -> Vec<Box<dyn Detector>> {
    vec![
        Box::new(super::copilot_skill_detector::CopilotSkillDetector),
        Box::new(super::copilot_agent_detector::CopilotAgentDetector),
        Box::new(super::copilot_mcp_detector::CopilotMcpDetector),
        Box::new(super::copilot_hook_detector::CopilotHookDetector),
        Box::new(super::copilot_extension_detector::CopilotExtensionDetector),
        Box::new(super::copilot_lsp_detector::CopilotLspDetector),
    ]
}

/// Returns all registered detectors for a given source type.
pub fn detectors_for_source(source_type: &str) -> Vec<Box<dyn Detector>> {
    match source_type {
        ".claude" => claude_detectors(),
        ".github" => copilot_detectors(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copilot_detectors_returns_six() {
        let detectors = copilot_detectors();
        assert_eq!(detectors.len(), 6);
    }

    #[test]
    fn copilot_detector_names() {
        let detectors = copilot_detectors();
        let names: Vec<&str> = detectors.iter().map(|d| d.name()).collect();
        assert_eq!(
            names,
            vec![
                "copilot-skill",
                "copilot-agent",
                "copilot-mcp",
                "copilot-hook",
                "copilot-extension",
                "copilot-lsp"
            ]
        );
    }

    #[test]
    fn claude_detectors_returns_six() {
        let detectors = claude_detectors();
        assert_eq!(detectors.len(), 6);
    }

    #[test]
    fn detectors_for_claude_source() {
        let detectors = detectors_for_source(".claude");
        assert_eq!(detectors.len(), 6);
        assert_eq!(detectors.first().map(|d| d.name()), Some("skill"));
    }

    #[test]
    fn detectors_for_github_source() {
        let detectors = detectors_for_source(".github");
        assert_eq!(detectors.len(), 6);
        assert_eq!(detectors.first().map(|d| d.name()), Some("copilot-skill"));
    }

    #[test]
    fn detectors_for_unknown_source() {
        let detectors = detectors_for_source(".unknown");
        assert!(detectors.is_empty());
    }
}
