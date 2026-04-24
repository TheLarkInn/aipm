//! Rule: `skill/name-too-long` — skill name exceeds 64 characters.
//!
//! Derived from Copilot CLI Zod schema: `z.string().max(64)`.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

/// Maximum length for a skill name (Copilot CLI limit).
const MAX_SKILL_NAME_LENGTH: usize = 64;

/// Checks that skill names don't exceed 64 characters.
pub struct NameTooLong;

impl Rule for NameTooLong {
    fn id(&self) -> &'static str {
        "skill/name-too-long"
    }

    fn name(&self) -> &'static str {
        "skill name too long"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/skill/name-too-long.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("shorten the name to 60 characters or fewer")
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let Some((source_type, skill)) = super::read_skill_preamble(file_path, fs) else {
            return Ok(vec![]);
        };
        let Some(ref fm) = skill.frontmatter else { return Ok(vec![]) };
        let Some(name) = fm.fields.get("name") else { return Ok(vec![]) };
        if name.len() <= MAX_SKILL_NAME_LENGTH {
            return Ok(vec![]);
        }
        let name_line = fm.field_lines.get("name").copied();
        let (col, end_col) = name_line
            .and_then(|n| skill.content.lines().nth(n - 1))
            .and_then(|line| crate::frontmatter::field_value_range(line, "name"))
            .unzip();
        Ok(vec![Diagnostic {
            rule_id: self.id().to_string(),
            severity: self.default_severity(),
            message: format!(
                "skill name exceeds {} characters ({} chars, Copilot CLI limit)",
                MAX_SKILL_NAME_LENGTH,
                name.len()
            ),
            file_path: skill.path,
            line: name_line,
            col,
            end_line: name_line,
            end_col,
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
        let result = NameTooLong.check_file(Path::new(".ai/p/skills/s/SKILL.md"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_exceeds_limit_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        let name = "a".repeat(65);
        let content = format!("---\nname: {name}\n---\nbody");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), content);

        let result = NameTooLong.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/name-too-long");
    }

    #[test]
    fn check_file_no_name_field_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\ndescription: no name\n---\nbody".to_string());

        let result = NameTooLong.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_populates_col_and_end_col() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        let name = "a".repeat(65);
        let content = format!("---\nname: {name}\n---\nbody");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), content);

        let diags = NameTooLong.check_file(&path, &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].col, Some(7));
        assert_eq!(diags[0].end_line, diags[0].line);
        assert_eq!(diags[0].end_col, Some(72));
    }

    #[test]
    fn check_file_no_frontmatter_returns_empty() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "no frontmatter here".to_string());

        let result = NameTooLong.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_name_within_limit_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        // Name is exactly at the 64-character limit — no diagnostic expected.
        let name = "a".repeat(64);
        let content = format!("---\nname: {name}\n---\nbody");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), content);

        let result = NameTooLong.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }
}
