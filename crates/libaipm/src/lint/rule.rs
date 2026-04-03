//! The `LintRule` trait — core interface for lint rules.
//!
//! Mirrors the `Detector` trait pattern from the migrate pipeline.
//! Each rule is a zero-sized unit struct implementing this trait.

use std::path::Path;

use crate::fs::Fs;

use super::diagnostic::{Diagnostic, Severity};

/// A lint rule that checks a source directory for quality issues.
///
/// Rules are stateless and must be safe to execute in parallel (`Send + Sync`).
/// Each rule accepts `&dyn Fs` for filesystem access, enabling mock-based testing.
pub trait Rule: Send + Sync {
    /// Unique hierarchical ID (e.g., `"skill/missing-description"`).
    fn id(&self) -> &'static str;

    /// Human-readable rule name for display.
    fn name(&self) -> &'static str;

    /// Default severity when not overridden by config.
    fn default_severity(&self) -> Severity;

    /// URL to the rule's documentation page, if available.
    fn help_url(&self) -> Option<&'static str> {
        None
    }

    /// Short actionable help text describing how to fix the issue.
    fn help_text(&self) -> Option<&'static str> {
        None
    }

    /// Run the rule against a source directory.
    ///
    /// Returns zero or more diagnostics. An empty vec means no issues found.
    /// Errors indicate infrastructure failures (I/O errors), not lint findings.
    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, super::Error>;
}
