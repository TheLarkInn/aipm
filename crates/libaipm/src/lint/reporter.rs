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
                // Check terminal color support (do NOT force color on CI — that
                // would inject ANSI escape codes into non-TTY CI log streams)
                anstyle_query::term_supports_ansi_color()
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

            if start_idx < total_lines {
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

        // Add snippet if we have pre-computed data; always attach origin for
        // location context even when source cannot be read or line is out of range.
        if let Some((ref snippet_source, start_idx, span_start, span_end)) = snippet_data {
            let snippet = Snippet::source(snippet_source)
                .line_start(start_idx + 1) // 1-based
                .origin(&origin)
                .annotation(level.span(span_start..span_end));
            message = message.snippet(snippet);
        } else {
            // Directory-level or unreadable file — show origin without snippet
            let snippet = Snippet::source("").origin(&origin);
            message = message.snippet(snippet);
        }

        // Add help text and help URL as footers, then render once
        if let Some(ref help_text) = d.help_text {
            message = message.footer(Level::Help.title(help_text));
        }
        let link_msg;
        if let Some(ref help_url) = d.help_url {
            link_msg = format!("for further information visit {help_url}");
            message = message.footer(Level::Help.title(&link_msg));
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
            let to_json_opt =
                |v: Option<usize>| v.map_or_else(|| "null".to_string(), |n| n.to_string());
            let severity_code = match d.severity {
                Severity::Error => 1,
                Severity::Warning => 2,
            };
            let help_url_json = d
                .help_url
                .as_ref()
                .map_or_else(|| "null".to_string(), |u| format!("\"{}\"", escape_json_string(u)));
            let help_text_json = d
                .help_text
                .as_ref()
                .map_or_else(|| "null".to_string(), |t| format!("\"{}\"", escape_json_string(t)));
            writeln!(writer, "    {{")?;
            writeln!(writer, "      \"rule_id\": \"{}\",", d.rule_id)?;
            writeln!(writer, "      \"severity\": \"{}\",", d.severity)?;
            writeln!(writer, "      \"severity_code\": {severity_code},")?;
            writeln!(writer, "      \"message\": \"{}\",", escape_json_string(&d.message))?;
            writeln!(
                writer,
                "      \"file_path\": \"{}\",",
                escape_json_string(&d.file_path.display().to_string())
            )?;
            writeln!(writer, "      \"line\": {},", to_json_opt(d.line))?;
            writeln!(writer, "      \"col\": {},", to_json_opt(d.col))?;
            writeln!(writer, "      \"end_line\": {},", to_json_opt(d.end_line))?;
            writeln!(writer, "      \"end_col\": {},", to_json_opt(d.end_col))?;
            writeln!(writer, "      \"help_url\": {help_url_json},")?;
            writeln!(writer, "      \"help_text\": {help_text_json},")?;
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

/// GitHub Actions annotation reporter.
///
/// Emits `::error` and `::warning` workflow commands that GitHub renders
/// as inline PR annotations.
pub struct CiGitHub;

impl Reporter for CiGitHub {
    fn report(&self, outcome: &Outcome, writer: &mut dyn Write) -> std::io::Result<()> {
        for d in &outcome.diagnostics {
            let severity = match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            let line = d.line.unwrap_or(1);
            let col = d.col.unwrap_or(1);
            // GitHub Actions workflow command escaping (properties and message)
            let file = escape_github_prop(&d.file_path.display().to_string());
            let rule_id = escape_github_message(&d.rule_id);
            let message = escape_github_message(&d.message);
            writeln!(
                writer,
                "::{severity} file={file},line={line},col={col}::{rule_id}: {message}",
            )?;
        }
        Ok(())
    }
}

/// Azure DevOps annotation reporter.
///
/// Emits `##vso[task.logissue]` logging commands that Azure DevOps
/// renders as pipeline annotations.
pub struct CiAzure;

impl Reporter for CiAzure {
    fn report(&self, outcome: &Outcome, writer: &mut dyn Write) -> std::io::Result<()> {
        if outcome.diagnostics.is_empty() {
            return Ok(());
        }

        let mut current_file: Option<&Path> = None;
        for d in &outcome.diagnostics {
            if current_file != Some(d.file_path.as_path()) {
                if current_file.is_some() {
                    writeln!(writer, "##[endgroup]")?;
                }
                writeln!(writer, "##[group]aipm lint: {}", d.file_path.display())?;
                current_file = Some(d.file_path.as_path());
            }

            let severity = match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            let line = d.line.unwrap_or(1);
            let col = d.col.unwrap_or(1);
            let sourcepath = escape_azure_log_command(&d.file_path.display().to_string());
            let code = escape_azure_log_command(&d.rule_id);
            let body = escape_azure_log_command(&format_azure_logissue_body(d));
            writeln!(
                writer,
                "##vso[task.logissue type={severity};sourcepath={sourcepath};linenumber={line};columnnumber={col};code={code}]{body}",
            )?;
        }

        if current_file.is_some() {
            writeln!(writer, "##[endgroup]")?;
        }

        if outcome.error_count == 0 && outcome.warning_count > 0 {
            writeln!(writer, "##vso[task.complete result=SucceededWithIssues;]")?;
        }

        Ok(())
    }
}

/// Escape a string for use in GitHub Actions workflow command properties.
fn escape_github_prop(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace(':', "%3A")
        .replace(',', "%2C")
}

/// Escape a string for use in GitHub Actions workflow command message bodies.
fn escape_github_message(s: &str) -> String {
    s.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A")
}

/// Escape a string for use in Azure DevOps `##vso[...]` log commands.
fn escape_azure_log_command(s: &str) -> String {
    s.replace('%', "%AZP25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace(';', "%3B")
        .replace(']', "%5D")
}

/// Build the body portion of an Azure DevOps `##vso[task.logissue]` line.
///
/// The result has the shape `<rule_id>: <message>` and, when present, appends
/// `" \u{2014} <help_text>"` and/or `" (see <help_url>)"`. The returned string
/// is not yet escaped for the Azure DevOps log-command grammar — callers must
/// apply `escape_azure_log_command` before embedding it in a logissue line.
fn format_azure_logissue_body(d: &Diagnostic) -> String {
    let mut body = format!("{}: {}", d.rule_id, d.message);
    if let Some(help_text) = d.help_text.as_ref() {
        body.push_str(" \u{2014} ");
        body.push_str(help_text);
    }
    if let Some(help_url) = d.help_url.as_ref() {
        body.push_str(" (see ");
        body.push_str(help_url);
        body.push(')');
    }
    body
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
            ..Outcome::default()
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
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        Text.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("no issues found"));
    }

    #[test]
    fn text_reporter_warnings_only() {
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/warn".into(),
                severity: Severity::Warning,
                message: "a warning".into(),
                file_path: PathBuf::from("test.md"),
                line: Some(1),
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
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        Text.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("1 warning(s) emitted"));
        assert!(!output.contains("error(s) emitted"));
        assert!(!output.contains("no issues found"));
    }

    #[test]
    fn text_reporter_errors_only() {
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/err".into(),
                severity: Severity::Error,
                message: "an error".into(),
                file_path: PathBuf::from("test.md"),
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
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        Text.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("1 error(s) emitted"));
        assert!(!output.contains("warning(s) emitted"));
        assert!(!output.contains("no issues found"));
    }

    #[test]
    fn human_reporter_line_past_file_end() {
        let mut mock_fs = MockFs::new();
        mock_fs.files.insert(PathBuf::from("/project/test.md"), "only one line".to_string());
        let reporter = make_human_reporter(&mock_fs);
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/oob".into(),
                severity: Severity::Warning,
                message: "out of bounds".into(),
                file_path: PathBuf::from("test.md"),
                line: Some(999),
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
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("test/oob"));
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
    fn json_reporter_includes_new_fields() {
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/rule".into(),
                severity: Severity::Error,
                message: "test".into(),
                file_path: PathBuf::from("test.md"),
                line: Some(3),
                col: Some(5),
                end_line: Some(3),
                end_col: Some(10),
                source_type: ".ai".into(),
                help_text: Some("fix it".into()),
                help_url: Some("https://example.com".into()),
            }],
            error_count: 1,
            warning_count: 0,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        Json.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("\"severity_code\": 1"));
        assert!(output.contains("\"col\": 5"));
        assert!(output.contains("\"end_line\": 3"));
        assert!(output.contains("\"end_col\": 10"));
        assert!(output.contains("\"help_url\": \"https://example.com\""));
        assert!(output.contains("\"help_text\": \"fix it\""));
    }

    #[test]
    fn json_reporter_null_optional_fields() {
        let outcome = sample_outcome();
        let mut buf = Vec::new();
        Json.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("\"col\": null"));
        assert!(output.contains("\"end_line\": null"));
        assert!(output.contains("\"end_col\": null"));
        assert!(output.contains("\"help_url\": null"));
        assert!(output.contains("\"help_text\": null"));
        assert!(output.contains("\"severity_code\": 2")); // warning
    }

    #[test]
    fn json_reporter_empty() {
        let outcome = Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![".ai".to_string()],
            ..Outcome::default()
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

    // --- CI GitHub reporter tests ---

    #[test]
    fn ci_github_error_format() {
        let outcome = sample_outcome();
        let mut buf = Vec::new();
        CiGitHub.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains(
            "::warning file=.ai/my-plugin/skills/default/SKILL.md,line=1,col=1::skill/missing-description"
        ));
        assert!(output.contains(
            "::error file=.ai/my-plugin/hooks/hooks.json,line=5,col=1::hook/unknown-event"
        ));
    }

    #[test]
    fn ci_github_empty_diagnostics() {
        let outcome = Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiGitHub.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.is_empty());
    }

    #[test]
    fn ci_github_defaults_line_col() {
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/rule".into(),
                severity: Severity::Warning,
                message: "msg".into(),
                file_path: PathBuf::from("dir/"),
                line: None,
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
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiGitHub.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("line=1,col=1"));
    }

    // --- CI Azure reporter tests ---

    #[test]
    fn ci_azure_error_format() {
        let outcome = sample_outcome();
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("##vso[task.logissue type=warning;sourcepath=.ai/my-plugin/skills/default/SKILL.md;linenumber=1;columnnumber=1;code=skill/missing-description]skill/missing-description"));
        assert!(output.contains("##vso[task.logissue type=error;sourcepath=.ai/my-plugin/hooks/hooks.json;linenumber=5;columnnumber=1;code=hook/unknown-event]hook/unknown-event"));
        assert!(output.contains("##[group]aipm lint: .ai/my-plugin/skills/default/SKILL.md"));
        assert!(output.contains("##[group]aipm lint: .ai/my-plugin/hooks/hooks.json"));
        assert!(output.contains("##[endgroup]"));
    }

    #[test]
    fn ci_azure_empty_diagnostics() {
        let outcome = Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.is_empty());
    }

    #[test]
    fn ci_azure_defaults_line_col() {
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/rule".into(),
                severity: Severity::Warning,
                message: "msg".into(),
                file_path: PathBuf::from("dir/"),
                line: None,
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
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("linenumber=1;columnnumber=1"));
    }

    // --- format_azure_logissue_body helper tests (spec §5.2 four-case table) ---

    fn body_fixture(help_text: Option<&str>, help_url: Option<&str>) -> Diagnostic {
        Diagnostic {
            rule_id: "skill/missing-description".into(),
            severity: Severity::Warning,
            message: "SKILL.md missing required field: description".into(),
            file_path: PathBuf::from("a.md"),
            line: Some(1),
            col: Some(1),
            end_line: None,
            end_col: None,
            source_type: ".ai".into(),
            help_text: help_text.map(String::from),
            help_url: help_url.map(String::from),
        }
    }

    #[test]
    fn format_azure_logissue_body_both_present() {
        let d = body_fixture(Some("run aipm migrate"), Some("https://example.com/rule"));
        let body = format_azure_logissue_body(&d);
        assert_eq!(
            body,
            "skill/missing-description: SKILL.md missing required field: description \u{2014} run aipm migrate (see https://example.com/rule)"
        );
    }

    #[test]
    fn format_azure_logissue_body_help_text_only() {
        let d = body_fixture(Some("do X"), None);
        let body = format_azure_logissue_body(&d);
        assert_eq!(
            body,
            "skill/missing-description: SKILL.md missing required field: description \u{2014} do X"
        );
        assert!(!body.contains("(see "));
    }

    #[test]
    fn format_azure_logissue_body_help_url_only() {
        let d = body_fixture(None, Some("https://docs.example.com"));
        let body = format_azure_logissue_body(&d);
        assert_eq!(
            body,
            "skill/missing-description: SKILL.md missing required field: description (see https://docs.example.com)"
        );
        assert!(!body.contains('\u{2014}'));
    }

    #[test]
    fn format_azure_logissue_body_neither() {
        let d = body_fixture(None, None);
        let body = format_azure_logissue_body(&d);
        assert_eq!(body, "skill/missing-description: SKILL.md missing required field: description");
        assert!(!body.contains('\u{2014}'));
        assert!(!body.contains("(see "));
    }

    fn ci_azure_single_diagnostic_outcome(
        help_text: Option<&str>,
        help_url: Option<&str>,
    ) -> Outcome {
        Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "skill/missing-description".into(),
                severity: Severity::Warning,
                message: "missing desc".into(),
                file_path: PathBuf::from("a.md"),
                line: Some(1),
                col: Some(1),
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: help_text.map(String::from),
                help_url: help_url.map(String::from),
            }],
            error_count: 0,
            warning_count: 1,
            sources_scanned: vec![],
            ..Outcome::default()
        }
    }

    #[test]
    fn ci_azure_with_help_text_and_url() {
        let outcome = ci_azure_single_diagnostic_outcome(
            Some("run aipm migrate"),
            Some("https://example.com/rule"),
        );
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();

        let logissue_line =
            output.lines().find(|line| line.starts_with("##vso[task.logissue")).unwrap_or_default();
        assert!(logissue_line.contains(
            "skill/missing-description: missing desc \u{2014} run aipm migrate (see https://example.com/rule)"
        ));
        assert!(logissue_line.contains(";code=skill/missing-description]"));
    }

    #[test]
    fn ci_azure_with_help_text_only() {
        let outcome = ci_azure_single_diagnostic_outcome(Some("do X"), None);
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();

        let logissue_line =
            output.lines().find(|line| line.starts_with("##vso[task.logissue")).unwrap_or_default();
        assert!(logissue_line.ends_with("skill/missing-description: missing desc \u{2014} do X"));
        assert!(!logissue_line.contains("(see "));
    }

    #[test]
    fn ci_azure_with_help_url_only() {
        let outcome = ci_azure_single_diagnostic_outcome(None, Some("https://docs.example.com"));
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();

        let logissue_line =
            output.lines().find(|line| line.starts_with("##vso[task.logissue")).unwrap_or_default();
        assert!(logissue_line
            .ends_with("skill/missing-description: missing desc (see https://docs.example.com)"));
        assert!(!logissue_line.contains('\u{2014}'));
    }

    #[test]
    fn ci_azure_with_neither() {
        let outcome = ci_azure_single_diagnostic_outcome(None, None);
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();

        let logissue_line =
            output.lines().find(|line| line.starts_with("##vso[task.logissue")).unwrap_or_default();
        assert!(logissue_line.ends_with("skill/missing-description: missing desc"));
        assert!(!logissue_line.contains('\u{2014}'));
        assert!(!logissue_line.contains("(see "));
    }

    fn ci_azure_diag_on(file_path: &str, rule_id: &str, line: usize) -> Diagnostic {
        Diagnostic {
            rule_id: rule_id.into(),
            severity: Severity::Warning,
            message: "msg".into(),
            file_path: PathBuf::from(file_path),
            line: Some(line),
            col: Some(1),
            end_line: None,
            end_col: None,
            source_type: ".ai".into(),
            help_text: None,
            help_url: None,
        }
    }

    #[test]
    fn ci_azure_sample_outcome_snapshot() {
        let outcome = sample_outcome();
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn ci_azure_rule_id_with_slashes_unchanged() {
        let outcome = ci_azure_single_diagnostic_outcome(None, None);
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();

        let logissue_line =
            output.lines().find(|line| line.starts_with("##vso[task.logissue")).unwrap_or_default();
        assert!(logissue_line.contains(";code=skill/missing-description]"));
        let body_start = logissue_line.find(']').unwrap_or_default() + 1;
        let body = logissue_line.get(body_start..).unwrap_or_default();
        assert!(body.starts_with("skill/missing-description: "));
    }

    #[test]
    fn ci_azure_escape_newline_in_message() {
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "rule/multi".into(),
                severity: Severity::Warning,
                message: "line one\nline two".into(),
                file_path: PathBuf::from("a.md"),
                line: Some(1),
                col: Some(1),
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: None,
                help_url: None,
            }],
            error_count: 0,
            warning_count: 1,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();

        let logissue_lines: Vec<&str> =
            output.lines().filter(|l| l.starts_with("##vso[task.logissue")).collect();
        assert_eq!(logissue_lines.len(), 1);
        let logissue_line = logissue_lines[0];
        assert!(logissue_line.contains("line one%0Aline two"));
        assert!(!logissue_line.contains("line one\nline two"));
    }

    #[test]
    fn ci_azure_escape_semicolon_in_help_url() {
        let outcome = ci_azure_single_diagnostic_outcome(None, Some("https://x/?a=1;b=2"));
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();

        let logissue_line =
            output.lines().find(|line| line.starts_with("##vso[task.logissue")).unwrap_or_default();
        assert!(logissue_line.contains("https://x/?a=1%3Bb=2"));
        assert!(!logissue_line.contains("https://x/?a=1;b=2"));
        assert!(logissue_line.contains(";code=skill/missing-description]"));
    }

    #[test]
    fn ci_azure_escape_bracket_in_help_text() {
        let outcome = ci_azure_single_diagnostic_outcome(Some("see [docs]"), None);
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();

        let logissue_line =
            output.lines().find(|line| line.starts_with("##vso[task.logissue")).unwrap_or_default();
        assert!(logissue_line.contains("see [docs%5D"));
        assert!(!logissue_line.ends_with("see [docs]"));
        assert!(logissue_line.contains(";code=skill/missing-description]"));
    }

    #[test]
    fn ci_azure_no_task_complete_on_clean_run() {
        let outcome = Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();

        assert_eq!(buf.len(), 0);
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(!output.contains("##vso[task.complete"));
    }

    #[test]
    fn ci_azure_no_task_complete_on_errors() {
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "rule/err".into(),
                severity: Severity::Error,
                message: "bad".into(),
                file_path: PathBuf::from("a.md"),
                line: Some(1),
                col: Some(1),
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: None,
                help_url: None,
            }],
            error_count: 1,
            warning_count: 0,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();

        assert!(!output.contains("##vso[task.complete"));
        let trimmed = output.trim_end_matches('\n');
        assert!(trimmed.ends_with("##[endgroup]"));
    }

    #[test]
    fn ci_azure_task_complete_on_warnings_only() {
        let outcome = Outcome {
            diagnostics: vec![
                ci_azure_diag_on("a.md", "rule/one", 1),
                ci_azure_diag_on("a.md", "rule/two", 2),
            ],
            error_count: 0,
            warning_count: 2,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();

        assert!(output.ends_with("##vso[task.complete result=SucceededWithIssues;]\n"));

        let lines: Vec<&str> = output.lines().collect();
        let task_complete_pos =
            lines.iter().position(|l| l.starts_with("##vso[task.complete")).unwrap_or_default();
        let last_endgroup_pos =
            lines.iter().rposition(|l| *l == "##[endgroup]").unwrap_or_default();
        assert!(last_endgroup_pos < task_complete_pos);
    }

    #[test]
    fn ci_azure_single_file_single_group() {
        let outcome = Outcome {
            diagnostics: vec![
                ci_azure_diag_on("only.md", "rule/one", 1),
                ci_azure_diag_on("only.md", "rule/two", 2),
                ci_azure_diag_on("only.md", "rule/three", 3),
                ci_azure_diag_on("only.md", "rule/four", 4),
            ],
            error_count: 0,
            warning_count: 4,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(output.matches("##[group]").count(), 1);
        assert_eq!(output.matches("##[endgroup]").count(), 1);

        let group_pos =
            lines.iter().position(|l| *l == "##[group]aipm lint: only.md").unwrap_or_default();
        let endgroup_pos = lines.iter().position(|l| *l == "##[endgroup]").unwrap_or_default();
        assert!(group_pos < endgroup_pos);

        let logissues: Vec<&&str> = lines
            .get(group_pos + 1..endgroup_pos)
            .unwrap_or_default()
            .iter()
            .filter(|l| l.starts_with("##vso[task.logissue"))
            .collect();
        assert_eq!(logissues.len(), 4);
        assert!(logissues[0].contains(";code=rule/one]"));
        assert!(logissues[1].contains(";code=rule/two]"));
        assert!(logissues[2].contains(";code=rule/three]"));
        assert!(logissues[3].contains(";code=rule/four]"));
    }

    #[test]
    fn ci_azure_group_per_file() {
        let outcome = Outcome {
            diagnostics: vec![
                ci_azure_diag_on("a.md", "rule/one", 1),
                ci_azure_diag_on("a.md", "rule/two", 2),
                ci_azure_diag_on("b.md", "rule/three", 1),
            ],
            error_count: 0,
            warning_count: 3,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(output.matches("##[group]").count(), 2);
        assert_eq!(output.matches("##[endgroup]").count(), 2);

        let idx_group_a = lines.iter().position(|l| *l == "##[group]aipm lint: a.md");
        let idx_group_b = lines.iter().position(|l| *l == "##[group]aipm lint: b.md");
        assert!(idx_group_a.is_some());
        assert!(idx_group_b.is_some());
        let group_a_pos = idx_group_a.unwrap_or_default();
        let group_b_pos = idx_group_b.unwrap_or_default();
        assert!(group_a_pos < group_b_pos);

        let a_logissues: Vec<&&str> = lines
            .get(group_a_pos + 1..group_b_pos)
            .unwrap_or_default()
            .iter()
            .filter(|l| l.starts_with("##vso[task.logissue"))
            .collect();
        assert_eq!(a_logissues.len(), 2);
        assert!(a_logissues[0].contains(";code=rule/one]"));
        assert!(a_logissues[1].contains(";code=rule/two]"));

        let b_logissues: Vec<&&str> = lines
            .get(group_b_pos + 1..)
            .unwrap_or_default()
            .iter()
            .filter(|l| l.starts_with("##vso[task.logissue"))
            .collect();
        assert_eq!(b_logissues.len(), 1);
        assert!(b_logissues[0].contains(";code=rule/three]"));
    }

    #[test]
    fn ci_azure_code_property_present() {
        let outcome = Outcome {
            diagnostics: vec![
                Diagnostic {
                    rule_id: "skill/missing-description".into(),
                    severity: Severity::Warning,
                    message: "missing desc".into(),
                    file_path: PathBuf::from("a.md"),
                    line: Some(1),
                    col: Some(1),
                    end_line: None,
                    end_col: None,
                    source_type: ".ai".into(),
                    help_text: None,
                    help_url: None,
                },
                Diagnostic {
                    rule_id: "hook/unknown-event".into(),
                    severity: Severity::Error,
                    message: "bad event".into(),
                    file_path: PathBuf::from("b.json"),
                    line: Some(2),
                    col: Some(3),
                    end_line: None,
                    end_col: None,
                    source_type: ".ai".into(),
                    help_text: None,
                    help_url: None,
                },
            ],
            error_count: 1,
            warning_count: 1,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();

        let logissue_lines: Vec<&str> =
            output.lines().filter(|line| line.starts_with("##vso[task.logissue")).collect();
        assert_eq!(logissue_lines.len(), 2);
        assert!(logissue_lines[0].contains(";code=skill/missing-description]"));
        assert!(logissue_lines[1].contains(";code=hook/unknown-event]"));
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
            ..Outcome::default()
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
            ..Outcome::default()
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
            ..Outcome::default()
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
            ..Outcome::default()
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
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("no issues found"));
    }

    #[test]
    fn human_reporter_col_only_span() {
        let mut mock_fs = MockFs::new();
        mock_fs.files.insert(
            PathBuf::from("/project/test.md"),
            "line one\nline two\nline three".to_string(),
        );
        let reporter = make_human_reporter(&mock_fs);
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/col".into(),
                severity: Severity::Warning,
                message: "col only".into(),
                file_path: PathBuf::from("test.md"),
                line: Some(2),
                col: Some(3),
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: None,
                help_url: None,
            }],
            error_count: 0,
            warning_count: 1,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("line two"));
        assert!(output.contains("test/col"));
    }

    #[test]
    fn human_reporter_col_and_end_col_span() {
        let mut mock_fs = MockFs::new();
        mock_fs.files.insert(
            PathBuf::from("/project/test.md"),
            "line one\nline two\nline three".to_string(),
        );
        let reporter = make_human_reporter(&mock_fs);
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/range".into(),
                severity: Severity::Error,
                message: "col range".into(),
                file_path: PathBuf::from("test.md"),
                line: Some(2),
                col: Some(1),
                end_line: Some(2),
                end_col: Some(4),
                source_type: ".ai".into(),
                help_text: None,
                help_url: None,
            }],
            error_count: 1,
            warning_count: 0,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("line two"));
        assert!(output.contains("test/range"));
    }

    #[test]
    fn human_reporter_help_text_and_url_together() {
        let mock_fs = MockFs::new();
        let reporter = make_human_reporter(&mock_fs);
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/both".into(),
                severity: Severity::Warning,
                message: "both help".into(),
                file_path: PathBuf::from("test.md"),
                line: None,
                col: None,
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: Some("fix this".into()),
                help_url: Some("https://example.com/rule".into()),
            }],
            error_count: 0,
            warning_count: 1,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("fix this"));
        assert!(output.contains("https://example.com/rule"));
    }

    #[test]
    fn human_reporter_colored_output_uses_styled_renderer() {
        // Cover the `should_color()` True branch (line 109): with ColorChoice::Always
        // the styled renderer is selected, which emits ANSI escape codes.
        let mut mock_fs = MockFs::new();
        mock_fs.files.insert(
            PathBuf::from("/project/test.md"),
            "line one\nline two\nline three".to_string(),
        );
        let reporter =
            Human { fs: &mock_fs, color: ColorChoice::Always, base_dir: Path::new("/project") };
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
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        // Styled renderer emits ANSI escape codes.
        assert!(output.contains("\x1b["), "expected ANSI codes in colored output");
    }

    #[test]
    fn color_choice_never_always() {
        assert!(!ColorChoice::Never.should_color());
        assert!(ColorChoice::Always.should_color());
    }

    #[test]
    fn color_choice_auto_no_color_env() {
        // Serialize env var access to avoid races
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK.lock();

        // Test NO_COLOR suppresses color
        std::env::set_var("NO_COLOR", "1");
        std::env::remove_var("CLICOLOR");
        assert!(!ColorChoice::Auto.should_color());

        // Test CLICOLOR=0 suppresses color
        std::env::remove_var("NO_COLOR");
        std::env::set_var("CLICOLOR", "0");
        assert!(!ColorChoice::Auto.should_color());

        // Test Auto fallback (no env overrides)
        std::env::remove_var("NO_COLOR");
        std::env::remove_var("CLICOLOR");
        // Result depends on TTY/CI detection — just ensure it doesn't panic
        let _ = ColorChoice::Auto.should_color();
    }

    #[test]
    fn ci_github_with_col() {
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/rule".into(),
                severity: Severity::Error,
                message: "msg".into(),
                file_path: PathBuf::from("file.md"),
                line: Some(10),
                col: Some(5),
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: None,
                help_url: None,
            }],
            error_count: 1,
            warning_count: 0,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiGitHub.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("line=10,col=5"));
    }

    #[test]
    fn ci_azure_with_col() {
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/rule".into(),
                severity: Severity::Error,
                message: "msg".into(),
                file_path: PathBuf::from("file.md"),
                line: Some(10),
                col: Some(5),
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: None,
                help_url: None,
            }],
            error_count: 1,
            warning_count: 0,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("linenumber=10;columnnumber=5"));
    }

    #[test]
    fn human_reporter_styled_renderer_executes() {
        // Use ColorChoice::Always to force Renderer::styled() — the branch in
        // `report()` that is otherwise unreachable when ColorChoice::Never is used.
        let mut mock_fs = MockFs::new();
        mock_fs.files.insert(
            PathBuf::from("/project/.ai/my-plugin/skills/default/SKILL.md"),
            "---\nname: my-skill\n---\nbody content".to_string(),
        );
        let reporter =
            Human { fs: &mock_fs, color: ColorChoice::Always, base_dir: Path::new("/project") };
        let outcome = sample_outcome();
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        // Styled renderer was used — report must succeed and emit content.
        assert!(!output.is_empty());
        assert!(output.contains("skill/missing-description"));
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
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        // Should still render the diagnostic header even without file
        assert!(output.contains("test/rule"));
        assert!(output.contains("test error"));
    }

    #[test]
    fn human_reporter_empty_file_with_line_number() {
        // Source file exists but is empty (0 lines). A diagnostic with a line
        // number pointing into it triggers the `start_idx < total_lines` false
        // branch in `write_rich_diagnostic`, returning None for the snippet and
        // falling back to a no-snippet rendering.
        let mut mock_fs = MockFs::new();
        mock_fs.files.insert(PathBuf::from("/project/empty.md"), String::new());
        let reporter = make_human_reporter(&mock_fs);
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/empty-file".into(),
                severity: Severity::Warning,
                message: "diagnostic on empty file".into(),
                file_path: PathBuf::from("empty.md"),
                line: Some(1),
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
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        reporter.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        // Diagnostic header should still appear even though the snippet is absent
        assert!(output.contains("test/empty-file"));
        assert!(output.contains("diagnostic on empty file"));
    }

    #[test]
    fn ci_azure_zero_counts_with_diagnostics_omits_succeeded_with_issues() {
        // Cover the `warning_count > 0` FALSE branch at line 374: when
        // error_count == 0 but warning_count is also 0 (counts inconsistent
        // with diagnostics), the SucceededWithIssues task-complete command must
        // NOT be emitted — the reporter should still write the diagnostic.
        let outcome = Outcome {
            diagnostics: vec![Diagnostic {
                rule_id: "test/rule".into(),
                severity: Severity::Warning,
                message: "a warning".into(),
                file_path: PathBuf::from("file.md"),
                line: Some(1),
                col: None,
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: None,
                help_url: None,
            }],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
            ..Outcome::default()
        };
        let mut buf = Vec::new();
        CiAzure.report(&outcome, &mut buf).ok();
        let output = String::from_utf8(buf).unwrap_or_default();
        assert!(output.contains("##vso[task.logissue"));
        assert!(!output.contains("SucceededWithIssues"));
    }
}
