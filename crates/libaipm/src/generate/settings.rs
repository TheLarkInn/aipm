//! Unified settings.json read-modify-write operations.
//!
//! Provides in-memory mutation helpers for `extraKnownMarketplaces` and
//! `enabledPlugins`, plus I/O wrappers for reading/creating and writing
//! settings files.

use std::path::Path;

use crate::fs::Fs;

/// Read a settings.json file, returning an empty JSON object if the file
/// does not exist.
///
/// # Errors
///
/// Returns `io::Error` if the file exists but cannot be read or parsed.
pub fn read_or_create(fs: &dyn Fs, path: &Path) -> std::io::Result<serde_json::Value> {
    match fs.read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(serde_json::Value::Object(serde_json::Map::new()))
        },
        Err(e) => Err(e),
    }
}

/// Write a settings.json value to disk with pretty formatting and a trailing
/// newline.
///
/// # Errors
///
/// Returns `io::Error` if the file cannot be written.
pub fn write(fs: &dyn Fs, path: &Path, value: &serde_json::Value) -> std::io::Result<()> {
    let mut output = serde_json::to_string_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    output.push('\n');
    fs.write_file(path, output.as_bytes())
}

/// Add a marketplace entry to `extraKnownMarketplaces` in settings JSON.
///
/// Inserts the marketplace with the standard directory source pointing to
/// `./.ai`.  Returns `true` if the entry was added, `false` if it already
/// existed.
///
/// If the root value is not an object, returns `false` without modification.
pub fn add_known_marketplace(settings: &mut serde_json::Value, marketplace_name: &str) -> bool {
    let Some(obj) = settings.as_object_mut() else {
        return false;
    };

    let marketplace_entry = serde_json::json!({
        "source": { "source": "directory", "path": "./.ai" }
    });

    let ekm = obj
        .entry("extraKnownMarketplaces")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

    if let Some(ekm_obj) = ekm.as_object_mut() {
        if ekm_obj.contains_key(marketplace_name) {
            return false;
        }
        ekm_obj.insert(marketplace_name.to_string(), marketplace_entry);
        true
    } else {
        false
    }
}

/// Enable a plugin in `enabledPlugins` in settings JSON.
///
/// Sets the plugin key to `true`.  Returns `true` if the entry was added,
/// `false` if it already existed.
///
/// If the root value is not an object, returns `false` without modification.
pub fn enable_plugin(settings: &mut serde_json::Value, plugin_key: &str) -> bool {
    let Some(obj) = settings.as_object_mut() else {
        return false;
    };

    let enabled = obj
        .entry("enabledPlugins")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

    if let Some(enabled_obj) = enabled.as_object_mut() {
        if enabled_obj.contains_key(plugin_key) {
            return false;
        }
        enabled_obj.insert(plugin_key.to_string(), serde_json::json!(true));
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockFs {
        files: Mutex<HashMap<std::path::PathBuf, Vec<u8>>>,
    }

    impl MockFs {
        fn new() -> Self {
            Self { files: Mutex::new(HashMap::new()) }
        }
    }

    impl crate::fs::Fs for MockFs {
        fn exists(&self, path: &Path) -> bool {
            self.files.lock().unwrap_or_else(|p| p.into_inner()).contains_key(path)
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Ok(())
        }

        fn write_file(&self, path: &Path, content: &[u8]) -> std::io::Result<()> {
            self.files
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(path.to_path_buf(), content.to_vec());
            Ok(())
        }

        fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
            self.files
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(path)
                .and_then(|b| String::from_utf8(b.clone()).ok())
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

    // --- read_or_create tests ---

    #[test]
    fn read_or_create_returns_empty_object_for_missing_file() {
        let fs = MockFs::new();
        let path = Path::new("/settings.json");
        let result = read_or_create(&fs, path);
        assert!(result.is_ok());
        let val = result.unwrap_or_default();
        assert!(val.is_object());
        assert!(val.as_object().is_some_and(|o| o.is_empty()));
    }

    #[test]
    fn read_or_create_parses_existing_file() {
        let fs = MockFs::new();
        let path = Path::new("/settings.json");
        fs.write_file(path, br#"{"foo": "bar"}"#).unwrap_or_default();

        let result = read_or_create(&fs, path);
        assert!(result.is_ok());
        let val = result.unwrap_or_default();
        assert_eq!(val.get("foo").and_then(serde_json::Value::as_str), Some("bar"));
    }

    #[test]
    fn read_or_create_returns_error_for_invalid_json() {
        let fs = MockFs::new();
        let path = Path::new("/settings.json");
        fs.write_file(path, b"not valid json").unwrap_or_default();

        let result = read_or_create(&fs, path);
        assert!(result.is_err());
    }

    // --- write tests ---

    #[test]
    fn write_pretty_prints_with_newline() {
        let fs = MockFs::new();
        let path = Path::new("/settings.json");
        let val = serde_json::json!({"key": "value"});

        let result = write(&fs, path, &val);
        assert!(result.is_ok());

        let content = fs.read_to_string(path).unwrap_or_default();
        assert!(content.contains("\"key\""));
        assert!(content.contains("\"value\""));
        assert!(content.ends_with('\n'));
    }

    // --- add_known_marketplace tests ---

    #[test]
    fn add_known_marketplace_to_empty_object() {
        let mut settings = serde_json::json!({});
        let changed = add_known_marketplace(&mut settings, "my-marketplace");
        assert!(changed);

        let ekm = settings.get("extraKnownMarketplaces");
        assert!(ekm.is_some());
        let entry = ekm.and_then(|e| e.get("my-marketplace"));
        assert!(entry.is_some());
        assert_eq!(
            entry
                .and_then(|e| e.get("source"))
                .and_then(|s| s.get("path"))
                .and_then(serde_json::Value::as_str),
            Some("./.ai")
        );
    }

    #[test]
    fn add_known_marketplace_skips_duplicate() {
        let mut settings = serde_json::json!({
            "extraKnownMarketplaces": {
                "existing": {"source": {"source": "directory", "path": "./.ai"}}
            }
        });
        let changed = add_known_marketplace(&mut settings, "existing");
        assert!(!changed);
    }

    #[test]
    fn add_known_marketplace_adds_alongside_existing() {
        let mut settings = serde_json::json!({
            "extraKnownMarketplaces": {
                "existing": {"source": {"source": "directory", "path": "./.ai"}}
            }
        });
        let changed = add_known_marketplace(&mut settings, "new-one");
        assert!(changed);

        let ekm = settings.get("extraKnownMarketplaces").and_then(serde_json::Value::as_object);
        assert!(ekm.is_some_and(|o| o.len() == 2));
    }

    #[test]
    fn add_known_marketplace_non_object_root_returns_false() {
        let mut settings = serde_json::json!([1, 2, 3]);
        let changed = add_known_marketplace(&mut settings, "test");
        assert!(!changed);
    }

    #[test]
    fn add_known_marketplace_non_object_ekm_returns_false() {
        let mut settings = serde_json::json!({"extraKnownMarketplaces": 42});
        let changed = add_known_marketplace(&mut settings, "test");
        assert!(!changed);
    }

    // --- enable_plugin tests ---

    #[test]
    fn enable_plugin_to_empty_object() {
        let mut settings = serde_json::json!({});
        let changed = enable_plugin(&mut settings, "my-plugin@marketplace");
        assert!(changed);

        let ep = settings.get("enabledPlugins");
        assert!(ep.is_some());
        assert_eq!(ep.and_then(|e| e.get("my-plugin@marketplace")), Some(&serde_json::json!(true)));
    }

    #[test]
    fn enable_plugin_skips_duplicate() {
        let mut settings = serde_json::json!({
            "enabledPlugins": {"existing@mp": true}
        });
        let changed = enable_plugin(&mut settings, "existing@mp");
        assert!(!changed);
    }

    #[test]
    fn enable_plugin_adds_alongside_existing() {
        let mut settings = serde_json::json!({
            "enabledPlugins": {"existing@mp": true}
        });
        let changed = enable_plugin(&mut settings, "new@mp");
        assert!(changed);

        let ep = settings.get("enabledPlugins").and_then(serde_json::Value::as_object);
        assert!(ep.is_some_and(|o| o.len() == 2));
    }

    #[test]
    fn enable_plugin_non_object_root_returns_false() {
        let mut settings = serde_json::json!("string");
        let changed = enable_plugin(&mut settings, "test");
        assert!(!changed);
    }

    #[test]
    fn enable_plugin_non_object_ep_returns_false() {
        let mut settings = serde_json::json!({"enabledPlugins": "not-an-object"});
        let changed = enable_plugin(&mut settings, "test");
        assert!(!changed);
    }

    // --- Integration: full RMW cycle ---

    #[test]
    fn full_rmw_cycle_with_missing_file() {
        let fs = MockFs::new();
        let path = Path::new("/settings.json");

        let mut settings = read_or_create(&fs, path).unwrap_or_default();
        let mp_changed = add_known_marketplace(&mut settings, "local");
        let ep_changed = enable_plugin(&mut settings, "starter@local");

        assert!(mp_changed);
        assert!(ep_changed);

        let result = write(&fs, path, &settings);
        assert!(result.is_ok());

        // Verify written content
        let content = fs.read_to_string(path).unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        assert!(v.get("extraKnownMarketplaces").and_then(|e| e.get("local")).is_some());
        assert_eq!(
            v.get("enabledPlugins").and_then(|e| e.get("starter@local")),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn full_rmw_cycle_preserves_existing_keys() {
        let fs = MockFs::new();
        let path = Path::new("/settings.json");
        fs.write_file(path, br#"{"permissions": {"allow": ["Read"]}}"#).unwrap_or_default();

        let mut settings = read_or_create(&fs, path).unwrap_or_default();
        add_known_marketplace(&mut settings, "local");
        write(&fs, path, &settings).unwrap_or_default();

        let content = fs.read_to_string(path).unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        // Preserved
        assert!(v.get("permissions").is_some());
        // Added
        assert!(v.get("extraKnownMarketplaces").and_then(|e| e.get("local")).is_some());
    }
}
