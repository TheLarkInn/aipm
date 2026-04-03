//! Rule: `skill/oversized` — SKILL.md exceeds 15,000 characters.
//!
//! Threshold derived from Copilot CLI's `SKILL_CHAR_BUDGET` (default 15000).

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::scan;

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

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        for skill in scan::scan_skills(source_dir, fs) {
            if skill.content.len() > SKILL_CHAR_BUDGET {
                diagnostics.push(Diagnostic {
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
                    source_type: ".ai".to_string(),
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
    fn small_file_no_finding() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\nshort body");

        let result = Oversized.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn exactly_at_budget_no_finding() {
        let mut fs = MockFs::new();
        let padding = SKILL_CHAR_BUDGET - "---\nname: s\n---\n".len();
        let content = format!("---\nname: s\n---\n{}", "x".repeat(padding));
        assert_eq!(content.len(), SKILL_CHAR_BUDGET);
        fs.add_skill("p", "s", &content);

        let result = Oversized.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn over_budget_finding() {
        let mut fs = MockFs::new();
        let content = format!("---\nname: s\n---\n{}", "x".repeat(SKILL_CHAR_BUDGET));
        fs.add_skill("p", "s", &content);

        let result = Oversized.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/oversized");
    }

    #[test]
    fn empty_ai_dir() {
        let mut fs = MockFs::new();
        fs.dirs.insert(std::path::PathBuf::from(".ai"), vec![]);

        let result = Oversized.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }
}
