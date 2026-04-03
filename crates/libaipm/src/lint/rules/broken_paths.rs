//! Rule: `plugin/broken-paths` — broken file references in skill markdown.
//!
//! Checks `${CLAUDE_SKILL_DIR}/` and `${SKILL_DIR}/` references in `SKILL.md` files.
//! Validates that referenced script paths exist on disk. Rejects path traversal
//! (`..`) and absolute paths for security.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::scan;

/// Variable prefixes used in skill file references.
const VARIABLE_PREFIXES: &[&str] = &["${CLAUDE_SKILL_DIR}/", "${SKILL_DIR}/"];

/// Checks that file references in plugin markdown point to existing files.
pub struct BrokenPaths;

impl Rule for BrokenPaths {
    fn id(&self) -> &'static str {
        "plugin/broken-paths"
    }

    fn name(&self) -> &'static str {
        "broken file paths"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/plugin/broken-paths.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("fix or remove the broken file reference")
    }

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        for skill in scan::scan_skills(source_dir, fs) {
            // The skill dir is the parent of SKILL.md
            let Some(skill_dir) = skill.path.parent() else {
                continue;
            };

            for prefix in VARIABLE_PREFIXES {
                for (line_num, line) in skill.content.lines().enumerate() {
                    let mut search = line;
                    let mut col_offset: usize = 0;
                    while let Some(pos) = search.find(prefix) {
                        let after = &search[pos + prefix.len()..];
                        let end = after
                            .find(|c: char| {
                                c.is_whitespace() || c == '"' || c == '\'' || c == '`' || c == ')'
                            })
                            .unwrap_or(after.len());
                        let ref_path = &after[..end];
                        if !ref_path.is_empty()
                            && !ref_path.starts_with('/')
                            && !ref_path.contains("..")
                        {
                            let resolved = skill_dir.join(ref_path);
                            if !fs.exists(&resolved) {
                                diagnostics.push(Diagnostic {
                                    rule_id: self.id().to_string(),
                                    severity: self.default_severity(),
                                    message: format!(
                                        "broken reference: {prefix}{ref_path} (file not found: {})",
                                        resolved.display()
                                    ),
                                    file_path: skill.path.clone(),
                                    line: Some(line_num + 1),
                                    col: Some(col_offset + pos + 1),
                                    end_line: None,
                                    end_col: None,
                                    source_type: ".ai".to_string(),
                                    help_text: None,
                                    help_url: None,
                                });
                            }
                        }
                        col_offset += pos + prefix.len() + end;
                        search = &search[pos + prefix.len() + end..];
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
    fn no_finding_when_reference_exists() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\nRun `${CLAUDE_SKILL_DIR}/scripts/deploy.sh`");
        // Mark the referenced file as existing
        fs.add_existing(".ai/p/skills/s/scripts/deploy.sh");

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn finding_when_reference_broken() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\nRun `${CLAUDE_SKILL_DIR}/scripts/deploy.sh`");
        // Don't mark the file as existing

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "plugin/broken-paths");
        assert!(diags[0].message.contains("deploy.sh"));
    }

    #[test]
    fn finding_with_skill_dir_prefix() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\nRun `${SKILL_DIR}/scripts/run.sh`");

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("run.sh"));
    }

    #[test]
    fn no_references_no_findings() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\nJust plain text");

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn multiple_references_mixed() {
        let mut fs = MockFs::new();
        let content = "---\nname: s\n---\n\
            Run `${CLAUDE_SKILL_DIR}/scripts/good.sh`\n\
            Also `${CLAUDE_SKILL_DIR}/scripts/bad.sh`";
        fs.add_skill("p", "s", content);
        fs.add_existing(".ai/p/skills/s/scripts/good.sh");
        // bad.sh doesn't exist

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bad.sh"));
    }

    #[test]
    fn reference_terminated_by_quote() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\nRun \"${CLAUDE_SKILL_DIR}/scripts/x.sh\" here");

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x.sh"));
    }

    #[test]
    fn empty_ai_dir() {
        let mut fs = MockFs::new();
        fs.dirs.insert(std::path::PathBuf::from(".ai"), vec![]);

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn reference_at_end_of_line() {
        let mut fs = MockFs::new();
        // Reference at the very end of the line, no terminator
        fs.add_skill("p", "s", "---\nname: s\n---\n${CLAUDE_SKILL_DIR}/scripts/x.sh");

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x.sh"));
    }

    #[test]
    fn absolute_path_rejected() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\n${CLAUDE_SKILL_DIR}//etc/passwd");

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        // Absolute path is silently rejected (security)
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn path_traversal_rejected() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\n${CLAUDE_SKILL_DIR}/../../../etc/passwd");

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        // Path traversal is silently rejected (security)
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn reference_terminated_by_backtick() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\n`${CLAUDE_SKILL_DIR}/scripts/x.sh`");

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn reference_terminated_by_single_quote() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\n'${CLAUDE_SKILL_DIR}/scripts/x.sh'");

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn reference_terminated_by_paren() {
        let mut fs = MockFs::new();
        fs.add_skill("p", "s", "---\nname: s\n---\n(${CLAUDE_SKILL_DIR}/scripts/x.sh)");

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn empty_ref_path_ignored() {
        let mut fs = MockFs::new();
        // Empty reference: ${CLAUDE_SKILL_DIR}/ immediately followed by whitespace
        fs.add_skill("p", "s", "---\nname: s\n---\n${CLAUDE_SKILL_DIR}/ ");

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        // Empty path should be skipped
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn line_number_is_correct() {
        let mut fs = MockFs::new();
        let content = "---\nname: s\n---\nline1\nline2\n${CLAUDE_SKILL_DIR}/scripts/x.sh";
        fs.add_skill("p", "s", content);

        let result = BrokenPaths.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        // The reference is on line 6
        assert_eq!(diags[0].line, Some(6));
    }
}
