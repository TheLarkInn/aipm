//! Marketplace plugin acquisition.
//!
//! A marketplace is a git repository containing a TOML manifest that lists
//! available plugins and their locations.  Plugins can be sourced from
//! relative paths within the marketplace repository, external git repos,
//! or declared as unsupported (npm, pip, etc.).
//!
//! # Manifest format
//!
//! ```toml
//! [metadata]
//! plugin_root = "./plugins"   # optional base directory
//!
//! [[plugins]]
//! name = "hello-skills"
//! source = "plugins/hello-skills-v1"  # relative path
//!
//! [[plugins]]
//! name = "external-tool"
//! description = "An external plugin"
//! [plugins.source]
//! type = "git"
//! url = "https://github.com/org/repo.git"
//! path = "plugins/foo"
//! ref = "v2.0"
//! ```

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Marketplace manifest types
// ---------------------------------------------------------------------------

/// A parsed marketplace manifest.
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Available plugins.
    pub plugins: Vec<PluginEntry>,
    /// Optional metadata.
    pub metadata: Option<Metadata>,
}

/// Optional marketplace metadata.
#[derive(Debug, Clone)]
pub struct Metadata {
    /// Base directory prepended to relative plugin source paths.
    pub plugin_root: Option<String>,
}

/// A single plugin entry in the manifest.
#[derive(Debug, Clone)]
pub struct PluginEntry {
    /// Plugin name.
    pub name: String,
    /// Plugin source.
    pub source: PluginSource,
    /// Optional description.
    pub description: Option<String>,
}

/// Plugin source specification in a marketplace manifest.
#[derive(Debug, Clone)]
pub enum PluginSource {
    /// Relative path within the marketplace repository.
    RelativePath(String),
    /// External git repository.
    Git { url: String, path: Option<String>, git_ref: Option<String>, sha: Option<String> },
    /// Unsupported source type (npm, pip, etc.).
    Unsupported { source_type: String },
}

impl PluginSource {
    /// Returns the source type as a string for testing/display.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::RelativePath(_) => "relative",
            Self::Git { .. } => "git",
            Self::Unsupported { .. } => "unsupported",
        }
    }
}

impl Manifest {
    /// Parse a marketplace manifest from a TOML string.
    pub fn parse(content: &str) -> Result<Self, Error> {
        // Custom parsing: handle PluginSource manually since toml and serde_json
        // don't mix well.  Parse as toml::Value first, then convert.
        let raw: toml::Value =
            toml::from_str(content).map_err(|e| Error::ManifestParse(e.to_string()))?;

        let plugins_val =
            raw.get("plugins").and_then(|v| v.as_array()).cloned().unwrap_or_default();

        let mut plugins = Vec::new();
        for entry_val in &plugins_val {
            let name = entry_val
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    Error::ManifestParse("plugin entry missing 'name' field".to_string())
                })?
                .to_string();
            let description =
                entry_val.get("description").and_then(|v| v.as_str()).map(str::to_string);
            let source = parse_plugin_source(entry_val.get("source"))?;
            plugins.push(PluginEntry { name, source, description });
        }

        let metadata = raw.get("metadata").map(|m| {
            let plugin_root = m
                .get("plugin_root")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            Metadata { plugin_root }
        });

        Ok(Self { plugins, metadata })
    }

    /// Find a plugin by name (case-sensitive).
    pub fn find_plugin(&self, name: &str) -> Option<&PluginEntry> {
        self.plugins.iter().find(|p| p.name == name)
    }

    /// List available plugin names.
    pub fn available_names(&self) -> Vec<&str> {
        self.plugins.iter().map(|p| p.name.as_str()).collect()
    }

    /// Get the plugin root path from metadata (if set and non-empty).
    pub fn plugin_root(&self) -> Option<&str> {
        self.metadata.as_ref().and_then(|m| m.plugin_root.as_deref()).filter(|s| !s.is_empty())
    }
}

/// Parse a plugin source from a TOML value.
fn parse_plugin_source(value: Option<&toml::Value>) -> Result<PluginSource, Error> {
    let value = value
        .ok_or_else(|| Error::ManifestParse("plugin entry missing 'source' field".to_string()))?;

    match value {
        toml::Value::String(s) => Ok(PluginSource::RelativePath(s.clone())),
        toml::Value::Table(map) => {
            let source_type = map
                .get("type")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("unknown");

            let git_ref = map
                .get("ref")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let sha = map
                .get("sha")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let path = map
                .get("path")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            match source_type {
                "git" => {
                    let url = map
                        .get("url")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| {
                            Error::ManifestParse("git source missing 'url' field".to_string())
                        })?
                        .to_string();
                    Ok(PluginSource::Git { url, path, git_ref, sha })
                },
                other => Ok(PluginSource::Unsupported { source_type: other.to_string() }),
            }
        },
        _ => Err(Error::ManifestParse("plugin source must be a string or table".to_string())),
    }
}

// ---------------------------------------------------------------------------
// Marketplace spec parsing (for `market:name@location#ref`)
// ---------------------------------------------------------------------------

/// Resolve a plugin's source path with the manifest's `plugin_root`.
pub fn resolve_source_path(manifest: &Manifest, relative_path: &str) -> PathBuf {
    manifest.plugin_root().map_or_else(
        || PathBuf::from(relative_path),
        |root| {
            let root = root.trim_start_matches("./");
            PathBuf::from(root).join(relative_path)
        },
    )
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from marketplace operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to parse the marketplace manifest.
    #[error("Failed to parse marketplace manifest: {0}")]
    ManifestParse(String),

    /// Plugin not found in the marketplace.
    #[error("Plugin '{name}' not found in marketplace. Available: {available:?}")]
    PluginNotFound { name: String, available: Vec<String> },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a manifest, returning a default empty manifest on failure.
    fn must_parse(toml_str: &str) -> Manifest {
        match Manifest::parse(toml_str) {
            Ok(m) => m,
            Err(_) => Manifest { plugins: Vec::new(), metadata: None },
        }
    }

    const SAMPLE_MANIFEST: &str = r#"
[[plugins]]
name = "hello-skills"
source = "./plugins/hello-skills-v1"

[[plugins]]
name = "other-plugin"
source = "plugins/other"
"#;

    // ---- Manifest parsing ----

    #[test]
    fn parse_manifest() {
        let manifest = Manifest::parse(SAMPLE_MANIFEST);
        assert!(manifest.is_ok());
        let manifest = must_parse(SAMPLE_MANIFEST);
        assert_eq!(manifest.plugins.len(), 2);
    }

    #[test]
    fn find_plugin_by_name() {
        let manifest = must_parse(SAMPLE_MANIFEST);
        let plugin = manifest.find_plugin("hello-skills");
        assert!(plugin.is_some());
    }

    #[test]
    fn find_plugin_not_found() {
        let manifest = must_parse(SAMPLE_MANIFEST);
        assert!(manifest.find_plugin("nonexistent").is_none());
    }

    #[test]
    fn available_names() {
        let manifest = must_parse(SAMPLE_MANIFEST);
        let names = manifest.available_names();
        assert_eq!(names, vec!["hello-skills", "other-plugin"]);
    }

    #[test]
    fn parse_manifest_invalid_toml() {
        let result = Manifest::parse("not valid toml!!!");
        assert!(result.is_err());
    }

    #[test]
    fn parse_manifest_empty_plugins() {
        let manifest = must_parse("plugins = []");
        assert!(manifest.plugins.is_empty());
        assert!(manifest.available_names().is_empty());
    }

    // ---- Metadata ----

    #[test]
    fn parse_manifest_no_metadata() {
        let manifest = must_parse(SAMPLE_MANIFEST);
        assert!(manifest.plugin_root().is_none());
    }

    #[test]
    fn parse_manifest_with_plugin_root() {
        let toml = r#"
[[plugins]]
name = "fmt"
source = "formatter"

[metadata]
plugin_root = "./plugins"
"#;
        let manifest = must_parse(toml);
        assert_eq!(manifest.plugin_root(), Some("./plugins"));
    }

    #[test]
    fn parse_manifest_with_empty_plugin_root() {
        let toml = r#"
[[plugins]]
name = "fmt"
source = "formatter"

[metadata]
plugin_root = ""
"#;
        let manifest = must_parse(toml);
        assert!(manifest.plugin_root().is_none());
    }

    // ---- Source types ----

    #[test]
    fn parse_source_string() {
        let toml = "[[plugins]]\nname = \"a\"\nsource = \"./my-plugin\"\n";
        let manifest = must_parse(toml);
        assert_eq!(manifest.plugins.len(), 1);
        assert_eq!(manifest.plugins.first().map(|e| e.source.kind()), Some("relative"));
    }

    #[test]
    fn parse_source_git_object() {
        let toml = "[[plugins]]\nname = \"a\"\n[plugins.source]\ntype = \"git\"\nurl = \"https://github.com/org/repo.git\"\n";
        let manifest = must_parse(toml);
        assert_eq!(manifest.plugins.len(), 1);
        assert_eq!(manifest.plugins.first().map(|e| e.source.kind()), Some("git"));
    }

    #[test]
    fn parse_source_git_with_path() {
        let toml = "[[plugins]]\nname = \"a\"\n[plugins.source]\ntype = \"git\"\nurl = \"https://github.com/org/repo.git\"\npath = \"plugins/foo\"\n";
        let manifest = must_parse(toml);
        assert_eq!(manifest.plugins.len(), 1);
        assert_eq!(manifest.plugins.first().map(|e| e.source.kind()), Some("git"));
    }

    #[test]
    fn parse_source_git_with_ref_and_sha() {
        let toml = "[[plugins]]\nname = \"a\"\n[plugins.source]\ntype = \"git\"\nurl = \"https://github.com/org/repo.git\"\nref = \"v2.0\"\nsha = \"abc123\"\n";
        let manifest = must_parse(toml);
        assert_eq!(manifest.plugins.len(), 1);
        assert_eq!(manifest.plugins.first().map(|e| e.source.kind()), Some("git"));
    }

    #[test]
    fn parse_source_unsupported_type() {
        let toml =
            "[[plugins]]\nname = \"a\"\n[plugins.source]\ntype = \"npm\"\npackage = \"foo\"\n";
        let manifest = must_parse(toml);
        assert_eq!(manifest.plugins.len(), 1);
        assert_eq!(manifest.plugins.first().map(|e| e.source.kind()), Some("unsupported"));
    }

    #[test]
    fn parse_source_missing_type_defaults_to_unknown() {
        let toml = "[[plugins]]\nname = \"a\"\n[plugins.source]\nurl = \"https://example.com\"\n";
        let manifest = must_parse(toml);
        assert_eq!(manifest.plugins.len(), 1);
        assert_eq!(manifest.plugins.first().map(|e| e.source.kind()), Some("unsupported"));
    }

    #[test]
    fn parse_source_git_missing_url() {
        let toml = "[[plugins]]\nname = \"a\"\n[plugins.source]\ntype = \"git\"\n";
        let result = Manifest::parse(toml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_source_empty_ref_treated_as_none() {
        let toml = "[[plugins]]\nname = \"a\"\n[plugins.source]\ntype = \"git\"\nurl = \"https://example.com/repo.git\"\nref = \"\"\n";
        let manifest = must_parse(toml);
        assert_eq!(manifest.plugins.len(), 1);
        assert_eq!(manifest.plugins.first().map(|e| e.source.kind()), Some("git"));
    }

    #[test]
    fn parse_mixed_source_types() {
        let toml = "[[plugins]]\nname = \"local\"\nsource = \"./plugins/local\"\n\n[[plugins]]\nname = \"git\"\n[plugins.source]\ntype = \"git\"\nurl = \"https://github.com/org/repo.git\"\n\n[[plugins]]\nname = \"unsupported\"\n[plugins.source]\ntype = \"pip\"\npackage = \"foo\"\n";
        let manifest = must_parse(toml);
        assert_eq!(manifest.plugins.len(), 3);
        assert_eq!(manifest.plugins.get(0).map(|p| p.source.kind()), Some("relative"));
        assert_eq!(manifest.plugins.get(1).map(|p| p.source.kind()), Some("git"));
        assert_eq!(manifest.plugins.get(2).map(|p| p.source.kind()), Some("unsupported"));
    }

    #[test]
    fn parse_source_with_description() {
        let toml = r#"
[[plugins]]
name = "my-tool"
source = "plugins/tool"
description = "A useful tool"
"#;
        let manifest = must_parse(toml);
        assert_eq!(
            manifest.plugins.first().and_then(|p| p.description.as_deref()),
            Some("A useful tool")
        );
    }

    // ---- Plugin root resolution ----

    #[test]
    fn resolve_source_path_with_root() {
        let toml = r#"
[[plugins]]
name = "fmt"
source = "formatter"

[metadata]
plugin_root = "./plugins"
"#;
        let manifest = must_parse(toml);
        let resolved = resolve_source_path(&manifest, "formatter");
        assert_eq!(resolved, PathBuf::from("plugins/formatter"));
    }

    #[test]
    fn resolve_source_path_without_root() {
        let manifest = must_parse(SAMPLE_MANIFEST);
        let resolved = resolve_source_path(&manifest, "plugins/hello-skills-v1");
        assert_eq!(resolved, PathBuf::from("plugins/hello-skills-v1"));
    }

    // ---- Source validation (no path traversal) ----

    #[test]
    fn parse_source_number_invalid() {
        let toml = r#"
[[plugins]]
name = "a"
source = 123
"#;
        let result = Manifest::parse(toml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_manifest_missing_name() {
        let toml = r#"
[[plugins]]
source = "./some-path"
"#;
        let result = Manifest::parse(toml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_manifest_missing_source() {
        let toml = r#"
[[plugins]]
name = "test"
"#;
        let result = Manifest::parse(toml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_empty_url_in_git_source_errors() {
        let toml = r#"
[[plugins]]
name = "a"
[plugins.source]
type = "git"
url = ""
"#;
        let result = Manifest::parse(toml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_empty_sha_treated_as_none() {
        let toml = r#"
[[plugins]]
name = "a"
[plugins.source]
type = "git"
url = "https://example.com/repo.git"
sha = ""
"#;
        let manifest = must_parse(toml);
        assert_eq!(manifest.plugins.len(), 1);
        assert_eq!(manifest.plugins.first().map(|e| e.source.kind()), Some("git"));
    }

    #[test]
    fn parse_empty_path_treated_as_none() {
        let toml = "[[plugins]]\nname = \"a\"\n[plugins.source]\ntype = \"git\"\nurl = \"https://example.com/repo.git\"\npath = \"\"\n";
        let manifest = must_parse(toml);
        assert_eq!(manifest.plugins.len(), 1);
        assert_eq!(manifest.plugins.first().map(|e| e.source.kind()), Some("git"));
    }

    // ---- Multiple plugins and descriptions ----

    #[test]
    fn parse_many_plugins() {
        let toml = r#"
[[plugins]]
name = "a"
source = "plugins/a"

[[plugins]]
name = "b"
source = "plugins/b"

[[plugins]]
name = "c"
source = "plugins/c"
"#;
        let manifest = must_parse(toml);
        assert_eq!(manifest.plugins.len(), 3);
    }

    #[test]
    fn available_names_returns_all() {
        let toml = r#"
[[plugins]]
name = "alpha"
source = "a"

[[plugins]]
name = "beta"
source = "b"
"#;
        let manifest = must_parse(toml);
        assert_eq!(manifest.available_names(), vec!["alpha", "beta"]);
    }

    #[test]
    fn find_plugin_case_sensitive() {
        let toml = r#"
[[plugins]]
name = "Hello"
source = "hello"
"#;
        let manifest = must_parse(toml);
        assert!(manifest.find_plugin("Hello").is_some());
        assert!(manifest.find_plugin("hello").is_none());
    }
}
