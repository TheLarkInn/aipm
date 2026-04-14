//! Rule: `hook/legacy-event-name` — `PascalCase` hook event that Copilot normalizes.
//!
//! Warns when hooks use `PascalCase` names that Copilot CLI normalizes to `camelCase`.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::{known_events, locate_json_key};

/// Warns about legacy `PascalCase` hook event names.
pub struct LegacyEventName;

impl Rule for LegacyEventName {
    fn id(&self) -> &'static str {
        "hook/legacy-event-name"
    }

    fn name(&self) -> &'static str {
        "legacy hook event name"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/hook/legacy-event-name.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("rename to the canonical camelCase event name")
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let Some((source_type, content)) = super::read_hook_preamble(file_path, fs) else {
            return Ok(vec![]);
        };
        let mut diagnostics = Vec::new();
        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };
        let hooks_obj = parsed
            .get("hooks")
            .and_then(serde_json::Value::as_object)
            .or_else(|| parsed.as_object());
        let Some(hooks) = hooks_obj else {
            return Ok(vec![]);
        };
        for key in hooks.keys() {
            if let Some(canonical) = known_events::suggest_canonical(key) {
                let (line, col, end_col) = locate_json_key(&content, key)
                    .map_or((None, None, None), |(l, c, e)| (Some(l), Some(c), Some(e)));
                diagnostics.push(Diagnostic {
                    rule_id: self.id().to_string(),
                    severity: self.default_severity(),
                    message: format!(
                        "\"{key}\" is a legacy event name, use \"{canonical}\" instead"
                    ),
                    file_path: file_path.to_path_buf(),
                    line,
                    col,
                    end_line: line,
                    end_col,
                    source_type: source_type.clone(),
                    help_text: None,
                    help_url: None,
                });
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
    fn check_file_no_file_returns_empty() {
        let fs = MockFs::new();
        let result = LegacyEventName.check_file(Path::new(".ai/p/hooks/hooks.json"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_canonical_names_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/hooks/hooks.json");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), r#"{"preToolUse": []}"#.to_string());

        let result = LegacyEventName.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_legacy_event_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/hooks/hooks.json");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), r#"{"Stop": []}"#.to_string());

        let result = LegacyEventName.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "hook/legacy-event-name");
        assert!(diags[0].message.contains("agentStop"));
    }

    #[test]
    fn check_file_non_object_json_skipped() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/hooks/hooks.json");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), r#"["not", "an", "object"]"#.to_string());

        let result = LegacyEventName.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_malformed_json_silently_skipped() {
        // LegacyEventName.check_file() silently skips malformed JSON
        // (the Err(_) arm at the match on serde_json::from_str), unlike check()
        // which never reaches that path.
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/hooks/hooks.json");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "not json {{{".to_string());

        let result = LegacyEventName.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_locate_json_key_returns_none_for_unicode_escaped_key() {
        // When a JSON key is written with a Unicode escape (e.g. "St\u006fp" which
        // serde_json resolves to "Stop"), locate_json_key searches the raw content for
        // the literal string "Stop" and cannot find it.  The diagnostic is still
        // produced but with line=None, col=None, end_col=None.
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/hooks/hooks.json");
        fs.exists.insert(path.clone());
        // \u006f is 'o', so "St\u006fp" parses to "Stop" but the raw bytes differ.
        fs.files.insert(path.clone(), r#"{"St\u006fp": []}"#.to_string());

        let diags = LegacyEventName.check_file(&path, &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "hook/legacy-event-name");
        assert!(diags[0].message.contains("agentStop"));
        assert_eq!(diags[0].line, None);
        assert_eq!(diags[0].col, None);
        assert_eq!(diags[0].end_col, None);
    }

    #[test]
    fn check_file_multiline_json_line_number_found() {
        // Multi-line JSON means the find_map closure returns None for lines that
        // don't contain the key (the `else { None }` arm), then Some once the
        // key line is reached. This covers the False branch of `line.contains`.
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/hooks/hooks.json");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "{\n  \"Stop\": []\n}".to_string());

        let result = LegacyEventName.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, Some(2)); // "Stop" is on line 2
    }
}
