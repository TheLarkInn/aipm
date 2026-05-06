//! Plugin engine validation.
//!
//! Different engines (Claude, Copilot) have different requirements for what
//! constitutes a valid plugin.  This module provides two-tier validation:
//!
//! 1. **Primary**: Check for `aipm.toml` at the plugin root — if present, read
//!    the `engines` field and compare against the target engine.
//! 2. **Fallback**: If no `aipm.toml`, check for engine-specific marker files.

use std::path::{Path, PathBuf};

pub use libaipm_engine_spec::Engine;

/// Marker file paths that indicate a valid plugin for this engine.
///
/// Looked up in the schema-driven [`libaipm_engine_spec::ENGINES`] table.
/// Returns an empty slice if the engine is unexpectedly absent from the
/// table (which should not happen for any built-in variant).
#[must_use]
pub fn marker_paths(engine: Engine) -> &'static [&'static str] {
    libaipm_engine_spec::ENGINES
        .iter()
        .find(|(e, _)| *e == engine)
        .map_or(&[][..], |(_, spec)| spec.marker_paths)
}

/// Marketplace manifest path for this engine.
///
/// Looked up in [`libaipm_engine_spec::ENGINES`]. Returns the empty string
/// if the engine is unexpectedly absent from the table.
#[must_use]
pub fn marketplace_manifest_path(engine: Engine) -> &'static str {
    libaipm_engine_spec::ENGINES
        .iter()
        .find(|(e, _)| *e == engine)
        .map_or("", |(_, spec)| spec.marketplace_manifest_path)
}

/// Human-readable engine display name (preserves legacy capitalization for
/// error messages — distinct from the kebab-case [`Engine::name`]).
#[must_use]
pub const fn display_name(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => "Claude",
        Engine::Copilot => "Copilot",
    }
}

/// All supported engine names (kebab-case identifiers, e.g. "claude",
/// "copilot").
#[must_use]
pub fn all_names() -> Vec<&'static str> {
    Engine::ALL.iter().map(|e| e.name()).collect()
}

/// Format the marker requirement for error messages.
#[must_use]
pub fn format_marker_requirement(engine: Engine) -> String {
    format_marker_string(marker_paths(engine))
}

fn format_marker_string(markers: &[&str]) -> String {
    if markers.len() == 1 {
        format!("missing {}", markers.first().copied().unwrap_or(""))
    } else {
        format!("expected at least one of: {}", markers.join(", "))
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

/// Validate using the `engines` field from `aipm.toml`.
///
/// Uses the canonical [`crate::manifest::parse`] + [`crate::manifest::effective_engines`]
/// pipeline so workspace-level engines are honored as a fallback when the
/// member package omits its own declaration. Preserves the legacy
/// "broken manifest is treated as universal" tolerance: I/O errors and
/// parse errors short-circuit to `Ok(())`.
fn validate_via_manifest(
    manifest_path: &Path,
    plugin_dir: &Path,
    engine: Engine,
) -> Result<(), ValidationError> {
    let Ok(content) = std::fs::read_to_string(manifest_path) else {
        return Ok(()); // Cannot read → treat as universal
    };

    let Ok(manifest) = crate::manifest::parse(&content) else {
        return Ok(()); // Parse error → treat as universal
    };

    let engines =
        crate::manifest::effective_engines(manifest.package.as_ref(), manifest.workspace.as_ref());

    // None or empty bitset = universal (all engines).
    let Some(engines) = engines.filter(|s| !s.is_empty()) else {
        return Ok(());
    };

    if engines.contains(engine.as_set()) {
        Ok(())
    } else {
        // Reconstruct a Vec<String> of declared engines from the bitset
        // for the error message. Only known engines (those that survived
        // deserialization) appear here — silently-dropped unknowns do
        // not.
        let declared: Vec<String> = Engine::ALL
            .iter()
            .filter(|e| engines.contains(e.as_set()))
            .map(|e| e.name().to_string())
            .collect();
        Err(ValidationError::IncompatibleEngine {
            target: display_name(engine).to_string(),
            declared,
            path: plugin_dir.to_path_buf(),
        })
    }
}

/// Validate by checking for engine-specific marker files.
fn validate_via_markers(plugin_dir: &Path, engine: Engine) -> Result<(), ValidationError> {
    let markers = marker_paths(engine);
    let found = markers.iter().any(|m| plugin_dir.join(m).exists());

    if found {
        Ok(())
    } else {
        Err(ValidationError::InvalidPlugin {
            engine: display_name(engine).to_string(),
            requirement: format_marker_requirement(engine),
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
    fn validate_inherits_engines_from_workspace_when_package_omits() {
        // Spec G7 part 2: package omits engines, workspace declares
        // ["claude"]. Validating against Claude must pass; validating
        // against Copilot must fail (workspace-level restriction is
        // honored via `effective_engines`).
        let temp = make_temp();
        let plugin_dir = temp.path().join("my-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap_or_else(|_| {});
        std::fs::write(
            plugin_dir.join("aipm.toml"),
            "[package]\n\
             name = \"test\"\n\
             version = \"1.0.0\"\n\
             [workspace]\n\
             members = [\".ai/*\"]\n\
             engines = [\"claude\"]\n",
        )
        .unwrap_or_else(|_| {});

        assert!(
            validate_plugin(&plugin_dir, Engine::Claude).is_ok(),
            "claude should be allowed by workspace declaration"
        );
        assert!(
            validate_plugin(&plugin_dir, Engine::Copilot).is_err(),
            "copilot should be rejected by workspace declaration"
        );
    }

    #[test]
    fn validate_package_engines_override_workspace_engines() {
        // Spec G7 inheritance contract: package wins over workspace
        // (no merging). Package declares only copilot; workspace declares
        // only claude.
        let temp = make_temp();
        let plugin_dir = temp.path().join("my-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap_or_else(|_| {});
        std::fs::write(
            plugin_dir.join("aipm.toml"),
            "[package]\n\
             name = \"test\"\n\
             version = \"1.0.0\"\n\
             engines = [\"copilot\"]\n\
             [workspace]\n\
             members = [\".ai/*\"]\n\
             engines = [\"claude\"]\n",
        )
        .unwrap_or_else(|_| {});

        assert!(
            validate_plugin(&plugin_dir, Engine::Claude).is_err(),
            "claude should be rejected by package declaration (overrides workspace)"
        );
        assert!(
            validate_plugin(&plugin_dir, Engine::Copilot).is_ok(),
            "copilot should be allowed by package declaration"
        );
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
        // The schema-driven Claude spec lists multiple marker paths, so the
        // requirement string uses the multi-marker form. Both individual
        // marker filenames must appear in the message.
        let req = format_marker_requirement(Engine::Claude);
        assert!(req.contains(".claude-plugin/plugin.json"));
    }

    #[test]
    fn human_readable_error_multi_marker() {
        let req = format_marker_requirement(Engine::Copilot);
        assert!(req.contains("expected at least one of"));
    }

    /// Covers the `markers.len() == 1` branch in `format_marker_string`.
    ///
    /// Both built-in engines have multiple marker paths, so this branch is
    /// dead code through `format_marker_requirement(engine)`.  Testing the
    /// private helper directly ensures the single-marker message is
    /// formatted correctly ("missing <path>") and exercises the branch.
    #[test]
    fn format_marker_string_single_marker() {
        let result = format_marker_string(&["single-marker.json"]);
        assert_eq!(result, "missing single-marker.json");
    }

    #[test]
    fn engine_display() {
        assert_eq!(display_name(Engine::Claude), "Claude");
        assert_eq!(display_name(Engine::Copilot), "Copilot");
    }

    #[test]
    fn engine_all_names() {
        let names = all_names();
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
        assert_eq!(marketplace_manifest_path(Engine::Claude), ".claude-plugin/marketplace.toml");
        assert_eq!(marketplace_manifest_path(Engine::Copilot), ".github/plugin/marketplace.json");
    }

    /// Covers the `engines.filter(|s| !s.is_empty())` → `None` branch that
    /// arises when the manifest has `engines = []` (explicit empty list).
    ///
    /// `Option::filter` calls the predicate when the `Option` is `Some`.  An
    /// explicit `engines = []` in the TOML deserializes to
    /// `Some(EngineSet::empty())`.  The `!s.is_empty()` guard evaluates to
    /// `false` for an empty set, so `filter` returns `None` — treating the
    /// plugin as universal (valid for all engines).  This path is distinct from
    /// the `engines` field being omitted entirely (`None` → filter bypassed).
    #[test]
    fn validate_with_aipm_toml_empty_engines_list_is_universal() {
        let temp = make_temp();
        let plugin_dir = temp.path().join("empty-engines-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap_or_else(|_| {});
        // engines = [] → Some(EngineSet::empty()) → filter predicate false → None → universal
        std::fs::write(
            plugin_dir.join("aipm.toml"),
            "[package]\nname = \"test\"\nversion = \"1.0.0\"\nengines = []\n",
        )
        .unwrap_or_else(|_| {});

        assert!(
            validate_plugin(&plugin_dir, Engine::Claude).is_ok(),
            "empty engines list should allow claude"
        );
        assert!(
            validate_plugin(&plugin_dir, Engine::Copilot).is_ok(),
            "empty engines list should allow copilot"
        );
    }
}
