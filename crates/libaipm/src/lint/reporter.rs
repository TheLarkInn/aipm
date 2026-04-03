//! Output reporters for lint diagnostics.

use std::io::Write;

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
}
