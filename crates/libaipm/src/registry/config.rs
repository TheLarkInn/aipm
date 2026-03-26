//! Registry configuration parsing.
//!
//! Handles parsing registry configuration from `aipm.toml` or
//! `~/.aipm/config.toml`, including named registries and scope routing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Registry configuration section.
///
/// Parsed from `[registries]` in manifest or global config.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    /// Named registries (e.g. `default`, `internal`).
    #[serde(default)]
    pub registries: BTreeMap<String, RegistryEntry>,

    /// Scope-to-registry routing (e.g. `"@company" = "internal"`).
    #[serde(default)]
    pub scopes: BTreeMap<String, String>,
}

/// A single named registry entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegistryEntry {
    /// The git clone URL for the index repository.
    pub index: String,
}

/// Index `config.json` file at the root of a git registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexMeta {
    /// URL template for tarball downloads.
    /// `{name}` and `{version}` are substituted at download time.
    pub dl: String,

    /// API base URL for HTTP registries (`null` for git-only).
    #[serde(default)]
    pub api: Option<String>,
}

impl Config {
    /// Look up which registry name a package should use.
    ///
    /// Scoped packages (`@scope/name`) are routed via `[registries.scopes]`.
    /// All other packages use the `"default"` registry.
    pub fn registry_for_package(&self, package_name: &str) -> &str {
        if let Some(scope) = extract_scope(package_name) {
            if let Some(registry_name) = self.scopes.get(scope) {
                return registry_name;
            }
        }
        "default"
    }

    /// Get the index URL for a named registry.
    ///
    /// Returns `None` if the registry name is not configured.
    pub fn get_index_url(&self, registry_name: &str) -> Option<&str> {
        self.registries.get(registry_name).map(|e| e.index.as_str())
    }
}

/// Extract the scope from a scoped package name (e.g. `@company/tool` → `@company`).
///
/// Returns `None` for non-scoped packages.
fn extract_scope(name: &str) -> Option<&str> {
    if !name.starts_with('@') {
        return None;
    }
    name.find('/').map(|idx| &name[..idx])
}

/// Placeholder for the package name in the download URL template.
const NAME_PLACEHOLDER: &str = "{name}";

/// Placeholder for the version in the download URL template.
const VERSION_PLACEHOLDER: &str = "{version}";

impl IndexMeta {
    /// Resolve the download URL for a specific package and version.
    pub fn download_url(&self, name: &str, version: &str) -> String {
        self.dl.replace(NAME_PLACEHOLDER, name).replace(VERSION_PLACEHOLDER, version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_registry_config_toml() {
        let toml_str = r#"
[registries.default]
index = "https://github.com/org/aipm-registry.git"

[registries.internal]
index = "https://github.com/mycompany/aipm-internal.git"

[scopes]
"@mycompany" = "internal"
"@team" = "internal"
"#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.registries.len(), 2);
        assert_eq!(
            config.registries.get("default").map(|e| e.index.as_str()),
            Some("https://github.com/org/aipm-registry.git")
        );
        assert_eq!(config.scopes.len(), 2);
        assert_eq!(config.scopes.get("@mycompany").map(String::as_str), Some("internal"));
    }

    #[test]
    fn registry_for_scoped_package() {
        let config = Config {
            registries: BTreeMap::new(),
            scopes: {
                let mut m = BTreeMap::new();
                m.insert("@mycompany".to_string(), "internal".to_string());
                m
            },
        };

        assert_eq!(config.registry_for_package("@mycompany/tool"), "internal");
    }

    #[test]
    fn registry_for_unscoped_package() {
        let config = Config::default();
        assert_eq!(config.registry_for_package("some-tool"), "default");
    }

    #[test]
    fn registry_for_unknown_scope_uses_default() {
        let config = Config {
            registries: BTreeMap::new(),
            scopes: {
                let mut m = BTreeMap::new();
                m.insert("@known".to_string(), "internal".to_string());
                m
            },
        };

        assert_eq!(config.registry_for_package("@unknown/tool"), "default");
    }

    #[test]
    fn get_index_url() {
        let config = Config {
            registries: {
                let mut m = BTreeMap::new();
                m.insert(
                    "default".to_string(),
                    RegistryEntry { index: "https://github.com/org/registry.git".to_string() },
                );
                m
            },
            scopes: BTreeMap::new(),
        };

        assert_eq!(config.get_index_url("default"), Some("https://github.com/org/registry.git"));
        assert!(config.get_index_url("nonexistent").is_none());
    }

    #[test]
    fn extract_scope_scoped() {
        assert_eq!(extract_scope("@company/tool"), Some("@company"));
    }

    #[test]
    fn extract_scope_unscoped() {
        assert_eq!(extract_scope("tool"), None);
    }

    #[test]
    fn extract_scope_no_slash() {
        assert_eq!(extract_scope("@company"), None);
    }

    #[test]
    fn index_config_parse() {
        let json = r#"{
            "dl": "https://github.com/{org}/aipm-registry/releases/download/{name}-{version}/{name}-{version}.aipm",
            "api": null
        }"#;

        let config: IndexMeta = serde_json::from_str(json).unwrap();
        assert!(config.dl.contains("{name}"));
        assert!(config.api.is_none());
    }

    #[test]
    fn index_config_download_url() {
        let config = IndexMeta {
            dl: "https://example.com/releases/{name}-{version}/{name}-{version}.aipm".to_string(),
            api: None,
        };

        let url = config.download_url("code-review", "1.2.0");
        assert_eq!(url, "https://example.com/releases/code-review-1.2.0/code-review-1.2.0.aipm");
    }

    #[test]
    fn empty_config_defaults() {
        let config = Config::default();
        assert!(config.registries.is_empty());
        assert!(config.scopes.is_empty());
    }
}
