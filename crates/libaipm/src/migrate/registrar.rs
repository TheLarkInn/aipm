//! Registrar: appends migrated plugins to `marketplace.json`.

use std::path::Path;

use crate::fs::Fs;

use super::Error;

/// Append migrated plugins to `marketplace.json` without modifying existing entries.
pub fn register_plugins(ai_dir: &Path, plugin_names: &[String], fs: &dyn Fs) -> Result<(), Error> {
    if plugin_names.is_empty() {
        return Ok(());
    }

    let marketplace_path = ai_dir.join(".claude-plugin").join("marketplace.json");
    let content = fs.read_to_string(&marketplace_path)?;
    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| Error::MarketplaceJsonParse { path: marketplace_path.clone(), source: e })?;

    let plugins =
        json.get_mut("plugins").and_then(serde_json::Value::as_array_mut).ok_or_else(|| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("missing 'plugins' array in {}", marketplace_path.display()),
            ))
        })?;

    for name in plugin_names {
        let already_registered = plugins
            .iter()
            .any(|p| p.get("name").and_then(serde_json::Value::as_str) == Some(name.as_str()));
        if already_registered {
            continue;
        }

        plugins.push(serde_json::json!({
            "name": name,
            "source": format!("./{name}"),
            "description": "Migrated from .claude/ configuration"
        }));
    }

    let output = serde_json::to_string_pretty(&json)
        .map_err(|e| Error::MarketplaceJsonParse { path: marketplace_path.clone(), source: e })?;
    fs.write_file(&marketplace_path, format!("{output}\n").as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;

    struct MockFs {
        exists: HashSet<PathBuf>,
        files: RefCell<HashMap<PathBuf, String>>,
        written: RefCell<HashMap<PathBuf, Vec<u8>>>,
    }

    impl MockFs {
        fn new() -> Self {
            Self {
                exists: HashSet::new(),
                files: RefCell::new(HashMap::new()),
                written: RefCell::new(HashMap::new()),
            }
        }

        fn set_file(&self, path: PathBuf, content: String) {
            self.files.borrow_mut().insert(path, content);
        }

        fn get_written(&self, path: &Path) -> Option<String> {
            self.written.borrow().get(path).and_then(|b| String::from_utf8(b.clone()).ok())
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
            self.written.borrow_mut().insert(path.to_path_buf(), content.to_vec());
            Ok(())
        }

        fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
            self.files.borrow().get(path).cloned().ok_or_else(|| {
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

    #[test]
    fn register_appends_to_empty_plugins_array() {
        let fs = MockFs::new();
        fs.set_file(marketplace_path(), r#"{"name":"test-marketplace","plugins":[]}"#.to_string());

        let names = vec!["deploy".to_string(), "lint".to_string()];
        let result = register_plugins(Path::new("/ai"), &names, &fs);
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

        let names = vec!["deploy".to_string()];
        let result = register_plugins(Path::new("/ai"), &names, &fs);
        assert!(result.is_ok());

        let written = fs.get_written(&marketplace_path());
        assert!(written.as_ref().is_some_and(|c| c.contains("starter-aipm-plugin")));
        assert!(written.as_ref().is_some_and(|c| c.contains("deploy")));
    }

    #[test]
    fn register_skips_already_registered() {
        let fs = MockFs::new();
        fs.set_file(
            marketplace_path(),
            r#"{"plugins":[{"name":"deploy","source":"./deploy"}]}"#.to_string(),
        );

        let names = vec!["deploy".to_string()];
        let result = register_plugins(Path::new("/ai"), &names, &fs);
        assert!(result.is_ok());

        let written = fs.get_written(&marketplace_path());
        if let Some(content) = written {
            // Count occurrences of "deploy" as a name value — should be exactly 1
            let count = content.matches("\"deploy\"").count();
            // name field + source field = at least 2 occurrences of "deploy" string,
            // but the name key should appear exactly once
            let parsed: serde_json::Value = serde_json::from_str(&content).ok().unwrap_or_default();
            let plugins =
                parsed.get("plugins").and_then(|v| v.as_array()).map(Vec::len).unwrap_or(0);
            assert_eq!(plugins, 1, "should not duplicate: found {count} 'deploy' strings");
        }
    }

    #[test]
    fn register_preserves_marketplace_metadata() {
        let fs = MockFs::new();
        fs.set_file(
            marketplace_path(),
            r#"{"name":"my-marketplace","owner":"team","metadata":{"version":"1"},"plugins":[]}"#
                .to_string(),
        );

        let names = vec!["deploy".to_string()];
        let result = register_plugins(Path::new("/ai"), &names, &fs);
        assert!(result.is_ok());

        let written = fs.get_written(&marketplace_path());
        assert!(written.as_ref().is_some_and(|c| c.contains("my-marketplace")));
        assert!(written.as_ref().is_some_and(|c| c.contains("team")));
        assert!(written.as_ref().is_some_and(|c| c.contains("version")));
    }
}
