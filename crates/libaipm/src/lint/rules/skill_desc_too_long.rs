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

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        for skill in scan::scan_skills(source_dir, fs) {
            if let Some(ref fm) = skill.frontmatter {
                if let Some(desc) = fm.fields.get("description") {
                    if desc.len() > MAX_DESCRIPTION_LENGTH {
                        diagnostics.push(Diagnostic {
                            rule_id: self.id().to_string(),
                            severity: self.default_severity(),
                            message: format!(
                                "skill description exceeds {} characters ({} chars, Copilot CLI limit)",
                                MAX_DESCRIPTION_LENGTH,
                                desc.len()
                            ),
                            file_path: skill.path,
                            line: fm.field_lines.get("description").copied(),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::test_helpers::MockFs;

    #[test]
    fn short_description_no_finding() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\ndescription: short\n---\nbody");

        let result = DescriptionTooLong.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn exactly_1024_chars_no_finding() {
        let mut fs = MockFs::new();
        let desc = "x".repeat(1024);
        let content = format!("---\nname: s\ndescription: {desc}\n---\nbody");
        fs.add_skill("p", "s", &content);

        let result = DescriptionTooLong.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn exceeds_1024_chars_finding() {
        let mut fs = MockFs::new();
        let desc = "x".repeat(1025);
        let content = format!("---\nname: s\ndescription: {desc}\n---\nbody");
        fs.add_skill("p", "s", &content);

        let result = DescriptionTooLong.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/description-too-long");
    }

    #[test]
    fn no_description_no_finding() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\nbody");

        let result = DescriptionTooLong.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn no_frontmatter_no_finding() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "no frontmatter here");

        let result = DescriptionTooLong.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }
}
