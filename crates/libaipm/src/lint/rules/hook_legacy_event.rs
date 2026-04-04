//! Rule: `hook/legacy-event-name` — `PascalCase` hook event that Copilot normalizes.
//!
//! Warns when hooks use `PascalCase` names that Copilot CLI normalizes to `camelCase`.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

use super::{known_events, scan};

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
        Some("rename to the PascalCase event name")
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
                    // Try to find the line number by searching raw content
                    let line_num = content.lines().enumerate().find_map(|(i, line)| {
                        let needle = format!("\"{key}\"");
                        if line.contains(&needle) {
                            Some(i + 1)
                        } else {
                            None
                        }
                    });
                    diagnostics.push(Diagnostic {
                        rule_id: self.id().to_string(),
                        severity: self.default_severity(),
                        message: format!(
                            "\"{key}\" is a legacy event name, use \"{canonical}\" instead"
                        ),
                        file_path: path.clone(),
                        line: line_num,
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
    fn multiline_json_reports_correct_line_number() {
        // Multi-line JSON exercises the False branch of line.contains(&needle)
        // on all lines preceding the one that contains the key.
        let mut fs = MockFs::new();
        let json = "{\n  \"Stop\": []\n}";
        fs.add_hooks("p", json);

        let result = LegacyEventName.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        let diags = result.ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        // Key is on line 2 — verify line number is reported correctly.
        assert_eq!(diags[0].line, Some(2));
        assert!(diags[0].message.contains("agentStop"));
    }
}
