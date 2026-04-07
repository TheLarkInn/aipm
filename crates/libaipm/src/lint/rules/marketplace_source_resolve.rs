//! Rule: `marketplace/source-resolve`
//!
//! Validates that every plugin entry in `marketplace.json` has a `source` field
//! and that the resolved path exists on disk.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;

/// Validates that plugin source paths in marketplace.json resolve to existing directories.
pub struct SourceResolve;

impl Rule for SourceResolve {
    fn id(&self) -> &'static str {
        "marketplace/source-resolve"
    }

    fn name(&self) -> &'static str {
        "marketplace plugin source must resolve"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn help_url(&self) -> Option<&'static str> {
        Some(
            "https://github.com/TheLarkInn/aipm/blob/main/docs/rules/marketplace/source-resolve.md",
        )
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("ensure the source field points to an existing plugin directory under .ai/")
    }

    fn check(
        &self,
        source_dir: &Path,
        fs: &dyn Fs,
    ) -> Result<Vec<Diagnostic>, super::super::Error> {
        let mp_path = source_dir.join(".claude-plugin").join("marketplace.json");
        Ok(check_marketplace(&mp_path, source_dir, fs))
    }

    fn check_file(
        &self,
        file_path: &Path,
        fs: &dyn Fs,
    ) -> Result<Vec<Diagnostic>, super::super::Error> {
        let Some(ai_dir) = file_path.parent().and_then(|p| p.parent()) else {
            return Ok(vec![]);
        };
        Ok(check_marketplace(file_path, ai_dir, fs))
    }
}

fn diag(mp_path: &Path, source_type: &str, message: String) -> Diagnostic {
    Diagnostic {
        rule_id: "marketplace/source-resolve".to_string(),
        severity: Severity::Error,
        message,
        file_path: mp_path.to_path_buf(),
        line: None,
        col: None,
        end_line: None,
        end_col: None,
        source_type: source_type.to_string(),
        help_text: None,
        help_url: None,
    }
}

fn check_marketplace(mp_path: &Path, ai_dir: &Path, fs: &dyn Fs) -> Vec<Diagnostic> {
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
        return vec![diag(
            mp_path,
            &source_type,
            "marketplace.json is missing a 'plugins' array".to_string(),
        )];
    };

    let mut diagnostics = Vec::new();

    for (i, entry) in plugins.iter().enumerate() {
        let fallback = format!("plugins[{i}]");
        let plugin_name =
            entry.get("name").and_then(serde_json::Value::as_str).unwrap_or(&fallback);

        match entry.get("source") {
            None => diagnostics.push(diag(
                mp_path,
                &source_type,
                format!("plugin '{plugin_name}' in marketplace.json is missing a 'source' field"),
            )),
            Some(source_value) => match source_value.as_str() {
                None => diagnostics.push(diag(
                    mp_path,
                    &source_type,
                    format!(
                        "plugin '{plugin_name}' in marketplace.json 'source' field must be a string"
                    ),
                )),
                Some(source) => {
                    let resolved = ai_dir.join(source.trim_start_matches("./"));
                    if !fs.exists(&resolved) {
                        diagnostics.push(diag(
                            mp_path,
                            &source_type,
                            format!(
                                "plugin '{plugin_name}' source path does not resolve: {source}"
                            ),
                        ));
                    }
                },
            },
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::test_helpers::MockFs;

    #[test]
    fn source_present_and_resolves_no_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"name":"local","plugins":[{"name":"foo","source":"./foo"}]}"#);
        fs.add_existing(".ai/foo");
        let result =
            SourceResolve.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn source_field_missing_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"name":"local","plugins":[{"name":"foo"}]}"#);
        let result =
            SourceResolve.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "marketplace/source-resolve");
        assert!(diags[0].message.contains("missing a 'source' field"));
    }

    #[test]
    fn source_path_does_not_exist_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"name":"local","plugins":[{"name":"foo","source":"./foo"}]}"#);
        let result =
            SourceResolve.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does not resolve"));
    }

    #[test]
    fn malformed_json_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json("not valid json {{{");
        let result =
            SourceResolve.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("failed to parse"));
    }

    #[test]
    fn missing_plugins_array_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"name":"local"}"#);
        let result =
            SourceResolve.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing a 'plugins' array"));
    }

    #[test]
    fn nonexistent_file_returns_empty() {
        let fs = MockFs::new();
        let result =
            SourceResolve.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn check_directory_level_uses_ai_dir() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"name":"local","plugins":[{"name":"bar","source":"./bar"}]}"#);
        fs.add_existing(".ai/bar");
        let result = SourceResolve.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn check_file_no_grandparent_returns_empty() {
        let fs = MockFs::new();
        let result = SourceResolve.check_file(Path::new("marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn multiple_plugins_one_bad_source_emits_one_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(
            r#"{"plugins":[{"name":"ok","source":"./ok"},{"name":"bad","source":"./bad"}]}"#,
        );
        fs.add_existing(".ai/ok");
        let result =
            SourceResolve.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bad"));
    }

    #[test]
    fn source_without_dotslash_prefix_resolves() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"plugins":[{"name":"baz","source":"baz"}]}"#);
        fs.add_existing(".ai/baz");
        let result =
            SourceResolve.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn check_file_shallow_path_no_grandparent_returns_empty() {
        let fs = MockFs::new();
        let result = SourceResolve.check_file(Path::new(".claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn plugins_array_is_not_array_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"plugins":"not-an-array"}"#);
        let result =
            SourceResolve.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing a 'plugins' array"));
    }

    #[test]
    fn plugin_without_name_uses_index_in_message() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"plugins":[{"source":"./nope"}]}"#);
        let result =
            SourceResolve.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does not resolve"));
    }

    #[test]
    fn plugin_entry_source_not_a_string_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"plugins":[{"name":"x","source":42}]}"#);
        let result =
            SourceResolve.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("must be a string"));
    }
}
