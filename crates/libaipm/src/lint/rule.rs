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

    /// Run the rule against a single feature file.
    ///
    /// Returns zero or more diagnostics. An empty vec means no issues found.
    /// Errors indicate infrastructure failures (I/O errors), not lint findings.
    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, super::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::{Error, Severity};

    /// A minimal rule implementing the trait.
    struct AlwaysEmptyRule;

    impl Rule for AlwaysEmptyRule {
        fn id(&self) -> &'static str {
            "test/always-empty"
        }

        fn name(&self) -> &'static str {
            "always empty"
        }

        fn default_severity(&self) -> Severity {
            Severity::Warning
        }

        fn check_file(
            &self,
            _file_path: &Path,
            _fs: &dyn crate::fs::Fs,
        ) -> Result<Vec<Diagnostic>, Error> {
            Ok(vec![])
        }
    }

    #[test]
    fn check_file_returns_empty() {
        let rule = AlwaysEmptyRule;
        let fs = crate::fs::Real;
        let result = rule.check_file(std::path::Path::new("/some/dir/SKILL.md"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn default_help_url_is_none() {
        let rule = AlwaysEmptyRule;
        assert!(rule.help_url().is_none());
    }

    #[test]
    fn default_help_text_is_none() {
        let rule = AlwaysEmptyRule;
        assert!(rule.help_text().is_none());
    }
}
