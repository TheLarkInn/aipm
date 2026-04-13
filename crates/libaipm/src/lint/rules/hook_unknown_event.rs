//! Rule: `hook/unknown-event` — tool-aware hook event validation.
//!
//! Validates hook event names against the tool-specific event list.
//! For `.ai/` marketplace plugins, validates against the union of all tools.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::{known_events, locate_json_key};

/// Checks that hook event names are valid.
pub struct UnknownEvent;

impl Rule for UnknownEvent {
    fn id(&self) -> &'static str {
        "hook/unknown-event"
    }

    fn name(&self) -> &'static str {
        "unknown hook event"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/hook/unknown-event.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("use a valid hook event name")
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let Some((source_type, content)) = super::read_hook_preamble(file_path, fs) else {
            return Ok(vec![]);
        };
        let mut diagnostics = Vec::new();
        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                return Ok(vec![Diagnostic {
                    rule_id: self.id().to_string(),
                    severity: self.default_severity(),
                    message: format!("failed to parse hooks.json: {e}"),
                    file_path: file_path.to_path_buf(),
                    line: Some(1),
                    col: None,
                    end_line: None,
                    end_col: None,
                    source_type,
                    help_text: None,
                    help_url: None,
                }]);
            },
        };
        let hooks_obj = parsed
            .get("hooks")
            .and_then(serde_json::Value::as_object)
            .or_else(|| parsed.as_object());
        let Some(hooks) = hooks_obj else {
            return Ok(vec![]);
        };
        for key in hooks.keys() {
            if key == "version" || key == "disableAllHooks" || key == "hooks" {
                continue;
            }
            if !known_events::is_valid_for_any_tool(key) {
                let (line, col, end_col) = locate_json_key(&content, key)
                    .map_or((None, None, None), |(l, c, e)| (Some(l), Some(c), Some(e)));
                diagnostics.push(Diagnostic {
                    rule_id: self.id().to_string(),
                    severity: self.default_severity(),
                    message: format!("unknown hook event: {key}"),
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
        let result = UnknownEvent.check_file(Path::new(".ai/p/hooks/hooks.json"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_valid_events_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/hooks/hooks.json");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), r#"{"PreToolUse": []}"#.to_string());

        let result = UnknownEvent.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_unknown_event_diagnostic() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/hooks/hooks.json");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), r#"{"InvalidEvent": []}"#.to_string());

        let result = UnknownEvent.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "hook/unknown-event");
    }

    #[test]
    fn check_file_malformed_json_error() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/hooks/hooks.json");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "not json {{{".to_string());

        let result = UnknownEvent.check_file(&path, &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("failed to parse"));
    }

    #[test]
    fn check_file_non_object_json_skipped() {
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/hooks/hooks.json");
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), r#""just a string""#.to_string());

        let result = UnknownEvent.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn check_file_structural_keys_are_skipped() {
        // "version", "disableAllHooks", and "hooks" are structural keys that
        // check_file() must skip rather than flagging as unknown events.
        //
        // Using "hooks": [] (array, not object) so that get("hooks").as_object()
        // returns None, causing the parser to fall back to parsed.as_object()
        // and iterate over all top-level keys including the structural ones.
        let mut fs = MockFs::new();
        let path = std::path::PathBuf::from(".ai/p/hooks/hooks.json");
        fs.exists.insert(path.clone());
        fs.files.insert(
            path.clone(),
            r#"{"version": 1, "disableAllHooks": false, "hooks": []}"#.to_string(),
        );

        let result = UnknownEvent.check_file(&path, &fs);
        assert!(result.is_ok());
        assert!(
            result.ok().unwrap_or_default().is_empty(),
            "structural keys must not be flagged as unknown events"
        );
    }
}
