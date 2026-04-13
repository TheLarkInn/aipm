//! Rule: `plugin/required-fields`
//!
//! Validates that `plugin.json` contains all required fields:
//! `name`, `description`, `version`, `author.name`, and `author.email`.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;

/// Validates required fields in plugin.json.
pub struct RequiredFields;

impl Rule for RequiredFields {
    fn id(&self) -> &'static str {
        "plugin/required-fields"
    }

    fn name(&self) -> &'static str {
        "plugin.json must have required fields"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/plugin/required-fields.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some(
            "add the missing required fields to plugin.json \
             (name, description, version, author.name, author.email)",
        )
    }

    fn check_file(
        &self,
        file_path: &Path,
        fs: &dyn Fs,
    ) -> Result<Vec<Diagnostic>, super::super::Error> {
        Ok(check_required_fields(file_path, fs))
    }
}

fn diag(pj_path: &Path, source_type: &str, message: String) -> Diagnostic {
    super::simple_diag("plugin/required-fields", Severity::Error, message, pj_path, source_type)
}

fn check_top_level(
    parsed: &serde_json::Value,
    pj_path: &Path,
    source_type: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for field in &["name", "description", "version"] {
        let present = parsed
            .get(*field)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|v| !v.trim().is_empty());
        if !present {
            diagnostics.push(diag(
                pj_path,
                source_type,
                format!("plugin.json is missing required field: {field}"),
            ));
        }
    }
}

fn check_author(
    parsed: &serde_json::Value,
    pj_path: &Path,
    source_type: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match parsed.get("author") {
        None => diagnostics.push(diag(
            pj_path,
            source_type,
            "plugin.json is missing required field: author".to_string(),
        )),
        Some(author) => {
            if author.is_object() {
                for sub in &["name", "email"] {
                    let present = author
                        .get(*sub)
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|v| !v.trim().is_empty());
                    if !present {
                        diagnostics.push(diag(
                            pj_path,
                            source_type,
                            format!("plugin.json is missing required field: author.{sub}"),
                        ));
                    }
                }
            } else {
                diagnostics.push(diag(
                    pj_path,
                    source_type,
                    "plugin.json 'author' field must be an object".to_string(),
                ));
            }
        },
    }
}

fn check_required_fields(pj_path: &Path, fs: &dyn Fs) -> Vec<Diagnostic> {
    let Ok(content) = fs.read_to_string(pj_path) else {
        return vec![];
    };

    let source_type = super::scan::source_type_from_path(pj_path).to_string();

    let parsed: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            return vec![diag(pj_path, &source_type, format!("failed to parse plugin.json: {e}"))];
        },
    };

    let mut diagnostics = Vec::new();
    check_top_level(&parsed, pj_path, &source_type, &mut diagnostics);
    check_author(&parsed, pj_path, &source_type, &mut diagnostics);
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::test_helpers::MockFs;

    const FULL_VALID: &str = r#"{
        "name": "my-plugin",
        "description": "A plugin",
        "version": "0.1.0",
        "author": { "name": "Dev", "email": "dev@example.com" }
    }"#;

    #[test]
    fn all_fields_present_no_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_plugin_json("my-plugin", FULL_VALID);
        let result =
            RequiredFields.check_file(Path::new(".ai/my-plugin/.claude-plugin/plugin.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn name_missing_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_plugin_json(
            "p",
            r#"{"description":"d","version":"0.1.0","author":{"name":"x","email":"x@x.com"}}"#,
        );
        let result = RequiredFields.check_file(Path::new(".ai/p/.claude-plugin/plugin.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert!(diags.iter().any(|d| d.message.contains("missing required field: name")));
    }

    #[test]
    fn description_missing_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_plugin_json(
            "p",
            r#"{"name":"p","version":"0.1.0","author":{"name":"x","email":"x@x.com"}}"#,
        );
        let result = RequiredFields.check_file(Path::new(".ai/p/.claude-plugin/plugin.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert!(diags.iter().any(|d| d.message.contains("missing required field: description")));
    }

    #[test]
    fn version_missing_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_plugin_json(
            "p",
            r#"{"name":"p","description":"d","author":{"name":"x","email":"x@x.com"}}"#,
        );
        let result = RequiredFields.check_file(Path::new(".ai/p/.claude-plugin/plugin.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert!(diags.iter().any(|d| d.message.contains("missing required field: version")));
    }

    #[test]
    fn author_missing_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_plugin_json("p", r#"{"name":"p","description":"d","version":"0.1.0"}"#);
        let result = RequiredFields.check_file(Path::new(".ai/p/.claude-plugin/plugin.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert!(diags.iter().any(|d| d.message.contains("missing required field: author")));
    }

    #[test]
    fn author_not_object_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_plugin_json(
            "p",
            r#"{"name":"p","description":"d","version":"0.1.0","author":"string"}"#,
        );
        let result = RequiredFields.check_file(Path::new(".ai/p/.claude-plugin/plugin.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert!(diags.iter().any(|d| d.message.contains("'author' field must be an object")));
    }

    #[test]
    fn author_name_missing_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_plugin_json(
            "p",
            r#"{"name":"p","description":"d","version":"0.1.0","author":{"email":"x@x.com"}}"#,
        );
        let result = RequiredFields.check_file(Path::new(".ai/p/.claude-plugin/plugin.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert!(diags.iter().any(|d| d.message.contains("author.name")));
    }

    #[test]
    fn author_email_missing_emits_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_plugin_json(
            "p",
            r#"{"name":"p","description":"d","version":"0.1.0","author":{"name":"Dev"}}"#,
        );
        let result = RequiredFields.check_file(Path::new(".ai/p/.claude-plugin/plugin.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert!(diags.iter().any(|d| d.message.contains("author.email")));
    }

    #[test]
    fn empty_string_fields_treated_as_missing() {
        let mut fs = MockFs::new();
        fs.add_plugin_json(
            "p",
            r#"{"name":"","description":"","version":"","author":{"name":"","email":""}}"#,
        );
        let result = RequiredFields.check_file(Path::new(".ai/p/.claude-plugin/plugin.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 5);
    }

    #[test]
    fn whitespace_only_fields_treated_as_missing() {
        let mut fs = MockFs::new();
        fs.add_plugin_json(
            "p",
            r#"{"name":"  ","description":"\t","version":" ","author":{"name":" ","email":"  "}}"#,
        );
        let result = RequiredFields.check_file(Path::new(".ai/p/.claude-plugin/plugin.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 5);
    }

    #[test]
    fn malformed_json_emits_single_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_plugin_json("p", "{ bad json {{");
        let result = RequiredFields.check_file(Path::new(".ai/p/.claude-plugin/plugin.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("failed to parse"));
    }

    #[test]
    fn nonexistent_file_returns_empty() {
        let fs = MockFs::new();
        let result = RequiredFields.check_file(Path::new(".ai/p/.claude-plugin/plugin.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
