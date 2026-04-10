//! Rule: `hook/legacy-event-name` — `PascalCase` hook event that Copilot normalizes.
//!
//! Warns when hooks use `PascalCase` names that Copilot CLI normalizes to `camelCase`.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::{known_events, scan};

/// Return `(line_num, col, end_col)` for a JSON key in a string of JSON content.
fn locate_json_key(content: &str, key: &str) -> Option<(usize, usize, usize)> {
    let needle = format!("\"{key}\"");
    for (i, line) in content.lines().enumerate() {
        if let Some(pos) = line.find(&needle) {
            return Some((i + 1, pos + 1, pos + needle.len() + 1));
        }
    }
    None
}

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

    fn check(&self, source_dir: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();

        for (path, content) in scan::scan_hook_files(source_dir, fs) {
            let parsed: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let hooks_obj = parsed
                .get("hooks")
                .and_then(serde_json::Value::as_object)
                .or_else(|| parsed.as_object());

            let Some(hooks) = hooks_obj else {
                continue;
            };

            for key in hooks.keys() {
                if let Some(canonical) = known_events::suggest_canonical(key) {
                    let (line, col, end_col) =
                        locate_json_key(&content, key)
                            .map_or((None, None, None), |(l, c, e)| (Some(l), Some(c), Some(e)));
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        severity: self.default_severity(),
                        message: format!(
                            "\"{key}\" is a legacy event name, use \"{canonical}\" instead"
                        ),
                        file_path: path.clone(),
                        line,
                        col,
                        end_line: line,
                        end_col,
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
                let (line, col, end_col) =
                    locate_json_key(&content, key)
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
    fn canonical_names_no_finding() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "preToolUse": [], "agentStop": [] }"#);

        let result = LegacyEventName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn legacy_stop_suggests_agent_stop() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "Stop": [] }"#);

        let result = LegacyEventName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("agentStop"));
    }

    #[test]
    fn legacy_user_prompt_submit() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "UserPromptSubmit": [] }"#);

        let result = LegacyEventName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("userPromptSubmitted"));
    }

    #[test]
    fn non_legacy_pascal_case_no_finding() {
        // PreToolUse is a valid Claude event, not in the legacy map
        // (well actually it IS in the legacy map for Copilot)
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "FileChanged": [] }"#);

        let result = LegacyEventName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        // FileChanged is NOT in the legacy map, so no finding
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn multiple_legacy_events() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "Stop": [], "SessionStart": [] }"#);

        let result = LegacyEventName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn nested_hooks_with_legacy() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "hooks": { "Stop": [], "preToolUse": [] } }"#);

        let result = LegacyEventName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("agentStop"));
    }

    #[test]
    fn empty_hooks_object() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "hooks": {} }"#);

        let result = LegacyEventName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn malformed_json_skipped() {
        let mut fs = MockFs::new();
        fs.add_hooks("p", "not json");

        let result = LegacyEventName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        // Malformed JSON is silently skipped (hook_unknown_event handles parse errors)
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn json_array_not_object_skipped() {
        let mut fs = MockFs::new();
        // Valid JSON but not an object — hooks_obj becomes None
        fs.add_hooks("p", r#"["not", "an", "object"]"#);

        let result = LegacyEventName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn legacy_event_has_col_and_end_col() {
        // Single-line: `{ "Stop": [] }` — "Stop" at col 3, needle `"Stop"` is 6 chars → end_col 9
        let mut fs = MockFs::new();
        fs.add_hooks("p", r#"{ "Stop": [] }"#);

        let diags = LegacyEventName.check(Path::new(".ai"), &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, Some(1));
        assert_eq!(diags[0].col, Some(3));
        assert_eq!(diags[0].end_line, Some(1));
        assert_eq!(diags[0].end_col, Some(9));
    }

    // --- check_file() tests ---

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

    #[test]
    fn check_multiline_json_line_number_found() {
        // Same for the `check` method's find_map — covers the `else { None }` arm
        // when the key is not on the first line of a multi-line hooks file.
        let mut fs = MockFs::new();
        fs.add_hooks("p", "{\n  \"Stop\": []\n}");

        let result = LegacyEventName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, Some(2)); // "Stop" is on line 2
    }
}
