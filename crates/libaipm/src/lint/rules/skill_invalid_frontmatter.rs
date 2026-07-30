//! Rule: `skill/invalid-frontmatter` — SKILL.md frontmatter is structurally invalid.
//!
//! Both Claude Code and Copilot CLI extract the frontmatter block with an
//! anchored `---` match and then hand the block to a YAML parser. A file that
//! does not start immediately with an exact `---` line, is not closed by an
//! exact `---` line, does not contain valid YAML, or whose YAML root is not a
//! mapping is silently ignored by those engines — the skill never loads.
//!
//! This rule additionally reports `name` / `description` fields that parse to a
//! non-string YAML value (a list, mapping, number, or boolean), which the
//! engines' schemas reject.

use std::path::Path;

use crate::frontmatter::strict;
use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

/// Frontmatter fields that must be YAML strings when present.
const STRING_FIELDS: [&str; 2] = ["name", "description"];

/// Checks that SKILL.md frontmatter is a well-formed YAML mapping.
pub struct InvalidFrontmatter;

impl Rule for InvalidFrontmatter {
    fn id(&self) -> &'static str {
        "skill/invalid-frontmatter"
    }

    fn name(&self) -> &'static str {
        "invalid skill frontmatter"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/skill/invalid-frontmatter.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("open and close the frontmatter with exact `---` lines wrapping a YAML mapping")
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let Some((source_type, skill)) = super::read_skill_preamble(file_path, fs) else {
            return Ok(vec![]);
        };
        let doc = match skill.strict {
            Ok(ref doc) => doc,
            Err(ref err) => {
                return Ok(vec![self.diagnostic(
                    err.to_string(),
                    &skill,
                    error_line(err),
                    &source_type,
                )]);
            },
        };

        let mut diagnostics = Vec::new();
        for field in STRING_FIELDS {
            let Some(type_name) = doc.type_name(field) else { continue };
            if doc.string(field).is_some() {
                continue;
            }
            let line = skill
                .frontmatter
                .as_ref()
                .and_then(|fm| fm.field_lines.get(field).copied())
                .unwrap_or(1);
            diagnostics.push(self.diagnostic(
                format!("SKILL.md field \"{field}\" must be a string, found {type_name}"),
                &skill,
                line,
                &source_type,
            ));
        }
        Ok(diagnostics)
    }
}

impl InvalidFrontmatter {
    /// Build a diagnostic anchored at `line`, highlighting the whole line.
    fn diagnostic(
        &self,
        message: String,
        skill: &super::scan::FoundSkill,
        line: usize,
        source_type: &str,
    ) -> Diagnostic {
        let end_col = skill
            .content
            .lines()
            .nth(line.saturating_sub(1))
            .map_or(4, |l| l.chars().count().saturating_add(1).max(2));
        Diagnostic {
            rule_id: self.id().to_string(),
            severity: self.default_severity(),
            message,
            file_path: skill.path.clone(),
            line: Some(line),
            col: Some(1),
            end_line: Some(line),
            end_col: Some(end_col),
            source_type: source_type.to_string(),
            help_text: None,
            help_url: None,
        }
    }
}

/// Line to anchor a structural failure on.
const fn error_line(err: &strict::Error) -> usize {
    match *err {
        strict::Error::InvalidYaml { line, .. } => line,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::test_helpers::MockFs;
    use std::path::PathBuf;

    fn skill_path() -> PathBuf {
        PathBuf::from(".ai/p/skills/s/SKILL.md")
    }

    fn check(content: &str) -> Vec<Diagnostic> {
        let mut fs = MockFs::new();
        let path = skill_path();
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), content.to_string());
        InvalidFrontmatter.check_file(&path, &fs).ok().unwrap_or_default()
    }

    #[test]
    fn no_file_returns_empty() {
        let fs = MockFs::new();
        let diags = InvalidFrontmatter.check_file(&skill_path(), &fs).ok().unwrap_or_default();
        assert!(diags.is_empty());
    }

    #[test]
    fn valid_frontmatter_produces_no_diagnostics() {
        assert!(check("---\nname: my-skill\ndescription: Does things\n---\nbody").is_empty());
    }

    #[test]
    fn valid_crlf_frontmatter_produces_no_diagnostics() {
        assert!(
            check("---\r\nname: my-skill\r\ndescription: Does things\r\n---\r\nbody").is_empty()
        );
    }

    #[test]
    fn leading_blank_line_is_reported() {
        let diags = check("\n---\nname: s\n---\nbody");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/invalid-frontmatter");
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].message.contains("start immediately"));
        assert_eq!(diags[0].line, Some(1));
    }

    #[test]
    fn byte_order_mark_is_called_out() {
        let diags = check("\u{feff}---\nname: s\n---\nbody");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("byte-order mark"));
    }

    #[test]
    fn missing_closing_delimiter_is_reported() {
        let diags = check("---\nname: s\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("not closed"));
    }

    #[test]
    fn inexact_closing_delimiter_is_reported() {
        let diags = check("---\nname: s\n----\nbody");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("not closed"));
    }

    #[test]
    fn invalid_yaml_is_reported() {
        let diags = check("---\nname: [unclosed\n---\nbody");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("not valid YAML"), "got {}", diags[0].message);
    }

    #[test]
    fn non_mapping_root_is_reported() {
        let diags = check("---\n- one\n- two\n---\nbody");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("mapping"));
    }

    #[test]
    fn no_frontmatter_at_all_is_reported() {
        let diags = check("# Just a heading\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("start immediately"));
    }

    #[test]
    fn non_string_name_is_reported() {
        let diags = check("---\nname: 42\ndescription: ok\n---\nbody");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("\"name\" must be a string, found integer"));
        assert_eq!(diags[0].line, Some(2));
    }

    #[test]
    fn non_string_description_is_reported() {
        let diags = check("---\nname: s\ndescription:\n  - a\n  - b\n---\nbody");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("\"description\" must be a string, found list"));
    }

    #[test]
    fn both_fields_non_string_produce_two_diagnostics() {
        let diags = check("---\nname: true\ndescription: 1.5\n---\nbody");
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn folded_description_is_a_string() {
        assert!(check("---\nname: s\ndescription: >\n  folded\n  text\n---\nbody").is_empty());
    }

    #[test]
    fn error_line_defaults_to_one() {
        assert_eq!(error_line(&strict::Error::RootNotMapping), 1);
        assert_eq!(error_line(&strict::Error::InvalidYaml { message: String::new(), line: 7 }), 7);
    }

    #[test]
    fn rule_metadata() {
        assert_eq!(InvalidFrontmatter.id(), "skill/invalid-frontmatter");
        assert_eq!(InvalidFrontmatter.name(), "invalid skill frontmatter");
        assert!(InvalidFrontmatter.help_url().is_some());
        assert!(InvalidFrontmatter.help_text().is_some());
    }
}
