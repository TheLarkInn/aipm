//! Rule: `skill/name-too-long` — skill name exceeds 64 characters.
//!
//! Derived from Copilot CLI Zod schema: `z.string().max(64)`.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::scan;

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

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        for skill in scan::scan_skills(source_dir, fs) {
            if let Some(ref fm) = skill.frontmatter {
                if let Some(name) = fm.fields.get("name") {
                    if name.len() > MAX_SKILL_NAME_LENGTH {
                        diagnostics.push(Diagnostic {
                            rule_id: self.id().to_string(),
                            severity: self.default_severity(),
                            message: format!(
                                "skill name exceeds {} characters ({} chars, Copilot CLI limit)",
                                MAX_SKILL_NAME_LENGTH,
                                name.len()
                            ),
                            file_path: skill.path,
                            line: fm.field_lines.get("name").copied(),
                            col: None,
                            end_line: None,
                            end_col: None,
                            source_type: ".ai".to_string(),
                            help_text: None,
                            help_url: None,
                        });
                    }
                }
            }
        }

        Ok(diagnostics)
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let source_type = scan::source_type_from_path(file_path).to_string();
        let Some(skill) = scan::read_skill(file_path, fs) else {
            return Ok(vec![]);
        };
        let Some(ref fm) = skill.frontmatter else { return Ok(vec![]) };
        let Some(name) = fm.fields.get("name") else { return Ok(vec![]) };
        if name.len() <= MAX_SKILL_NAME_LENGTH {
            return Ok(vec![]);
        }
        Ok(vec![Diagnostic {
            rule_id: self.id().to_string(),
            severity: self.default_severity(),
            message: format!(
                "skill name exceeds {} characters ({} chars, Copilot CLI limit)",
                MAX_SKILL_NAME_LENGTH,
                name.len()
            ),
            file_path: skill.path,
            line: fm.field_lines.get("name").copied(),
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
    fn short_name_no_finding() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: short\n---\nbody");

        let result = NameTooLong.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn exactly_64_chars_no_finding() {
        let mut fs = MockFs::new();
        let name = "a".repeat(64);
        let content = format!("---\nname: {name}\n---\nbody");
        fs.add_skill("p", "s", &content);

        let result = NameTooLong.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn exceeds_64_chars_finding() {
        let mut fs = MockFs::new();
        let name = "a".repeat(65);
        let content = format!("---\nname: {name}\n---\nbody");
        fs.add_skill("p", "s", &content);

        let result = NameTooLong.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/name-too-long");
    }

    #[test]
    fn no_name_no_finding() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\ndescription: test\n---\nbody");

        let result = NameTooLong.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn no_frontmatter_no_finding() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "no frontmatter here");

        let result = NameTooLong.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    // --- check_file() tests ---

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
    fn check_file_no_frontmatter_returns_empty() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "no frontmatter here".to_string());

        let result = NameTooLong.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }
}
