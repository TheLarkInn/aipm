//! Rule: `skill/oversized` — SKILL.md exceeds 15,000 characters.
//!
//! Threshold derived from Copilot CLI's `SKILL_CHAR_BUDGET` (default 15000).

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

/// Maximum character count for a SKILL.md file (Copilot CLI default).
const SKILL_CHAR_BUDGET: usize = 15_000;

/// Checks that SKILL.md files don't exceed the character budget.
pub struct Oversized;

impl Rule for Oversized {
    fn id(&self) -> &'static str {
        "skill/oversized"
    }

    fn name(&self) -> &'static str {
        "oversized skill"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/skill/oversized.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("reduce file size below 15000 characters")
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let Some((source_type, skill)) = super::read_skill_preamble(file_path, fs) else {
            return Ok(vec![]);
        };
        if skill.content.len() <= SKILL_CHAR_BUDGET {
            return Ok(vec![]);
        }
        Ok(vec![Diagnostic {
            rule_id: self.id().to_string(),
            severity: self.default_severity(),
            message: format!(
                "SKILL.md exceeds {} character limit ({} chars)",
                SKILL_CHAR_BUDGET,
                skill.content.len()
            ),
            file_path: skill.path,
            line: Some(1),
            col: None,
            end_line: None,
            end_col: None,
            source_type,
            help_text: None,
            help_url: None,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::test_helpers::MockFs;

    #[test]
    fn check_file_no_file_returns_empty() {
        let fs = MockFs::new();
        let result = Oversized.check_file(Path::new(".ai/p/skills/s/SKILL.md"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_oversized_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        let content = format!("---\nname: s\n---\n{}", "x".repeat(SKILL_CHAR_BUDGET));
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), content);

        let result = Oversized.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/oversized");
    }

    #[test]
    fn check_file_within_budget_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        // Frontmatter + small body well under the 15 000-character budget.
        let content = "---\nname: s\n---\nshort skill".to_string();
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), content);

        let result = Oversized.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }
}
