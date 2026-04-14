//! Registrar: appends migrated plugins to `marketplace.json`.

use std::path::Path;

use crate::fs::Fs;

use super::{Error, PluginEntry};

/// Append migrated plugins to `marketplace.json` without modifying existing entries.
pub fn register_plugins(ai_dir: &Path, entries: &[PluginEntry], fs: &dyn Fs) -> Result<(), Error> {
    if entries.is_empty() {
        return Ok(());
    }

    tracing::debug!(count = entries.len(), "registering plugins in marketplace.json");

    let marketplace_path = ai_dir.join(".claude-plugin").join("marketplace.json");

    let gen_entries: Vec<crate::generate::marketplace::Entry<'_>> = entries
        .iter()
        .map(|e| crate::generate::marketplace::Entry {
            name: &e.name,
            description: e.description.as_deref().unwrap_or("Migrated from .claude/ configuration"),
        })
        .collect();

    crate::generate::marketplace::register_all(fs, &marketplace_path, &gen_entries).map_err(
        |e| {
            // Only map to MarketplaceJsonParse when the underlying cause is a real
            // serde_json::Error (JSON parse failure). Structural issues like a missing
            // or non-array "plugins" key, and I/O errors, map to Error::Io.
            let is_json_parse = e.kind() == std::io::ErrorKind::InvalidData
                && e.get_ref().and_then(|s| s.downcast_ref::<serde_json::Error>()).is_some();
            if !is_json_parse {
                return Error::Io(e);
            }
            e.into_inner()
                .and_then(|s| s.downcast::<serde_json::Error>().ok())
                .map(|b| *b)
                .map_or_else(
                    || {
                        Error::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "marketplace JSON parse error",
                        ))
                    },
                    |source| Error::MarketplaceJsonParse { path: marketplace_path.clone(), source },
                )
        },
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;
    use std::sync::Mutex;

    struct MockFs {
        exists: HashSet<PathBuf>,
        files: Mutex<HashMap<PathBuf, String>>,
        written: Mutex<HashMap<PathBuf, Vec<u8>>>,
        fail_write: Mutex<bool>,
    }

    impl MockFs {
        fn new() -> Self {
            Self {
                exists: HashSet::new(),
                files: Mutex::new(HashMap::new()),
                written: Mutex::new(HashMap::new()),
                fail_write: Mutex::new(false),
            }
        }

        fn set_file(&self, path: PathBuf, content: String) {
            self.files.lock().expect("MockFs::set_file: mutex poisoned").insert(path, content);
        }

        fn get_written(&self, path: &Path) -> Option<String> {
            self.written
                .lock()
                .expect("MockFs::get_written: mutex poisoned")
                .get(path)
                .and_then(|b| String::from_utf8(b.clone()).ok())
        }

        fn set_fail_write(&self) {
            *self.fail_write.lock().expect("MockFs::set_fail_write: mutex poisoned") = true;
        }
    }

    impl crate::fs::Fs for MockFs {
        fn exists(&self, path: &Path) -> bool {
            self.exists.contains(path)
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Ok(())
        }

        fn write_file(&self, path: &Path, content: &[u8]) -> std::io::Result<()> {
            if *self.fail_write.lock().expect("MockFs::write_file: mutex poisoned") {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "write failed",
                ));
            }
            self.written
                .lock()
                .expect("MockFs::write_file: mutex poisoned")
                .insert(path.to_path_buf(), content.to_vec());
            // Keep files map in sync so subsequent reads see the latest write.
            if let Ok(s) = String::from_utf8(content.to_vec()) {
                self.files
                    .lock()
                    .expect("MockFs::write_file: mutex poisoned")
                    .insert(path.to_path_buf(), s);
            }
            Ok(())
        }

        fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
            self.files
                .lock()
                .expect("MockFs::read_to_string: mutex poisoned")
                .get(path)
                .cloned()
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("not found: {}", path.display()),
                    )
                })
        }

        fn read_dir(&self, _: &Path) -> std::io::Result<Vec<crate::fs::DirEntry>> {
            Ok(Vec::new())
        }
    }

    fn marketplace_path() -> PathBuf {
        PathBuf::from("/ai/.claude-plugin/marketplace.json")
    }

    fn entry(name: &str, description: Option<&str>) -> PluginEntry {
        PluginEntry { name: name.to_string(), description: description.map(String::from) }
    }

    #[test]
    fn register_appends_to_empty_plugins_array() {
        let fs = MockFs::new();
        fs.set_file(marketplace_path(), r#"{"name":"test-marketplace","plugins":[]}"#.to_string());

        let entries = vec![entry("deploy", None), entry("lint", None)];
        let result = register_plugins(Path::new("/ai"), &entries, &fs);
        assert!(result.is_ok());

        let written = fs.get_written(&marketplace_path());
        assert!(written.as_ref().is_some_and(|c| c.contains("deploy")));
        assert!(written.as_ref().is_some_and(|c| c.contains("lint")));
    }

    #[test]
    fn register_appends_alongside_existing() {
        let fs = MockFs::new();
        fs.set_file(
            marketplace_path(),
            r#"{"name":"test","plugins":[{"name":"starter-aipm-plugin","source":"./starter-aipm-plugin"}]}"#.to_string(),
        );

        let entries = vec![entry("deploy", None)];
        let result = register_plugins(Path::new("/ai"), &entries, &fs);
        assert!(result.is_ok());

        let written = fs.get_written(&marketplace_path());
        assert!(written.as_ref().is_some_and(|c| c.contains("starter-aipm-plugin")));
        assert!(written.as_ref().is_some_and(|c| c.contains("deploy")));
    }

    #[test]
    fn register_skips_already_registered() {
        let original = r#"{"plugins":[{"name":"deploy","source":"./deploy"}]}"#;
        let fs = MockFs::new();
        fs.set_file(marketplace_path(), original.to_string());

        let entries = vec![entry("deploy", None)];
        let result = register_plugins(Path::new("/ai"), &entries, &fs);
        assert!(result.is_ok());

        // No write should occur when all entries are already registered (no unnecessary I/O).
        assert!(
            fs.get_written(&marketplace_path()).is_none(),
            "marketplace.json should not be rewritten when no changes are needed"
        );

        // Original content is preserved — still has exactly 1 plugin.
        let content = fs.read_to_string(&marketplace_path()).unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(&content).expect("marketplace.json should be valid JSON");
        let plugins = parsed.get("plugins").and_then(|v| v.as_array()).map(Vec::len).unwrap_or(0);
        assert_eq!(plugins, 1, "deploy should not be duplicated in marketplace.json");
    }

    #[test]
    fn register_preserves_marketplace_metadata() {
        let fs = MockFs::new();
        fs.set_file(
            marketplace_path(),
            r#"{"name":"my-marketplace","owner":"team","metadata":{"version":"1"},"plugins":[]}"#
                .to_string(),
        );

        let entries = vec![entry("deploy", None)];
        let result = register_plugins(Path::new("/ai"), &entries, &fs);
        assert!(result.is_ok());

        let written = fs.get_written(&marketplace_path());
        assert!(written.as_ref().is_some_and(|c| c.contains("my-marketplace")));
        assert!(written.as_ref().is_some_and(|c| c.contains("team")));
        assert!(written.as_ref().is_some_and(|c| c.contains("version")));
    }

    /// Parse the written marketplace.json from MockFs, asserting it was written and is valid.
    fn parse_written_marketplace(fs: &MockFs) -> serde_json::Value {
        let content =
            fs.get_written(&marketplace_path()).expect("marketplace.json should have been written");
        serde_json::from_str(&content).expect("marketplace.json should be valid JSON")
    }

    #[test]
    fn register_uses_entry_description() {
        let fs = MockFs::new();
        fs.set_file(marketplace_path(), r#"{"plugins":[]}"#.to_string());

        let entries = vec![entry("deploy", Some("Deploy app"))];
        let result = register_plugins(Path::new("/ai"), &entries, &fs);
        assert!(result.is_ok());

        let parsed = parse_written_marketplace(&fs);
        let plugin = parsed.get("plugins").and_then(|v| v.as_array()).and_then(|a| a.first());
        assert_eq!(
            plugin.and_then(|p| p.get("description")).and_then(serde_json::Value::as_str),
            Some("Deploy app")
        );
    }

    #[test]
    fn register_uses_fallback_when_no_description() {
        let fs = MockFs::new();
        fs.set_file(marketplace_path(), r#"{"plugins":[]}"#.to_string());

        let entries = vec![entry("deploy", None)];
        let result = register_plugins(Path::new("/ai"), &entries, &fs);
        assert!(result.is_ok());

        let parsed = parse_written_marketplace(&fs);
        let plugin = parsed.get("plugins").and_then(|v| v.as_array()).and_then(|a| a.first());
        assert_eq!(
            plugin.and_then(|p| p.get("description")).and_then(serde_json::Value::as_str),
            Some("Migrated from .claude/ configuration")
        );
    }

    #[test]
    fn register_mixed_descriptions() {
        let fs = MockFs::new();
        fs.set_file(marketplace_path(), r#"{"plugins":[]}"#.to_string());

        let entries = vec![entry("deploy", Some("Deploy app")), entry("lint", None)];
        let result = register_plugins(Path::new("/ai"), &entries, &fs);
        assert!(result.is_ok());

        let parsed = parse_written_marketplace(&fs);
        let plugins = parsed.get("plugins").and_then(|v| v.as_array());

        let deploy = plugins.and_then(|a| {
            a.iter().find(|p| p.get("name").and_then(|n| n.as_str()) == Some("deploy"))
        });
        assert_eq!(
            deploy.and_then(|p| p.get("description")).and_then(serde_json::Value::as_str),
            Some("Deploy app")
        );

        let lint = plugins.and_then(|a| {
            a.iter().find(|p| p.get("name").and_then(|n| n.as_str()) == Some("lint"))
        });
        assert_eq!(
            lint.and_then(|p| p.get("description")).and_then(serde_json::Value::as_str),
            Some("Migrated from .claude/ configuration")
        );
    }

    #[test]
    fn register_returns_error_when_plugins_key_missing() {
        // marketplace.json exists and is valid JSON, but has no "plugins" array.
        // This covers the ok_or_else branch that returns Error::Io with InvalidData.
        let fs = MockFs::new();
        fs.set_file(marketplace_path(), r#"{"name":"my-marketplace"}"#.to_string());

        let entries = vec![entry("deploy", None)];
        let result = register_plugins(Path::new("/ai"), &entries, &fs);
        assert!(result.is_err());
    }

    #[test]
    fn register_returns_error_when_plugins_is_not_array() {
        // marketplace.json has "plugins" but it's a string, not an array.
        // This also hits the ok_or_else branch (as_array_mut returns None).
        let fs = MockFs::new();
        fs.set_file(marketplace_path(), r#"{"plugins":"not-an-array"}"#.to_string());

        let entries = vec![entry("deploy", None)];
        let result = register_plugins(Path::new("/ai"), &entries, &fs);
        assert!(result.is_err());
    }

    #[test]
    fn register_returns_error_when_marketplace_file_not_found() {
        // No file is set in the mock — read_to_string returns NotFound,
        // covering the Err branch of the `?` on the read_to_string call.
        let fs = MockFs::new();

        let entries = vec![entry("deploy", None)];
        let result = register_plugins(Path::new("/ai"), &entries, &fs);
        assert!(result.is_err());
    }

    #[test]
    fn register_returns_error_when_write_file_fails() {
        // Set up valid marketplace.json but make write_file fail,
        // covering the Err branch of the `?` on the write_file call.
        let fs = MockFs::new();
        fs.set_file(marketplace_path(), r#"{"plugins":[]}"#.to_string());
        fs.set_fail_write();

        let entries = vec![entry("deploy", None)];
        let result = register_plugins(Path::new("/ai"), &entries, &fs);
        assert!(result.is_err());
    }

    #[test]
    fn register_returns_error_when_marketplace_json_invalid() {
        // marketplace.json content is not valid JSON, causing from_str to fail
        // and covering the Err branch of the `?` on the serde_json::from_str call.
        let fs = MockFs::new();
        fs.set_file(marketplace_path(), "not valid json {{{".to_string());

        let entries = vec![entry("deploy", None)];
        let result = register_plugins(Path::new("/ai"), &entries, &fs);
        assert!(result.is_err());
    }

    #[test]
    fn register_empty_entries_returns_ok_without_reading_marketplace() {
        // Covers the `if entries.is_empty()` early-return branch.
        // No marketplace file is set: if the function tried to read it,
        // it would get a NotFound error. With an empty slice it must return
        // Ok(()) immediately without touching the filesystem.
        let fs = MockFs::new();
        let result = register_plugins(Path::new("/ai"), &[], &fs);
        assert!(result.is_ok());
        // Nothing should have been written either.
        assert!(fs.get_written(&marketplace_path()).is_none());
    }
}
