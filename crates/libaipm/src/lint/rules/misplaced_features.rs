//! Rule: `source/misplaced-features` — plugin features in source dirs instead of marketplace.
//!
//! Checks `.claude/` or `.github/` for skills, agents, hooks, etc. that should be in `.ai/`.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

/// Known subdirectory names that indicate plugin features.
const FEATURE_DIRS: &[&str] = &["skills", "commands", "agents", "hooks", "output-styles"];

/// Checks for plugin features sitting in tool-specific directories.
pub struct MisplacedFeatures {
    /// The source type this rule checks (e.g., `".claude"` or `".github"`).
    pub source_type: &'static str,
}

impl Rule for MisplacedFeatures {
    fn id(&self) -> &'static str {
        "source/misplaced-features"
    }

    fn name(&self) -> &'static str {
        "misplaced plugin features"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        // Check if .ai/ marketplace exists (only warn if it does)
        let project_root = source_dir.parent().unwrap_or(source_dir);
        let ai_dir = project_root.join(".ai");
        if !fs.exists(&ai_dir) {
            return Ok(diagnostics);
        }

        for feature_dir in FEATURE_DIRS {
            let dir = source_dir.join(feature_dir);
            if fs.exists(&dir) {
                diagnostics.push(Diagnostic {
                    rule_id: self.id().to_string(),
                    severity: self.default_severity(),
                    message: format!(
                        "{feature_dir}/ found in {} instead of .ai/ marketplace",
                        self.source_type
                    ),
                    file_path: dir,
                    line: None,
                    source_type: self.source_type.to_string(),
                });
            }
        }

        Ok(diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::test_helpers::MockFs;

    #[test]
    fn no_feature_dirs_no_finding() {
        let mut fs = MockFs::new();
        fs.add_existing(".ai");

        let rule = MisplacedFeatures { source_type: ".claude" };
        let result = rule.check(Path::new(".claude"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn skills_dir_with_marketplace_warns() {
        let mut fs = MockFs::new();
        fs.add_existing(".ai");
        fs.add_existing(".claude/skills");

        let rule = MisplacedFeatures { source_type: ".claude" };
        let result = rule.check(Path::new(".claude"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("skills/"));
    }

    #[test]
    fn skills_dir_without_marketplace_no_finding() {
        let mut fs = MockFs::new();
        // .ai does NOT exist
        fs.add_existing(".claude/skills");

        let rule = MisplacedFeatures { source_type: ".claude" };
        let result = rule.check(Path::new(".claude"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn multiple_feature_dirs() {
        let mut fs = MockFs::new();
        fs.add_existing(".ai");
        fs.add_existing(".claude/skills");
        fs.add_existing(".claude/agents");
        fs.add_existing(".claude/hooks");

        let rule = MisplacedFeatures { source_type: ".claude" };
        let result = rule.check(Path::new(".claude"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 3);
    }

    #[test]
    fn github_source_type() {
        let mut fs = MockFs::new();
        fs.add_existing(".ai");
        fs.add_existing(".github/skills");

        let rule = MisplacedFeatures { source_type: ".github" };
        let result = rule.check(Path::new(".github"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].source_type, ".github");
    }
}
