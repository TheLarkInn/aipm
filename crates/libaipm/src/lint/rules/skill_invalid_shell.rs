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

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        for skill in scan::scan_skills(source_dir, fs) {
            if let Some(ref fm) = skill.frontmatter {
                if let Some(shell) = fm.fields.get("shell") {
                    let normalized = shell.trim().to_lowercase();
                    if !VALID_SHELLS.contains(&normalized.as_str()) {
                        diagnostics.push(Diagnostic {
                            rule_id: self.id().to_string(),
                            severity: self.default_severity(),
                            message: format!(
                                "invalid shell value \"{shell}\", must be \"bash\" or \"powershell\""
                            ),
                            file_path: skill.path,
                            line: fm.field_lines.get("shell").copied(),
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
        let Some(shell) = fm.fields.get("shell") else { return Ok(vec![]) };
        let normalized = shell.trim().to_lowercase();
        if VALID_SHELLS.contains(&normalized.as_str()) {
            return Ok(vec![]);
        }
        Ok(vec![Diagnostic {
            rule_id: self.id().to_string(),
            severity: self.default_severity(),
            message: format!("invalid shell value \"{shell}\", must be \"bash\" or \"powershell\""),
            file_path: skill.path,
            line: fm.field_lines.get("shell").copied(),
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
    fn bash_is_valid() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\nshell: bash\n---\nbody");

        let result = InvalidShell.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn powershell_is_valid() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\nshell: powershell\n---\nbody");

        let result = InvalidShell.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn case_insensitive() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\nshell: Bash\n---\nbody");

        let result = InvalidShell.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn zsh_is_invalid() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\nshell: zsh\n---\nbody");

        let result = InvalidShell.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "skill/invalid-shell");
        assert!(diags[0].message.contains("zsh"));
    }

    #[test]
    fn no_shell_field_no_finding() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\nbody");

        let result = InvalidShell.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn no_frontmatter_no_finding() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "no frontmatter here");

        let result = InvalidShell.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    // --- check_file() tests ---

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
}
