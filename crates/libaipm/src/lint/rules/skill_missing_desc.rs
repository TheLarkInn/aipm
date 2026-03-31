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

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        for skill in scan::scan_skills(source_dir, fs) {
            match skill.frontmatter {
                Some(ref fm) if fm.fields.contains_key("description") => {},
                Some(_) => {
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        severity: self.default_severity(),
                        message: "SKILL.md missing required field: description".to_string(),
                        file_path: skill.path,
                        line: Some(1),
                        source_type: ".ai".to_string(),
                    });
                },
                None => {
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        severity: self.default_severity(),
                        message: "SKILL.md has no frontmatter".to_string(),
                        file_path: skill.path,
                        line: Some(1),
                        source_type: ".ai".to_string(),
                    });
                },
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
}
