//! Rule: `plugin/missing-manifest`
//!
//! Validates that every plugin directory under `.ai/` has a
//! `.claude-plugin/plugin.json` file. The `.claude-plugin` directory itself
//! is excluded from this check.

use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;

/// Validates that every plugin directory under `.ai/` has a plugin.json manifest.
pub struct MissingManifest;

impl Rule for MissingManifest {
    fn id(&self) -> &'static str {
        "plugin/missing-manifest"
    }

    fn name(&self) -> &'static str {
        "plugin directory must have .claude-plugin/plugin.json"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/plugin/missing-manifest.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("create a .claude-plugin/plugin.json file in the plugin directory")
    }

    fn check_file(
        &self,
        file_path: &Path,
        fs: &dyn Fs,
    ) -> Result<Vec<Diagnostic>, super::super::Error> {
        let Some(ai_dir) = file_path.parent().and_then(|p| p.parent()) else {
            return Ok(vec![]);
        };
        Ok(check_manifests(ai_dir, fs))
    }
}

fn check_manifests(ai_dir: &Path, fs: &dyn Fs) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for name in super::scan::list_plugin_dirs(ai_dir, fs) {
        let pj_path = ai_dir.join(&name).join(".claude-plugin").join("plugin.json");
        if !fs.exists(&pj_path) {
            diagnostics.push(Diagnostic {
                rule_id: "plugin/missing-manifest".to_string(),
                severity: Severity::Error,
                message: format!("plugin '{name}' is missing .claude-plugin/plugin.json"),
                file_path: pj_path,
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
    diagnostics
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::fs::DirEntry;
    use crate::lint::rules::test_helpers::MockFs;

    #[test]
    fn all_plugins_have_manifest_no_diagnostic() {
        let mut fs = MockFs::new();
        let ai_path = PathBuf::from(".ai");
        fs.dirs
            .entry(ai_path)
            .or_default()
            .push(DirEntry { name: "my-plugin".to_string(), is_dir: true });
        fs.add_plugin_json("my-plugin", r#"{"name":"my-plugin","version":"0.1.0"}"#);
        let result =
            MissingManifest.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn missing_plugin_json_emits_diagnostic() {
        let mut fs = MockFs::new();
        let ai_path = PathBuf::from(".ai");
        fs.dirs
            .entry(ai_path)
            .or_default()
            .push(DirEntry { name: "my-plugin".to_string(), is_dir: true });
        let result =
            MissingManifest.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "plugin/missing-manifest");
        assert!(diags[0].message.contains("my-plugin"));
        assert!(diags[0].file_path.ends_with("plugin.json"));
    }

    #[test]
    fn claude_plugin_dir_excluded() {
        let mut fs = MockFs::new();
        let ai_path = PathBuf::from(".ai");
        fs.dirs
            .entry(ai_path)
            .or_default()
            .push(DirEntry { name: ".claude-plugin".to_string(), is_dir: true });
        let result =
            MissingManifest.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn file_entries_ignored() {
        let mut fs = MockFs::new();
        let ai_path = PathBuf::from(".ai");
        fs.dirs
            .entry(ai_path)
            .or_default()
            .push(DirEntry { name: "README.md".to_string(), is_dir: false });
        let result =
            MissingManifest.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn check_file_no_grandparent_returns_empty() {
        let fs = MockFs::new();
        let result = MissingManifest.check_file(Path::new("marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn ai_dir_not_readable_returns_empty() {
        let fs = MockFs::new();
        let result =
            MissingManifest.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
