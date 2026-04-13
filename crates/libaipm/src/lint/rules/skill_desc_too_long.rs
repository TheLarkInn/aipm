//! Rule: `skill/description-too-long` — description exceeds 1024 characters.
//!
//! Derived from Copilot CLI Zod schema: `z.string().max(1024)`.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::scan;

/// Maximum length for a skill description (Copilot CLI limit).
const MAX_DESCRIPTION_LENGTH: usize = 1024;

/// Checks that skill descriptions don't exceed 1024 characters.
pub struct DescriptionTooLong;

impl Rule for DescriptionTooLong {
    fn id(&self) -> &'static str {
        "skill/description-too-long"
    }

    fn name(&self) -> &'static str {
        "skill description too long"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn help_url(&self) -> Option<&'static str> {
        Some(
            "https://github.com/TheLarkInn/aipm/blob/main/docs/rules/skill/description-too-long.md",
        )
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("shorten the description to 200 characters or fewer")
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let source_type = scan::source_type_from_path(file_path).to_string();
        let Some(skill) = scan::read_skill(file_path, fs) else {
            return Ok(vec![]);
        };
        let Some(ref fm) = skill.frontmatter else { return Ok(vec![]) };
        let Some(desc) = fm.fields.get("description") else { return Ok(vec![]) };
        if desc.len() <= MAX_DESCRIPTION_LENGTH {
            return Ok(vec![]);
        }
        let desc_line = fm.field_lines.get("description").copied();
        let (col, end_col) = desc_line
            .and_then(|n| skill.content.lines().nth(n - 1))
            .and_then(|line| crate::frontmatter::field_value_range(line, "description"))
            .unzip();
        Ok(vec![Diagnostic {
            rule_id: self.id().to_string(),
            severity: self.default_severity(),
            message: format!(
                "skill description exceeds {} characters ({} chars, Copilot CLI limit)",
                MAX_DESCRIPTION_LENGTH,
                desc.len()
            ),
            file_path: skill.path,
            line: desc_line,
            col,
            end_line: desc_line,
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
        let result = DescriptionTooLong.check_file(Path::new(".ai/p/skills/s/SKILL.md"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_exceeds_limit_produces_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        let desc = "x".repeat(1025);
        let content = format!("---\nname: s\ndescription: {desc}\n---\nbody");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), content);

        let result = DescriptionTooLong.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/description-too-long");
    }

    #[test]
    fn check_file_no_description_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: s\n---\nbody".to_string());

        let result = DescriptionTooLong.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_populates_col_and_end_col() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        let desc = "x".repeat(1025);
        let content = format!("---\nname: s\ndescription: {desc}\n---\nbody");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), content);

        let diags = DescriptionTooLong.check_file(&path, &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].col, Some(14));
        assert_eq!(diags[0].end_line, diags[0].line);
        assert_eq!(diags[0].end_col, Some(1039));
    }

    #[test]
    fn check_file_no_frontmatter_returns_empty() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "no frontmatter here".to_string());

        let result = DescriptionTooLong.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }
}
