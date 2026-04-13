//! Rule: `skill/invalid-shell` — `shell` field not `bash` or `powershell`.
//!
//! Derived from Claude Code CLI binary analysis: validated against `["bash", "powershell"]`.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::scan;

/// Valid shell values (from Claude Code CLI).
const VALID_SHELLS: &[&str] = &["bash", "powershell"];

/// Checks that the `shell` frontmatter field is a valid value.
pub struct InvalidShell;

impl Rule for InvalidShell {
    fn id(&self) -> &'static str {
        "skill/invalid-shell"
    }

    fn name(&self) -> &'static str {
        "invalid shell value"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/skill/invalid-shell.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("use a supported shell value")
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let source_type = scan::source_type_from_path(file_path).to_string();
        let Some(skill) = scan::read_skill(file_path, fs) else {
            return Ok(vec![]);
        };
        let Some(ref fm) = skill.frontmatter else { return Ok(vec![]) };
        let Some(shell) = fm.fields.get("shell") else { return Ok(vec![]) };
        let normalized = shell.trim().to_lowercase();
        if VALID_SHELLS.contains(&normalized.as_str()) {
            return Ok(vec![]);
        }
        let shell_line = fm.field_lines.get("shell").copied();
        let (col, end_col) = shell_line
            .and_then(|n| skill.content.lines().nth(n - 1))
            .and_then(|line| crate::frontmatter::field_value_range(line, "shell"))
            .unzip();
        Ok(vec![Diagnostic {
            rule_id: self.id().to_string(),
            severity: self.default_severity(),
            message: format!("invalid shell value \"{shell}\", must be \"bash\" or \"powershell\""),
            file_path: skill.path,
            line: shell_line,
            col,
            end_line: shell_line,
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
        let result = InvalidShell.check_file(Path::new(".ai/p/skills/s/SKILL.md"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_valid_shell_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: s\nshell: bash\n---\nbody".to_string());

        let result = InvalidShell.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_invalid_shell_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: s\nshell: zsh\n---\nbody".to_string());

        let result = InvalidShell.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/invalid-shell");
        assert!(diags[0].message.contains("zsh"));
    }

    #[test]
    fn check_file_no_shell_field_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: s\n---\nbody".to_string());

        let result = InvalidShell.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_populates_col_and_end_col() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: s\nshell: zsh\n---\nbody".to_string());

        let diags = InvalidShell.check_file(&path, &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].col, Some(8));
        assert_eq!(diags[0].end_line, diags[0].line);
        assert_eq!(diags[0].end_col, Some(11));
    }

    #[test]
    fn check_file_no_frontmatter_returns_empty() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "no frontmatter here".to_string());

        let result = InvalidShell.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }
}
