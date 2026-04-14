//! Unified `marketplace.json` read-modify-write operations.
//!
//! Replaces scattered marketplace handling in `workspace_init`
//! (initial creation) and `migrate/registrar` (append during migration).

use std::path::Path;

use crate::fs::Fs;

/// A plugin entry for `marketplace.json`.
pub struct Entry<'a> {
    /// Plugin name.
    pub name: &'a str,
    /// Human-readable description.
    pub description: &'a str,
}

/// Create a new `marketplace.json` string with the given name and initial plugins.
///
/// Pass an empty slice for `initial_plugins` to create a marketplace with no plugins.
pub fn create(marketplace_name: &str, initial_plugins: &[Entry<'_>]) -> String {
    let plugins: Vec<serde_json::Value> = initial_plugins
        .iter()
        .map(|e| {
            serde_json::json!({
                "name": e.name,
                "source": format!("./{}", e.name),
                "description": e.description
            })
        })
        .collect();

    let obj = serde_json::json!({
        "name": marketplace_name,
        "owner": { "name": "local" },
        "metadata": { "description": "Local plugins for this repository" },
        "plugins": plugins
    });

    let mut output = serde_json::to_string_pretty(&obj).unwrap_or_default();
    output.push('\n');
    output
}

/// Register a plugin in an existing `marketplace.json`, skipping duplicates.
///
/// Reads the file at `path`, parses the JSON, checks for an existing plugin
/// with the same name, appends if not found, and writes back.
///
/// # Errors
///
/// Returns `io::Error` if the file cannot be read, parsed, or written.
pub fn register(fs: &dyn Fs, path: &Path, entry: &Entry<'_>) -> std::io::Result<()> {
    let content = fs.read_to_string(path)?;
    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    let plugins =
        json.get_mut("plugins").and_then(serde_json::Value::as_array_mut).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("missing 'plugins' array in {}", path.display()),
            )
        })?;

    let already_registered = plugins
        .iter()
        .any(|p| p.get("name").and_then(serde_json::Value::as_str) == Some(entry.name));

    if !already_registered {
        plugins.push(serde_json::json!({
            "name": entry.name,
            "source": format!("./{}", entry.name),
            "description": entry.description
        }));

        let output = serde_json::to_string_pretty(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        fs.write_file(path, format!("{output}\n").as_bytes())?;
    }

    Ok(())
}

/// Register multiple plugins in an existing `marketplace.json`, skipping duplicates.
///
/// Reads the file once, appends all non-duplicate entries, and writes back once.
/// This is more efficient than calling [`register`] in a loop when adding
/// multiple plugins.
///
/// # Errors
///
/// Returns `io::Error` if the file cannot be read, parsed, or written.
pub fn register_all(fs: &dyn Fs, path: &Path, entries: &[Entry<'_>]) -> std::io::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }

    let content = fs.read_to_string(path)?;
    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    let plugins =
        json.get_mut("plugins").and_then(serde_json::Value::as_array_mut).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("missing 'plugins' array in {}", path.display()),
            )
        })?;

    let mut changed = false;
    for entry in entries {
        let already_registered = plugins
            .iter()
            .any(|p| p.get("name").and_then(serde_json::Value::as_str) == Some(entry.name));

        if !already_registered {
            plugins.push(serde_json::json!({
                "name": entry.name,
                "source": format!("./{}", entry.name),
                "description": entry.description
            }));
            changed = true;
        }
    }

    if changed {
        let output = serde_json::to_string_pretty(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        fs.write_file(path, format!("{output}\n").as_bytes())?;
    }

    Ok(())
}

/// Remove a plugin from an existing `marketplace.json` by name.
///
/// # Errors
///
/// Returns `io::Error` if the file cannot be read, parsed, or written.
pub fn unregister(fs: &dyn Fs, path: &Path, plugin_name: &str) -> std::io::Result<()> {
    let content = fs.read_to_string(path)?;
    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    let plugins =
        json.get_mut("plugins").and_then(serde_json::Value::as_array_mut).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("missing 'plugins' array in {}", path.display()),
            )
        })?;

    let original_len = plugins.len();
    plugins.retain(|p| p.get("name").and_then(serde_json::Value::as_str) != Some(plugin_name));

    if plugins.len() != original_len {
        let output = serde_json::to_string_pretty(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        fs.write_file(path, format!("{output}\n").as_bytes())?;
    }

    Ok(())
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
            self.files.lock().unwrap().contains_key(path)
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Ok(())
        }

        fn write_file(&self, path: &Path, content: &[u8]) -> std::io::Result<()> {
            self.files.lock().unwrap().insert(path.to_path_buf(), content.to_vec());
            Ok(())
        }

        fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
            self.files
                .lock()
                .unwrap()
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

    #[test]
    fn create_with_no_plugins() {
        let json = create("my-marketplace", &[]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(v.get("name").and_then(serde_json::Value::as_str), Some("my-marketplace"));
        let plugins = v.get("plugins").and_then(serde_json::Value::as_array);
        assert!(plugins.is_some_and(|p| p.is_empty()));
        assert!(json.ends_with('\n'));
    }

    #[test]
    fn create_with_plugins() {
        let entries = [Entry { name: "my-plugin", description: "A cool plugin" }];
        let json = create("local", &entries);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
        let plugins = v.get("plugins").and_then(serde_json::Value::as_array);
        assert!(plugins.is_some_and(|p| p.len() == 1));
        let first = plugins.and_then(|p| p.first());
        assert_eq!(
            first.and_then(|p| p.get("name")).and_then(serde_json::Value::as_str),
            Some("my-plugin")
        );
        assert_eq!(
            first.and_then(|p| p.get("source")).and_then(serde_json::Value::as_str),
            Some("./my-plugin")
        );
    }

    #[test]
    fn register_new_plugin() {
        let fs = MockFs::new();
        let path = std::path::Path::new("/marketplace.json");
        let initial = create("test", &[]);
        fs.write_file(path, initial.as_bytes()).unwrap_or_default();

        let entry = Entry { name: "new-plugin", description: "desc" };
        let result = register(&fs, path, &entry);
        assert!(result.is_ok());

        let content = fs.read_to_string(path).unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        let plugins = v.get("plugins").and_then(serde_json::Value::as_array);
        assert!(plugins.is_some_and(|p| p.len() == 1));
    }

    #[test]
    fn register_duplicate_is_skipped() {
        let fs = MockFs::new();
        let path = std::path::Path::new("/marketplace.json");
        let entries = [Entry { name: "existing", description: "d" }];
        let initial = create("test", &entries);
        fs.write_file(path, initial.as_bytes()).unwrap_or_default();

        let entry = Entry { name: "existing", description: "different desc" };
        let result = register(&fs, path, &entry);
        assert!(result.is_ok());

        let content = fs.read_to_string(path).unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        let plugins = v.get("plugins").and_then(serde_json::Value::as_array);
        assert!(plugins.is_some_and(|p| p.len() == 1));
    }

    #[test]
    fn unregister_existing_plugin() {
        let fs = MockFs::new();
        let path = std::path::Path::new("/marketplace.json");
        let entries =
            [Entry { name: "keep", description: "d" }, Entry { name: "remove", description: "d" }];
        let initial = create("test", &entries);
        fs.write_file(path, initial.as_bytes()).unwrap_or_default();

        let result = unregister(&fs, path, "remove");
        assert!(result.is_ok());

        let content = fs.read_to_string(path).unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        let plugins = v.get("plugins").and_then(serde_json::Value::as_array);
        assert!(plugins.is_some_and(|p| p.len() == 1));
        let remaining = plugins.and_then(|p| p.first());
        assert_eq!(
            remaining.and_then(|p| p.get("name")).and_then(serde_json::Value::as_str),
            Some("keep")
        );
    }

    #[test]
    fn register_all_adds_multiple_plugins() {
        let fs = MockFs::new();
        let path = std::path::Path::new("/marketplace.json");
        let initial = create("test", &[]);
        fs.write_file(path, initial.as_bytes()).unwrap_or_default();

        let entries = [
            Entry { name: "deploy", description: "Deploy plugin" },
            Entry { name: "lint", description: "Lint plugin" },
        ];
        let result = register_all(&fs, path, &entries);
        assert!(result.is_ok());

        let content = fs.read_to_string(path).unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        let plugins = v.get("plugins").and_then(serde_json::Value::as_array);
        assert!(plugins.is_some_and(|p| p.len() == 2));
    }

    #[test]
    fn register_all_skips_duplicates() {
        let fs = MockFs::new();
        let path = std::path::Path::new("/marketplace.json");
        let initial = create("test", &[Entry { name: "existing", description: "d" }]);
        fs.write_file(path, initial.as_bytes()).unwrap_or_default();

        let entries = [
            Entry { name: "existing", description: "duplicate" },
            Entry { name: "new-one", description: "new" },
        ];
        let result = register_all(&fs, path, &entries);
        assert!(result.is_ok());

        let content = fs.read_to_string(path).unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        let plugins = v.get("plugins").and_then(serde_json::Value::as_array);
        assert!(plugins.is_some_and(|p| p.len() == 2));
    }

    #[test]
    fn register_all_empty_entries_is_noop() {
        let fs = MockFs::new();
        let path = std::path::Path::new("/marketplace.json");
        let initial = create("test", &[Entry { name: "a", description: "d" }]);
        fs.write_file(path, initial.as_bytes()).unwrap_or_default();

        let before = fs.read_to_string(path).unwrap_or_default();
        let result = register_all(&fs, path, &[]);
        assert!(result.is_ok());
        let after = fs.read_to_string(path).unwrap_or_default();
        assert_eq!(before, after);
    }

    #[test]
    fn register_all_all_duplicates_no_write() {
        let fs = MockFs::new();
        let path = std::path::Path::new("/marketplace.json");
        let initial = create("test", &[Entry { name: "a", description: "d" }]);
        fs.write_file(path, initial.as_bytes()).unwrap_or_default();

        let before = fs.read_to_string(path).unwrap_or_default();
        let entries = [Entry { name: "a", description: "different" }];
        let result = register_all(&fs, path, &entries);
        assert!(result.is_ok());
        let after = fs.read_to_string(path).unwrap_or_default();
        assert_eq!(before, after);
    }

    #[test]
    fn register_all_returns_error_when_plugins_missing() {
        let fs = MockFs::new();
        let path = std::path::Path::new("/marketplace.json");
        fs.write_file(path, br#"{"name":"test"}"#).unwrap_or_default();

        let entries = [Entry { name: "a", description: "d" }];
        let result = register_all(&fs, path, &entries);
        assert!(result.is_err());
    }

    #[test]
    fn register_all_returns_error_when_file_not_found() {
        let fs = MockFs::new();
        let path = std::path::Path::new("/nonexistent.json");

        let entries = [Entry { name: "a", description: "d" }];
        let result = register_all(&fs, path, &entries);
        assert!(result.is_err());
    }

    #[test]
    fn unregister_nonexistent_is_noop() {
        let fs = MockFs::new();
        let path = std::path::Path::new("/marketplace.json");
        let initial = create("test", &[Entry { name: "a", description: "d" }]);
        fs.write_file(path, initial.as_bytes()).unwrap_or_default();

        let before = fs.read_to_string(path).unwrap_or_default();
        let result = unregister(&fs, path, "nonexistent");
        assert!(result.is_ok());
        let after = fs.read_to_string(path).unwrap_or_default();
        assert_eq!(before, after);
    }
}
