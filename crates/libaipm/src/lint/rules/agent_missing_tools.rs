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

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        for agent in scan::scan_agents(source_dir, fs) {
            match agent.frontmatter {
                Some(ref fm) if fm.fields.contains_key("tools") => {},
                Some(_) | None => {
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        severity: self.default_severity(),
                        message: "agent definition missing tools declaration".to_string(),
                        file_path: agent.path,
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
}
