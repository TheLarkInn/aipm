//! Rule: `hook/unknown-event` — tool-aware hook event validation.
//!
//! Validates hook event names against the tool-specific event list.
//! For `.ai/` marketplace plugins, validates against the union of all tools.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::{known_events, scan};

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

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        for (path, content) in scan::scan_hook_files(source_dir, fs) {
            // Parse as JSON object — extract top-level keys as event names
            // hooks.json format: { "hooks": { "EventName": [...] } } or { "EventName": [...] }
            let parsed: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(e) => {
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        severity: self.default_severity(),
                        message: format!("failed to parse hooks.json: {e}"),
                        file_path: path,
                        line: Some(1),
                        col: None,
                        end_line: None,
                        end_col: None,
                        source_type: ".ai".to_string(),
                        help_text: None,
                        help_url: None,
                    });
                    continue;
                },
            };

            // Try to find the hooks object (either top-level or nested under "hooks")
            let hooks_obj = parsed
                .get("hooks")
                .and_then(serde_json::Value::as_object)
                .or_else(|| parsed.as_object());

            let Some(hooks) = hooks_obj else {
                continue;
            };

            for key in hooks.keys() {
                // Skip known structural keys that are not event names
                if key == "version" || key == "disableAllHooks" || key == "hooks" {
                    continue;
                }
                if !known_events::is_valid_for_any_tool(key) {
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        severity: self.default_severity(),
                        message: format!("unknown hook event: {key}"),
                        file_path: path.clone(),
                        line: None,
                        col: None,
                        end_line: None,
                        end_col: None,
                        source_type: ".ai".to_string(),
                        help_text: None,
                        help_url: None,
                    });
                }
            }
        }

        Ok(diagnostics)
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let source_type = scan::source_type_from_path(file_path).to_string();
        let Some((_path, content)) = scan::read_hook(file_path, fs) else {
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
                diagnostics.push(Diagnostic {
                    rule_id: self.id().to_string(),
                    severity: self.default_severity(),
                    message: format!("unknown hook event: {key}"),
                    file_path: file_path.to_path_buf(),
                    line: None,
                    col: None,
                    end_line: None,
                    end_col: None,
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
    fn valid_claude_events_no_finding() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "PreToolUse": [], "PostToolUse": [] }"#);

        let result = UnknownEvent.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn valid_copilot_events_no_finding() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "preToolUse": [], "agentStop": [] }"#);

        let result = UnknownEvent.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn unknown_event_produces_error() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "InvalidEvent": [] }"#);

        let result = UnknownEvent.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "hook/unknown-event");
        assert!(diags[0].message.contains("InvalidEvent"));
    }

    #[test]
    fn multiple_unknown_events() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "Foo": [], "Bar": [] }"#);

        let result = UnknownEvent.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap_or_default().len(), 2);
    }

    #[test]
    fn malformed_json_reports_error() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", "not json {{{");

        let result = UnknownEvent.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("failed to parse"));
    }

    #[test]
    fn hooks_key_is_skipped() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "hooks": [] }"#);

        let result = UnknownEvent.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn version_key_is_skipped() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "version": 1, "PreToolUse": [] }"#);

        let result = UnknownEvent.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn nested_hooks_object() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "hooks": { "PreToolUse": [], "BadEvent": [] } }"#);

        let result = UnknownEvent.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("BadEvent"));
    }

    #[test]
    fn empty_ai_dir() {
        let mut fs = MockFs::new();
        fs.dirs.insert(std::path::PathBuf::from(".ai"), vec![]);

        let result = UnknownEvent.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn disable_all_hooks_key_is_skipped() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "disableAllHooks": true, "PreToolUse": [] }"#);

        let result = UnknownEvent.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn non_object_json_skipped() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#""just a string""#);

        let result = UnknownEvent.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        // A JSON string is not an object — no events to check
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    // --- check_file() tests ---

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
}
