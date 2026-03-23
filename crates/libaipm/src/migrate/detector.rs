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
    ]
}
