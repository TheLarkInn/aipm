//! Output reporters for lint diagnostics.

use std::io::Write;
use std::path::Path;

use annotate_snippets::{Level, Renderer, Snippet};

use crate::fs::Fs;

use super::{Diagnostic, Outcome, Severity};

/// Format and write lint results.
pub trait Reporter {
    /// Write the lint outcome to the given writer.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if writing fails.
    fn report(&self, outcome: &Outcome, writer: &mut dyn Write) -> std::io::Result<()>;
}

/// Human-readable text reporter, modeled after clippy/rustc output.
pub struct Text;

impl Reporter for Text {
    fn report(&self, outcome: &Outcome, writer: &mut dyn Write) -> std::io::Result<()> {
        for d in &outcome.diagnostics {
            write_diagnostic(d, writer)?;
        }

        if outcome.error_count == 0 && outcome.warning_count == 0 {
            writeln!(writer, "no issues found")?;
        } else {
            if outcome.warning_count > 0 {
                writeln!(
                    writer,
                    "{}: {} warning(s) emitted",
                    Severity::Warning,
                    outcome.warning_count
                )?;
            }
            if outcome.error_count > 0 {
                writeln!(writer, "{}: {} error(s) emitted", Severity::Error, outcome.error_count)?;
            }
        }

        Ok(())
    }
}

fn write_diagnostic(d: &Diagnostic, writer: &mut dyn Write) -> std::io::Result<()> {
    writeln!(writer, "{}[{}]: {}", d.severity, d.rule_id, d.message)?;
    let line_suffix = d.line.map_or_else(String::new, |l| format!(":{l}"));
    writeln!(writer, "  --> {}{}", d.file_path.display(), line_suffix)?;
    writeln!(writer, "  |")?;
    Ok(())
}

/// Color choice for the Human reporter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    /// Never emit ANSI color codes.
    Never,
    /// Auto-detect based on TTY, `NO_COLOR`, and `CLICOLOR`.
    Auto,
    /// Always emit ANSI color codes.
    Always,
}

impl ColorChoice {
    /// Resolve the color choice into a boolean using environment detection.
    pub fn should_color(self) -> bool {
        match self {
            Self::Never => false,
            Self::Always => true,
            Self::Auto => {
                // Respect NO_COLOR (https://no-color.org/)
                if std::env::var_os("NO_COLOR").is_some() {
                    return false;
                }
                // Respect CLICOLOR=0
                if std::env::var("CLICOLOR").ok().as_deref() == Some("0") {
                    return false;
                }
                // Check if stdout is a TTY
                anstyle_query::is_ci() || anstyle_query::term_supports_ansi_color()
            },
        }
    }
}

/// Rich colored terminal reporter with source code snippets.
///
/// Uses `annotate-snippets` for rustc-style diagnostic rendering with
/// ANSI color support. Reads source files via `&dyn Fs` at render time.
pub struct Human<'a> {
    /// Filesystem for reading source files to render snippets.
    pub fs: &'a dyn Fs,
    /// Color output mode.
    pub color: ColorChoice,
    /// Base directory for resolving diagnostic file paths.
    pub base_dir: &'a Path,
}

impl Reporter for Human<'_> {
    fn report(&self, outcome: &Outcome, writer: &mut dyn Write) -> std::io::Result<()> {
        let renderer =
            if self.color.should_color() { Renderer::styled() } else { Renderer::plain() };

        for d in &outcome.diagnostics {
            self.write_rich_diagnostic(d, &renderer, writer)?;
        }

        if outcome.error_count == 0 && outcome.warning_count == 0 {
            writeln!(writer, "no issues found")?;
        } else {
            if outcome.warning_count > 0 {
                writeln!(
                    writer,
                    "{}: {} warning(s) emitted",
                    Severity::Warning,
                    outcome.warning_count
                )?;
            }
            if outcome.error_count > 0 {
                writeln!(writer, "{}: {} error(s) emitted", Severity::Error, outcome.error_count)?;
            }
        }

        Ok(())
    }
}

impl Human<'_> {
    fn write_rich_diagnostic(
        &self,
        d: &Diagnostic,
        renderer: &Renderer,
        writer: &mut dyn Write,
    ) -> std::io::Result<()> {
        let level = match d.severity {
            Severity::Error => Level::Error,
            Severity::Warning => Level::Warning,
        };

        // Try to read the source file for snippet context
        let file_path = self.base_dir.join(&d.file_path);
        let source_content = self.fs.read_to_string(&file_path).ok();

        let origin = d.file_path.display().to_string();

        // Pre-compute snippet source string so it outlives the message borrow
        let snippet_data = if let (Some(line), Some(ref content)) = (d.line, &source_content) {
            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len();

            let target_idx = line.saturating_sub(1);
            let start_idx = target_idx.saturating_sub(1);
            let end_idx = (target_idx + 2).min(total_lines);

            if start_idx < total_lines && start_idx < end_idx {
                let context_lines: Vec<&str> =
                    lines.get(start_idx..end_idx).unwrap_or(&[]).to_vec();
                let snippet_source = context_lines.join("\n");

                let mut byte_offset = 0;
                for (i, ctx_line) in context_lines.iter().enumerate() {
                    if start_idx + i == target_idx {
                        break;
                    }
                    byte_offset += ctx_line.len() + 1;
                }

                let target_line_in_snippet =
                    context_lines.get(target_idx - start_idx).unwrap_or(&"");
                let target_line_len = target_line_in_snippet.len();

                let (span_start, span_end) = if let (Some(col), Some(end_col)) = (d.col, d.end_col)
                {
                    let start = byte_offset + col.saturating_sub(1);
                    let end = byte_offset + end_col.min(target_line_len);
                    (start, end)
                } else if let Some(col) = d.col {
                    let start = byte_offset + col.saturating_sub(1);
                    let end = (start + 1).min(byte_offset + target_line_len);
                    (start, end)
                } else {
                    (byte_offset, byte_offset + target_line_len)
                };

                let snippet_len = snippet_source.len();
                let span_start = span_start.min(snippet_len);
                let span_end = span_end.min(snippet_len).max(span_start);

                Some((snippet_source, start_idx, span_start, span_end))
            } else {
                None
            }
        } else {
            None
        };

        // Build the annotate-snippets Message
        let mut message = level.title(&d.message).id(&d.rule_id);

        // Add snippet if we have pre-computed data
        if let Some((ref snippet_source, start_idx, span_start, span_end)) = snippet_data {
            let snippet = Snippet::source(snippet_source)
                .line_start(start_idx + 1) // 1-based
                .origin(&origin)
                .annotation(level.span(span_start..span_end));
            message = message.snippet(snippet);
        } else if d.line.is_none() {
            // Directory-level diagnostic — show origin without snippet
            let snippet = Snippet::source("").origin(&origin);
            message = message.snippet(snippet);
        }

        // Add help text and help URL as footers
        if let Some(ref help_text) = d.help_text {
            message = message.footer(Level::Help.title(help_text));
        }
        if let Some(ref help_url) = d.help_url {
            let link_msg = format!("for further information visit {help_url}");
            // We need to render the link message inline since footer takes &str
            // but we have a String. Use a two-step approach.
            let msg_output = renderer.render(message);
            write!(writer, "{msg_output}")?;
            let footer = Level::Help.title(&link_msg);
            let footer_output = renderer.render(footer);
            writeln!(writer, "{footer_output}")?;
            return Ok(());
        }

        let msg_output = renderer.render(message);
        writeln!(writer, "{msg_output}")?;

        Ok(())
    }
}

/// JSON reporter for CI/tooling integration.
pub struct Json;

impl Reporter for Json {
    fn report(&self, outcome: &Outcome, writer: &mut dyn Write) -> std::io::Result<()> {
        // Build JSON manually to avoid serde_json dependency in this module.
        // The structure matches the spec: { diagnostics: [...], summary: {...} }
        writeln!(writer, "{{")?;
        writeln!(writer, "  \"diagnostics\": [")?;

        for (i, d) in outcome.diagnostics.iter().enumerate() {
            let comma = if i + 1 < outcome.diagnostics.len() { "," } else { "" };
            let line_str = d.line.map_or_else(|| "null".to_string(), |l| l.to_string());
            writeln!(writer, "    {{")?;
            writeln!(writer, "      \"rule_id\": \"{}\",", d.rule_id)?;
            writeln!(writer, "      \"severity\": \"{}\",", d.severity)?;
            writeln!(writer, "      \"message\": \"{}\",", escape_json_string(&d.message))?;
            writeln!(
                writer,
                "      \"file_path\": \"{}\",",
                escape_json_string(&d.file_path.display().to_string())
            )?;
            writeln!(writer, "      \"line\": {line_str},")?;
            writeln!(writer, "      \"source_type\": \"{}\"", d.source_type)?;
            writeln!(writer, "    }}{comma}")?;
        }

        writeln!(writer, "  ],")?;
        writeln!(writer, "  \"summary\": {{")?;
        writeln!(writer, "    \"errors\": {},", outcome.error_count)?;
        writeln!(writer, "    \"warnings\": {},", outcome.warning_count)?;

        write!(writer, "    \"sources_scanned\": [")?;
        for (i, s) in outcome.sources_scanned.iter().enumerate() {
            if i > 0 {
                write!(writer, ", ")?;
            }
            write!(writer, "\"{}\"", escape_json_string(s))?;
        }
        writeln!(writer, "]")?;

        writeln!(writer, "  }}")?;
        writeln!(writer, "}}")?;

        Ok(())
    }
}

fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_outcome() -> Outcome {
        Outcome {
            diagnostics: vec![
                Diagnostic {
                    rule_id: "skill/missing-description".to_string(),
                    severity: Severity::Warning,
                    message: "SKILL.md missing required field: description".to_string(),
                    file_path: PathBuf::from(".ai/my-plugin/skills/default/SKILL.md"),
                    line: Some(1),
                    col: None,
                    end_line: None,
                    end_col: None,
                    source_type: ".ai".to_string(),
                    help_text: None,
                    help_url: None,
                },
                Diagnostic {
                    rule_id: "hook/unknown-event".to_string(),
                    severity: Severity::Error,
                    message: "unknown hook event: InvalidEvent".to_string(),
                    file_path: PathBuf::from(".ai/my-plugin/hooks/hooks.json"),
                    line: Some(5),
                    col: None,
                    end_line: None,
                    end_col: None,
                    source_type: ".ai".to_string(),
                    help_text: None,
                    help_url: None,
                },
            ],
            error_count: 1,
            warning_count: 1,
            sources_scanned: vec![".claude".to_string(), ".ai".to_string()],
        }
    }

    #[test]
    fn text_reporter_formats_diagnostics() {
        let outcome = sample_outcome();
        let mut buf = Vec::new();
        Text.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("warning[skill/missing-description]"));
        assert!(output.contains("error[hook/unknown-event]"));
        assert!(output.contains("1 warning(s) emitted"));
        assert!(output.contains("1 error(s) emitted"));
    }

    #[test]
    fn text_reporter_no_issues() {
        let outcome = Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        };
        let mut buf = Vec::new();
        Text.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("no issues found"));
    }

    #[test]
    fn json_reporter_valid_json() {
        let outcome = sample_outcome();
        let mut buf = Vec::new();
        Json.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("\"diagnostics\""));
        assert!(output.contains("\"summary\""));
        assert!(output.contains("\"skill/missing-description\""));
        assert!(output.contains("\"errors\": 1"));
        assert!(output.contains("\"warnings\": 1"));
    }

    #[test]
    fn json_reporter_empty() {
        let outcome = Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![".ai".to_string()],
        };
        let mut buf = Vec::new();
        Json.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("\"diagnostics\": ["));
        assert!(output.contains("\"errors\": 0"));
    }

    #[test]
    fn text_reporter_file_path_and_line() {
        let outcome = sample_outcome();
        let mut buf = Vec::new();
        Text.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("--> .ai/my-plugin/skills/default/SKILL.md:1"));
        assert!(output.contains("--> .ai/my-plugin/hooks/hooks.json:5"));
    }

    #[test]
    fn escape_json_string_special_chars() {
        assert_eq!(escape_json_string("hello \"world\""), "hello \\\"world\\\"");
        assert_eq!(escape_json_string("line1\nline2"), "line1\\nline2");
        assert_eq!(escape_json_string("path\\to\\file"), "path\\\\to\\\\file");
    }

    // --- Human reporter tests ---

    use crate::lint::rules::test_helpers::MockFs;

    fn make_human_reporter(fs: &dyn Fs) -> Human<'_> {
        Human { fs, color: ColorChoice::Never, base_dir: Path::new("/project") }
    }

    #[test]
    fn human_reporter_no_color_output() {
        let mut mock_fs = MockFs::new();
        mock_fs.files.insert(
            PathBuf::from("/project/.ai/my-plugin/skills/default/SKILL.md"),
            "---\nname: my-skill\n---\nbody content".to_string(),
        );
        let reporter = make_human_reporter(&mock_fs);
        let outcome = sample_outcome();
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        // Should contain rule id and message
        assert!(output.contains("skill/missing-description"));
        assert!(output.contains("SKILL.md missing required field: description"));
        // Should not contain ANSI escape codes
        assert!(!output.contains("\x1b["));
        // Should have summary
        assert!(output.contains("warning(s) emitted"));
        assert!(output.contains("error(s) emitted"));
    }

    #[test]
    fn human_reporter_renders_snippet_with_source() {
        let mut mock_fs = MockFs::new();
        mock_fs.files.insert(
            PathBuf::from("/project/test.md"),
            "line one\nline two\nline three\nline four".to_string(),
        );
        let reporter = make_human_reporter(&mock_fs);
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/rule".into(),
                severity: Severity::Warning,
                message: "test warning".into(),
                file_path: PathBuf::from("test.md"),
                line: Some(2),
                col: None,
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: None,
                help_url: None,
            }],
            error_count: 0,
            warning_count: 1,
            sources_scanned: vec![],
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        // Should contain context lines around line 2
        assert!(output.contains("line one"));
        assert!(output.contains("line two"));
        assert!(output.contains("line three"));
    }

    #[test]
    fn human_reporter_directory_level_no_snippet() {
        let mock_fs = MockFs::new();
        let reporter = make_human_reporter(&mock_fs);
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "source/misplaced-features".into(),
                severity: Severity::Warning,
                message: "skill found in .claude/".into(),
                file_path: PathBuf::from(".claude/skills/code-review/"),
                line: None,
                col: None,
                end_line: None,
                end_col: None,
                source_type: ".claude".into(),
                help_text: None,
                help_url: None,
            }],
            error_count: 0,
            warning_count: 1,
            sources_scanned: vec![],
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("source/misplaced-features"));
        assert!(output.contains(".claude/skills/code-review/"));
    }

    #[test]
    fn human_reporter_renders_help_text() {
        let mock_fs = MockFs::new();
        let reporter = make_human_reporter(&mock_fs);
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/rule".into(),
                severity: Severity::Warning,
                message: "test".into(),
                file_path: PathBuf::from("test.md"),
                line: None,
                col: None,
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: Some("add a name field".into()),
                help_url: None,
            }],
            error_count: 0,
            warning_count: 1,
            sources_scanned: vec![],
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("add a name field"));
    }

    #[test]
    fn human_reporter_renders_help_url() {
        let mock_fs = MockFs::new();
        let reporter = make_human_reporter(&mock_fs);
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/rule".into(),
                severity: Severity::Warning,
                message: "test".into(),
                file_path: PathBuf::from("test.md"),
                line: None,
                col: None,
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: None,
                help_url: Some("https://example.com/rules/test".into()),
            }],
            error_count: 0,
            warning_count: 1,
            sources_scanned: vec![],
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("https://example.com/rules/test"));
    }

    #[test]
    fn human_reporter_no_issues() {
        let mock_fs = MockFs::new();
        let reporter = make_human_reporter(&mock_fs);
        let outcome = Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("no issues found"));
    }

    #[test]
    fn human_reporter_graceful_missing_file() {
        // File doesn't exist in mock fs — should still render without snippet
        let mock_fs = MockFs::new();
        let reporter = make_human_reporter(&mock_fs);
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/rule".into(),
                severity: Severity::Error,
                message: "test error".into(),
                file_path: PathBuf::from("nonexistent.md"),
                line: Some(1),
                col: None,
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: None,
                help_url: None,
            }],
            error_count: 1,
            warning_count: 0,
            sources_scanned: vec![],
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        // Should still render the diagnostic header even without file
        assert!(output.contains("test/rule"));
        assert!(output.contains("test error"));
    }
}
