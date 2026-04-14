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

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let source_type = scan::source_type_from_path(file_path).to_string();
        let Some(skill) = scan::read_skill(file_path, fs) else {
            return Ok(vec![]);
        };
        let Some(skill_dir) = skill.path.parent() else {
            return Ok(vec![]);
        };
        let mut diagnostics = Vec::new();
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
                                end_line: Some(line_num + 1),
                                end_col: Some(col_offset + pos + prefix.len() + end + 1),
                                source_type: source_type.clone(),
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
        Ok(diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::lint::rules::test_helpers::MockFs;

    #[test]
    fn check_file_no_skill_file_returns_empty() {
        let fs = MockFs::new();
        let result = BrokenPaths.check_file(Path::new(".ai/p/skills/s/SKILL.md"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_broken_reference_produces_diagnostic() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(
            path.clone(),
            "---\nname: s\n---\nRun `${CLAUDE_SKILL_DIR}/scripts/deploy.sh`\n".to_string(),
        );
        // Do NOT add the referenced file to fs.exists

        let result = BrokenPaths.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "plugin/broken-paths");
        assert!(diags[0].message.contains("deploy.sh"));
    }

    #[test]
    fn check_file_existing_reference_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(
            path.clone(),
            "---\nname: s\n---\nRun `${CLAUDE_SKILL_DIR}/scripts/deploy.sh`\n".to_string(),
        );
        fs.exists.insert(PathBuf::from(".ai/p/skills/s/scripts/deploy.sh"));

        let result = BrokenPaths.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_absolute_path_rejected() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(
            path.clone(),
            "---\nname: s\n---\n${CLAUDE_SKILL_DIR}//etc/passwd\n".to_string(),
        );

        let result = BrokenPaths.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_path_traversal_rejected() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(
            path.clone(),
            "---\nname: s\n---\n${CLAUDE_SKILL_DIR}/../../../etc/passwd\n".to_string(),
        );

        let result = BrokenPaths.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_empty_ref_path_skipped() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: s\n---\n${CLAUDE_SKILL_DIR}/ \n".to_string());

        let result = BrokenPaths.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_multiple_references_same_line() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(
            path.clone(),
            "---\nname: s\n---\n${CLAUDE_SKILL_DIR}/a.sh ${CLAUDE_SKILL_DIR}/b.sh\n".to_string(),
        );

        let result = BrokenPaths.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().any(|d| d.message.contains("a.sh")));
        assert!(diags.iter().any(|d| d.message.contains("b.sh")));
    }

    #[test]
    fn check_file_skill_dir_prefix() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(
            path.clone(),
            "---\nname: s\n---\nRun `${SKILL_DIR}/scripts/run.sh`\n".to_string(),
        );

        let result = BrokenPaths.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("run.sh"));
    }

    #[test]
    fn check_file_reference_terminated_by_double_quote() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(
            path.clone(),
            "---\nname: s\n---\nRun \"${CLAUDE_SKILL_DIR}/scripts/x.sh\" here\n".to_string(),
        );

        let result = BrokenPaths.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x.sh"));
    }

    #[test]
    fn check_file_reference_terminated_by_single_quote() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.exists.insert(path.clone());
        fs.files.insert(
            path.clone(),
            "---\nname: s\n---\nRun '${CLAUDE_SKILL_DIR}/scripts/x.sh' here\n".to_string(),
        );

        let result = BrokenPaths.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x.sh"));
    }

    #[test]
    fn check_file_root_path_no_parent_returns_empty() {
        // `Path::new("/").parent()` returns `None` on Unix, exercising the
        // `else` branch of `let Some(skill_dir) = skill.path.parent()` in
        // `check_file` when `read_skill` succeeds but the skill path has no parent.
        let mut fs = MockFs::new();
        fs.files
            .insert(PathBuf::from("/"), "---\nname: root-skill\n---\nSome content\n".to_string());
        let result = BrokenPaths.check_file(Path::new("/"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }
}
