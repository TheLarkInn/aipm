//! Rule: `instructions/oversized` — instruction file exceeds size thresholds.
//!
//! Emits up to two diagnostics per file: one if the line count exceeds
//! `max_lines`, and one if the character count exceeds `max_chars`.  When
//! `resolve_imports` is enabled, transitive `@import` and relative markdown
//! links are followed and the aggregated size is checked instead.

use std::collections::HashSet;
use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::import_resolver;
use super::scan;

/// Default maximum line count for instruction files.
pub const DEFAULT_MAX_LINES: usize = 100;
/// Default maximum character count for instruction files.
pub const DEFAULT_MAX_CHARS: usize = 15_000;

/// Checks that instruction files don't exceed line and character limits.
pub struct Oversized {
    /// Maximum number of lines allowed.
    pub max_lines: usize,
    /// Maximum number of characters allowed.
    pub max_chars: usize,
    /// When `true`, `@import` and relative markdown link chains are followed
    /// and the resolved totals are checked instead of the direct file size.
    pub resolve_imports: bool,
}

impl Default for Oversized {
    fn default() -> Self {
        Self { max_lines: DEFAULT_MAX_LINES, max_chars: DEFAULT_MAX_CHARS, resolve_imports: false }
    }
}

impl Rule for Oversized {
    fn id(&self) -> &'static str {
        "instructions/oversized"
    }

    fn name(&self) -> &'static str {
        "oversized instruction file"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/instructions/oversized.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("reduce instruction file size below the configured line and character limits")
    }

    fn check(&self, _source_dir: &Path, _fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        // The unified discovery pipeline dispatches via check_file(); the legacy
        // check() path is unused for instruction files.
        Ok(vec![])
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let Ok(content) = fs.read_to_string(file_path) else { return Ok(vec![]) };

        let source_type_raw = scan::source_type_from_path(file_path);
        let source_type = if source_type_raw == "other" { "project" } else { source_type_raw };

        // Compute direct counts once; reuse them in the non-resolve-imports path.
        let direct_lines = content.lines().count();
        let direct_chars = content.len();

        let (checked_lines, checked_chars, is_resolved) = if self.resolve_imports {
            let mut visited = HashSet::new();
            let (lines, chars) = import_resolver::resolve_imports(file_path, fs, &mut visited);
            (lines, chars, true)
        } else {
            (direct_lines, direct_chars, false)
        };

        let mut diagnostics = Vec::new();

        if checked_lines > self.max_lines {
            let message = if is_resolved {
                format!(
                    "instruction file exceeds {} line limit (resolved total: {} lines, direct: {} lines)",
                    self.max_lines, checked_lines, direct_lines
                )
            } else {
                format!(
                    "instruction file exceeds {} line limit ({} lines)",
                    self.max_lines, checked_lines
                )
            };
            diagnostics.push(Diagnostic {
                rule_id: self.id().to_string(),
                severity: self.default_severity(),
                message,
                file_path: file_path.to_path_buf(),
                line: Some(1),
                col: None,
                end_line: None,
                end_col: None,
                source_type: source_type.to_string(),
                help_text: None,
                help_url: None,
            });
        }

        if checked_chars > self.max_chars {
            let message = if is_resolved {
                format!(
                    "instruction file exceeds {} character limit (resolved total: {} chars, direct: {} chars)",
                    self.max_chars, checked_chars, direct_chars
                )
            } else {
                format!(
                    "instruction file exceeds {} character limit ({} chars)",
                    self.max_chars, checked_chars
                )
            };
            diagnostics.push(Diagnostic {
                rule_id: self.id().to_string(),
                severity: self.default_severity(),
                message,
                file_path: file_path.to_path_buf(),
                line: Some(1),
                col: None,
                end_line: None,
                end_col: None,
                source_type: source_type.to_string(),
                help_text: None,
                help_url: None,
            });
        }

        Ok(diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::lint::rules::test_helpers::MockFs;

    fn make_rule() -> Oversized {
        Oversized::default()
    }

    fn make_file(fs: &mut MockFs, path: &str, content: &str) -> PathBuf {
        let p = PathBuf::from(path);
        fs.exists.insert(p.clone());
        fs.files.insert(p.clone(), content.to_string());
        p
    }

    fn lines(n: usize) -> String {
        (0..n).map(|i| format!("line {i}\n")).collect()
    }

    // --- basic threshold tests ---

    #[test]
    fn small_file_no_finding() {
        let mut fs = MockFs::new();
        let path = make_file(&mut fs, "CLAUDE.md", "short content\n");
        let result = make_rule().check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn exactly_at_line_limit_no_finding() {
        let content = lines(DEFAULT_MAX_LINES);
        let mut fs = MockFs::new();
        let path = make_file(&mut fs, "CLAUDE.md", &content);
        let result = make_rule().check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn exactly_at_char_limit_no_finding() {
        let content = "x".repeat(DEFAULT_MAX_CHARS);
        let mut fs = MockFs::new();
        let path = make_file(&mut fs, "CLAUDE.md", &content);
        let result = make_rule().check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn over_line_limit_one_diagnostic() {
        let content = lines(DEFAULT_MAX_LINES + 1);
        let mut fs = MockFs::new();
        let path = make_file(&mut fs, "CLAUDE.md", &content);
        let diags = make_rule().check_file(&path, &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "instructions/oversized");
        assert!(diags[0].message.contains("line limit"));
    }

    #[test]
    fn over_char_limit_one_diagnostic() {
        let content = "x".repeat(DEFAULT_MAX_CHARS + 1);
        let mut fs = MockFs::new();
        let path = make_file(&mut fs, "CLAUDE.md", &content);
        let diags = make_rule().check_file(&path, &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("character limit"));
    }

    #[test]
    fn both_limits_exceeded_two_diagnostics() {
        // Build content that exceeds both limits
        let line_content = format!("{}\n", "x".repeat(200));
        let content = line_content.repeat(DEFAULT_MAX_LINES + 1);
        let mut fs = MockFs::new();
        let path = make_file(&mut fs, "CLAUDE.md", &content);
        let rule = make_rule();
        let diags = rule.check_file(&path, &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn empty_file_no_finding() {
        let mut fs = MockFs::new();
        let path = make_file(&mut fs, "CLAUDE.md", "");
        let diags = make_rule().check_file(&path, &fs).ok().unwrap_or_default();
        assert!(diags.is_empty());
    }

    #[test]
    fn missing_file_returns_empty() {
        let fs = MockFs::new();
        let path = PathBuf::from("CLAUDE.md");
        let diags = make_rule().check_file(&path, &fs).ok().unwrap_or_default();
        assert!(diags.is_empty());
    }

    #[test]
    fn case_insensitive_detection_claude_lowercase() {
        let content = lines(DEFAULT_MAX_LINES + 1);
        let mut fs = MockFs::new();
        let path = make_file(&mut fs, "claude.md", &content);
        let diags = make_rule().check_file(&path, &fs).ok().unwrap_or_default();
        assert!(!diags.is_empty());
    }

    #[test]
    fn instructions_md_suffix() {
        let content = lines(DEFAULT_MAX_LINES + 1);
        let mut fs = MockFs::new();
        let path = make_file(&mut fs, "frontend.instructions.md", &content);
        let diags = make_rule().check_file(&path, &fs).ok().unwrap_or_default();
        assert!(!diags.is_empty());
    }

    #[test]
    fn subdirectory_detection() {
        let content = lines(DEFAULT_MAX_LINES + 1);
        let mut fs = MockFs::new();
        let path = make_file(&mut fs, "packages/auth/CLAUDE.md", &content);
        let diags = make_rule().check_file(&path, &fs).ok().unwrap_or_default();
        assert!(!diags.is_empty());
    }

    #[test]
    fn custom_thresholds_trigger_at_lower_limit() {
        let rule =
            Oversized { max_lines: 50, max_chars: DEFAULT_MAX_CHARS, resolve_imports: false };
        let content = lines(51);
        let mut fs = MockFs::new();
        let path = make_file(&mut fs, "CLAUDE.md", &content);
        let diags = rule.check_file(&path, &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("50 line limit"));
    }

    #[test]
    fn source_type_project_for_root_file() {
        let content = lines(DEFAULT_MAX_LINES + 1);
        let mut fs = MockFs::new();
        let path = make_file(&mut fs, "CLAUDE.md", &content);
        let diags = make_rule().check_file(&path, &fs).ok().unwrap_or_default();
        assert_eq!(diags[0].source_type, "project");
    }

    #[test]
    fn source_type_claude_for_dotclaude_file() {
        let content = lines(DEFAULT_MAX_LINES + 1);
        let mut fs = MockFs::new();
        let path = make_file(&mut fs, ".claude/CLAUDE.md", &content);
        let diags = make_rule().check_file(&path, &fs).ok().unwrap_or_default();
        assert_eq!(diags[0].source_type, ".claude");
    }

    // --- resolve-imports integration ---

    #[test]
    fn resolve_imports_basic() {
        let rule = Oversized { max_lines: 5, max_chars: 10_000, resolve_imports: true };
        let mut fs = MockFs::new();
        let main = make_file(&mut fs, "CLAUDE.md", "@shared.md\nline2\nline3");
        let shared = PathBuf::from("shared.md");
        fs.exists.insert(shared.clone());
        fs.files.insert(shared, "a\nb\nc\n".to_string());

        // main: 3 lines, shared: 3 lines → total 6 > max_lines 5
        let diags = rule.check_file(&main, &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("resolved total"));
    }

    #[test]
    fn resolve_imports_circular_no_loop() {
        let rule = Oversized { max_lines: 1, max_chars: 10_000, resolve_imports: true };
        let mut fs = MockFs::new();
        let a = PathBuf::from("a.md");
        let b = PathBuf::from("b.md");
        fs.exists.insert(a.clone());
        fs.files.insert(a.clone(), "@b.md\ncontent a".to_string());
        fs.exists.insert(b.clone());
        fs.files.insert(b, "@a.md\ncontent b".to_string());

        // Should not loop
        let diags = rule.check_file(&a, &fs).ok().unwrap_or_default();
        assert!(!diags.is_empty());
    }

    #[test]
    fn resolve_imports_diagnostic_message_includes_both_counts() {
        let rule = Oversized { max_lines: 3, max_chars: 10_000, resolve_imports: true };
        let mut fs = MockFs::new();
        let main = make_file(&mut fs, "CLAUDE.md", "@shared.md\nline2\nline3");
        let shared = PathBuf::from("shared.md");
        fs.exists.insert(shared.clone());
        fs.files.insert(shared, "a\nb\n".to_string());

        // total 5 lines > 3
        let diags = rule.check_file(&main, &fs).ok().unwrap_or_default();
        assert!(!diags.is_empty());
        assert!(diags[0].message.contains("resolved total"));
        assert!(diags[0].message.contains("direct"));
    }

    #[test]
    fn resolve_imports_under_limit_no_finding() {
        let rule = Oversized { max_lines: 100, max_chars: 100_000, resolve_imports: true };
        let mut fs = MockFs::new();
        let main = make_file(&mut fs, "CLAUDE.md", "@shared.md\nline2");
        let shared = PathBuf::from("shared.md");
        fs.exists.insert(shared.clone());
        fs.files.insert(shared, "one line".to_string());

        let diags = rule.check_file(&main, &fs).ok().unwrap_or_default();
        assert!(diags.is_empty());
    }

    #[test]
    fn resolve_imports_markdown_link_followed() {
        let rule = Oversized { max_lines: 2, max_chars: 10_000, resolve_imports: true };
        let mut fs = MockFs::new();
        let main = make_file(&mut fs, "CLAUDE.md", "See [other](linked.md)\ntext");
        let linked = PathBuf::from("linked.md");
        fs.exists.insert(linked.clone());
        fs.files.insert(linked, "extra\nextra2\n".to_string());

        // main: 2 lines, linked: 2 lines → total 4 > 2
        let diags = rule.check_file(&main, &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn resolve_imports_disabled_by_default() {
        let rule = Oversized::default();
        assert!(!rule.resolve_imports);
    }

    #[test]
    fn check_method_returns_empty() {
        let rule = make_rule();
        let fs = MockFs::new();
        let result = rule.check(Path::new("."), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }
}
