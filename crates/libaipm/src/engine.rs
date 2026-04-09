//! Plugin engine validation.
//!
//! Different engines (Claude, Copilot) have different requirements for what
//! constitutes a valid plugin.  This module provides two-tier validation:
//!
//! 1. **Primary**: Check for `aipm.toml` at the plugin root — if present, read
//!    the `engines` field and compare against the target engine.
//! 2. **Fallback**: If no `aipm.toml`, check for engine-specific marker files.

use std::path::{Path, PathBuf};

/// The engine that will run plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Engine {
    /// Claude Code plugins require `.claude-plugin/plugin.json`.
    #[default]
    Claude,
    /// Copilot plugins require at least one of: `plugin.json`,
    /// `.github/plugin/plugin.json`, or `.claude-plugin/plugin.json`.
    Copilot,
}

impl Engine {
    /// Marker file paths that indicate a valid plugin for this engine.
    pub const fn marker_paths(&self) -> &'static [&'static str] {
        match self {
            Self::Claude => &[".claude-plugin/plugin.json"],
            Self::Copilot => {
                &["plugin.json", ".github/plugin/plugin.json", ".claude-plugin/plugin.json"]
            },
        }
    }

    /// Marketplace manifest path for this engine.
    pub const fn marketplace_manifest_path(&self) -> &'static str {
        match self {
            Self::Claude => ".claude-plugin/marketplace.toml",
            Self::Copilot => ".github/plugin/marketplace.toml",
        }
    }

    /// Human-readable engine name.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Copilot => "Copilot",
        }
    }

    /// All supported engine names (lowercase).
    pub const fn all_names() -> &'static [&'static str] {
        &["claude", "copilot"]
    }

    /// Format the marker requirement for error messages.
    fn format_marker_requirement(self) -> String {
        let markers = self.marker_paths();
        if markers.len() == 1 {
            format!("missing {}", markers.first().copied().unwrap_or(""))
        } else {
            let names: Vec<&str> = markers.to_vec();
            format!("expected at least one of: {}", names.join(", "))
        }
    }
}

impl std::fmt::Display for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Error returned when plugin validation fails.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    /// The plugin does not have `aipm.toml` and is missing all engine markers.
    #[error("Not a valid {engine} plugin ({requirement}): {}", path.display())]
    InvalidPlugin { engine: String, requirement: String, path: PathBuf },

    /// The plugin's `aipm.toml` declares engines that don't include the target.
    #[error("Plugin at {} declares engines {declared:?} which does not include {target}", path.display())]
    IncompatibleEngine { target: String, declared: Vec<String>, path: PathBuf },
}

/// Validate a local plugin directory for the given engine.
///
/// **Two-tier strategy:**
/// 1. If `aipm.toml` exists at the plugin root, read the `engines` field.
///    If present and non-empty, the target engine must be in the list.
///    If absent or empty, the plugin is universal (passes).
/// 2. If no `aipm.toml`, fall back to checking engine-specific marker files.
///    At least one marker must exist.
pub fn validate_plugin(plugin_dir: &Path, engine: Engine) -> Result<(), ValidationError> {
    let manifest_path = plugin_dir.join("aipm.toml");

    if manifest_path.exists() {
        return validate_via_manifest(&manifest_path, plugin_dir, engine);
    }

    // Fallback: check engine marker files
    validate_via_markers(plugin_dir, engine)
}

/// Minimal manifest structs for engine validation.
/// Defined at module level to avoid "items after statements" clippy lint.
#[derive(serde::Deserialize, Default)]
struct MinimalPackage {
    #[serde(default)]
    engines: Option<Vec<String>>,
}

#[derive(serde::Deserialize, Default)]
struct MinimalManifest {
    #[serde(default)]
    package: Option<MinimalPackage>,
}

/// Validate using the `engines` field from `aipm.toml`.
fn validate_via_manifest(
    manifest_path: &Path,
    plugin_dir: &Path,
    engine: Engine,
) -> Result<(), ValidationError> {
    let Ok(content) = std::fs::read_to_string(manifest_path) else {
        return Ok(()); // Cannot read → treat as universal
    };

    let Ok(manifest) = toml::from_str::<MinimalManifest>(&content) else {
        return Ok(()); // Parse error → treat as universal
    };

    let engines = manifest.package.and_then(|p| p.engines).unwrap_or_default();

    // Empty engines list = universal (all engines)
    if engines.is_empty() {
        return Ok(());
    }

    // Check if target engine is in the list (case-insensitive)
    let target_lower = engine.name().to_lowercase();
    let matches = engines.iter().any(|e| e.to_lowercase() == target_lower);

    if matches {
        Ok(())
    } else {
        Err(ValidationError::IncompatibleEngine {
            target: engine.name().to_string(),
            declared: engines,
            path: plugin_dir.to_path_buf(),
        })
    }
}

/// Validate by checking for engine-specific marker files.
fn validate_via_markers(plugin_dir: &Path, engine: Engine) -> Result<(), ValidationError> {
    let markers = engine.marker_paths();
    let found = markers.iter().any(|m| plugin_dir.join(m).exists());

    if found {
        Ok(())
    } else {
        Err(ValidationError::InvalidPlugin {
            engine: engine.name().to_string(),
            requirement: engine.format_marker_requirement(),
            path: plugin_dir.to_path_buf(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap_or_else(|_| std::process::abort())
    }

    #[test]
    fn validate_with_aipm_toml_matching_engine() {
        let temp = make_temp();
        let plugin_dir = temp.path().join("my-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap_or_else(|_| {});
        std::fs::write(
            plugin_dir.join("aipm.toml"),
            "[package]\nname = \"test\"\nversion = \"1.0.0\"\nengines = [\"claude\"]\n",
        )
        .unwrap_or_else(|_| {});

        assert!(validate_plugin(&plugin_dir, Engine::Claude).is_ok());
    }

    #[test]
    fn validate_with_aipm_toml_engines_omitted_is_universal() {
        let temp = make_temp();
        let plugin_dir = temp.path().join("my-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap_or_else(|_| {});
        std::fs::write(
            plugin_dir.join("aipm.toml"),
            "[package]\nname = \"test\"\nversion = \"1.0.0\"\n",
        )
        .unwrap_or_else(|_| {});

        assert!(validate_plugin(&plugin_dir, Engine::Claude).is_ok());
        assert!(validate_plugin(&plugin_dir, Engine::Copilot).is_ok());
    }

    #[test]
    fn validate_with_aipm_toml_engine_not_in_list() {
        let temp = make_temp();
        let plugin_dir = temp.path().join("my-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap_or_else(|_| {});
        std::fs::write(
            plugin_dir.join("aipm.toml"),
            "[package]\nname = \"test\"\nversion = \"1.0.0\"\nengines = [\"copilot\"]\n",
        )
        .unwrap_or_else(|_| {});

        assert!(validate_plugin(&plugin_dir, Engine::Claude).is_err());
    }

    #[test]
    fn validate_fallback_no_aipm_toml_valid_markers_claude() {
        let temp = make_temp();
        let plugin_dir = temp.path().join("my-plugin");
        std::fs::create_dir_all(plugin_dir.join(".claude-plugin")).unwrap_or_else(|_| {});
        std::fs::write(plugin_dir.join(".claude-plugin/plugin.json"), "{}").unwrap_or_else(|_| {});

        assert!(validate_plugin(&plugin_dir, Engine::Claude).is_ok());
    }

    #[test]
    fn validate_fallback_no_aipm_toml_valid_markers_copilot() {
        let temp = make_temp();
        let plugin_dir = temp.path().join("my-plugin");
        std::fs::create_dir_all(plugin_dir.join(".github/plugin")).unwrap_or_else(|_| {});
        std::fs::write(plugin_dir.join(".github/plugin/plugin.json"), "{}").unwrap_or_else(|_| {});

        assert!(validate_plugin(&plugin_dir, Engine::Copilot).is_ok());
    }

    #[test]
    fn validate_fallback_no_aipm_toml_missing_all_markers_claude() {
        let temp = make_temp();
        let plugin_dir = temp.path().join("my-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap_or_else(|_| {});

        assert!(validate_plugin(&plugin_dir, Engine::Claude).is_err());
    }

    #[test]
    fn validate_fallback_no_aipm_toml_missing_all_markers_copilot() {
        let temp = make_temp();
        let plugin_dir = temp.path().join("my-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap_or_else(|_| {});

        assert!(validate_plugin(&plugin_dir, Engine::Copilot).is_err());
    }

    #[test]
    fn human_readable_error_single_marker() {
        let req = Engine::Claude.format_marker_requirement();
        assert!(req.contains("missing"));
        assert!(req.contains(".claude-plugin/plugin.json"));
    }

    #[test]
    fn human_readable_error_multi_marker() {
        let req = Engine::Copilot.format_marker_requirement();
        assert!(req.contains("expected at least one of"));
    }

    #[test]
    fn engine_display() {
        assert_eq!(Engine::Claude.to_string(), "Claude");
        assert_eq!(Engine::Copilot.to_string(), "Copilot");
    }

    #[test]
    fn engine_all_names() {
        let names = Engine::all_names();
        assert!(names.contains(&"claude"));
        assert!(names.contains(&"copilot"));
    }

    #[test]
    fn validate_unreadable_aipm_toml_treated_as_universal() {
        let temp = make_temp();
        let plugin_dir = temp.path().join("unreadable-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap_or_else(|_| {});
        // Create aipm.toml as a directory (unreadable as file)
        std::fs::create_dir_all(plugin_dir.join("aipm.toml")).unwrap_or_else(|_| {});
        // Fallback: needs marker files since aipm.toml read will fail
        std::fs::create_dir_all(plugin_dir.join(".claude-plugin")).unwrap_or_else(|_| {});
        std::fs::write(plugin_dir.join(".claude-plugin/plugin.json"), "{}").unwrap_or_else(|_| {});

        // Should succeed — unreadable manifest is treated as universal
        assert!(validate_plugin(&plugin_dir, Engine::Claude).is_ok());
    }

    #[test]
    fn validate_malformed_aipm_toml_treated_as_universal() {
        let temp = make_temp();
        let plugin_dir = temp.path().join("bad-toml-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap_or_else(|_| {});
        std::fs::write(plugin_dir.join("aipm.toml"), "not valid toml {{{{").unwrap_or_else(|_| {});
        // Needs marker files since parse will fail
        std::fs::create_dir_all(plugin_dir.join(".claude-plugin")).unwrap_or_else(|_| {});
        std::fs::write(plugin_dir.join(".claude-plugin/plugin.json"), "{}").unwrap_or_else(|_| {});

        assert!(validate_plugin(&plugin_dir, Engine::Claude).is_ok());
    }

    #[test]
    fn marketplace_manifest_path_returns_correct_path() {
        assert_eq!(Engine::Claude.marketplace_manifest_path(), ".claude-plugin/marketplace.toml");
        assert_eq!(Engine::Copilot.marketplace_manifest_path(), ".github/plugin/marketplace.toml");
    }
}
