//! Rule: `agent/missing-tools` — agent markdown missing `tools` frontmatter.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::scan;

/// Checks that agent definitions include a `tools` frontmatter field.
pub struct MissingTools;

impl Rule for MissingTools {
    fn id(&self) -> &'static str {
        "agent/missing-tools"
    }

    fn name(&self) -> &'static str {
        "missing agent tools"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/agent/missing-tools.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("add a \"tools\" field listing required tools")
    }

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        for agent in scan::scan_agents(source_dir, fs) {
            match agent.frontmatter {
                Some(ref fm) if fm.fields.contains_key("tools") => {},
                Some(ref fm) => {
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        severity: self.default_severity(),
                        message: "agent definition missing tools declaration".to_string(),
                        file_path: agent.path,
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
                        message: "agent definition missing tools declaration".to_string(),
                        file_path: agent.path,
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
        let Some(agent) = scan::read_agent(file_path, fs) else {
            return Ok(vec![]);
        };
        let diag = match agent.frontmatter {
            Some(ref fm) if fm.fields.contains_key("tools") => return Ok(vec![]),
            Some(ref fm) => Diagnostic {
                rule_id: self.id().to_string(),
                severity: self.default_severity(),
                message: "agent definition missing tools declaration".to_string(),
                file_path: agent.path,
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
                message: "agent definition missing tools declaration".to_string(),
                file_path: agent.path,
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
    fn agent_with_tools_no_finding() {
        let mut fs = MockFs::new();
        fs.add_agent("p", "reviewer", "---\nname: reviewer\ntools: Read,Write\n---\nPrompt");

        let result = MissingTools.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn agent_without_tools_warns() {
        let mut fs = MockFs::new();
        fs.add_agent("p", "reviewer", "---\nname: reviewer\ndescription: test\n---\nPrompt");

        let result = MissingTools.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "agent/missing-tools");
    }

    #[test]
    fn agent_no_frontmatter_warns() {
        let mut fs = MockFs::new();
        fs.add_agent("p", "reviewer", "Just a prompt with no frontmatter");

        let result = MissingTools.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "agent/missing-tools");
    }

    #[test]
    fn empty_ai_dir() {
        let mut fs = MockFs::new();
        fs.dirs.insert(std::path::PathBuf::from(".ai"), vec![]);

        let result = MissingTools.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn missing_tools_points_to_frontmatter_opener() {
        let mut fs = MockFs::new();
        fs.add_agent("p", "reviewer", "---\nname: reviewer\ndescription: test\n---\nPrompt");

        let diags = MissingTools.check(Path::new(".ai"), &fs).ok().unwrap_or_default();
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
        let result = MissingTools.check_file(Path::new(".ai/p/agents/reviewer.md"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_agent_with_tools_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/agents/reviewer.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: reviewer\ntools: Read\n---\nPrompt".to_string());

        let result = MissingTools.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_agent_without_tools_warns() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/agents/reviewer.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: reviewer\n---\nPrompt".to_string());

        let result = MissingTools.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "agent/missing-tools");
    }

    #[test]
    fn check_file_no_frontmatter_warns() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/agents/reviewer.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "Just a prompt with no frontmatter".to_string());

        let result = MissingTools.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "agent/missing-tools");
    }
}
