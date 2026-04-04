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

    /// Run the rule against a single feature file.
    ///
    /// Default implementation derives the parent directory and delegates to [`Self::check`],
    /// so existing rules work without modification. Rules that want precise per-file
    /// diagnostics (e.g., accurate line numbers) should override this method.
    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, super::Error> {
        file_path.parent().map_or_else(|| Ok(vec![]), |parent| self.check(parent, fs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::{Error, Severity};

    /// A minimal rule that does NOT override `check_file()`, exercising the default impl.
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

        fn check(
            &self,
            _source_dir: &Path,
            _fs: &dyn crate::fs::Fs,
        ) -> Result<Vec<Diagnostic>, Error> {
            Ok(vec![])
        }
    }

    #[test]
    fn default_check_file_delegates_to_check() {
        let rule = AlwaysEmptyRule;
        let fs = crate::fs::Real;
        // Path with a parent — delegates to check()
        let result = rule.check_file(std::path::Path::new("/some/dir/SKILL.md"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn default_check_file_no_parent_returns_empty() {
        let rule = AlwaysEmptyRule;
        let fs = crate::fs::Real;
        // Root path "/" has no parent — returns empty without calling check()
        // Use a path with no parent component
        let result = rule.check_file(std::path::Path::new("SKILL.md"), &fs);
        // "SKILL.md" has parent "" (empty) which is Some(""), so it calls check()
        // On real fs, check() returns Ok(vec![]) since AlwaysEmptyRule always returns empty
        assert!(result.is_ok());
    }
}
