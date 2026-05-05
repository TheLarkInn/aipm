//! Rule: `marketplace/plugin-field-mismatch`
//!
//! Validates that the `name` and `description` fields in each marketplace.json
//! plugin entry match the corresponding values in the plugin's own `plugin.json`.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;

/// Validates name/description consistency between marketplace.json and plugin.json.
pub struct FieldMismatch;

impl Rule for FieldMismatch {
    fn id(&self) -> &'static str {
        "marketplace/plugin-field-mismatch"
    }

    fn name(&self) -> &'static str {
        "marketplace plugin fields must match plugin.json"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/marketplace/plugin-field-mismatch.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("update marketplace.json or plugin.json so the name and description fields match")
    }

    fn check_file(
        &self,
        file_path: &Path,
        fs: &dyn Fs,
    ) -> Result<Vec<Diagnostic>, super::super::Error> {
        let Some(ai_dir) = file_path.parent().and_then(|p| p.parent()) else {
            return Ok(vec![]);
        };
        Ok(check_mismatch(file_path, ai_dir, fs))
    }
}

fn diag(mp_path: &Path, source_type: &str, message: String) -> Diagnostic {
    super::simple_diag(
        "marketplace/plugin-field-mismatch",
        Severity::Error,
        message,
        mp_path,
        source_type,
    )
}

fn check_mismatch(mp_path: &Path, ai_dir: &Path, fs: &dyn Fs) -> Vec<Diagnostic> {
    let Ok(content) = fs.read_to_string(mp_path) else {
        return vec![];
    };

    let source_type = super::scan::source_type_from_path(mp_path).to_string();

    let parsed: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            return vec![diag(
                mp_path,
                &source_type,
                format!("failed to parse marketplace.json: {e}"),
            )];
        },
    };

    let Some(plugins) = parsed.get("plugins").and_then(serde_json::Value::as_array) else {
        return vec![];
    };

    let mut diagnostics = Vec::new();

    for entry in plugins {
        let Some(source) = entry.get("source").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let mp_name = entry.get("name").and_then(serde_json::Value::as_str).unwrap_or("");
        let mp_desc = entry.get("description").and_then(serde_json::Value::as_str);

        // Reject parent-dir traversal, absolute roots, and Windows drive
        // prefixes before any filesystem read (issue #793 Finding 2). The
        // sibling rule `marketplace/source-resolve` surfaces unsafe source
        // paths to the user; this rule's job is reconciling marketplace
        // fields with plugin.json content, which is impossible for a
        // rejected path — silently skip the entry.
        let trimmed = source.trim_start_matches("./");
        if !crate::lint::path_guard::is_safe_path(trimmed) {
            continue;
        }
        let pj_path = ai_dir.join(trimmed).join(".claude-plugin").join("plugin.json");

        let Ok(pj_content) = fs.read_to_string(&pj_path) else {
            continue; // other rules handle missing plugin.json
        };

        let pj: serde_json::Value = match serde_json::from_str(&pj_content) {
            Ok(v) => v,
            Err(e) => {
                diagnostics.push(diag(
                    mp_path,
                    &source_type,
                    format!("failed to parse plugin.json for '{mp_name}': {e}"),
                ));
                continue;
            },
        };

        let pj_name = pj.get("name").and_then(serde_json::Value::as_str).unwrap_or("");
        if !mp_name.is_empty() && !pj_name.is_empty() && mp_name != pj_name {
            diagnostics.push(diag(
                mp_path,
                &source_type,
                format!(
                    "plugin name mismatch: marketplace.json has '{mp_name}' but plugin.json \
                     has '{pj_name}'"
                ),
            ));
        }

        if let Some(mp_d) = mp_desc {
            if let Some(pj_d) = pj.get("description").and_then(serde_json::Value::as_str) {
                if mp_d != pj_d {
                    diagnostics.push(diag(
                        mp_path,
                        &source_type,
                        format!(
                            "plugin '{mp_name}' description mismatch: marketplace.json has \
                             '{mp_d}' but plugin.json has '{pj_d}'"
                        ),
                    ));
                }
            }
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::test_helpers::MockFs;

    fn make_marketplace(name: &str, desc: &str, source: &str) -> String {
        format!(r#"{{"plugins":[{{"name":"{name}","description":"{desc}","source":"{source}"}}]}}"#)
    }

    fn make_plugin_json(name: &str, desc: &str) -> String {
        format!(r#"{{"name":"{name}","description":"{desc}","version":"0.1.0"}}"#)
    }

    #[test]
    fn rule_metadata() {
        assert_eq!(FieldMismatch.id(), "marketplace/plugin-field-mismatch");
        assert_eq!(FieldMismatch.name(), "marketplace plugin fields must match plugin.json");
        assert!(FieldMismatch.help_text().is_some());
    }

    #[test]
    fn matching_fields_no_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(&make_marketplace("foo", "A foo plugin", "./foo"));
        fs.add_plugin_json("foo", &make_plugin_json("foo", "A foo plugin"));
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn name_mismatch_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(&make_marketplace("foo", "desc", "./foo"));
        fs.add_plugin_json("foo", &make_plugin_json("foo-different", "desc"));
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("name mismatch"));
    }

    #[test]
    fn description_mismatch_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(&make_marketplace("foo", "original desc", "./foo"));
        fs.add_plugin_json("foo", &make_plugin_json("foo", "different desc"));
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("description mismatch"));
    }

    #[test]
    fn both_mismatch_emits_two_diagnostics() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(&make_marketplace("mp-name", "mp-desc", "./foo"));
        fs.add_plugin_json("foo", &make_plugin_json("pj-name", "pj-desc"));
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn plugin_json_not_found_no_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(&make_marketplace("foo", "desc", "./foo"));
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn plugin_json_parse_error_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(&make_marketplace("foo", "desc", "./foo"));
        fs.add_plugin_json("foo", "{ invalid json");
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("failed to parse plugin.json"));
    }

    #[test]
    fn nonexistent_marketplace_returns_empty() {
        let fs = MockFs::new();
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn malformed_marketplace_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json("{ bad json");
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("failed to parse marketplace.json"));
    }

    #[test]
    fn no_plugins_array_returns_empty() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"name":"local"}"#);
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn check_file_no_grandparent_returns_empty() {
        let fs = MockFs::new();
        let result = FieldMismatch.check_file(Path::new("marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn description_only_in_marketplace_no_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(
            r#"{"plugins":[{"name":"foo","description":"mp-desc","source":"./foo"}]}"#,
        );
        fs.add_plugin_json("foo", r#"{"name":"foo","version":"0.1.0"}"#);
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn entry_without_source_skipped() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"plugins":[{"name":"foo"}]}"#);
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn mp_name_empty_no_diagnostic() {
        // marketplace entry without a "name" field → mp_name="" → condition short-circuits
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"plugins":[{"description":"some desc","source":"./foo"}]}"#);
        fs.add_plugin_json("foo", &make_plugin_json("foo-different", "some desc"));
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn pj_name_empty_no_diagnostic() {
        // plugin.json without a "name" field → pj_name="" → condition short-circuits
        let mut fs = MockFs::new();
        fs.add_marketplace_json(&make_marketplace("foo", "some desc", "./foo"));
        fs.add_plugin_json("foo", r#"{"description":"some desc","version":"0.1.0"}"#);
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // --- Path-containment guard (issue #793 Finding 2) ---
    //
    // Each negative test seeds MockFs so that the file the unfixed code
    // would read via `ai_dir.join(source).join(".claude-plugin").join("plugin.json")`
    // resolves to a plugin.json with fields that DIFFER from the marketplace
    // entry. Without the guard this would generate a name- or description-
    // mismatch diagnostic; with the guard the rule silently skips the entry
    // (the sibling `marketplace/source-resolve` rule is what surfaces the
    // unsafe source to the user).

    #[test]
    fn parent_dir_traversal_in_source_skipped_no_fs_read() {
        use std::path::PathBuf;
        let mut fs = MockFs::new();
        fs.add_marketplace_json(&make_marketplace("evil", "real-desc", "../../etc/passwd"));
        // Seed a deliberate-mismatch plugin.json at the path the unfixed
        // code would build via Path::join.
        fs.files.insert(
            PathBuf::from(".ai/../../etc/passwd/.claude-plugin/plugin.json"),
            make_plugin_json("DIFFERENT-NAME", "DIFFERENT-DESC"),
        );
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        // With the guard in place the entry is skipped → no diagnostic.
        // Without it, the seeded mismatch would produce a name-mismatch
        // diagnostic (and likely a description-mismatch diagnostic too).
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn absolute_path_in_source_skipped_no_fs_read() {
        use std::path::PathBuf;
        let mut fs = MockFs::new();
        fs.add_marketplace_json(&make_marketplace("evil", "real-desc", "/etc/passwd"));
        // ai_dir.join("/etc/passwd") = "/etc/passwd" (Path::join resets on
        // absolute), so the unfixed pj_path is "/etc/passwd/.claude-plugin/plugin.json".
        fs.files.insert(
            PathBuf::from("/etc/passwd/.claude-plugin/plugin.json"),
            make_plugin_json("DIFFERENT-NAME", "DIFFERENT-DESC"),
        );
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn middle_segment_traversal_in_source_skipped() {
        use std::path::PathBuf;
        let mut fs = MockFs::new();
        fs.add_marketplace_json(&make_marketplace("evil", "real-desc", "foo/../bar"));
        fs.files.insert(
            PathBuf::from(".ai/foo/../bar/.claude-plugin/plugin.json"),
            make_plugin_json("DIFFERENT", "DIFFERENT"),
        );
        let result =
            FieldMismatch.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
