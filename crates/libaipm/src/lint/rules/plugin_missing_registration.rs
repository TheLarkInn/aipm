//! Rule: `plugin/missing-registration`
//!
//! Validates that every plugin directory under `.ai/` is registered in
//! `marketplace.json`. The `.claude-plugin` directory itself is excluded.

use std::collections::HashSet;
use std::path::Path;

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;

/// Validates that every plugin directory under `.ai/` is registered in marketplace.json.
pub struct MissingRegistration;

impl Rule for MissingRegistration {
    fn id(&self) -> &'static str {
        "plugin/missing-registration"
    }

    fn name(&self) -> &'static str {
        "plugin directory must be registered in marketplace.json"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/plugin/missing-registration.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("add this plugin to the plugins array in .ai/.claude-plugin/marketplace.json")
    }

    fn check(
        &self,
        source_dir: &Path,
        fs: &dyn Fs,
    ) -> Result<Vec<Diagnostic>, super::super::Error> {
        let mp_path = source_dir.join(".claude-plugin").join("marketplace.json");
        Ok(check_registration(&mp_path, source_dir, fs))
    }

    fn check_file(
        &self,
        file_path: &Path,
        fs: &dyn Fs,
    ) -> Result<Vec<Diagnostic>, super::super::Error> {
        let Some(ai_dir) = file_path.parent().and_then(|p| p.parent()) else {
            return Ok(vec![]);
        };
        Ok(check_registration(file_path, ai_dir, fs))
    }
}

fn diag(mp_path: &Path, source_type: &str, message: String) -> Diagnostic {
    Diagnostic {
        rule_id: "plugin/missing-registration".to_string(),
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

fn check_registration(mp_path: &Path, ai_dir: &Path, fs: &dyn Fs) -> Vec<Diagnostic> {
    let source_type = super::scan::source_type_from_path(mp_path).to_string();

    let Ok(content) = fs.read_to_string(mp_path) else {
        // marketplace.json absent — report all plugin dirs as unregistered
        return super::scan::list_plugin_dirs(ai_dir, fs)
            .into_iter()
            .map(|name| {
                diag(
                    mp_path,
                    &source_type,
                    format!("plugin directory '{name}' is not registered in marketplace.json"),
                )
            })
            .collect();
    };

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

    let registered: HashSet<String> = plugins
        .iter()
        .filter_map(|e| {
            e.get("source")
                .and_then(serde_json::Value::as_str)
                .map(|s| s.trim_start_matches("./").to_string())
        })
        .collect();

    super::scan::list_plugin_dirs(ai_dir, fs)
        .into_iter()
        .filter(|name| !registered.contains(name))
        .map(|name| {
            diag(
                mp_path,
                &source_type,
                format!("plugin directory '{name}' is not registered in marketplace.json"),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::fs::DirEntry;
    use crate::lint::rules::test_helpers::MockFs;

    fn make_fs_with_plugins(plugin_names: &[&str], registered: &[&str]) -> MockFs {
        let mut fs = MockFs::new();

        let plugins_json: Vec<String> =
            registered.iter().map(|n| format!(r#"{{"name":"{n}","source":"./{n}"}}"#)).collect();
        let mp_content = format!(r#"{{"plugins":[{}]}}"#, plugins_json.join(","));
        fs.add_marketplace_json(&mp_content);

        let ai_path = PathBuf::from(".ai");
        for name in plugin_names {
            let entries = fs.dirs.entry(ai_path.clone()).or_default();
            if !entries.iter().any(|e| e.name == *name) {
                entries.push(DirEntry { name: (*name).to_string(), is_dir: true });
            }
        }

        fs
    }

    #[test]
    fn rule_metadata() {
        assert_eq!(MissingRegistration.id(), "plugin/missing-registration");
        assert_eq!(
            MissingRegistration.name(),
            "plugin directory must be registered in marketplace.json"
        );
        assert!(MissingRegistration.help_text().is_some());
    }

    #[test]
    fn all_dirs_registered_no_diagnostic() {
        let fs = make_fs_with_plugins(&["foo", "bar"], &["foo", "bar"]);
        let result =
            MissingRegistration.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn unregistered_dir_emits_diagnostic() {
        let fs = make_fs_with_plugins(&["foo", "unregistered"], &["foo"]);
        let result =
            MissingRegistration.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "plugin/missing-registration");
        assert!(diags[0].message.contains("unregistered"));
    }

    #[test]
    fn claude_plugin_dir_excluded_no_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"plugins":[]}"#);
        let ai_path = PathBuf::from(".ai");
        fs.dirs
            .entry(ai_path)
            .or_default()
            .push(DirEntry { name: ".claude-plugin".to_string(), is_dir: true });
        let result =
            MissingRegistration.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn empty_plugins_array_emits_diagnostic_for_each_dir() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"plugins":[]}"#);
        let ai_path = PathBuf::from(".ai");
        for name in &["foo", "bar"] {
            fs.dirs
                .entry(ai_path.clone())
                .or_default()
                .push(DirEntry { name: (*name).to_string(), is_dir: true });
        }
        let result =
            MissingRegistration.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn nonexistent_marketplace_still_reports_unregistered() {
        let mut fs = MockFs::new();
        let ai_path = PathBuf::from(".ai");
        fs.dirs
            .entry(ai_path)
            .or_default()
            .push(DirEntry { name: "my-plugin".to_string(), is_dir: true });
        let result =
            MissingRegistration.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn file_entries_in_ai_dir_ignored() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"plugins":[]}"#);
        let ai_path = PathBuf::from(".ai");
        fs.dirs
            .entry(ai_path)
            .or_default()
            .push(DirEntry { name: "README.md".to_string(), is_dir: false });
        let result =
            MissingRegistration.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn check_file_no_grandparent_returns_empty() {
        let fs = MockFs::new();
        let result = MissingRegistration.check_file(Path::new("marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn check_directory_level() {
        let fs = make_fs_with_plugins(&["foo"], &["foo"]);
        let result = MissingRegistration.check(Path::new(".ai"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn malformed_marketplace_json_emits_parse_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json("{ bad json {{");
        let ai_path = PathBuf::from(".ai");
        fs.dirs
            .entry(ai_path)
            .or_default()
            .push(DirEntry { name: "my-plugin".to_string(), is_dir: true });
        let result =
            MissingRegistration.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("failed to parse"));
    }

    #[test]
    fn ai_dir_not_readable_returns_empty() {
        let mut fs = MockFs::new();
        // Manually insert marketplace.json without setting up .ai dir entries
        fs.files.insert(
            PathBuf::from(".ai/.claude-plugin/marketplace.json"),
            r#"{"plugins":[]}"#.to_string(),
        );
        let result =
            MissingRegistration.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn no_plugins_key_emits_missing_array_diagnostic() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"name":"local"}"#);
        let ai_path = PathBuf::from(".ai");
        fs.dirs
            .entry(ai_path)
            .or_default()
            .push(DirEntry { name: "my-plugin".to_string(), is_dir: true });
        let result =
            MissingRegistration.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing a 'plugins' array"));
    }

    #[test]
    fn duplicate_plugin_names_in_helper_are_deduplicated() {
        // Passing the same name twice to make_fs_with_plugins exercises the
        // deduplication branch: the second "foo" finds `any()` returning true
        // and skips the push, leaving only one "foo" dir in the mock filesystem.
        let fs = make_fs_with_plugins(&["foo", "foo"], &[]);
        let result =
            MissingRegistration.check_file(Path::new(".ai/.claude-plugin/marketplace.json"), &fs);
        assert!(result.is_ok());
        let diags = result.unwrap();
        // Only one "foo" entry exists (deduplicated), and it is not registered.
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("foo"));
    }
}
