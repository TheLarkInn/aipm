//! Rule: `skill/name-invalid-chars` — skill name doesn't match Copilot regex.
//!
//! Derived from Copilot CLI Zod schema: `/^[a-zA-Z0-9][a-zA-Z0-9._\- ]*$/`.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::scan;

/// Check if a name matches Copilot's allowed pattern.
fn is_valid_copilot_name(name: &str) -> bool {
    let mut chars = name.bytes();
    // First char must be alphanumeric
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    // Remaining: alphanumeric, dot, underscore, hyphen, space
    chars.all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-' || b == b' ')
}

/// Checks that skill names match Copilot CLI's name pattern.
pub struct NameInvalidChars;

impl Rule for NameInvalidChars {
    fn id(&self) -> &'static str {
        "skill/name-invalid-chars"
    }

    fn name(&self) -> &'static str {
        "skill name invalid characters"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/skill/name-invalid-chars.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("use only alphanumeric, hyphen, and underscore characters")
    }

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        for skill in scan::scan_skills(source_dir, fs) {
            if let Some(ref fm) = skill.frontmatter {
                if let Some(name) = fm.fields.get("name") {
                    if !name.is_empty() && !is_valid_copilot_name(name) {
                        let name_line = fm.field_lines.get("name").copied();
                        let (col, end_col) = name_line
                            .and_then(|n| skill.content.lines().nth(n - 1))
                            .and_then(|line| crate::frontmatter::field_value_range(line, "name"))
                            .unzip();
                        diagnostics.push(Diagnostic {
                            rule_id: self.id().to_string(),
                            severity: self.default_severity(),
                            message: format!(
                                "skill name \"{name}\" contains characters not allowed by Copilot CLI (must match /^[a-zA-Z0-9][a-zA-Z0-9._\\- ]*$/)"
                            ),
                            file_path: skill.path,
                            line: name_line,
                            col,
                            end_line: name_line,
                            end_col,
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
        if name.is_empty() || is_valid_copilot_name(name) {
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
                "skill name \"{name}\" contains characters not allowed by Copilot CLI (must match /^[a-zA-Z0-9][a-zA-Z0-9._\\- ]*$/)"
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
    use std::path::Path;

    use super::*;
    use crate::lint::rules::test_helpers::MockFs;

    #[test]
    fn valid_names() {
        assert!(is_valid_copilot_name("my-skill"));
        assert!(is_valid_copilot_name("MySkill"));
        assert!(is_valid_copilot_name("skill.v2"));
        assert!(is_valid_copilot_name("skill_name"));
        assert!(is_valid_copilot_name("skill with spaces"));
        assert!(is_valid_copilot_name("a"));
    }

    #[test]
    fn invalid_names() {
        assert!(!is_valid_copilot_name(""));
        assert!(!is_valid_copilot_name("-starts-with-hyphen"));
        assert!(!is_valid_copilot_name(".starts-with-dot"));
        assert!(!is_valid_copilot_name("has@special"));
        assert!(!is_valid_copilot_name("has/slash"));
    }

    #[test]
    fn check_invalid_name_produces_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "bad-name", "---\nname: has@special\n---\nbody");

        let result = NameInvalidChars.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/name-invalid-chars");
        assert!(diags[0].message.contains("has@special"));
    }

    #[test]
    fn check_valid_name_no_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "good-name", "---\nname: valid-skill\n---\nbody");

        let result = NameInvalidChars.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_no_name_field_no_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "no-name", "---\ndescription: A skill\n---\nbody");

        let result = NameInvalidChars.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_no_frontmatter_no_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "no frontmatter here");

        let result = NameInvalidChars.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    // --- check_file() tests ---

    #[test]
    fn check_file_no_file_returns_empty() {
        let fs = MockFs::new();
        let result = NameInvalidChars.check_file(Path::new(".ai/p/skills/s/SKILL.md"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_valid_name_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: valid-skill\n---\nbody".to_string());

        let result = NameInvalidChars.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_invalid_name_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: has@special\n---\nbody".to_string());

        let result = NameInvalidChars.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/name-invalid-chars");
    }

    #[test]
    fn check_file_no_name_field_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\ndescription: no name\n---\nbody".to_string());

        let result = NameInvalidChars.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_no_frontmatter_no_diagnostic() {
        // Covers the `else { return Ok(vec![]) }` branch in check_file when
        // the skill file exists but contains no YAML frontmatter.
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "no frontmatter here".to_string());

        let result = NameInvalidChars.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_empty_name_no_diagnostic() {
        // Covers the short-circuit False branch of `!name.is_empty() && ...`
        // in check() when the name field is an empty string.
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: \"\"\n---\nbody");

        let result = NameInvalidChars.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_populates_col_and_end_col() {
        // "name: has@special" — value starts at col 7, "has@special" is 11 chars → end_col 18
        let mut fs = MockFs::new();
        fs.add_skill("p", "bad-name", "---\nname: has@special\n---\nbody");

        let diags = NameInvalidChars.check(Path::new(".ai"), &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].col, Some(7));
        assert_eq!(diags[0].end_line, diags[0].line);
        assert_eq!(diags[0].end_col, Some(18));
    }

    #[test]
    fn check_file_populates_col_and_end_col() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: has@special\n---\nbody".to_string());

        let diags = NameInvalidChars.check_file(&path, &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].col, Some(7));
        assert_eq!(diags[0].end_line, diags[0].line);
        assert_eq!(diags[0].end_col, Some(18));
    }

    #[test]
    fn check_file_empty_name_no_diagnostic() {
        // Covers the True branch of `if name.is_empty() || ...` in check_file
        // when the name field is present but empty.
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: \"\"\n---\nbody".to_string());

        let result = NameInvalidChars.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }
}
