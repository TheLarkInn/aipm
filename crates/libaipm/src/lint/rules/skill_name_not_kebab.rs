//! Rule: `skill/name-not-kebab-case` — skill name is not lowercase kebab-case.
//!
//! Skill directories and skill names are addressed on the command line and in
//! plugin manifests, so the portable form is lowercase kebab-case: groups of
//! lowercase ASCII letters and digits joined by single hyphens.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

/// Check whether `name` is lowercase kebab-case (`^[a-z0-9]+(-[a-z0-9]+)*$`).
fn is_kebab_case(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|part| {
            !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// Checks that skill names are lowercase kebab-case.
pub struct NameNotKebabCase;

impl Rule for NameNotKebabCase {
    fn id(&self) -> &'static str {
        "skill/name-not-kebab-case"
    }

    fn name(&self) -> &'static str {
        "skill name not kebab-case"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/skill/name-not-kebab-case.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("use lowercase letters and digits separated by single hyphens")
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let Some((source_type, skill)) = super::read_skill_preamble(file_path, fs) else {
            return Ok(vec![]);
        };
        let Some(name) = skill.field("name") else { return Ok(vec![]) };
        if name.trim().is_empty() || is_kebab_case(name) {
            return Ok(vec![]);
        }
        let name_line =
            skill.frontmatter.as_ref().and_then(|fm| fm.field_lines.get("name").copied());
        let (col, end_col) = name_line
            .and_then(|n| skill.content.lines().nth(n.saturating_sub(1)))
            .and_then(|line| crate::frontmatter::field_value_range(line, "name"))
            .unzip();
        Ok(vec![Diagnostic {
            rule_id: self.id().to_string(),
            severity: self.default_severity(),
            message: format!(
                "skill name \"{name}\" is not lowercase kebab-case (must match /^[a-z0-9]+(-[a-z0-9]+)*$/)"
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
    use std::path::PathBuf;

    fn check(content: &str) -> Vec<Diagnostic> {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), content.to_string());
        NameNotKebabCase.check_file(&path, &fs).ok().unwrap_or_default()
    }

    #[test]
    fn accepts_kebab_case() {
        assert!(is_kebab_case("skill"));
        assert!(is_kebab_case("my-skill"));
        assert!(is_kebab_case("pdf-2-text"));
        assert!(is_kebab_case("a1"));
    }

    #[test]
    fn rejects_non_kebab_case() {
        assert!(!is_kebab_case(""));
        assert!(!is_kebab_case("MySkill"));
        assert!(!is_kebab_case("my_skill"));
        assert!(!is_kebab_case("my skill"));
        assert!(!is_kebab_case("my.skill"));
        assert!(!is_kebab_case("-leading"));
        assert!(!is_kebab_case("trailing-"));
        assert!(!is_kebab_case("double--hyphen"));
    }

    #[test]
    fn no_file_returns_empty() {
        let fs = MockFs::new();
        let diags = NameNotKebabCase
            .check_file(Path::new(".ai/p/skills/s/SKILL.md"), &fs)
            .ok()
            .unwrap_or_default();
        assert!(diags.is_empty());
    }

    #[test]
    fn kebab_case_name_passes() {
        assert!(check("---\nname: my-skill\ndescription: d\n---\nbody").is_empty());
    }

    #[test]
    fn uppercase_name_is_reported() {
        let diags = check("---\nname: MySkill\ndescription: d\n---\nbody");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/name-not-kebab-case");
        assert_eq!(diags[0].line, Some(2));
        assert_eq!(diags[0].col, Some(7));
        assert_eq!(diags[0].end_col, Some(14));
    }

    #[test]
    fn underscore_name_is_reported() {
        assert_eq!(check("---\nname: my_skill\n---\nbody").len(), 1);
    }

    #[test]
    fn missing_name_is_ignored() {
        assert!(check("---\ndescription: d\n---\nbody").is_empty());
    }

    #[test]
    fn blank_name_is_ignored() {
        assert!(check("---\nname: \"  \"\n---\nbody").is_empty());
    }

    #[test]
    fn no_frontmatter_is_ignored() {
        assert!(check("no frontmatter here").is_empty());
    }

    #[test]
    fn non_string_name_is_ignored() {
        // `skill/invalid-frontmatter` owns the type error; this rule stays quiet.
        assert!(check("---\nname: [a, b]\n---\nbody").is_empty());
    }

    #[test]
    fn rule_metadata() {
        assert_eq!(NameNotKebabCase.name(), "skill name not kebab-case");
        assert_eq!(NameNotKebabCase.default_severity(), Severity::Warning);
        assert!(NameNotKebabCase.help_url().is_some());
        assert!(NameNotKebabCase.help_text().is_some());
    }
}
