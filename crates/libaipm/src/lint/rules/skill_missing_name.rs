//! Rule: `skill/missing-name` — SKILL.md missing `name` field in frontmatter.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::scan;

/// Checks that every `SKILL.md` in marketplace plugins has a `name` frontmatter field.
pub struct MissingName;

impl Rule for MissingName {
    fn id(&self) -> &'static str {
        "skill/missing-name"
    }

    fn name(&self) -> &'static str {
        "missing skill name"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/skill/missing-name.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("add a \"name\" field to the YAML frontmatter")
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let source_type = scan::source_type_from_path(file_path).to_string();
        let Some(skill) = scan::read_skill(file_path, fs) else {
            return Ok(vec![]);
        };
        let diag = match skill.frontmatter {
            Some(ref fm) if fm.fields.get("name").is_some_and(|v| !v.trim().is_empty()) => {
                return Ok(vec![]);
            },
            Some(ref fm) => Diagnostic {
                rule_id: self.id().to_string(),
                severity: self.default_severity(),
                message: "SKILL.md missing required field: name".to_string(),
                file_path: skill.path,
                line: Some(fm.start_line),
                col: Some(1),
                end_line: Some(fm.start_line),
                end_col: Some(4),
                source_type,
                help_text: None,
                help_url: None,
            },
            None => Diagnostic {
                rule_id: self.id().to_string(),
                severity: self.default_severity(),
                message: "SKILL.md has no frontmatter".to_string(),
                file_path: skill.path,
                line: Some(1),
                col: Some(1),
                end_line: Some(1),
                end_col: Some(4),
                source_type,
                help_text: None,
                help_url: None,
            },
        };
        Ok(vec![diag])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::test_helpers::MockFs;
    use std::path::PathBuf;

    #[test]
    fn check_file_no_file_returns_empty() {
        let fs = MockFs::new();
        let result = MissingName.check_file(Path::new(".ai/p/skills/s/SKILL.md"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_name_present_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: s\ndescription: test\n---\nbody".to_string());

        let result = MissingName.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_name_absent_diagnostic() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\ndescription: no name\n---\nbody".to_string());

        let result = MissingName.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/missing-name");
    }

    #[test]
    fn check_file_no_frontmatter_warns() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "just text without frontmatter".to_string());

        let result = MissingName.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/missing-name");
        assert_eq!(diags[0].line, Some(1));
    }
}
