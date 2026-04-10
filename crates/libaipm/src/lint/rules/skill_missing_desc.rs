//! Rule: `skill/missing-description` — SKILL.md missing `description` field.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::scan;

/// Checks that every `SKILL.md` has a `description` frontmatter field.
pub struct MissingDescription;

impl Rule for MissingDescription {
    fn id(&self) -> &'static str {
        "skill/missing-description"
    }

    fn name(&self) -> &'static str {
        "missing skill description"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/skill/missing-description.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("add a \"description\" field to the YAML frontmatter")
    }

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        for skill in scan::scan_skills(source_dir, fs) {
            match skill.frontmatter {
                Some(ref fm) if fm.fields.contains_key("description") => {},
                Some(ref fm) => {
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        severity: self.default_severity(),
                        message: "SKILL.md missing required field: description".to_string(),
                        file_path: skill.path,
                        line: Some(fm.start_line),
                        col: Some(1),
                        end_line: Some(fm.start_line),
                        end_col: Some(4),
                        source_type: ".ai".to_string(),
                        help_text: None,
                        help_url: None,
                    });
                },
                None => {
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        severity: self.default_severity(),
                        message: "SKILL.md has no frontmatter".to_string(),
                        file_path: skill.path,
                        line: Some(1),
                        col: Some(1),
                        end_line: Some(1),
                        end_col: Some(4),
                        source_type: ".ai".to_string(),
                        help_text: None,
                        help_url: None,
                    });
                },
            }
        }

        Ok(diagnostics)
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let source_type = scan::source_type_from_path(file_path).to_string();
        let Some(skill) = scan::read_skill(file_path, fs) else {
            return Ok(vec![]);
        };
        let diag = match skill.frontmatter {
            Some(ref fm) if fm.fields.contains_key("description") => return Ok(vec![]),
            Some(ref fm) => Diagnostic {
                rule_id: self.id().to_string(),
                severity: self.default_severity(),
                message: "SKILL.md missing required field: description".to_string(),
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

    #[test]
    fn description_present_no_finding() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\ndescription: test\n---\nbody");

        let result = MissingDescription.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn description_absent_finding() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\nbody");

        let result = MissingDescription.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/missing-description");
    }

    #[test]
    fn no_frontmatter_warns() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "just text");

        let result = MissingDescription.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/missing-description");
    }

    #[test]
    fn missing_description_points_to_frontmatter_opener() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\nbody");

        let diags = MissingDescription.check(Path::new(".ai"), &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, Some(1));
        assert_eq!(diags[0].col, Some(1));
        assert_eq!(diags[0].end_line, Some(1));
        assert_eq!(diags[0].end_col, Some(4));
    }

    // --- check_file() tests ---

    #[test]
    fn check_file_no_file_returns_empty() {
        let fs = MockFs::new();
        let result = MissingDescription.check_file(Path::new(".ai/p/skills/s/SKILL.md"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_description_present_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: s\ndescription: test\n---\nbody".to_string());

        let result = MissingDescription.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_description_absent_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: s\n---\nbody".to_string());

        let result = MissingDescription.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/missing-description");
    }
}
