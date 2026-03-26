//! Manifest parsing, types, and validation for `aipm.toml`.
//!
//! This module provides the complete manifest schema used by both workspace
//! root manifests and plugin member manifests.

pub mod error;
pub mod types;
pub mod validate;

use std::path::Path;

use error::Error;
use types::Manifest;

/// Parse an `aipm.toml` manifest from a TOML string.
///
/// # Errors
///
/// Returns `Error::Parse` if the TOML is syntactically invalid or
/// does not match the expected manifest schema.
pub fn parse(toml_str: &str) -> Result<Manifest, Error> {
    toml::from_str(toml_str).map_err(|source| Error::Parse { source })
}

/// Parse and validate an `aipm.toml` manifest from a TOML string.
///
/// If `base_dir` is provided, component paths are checked for existence on disk.
///
/// # Errors
///
/// Returns `Error` on parse failure or validation errors (missing fields,
/// invalid names/versions, bad dependency requirements, missing component paths).
pub fn parse_and_validate(toml_str: &str, base_dir: Option<&Path>) -> Result<Manifest, Error> {
    let manifest = parse(toml_str)?;
    validate::validate(&manifest, base_dir)?;
    Ok(manifest)
}

/// Read, parse, and validate an `aipm.toml` file from the filesystem.
///
/// # Errors
///
/// Returns `Error::Io` if the file cannot be read, or any validation
/// error from `parse_and_validate`.
pub fn load(manifest_path: &Path) -> Result<Manifest, Error> {
    let content = std::fs::read_to_string(manifest_path).map_err(|source| Error::Io { source })?;
    let base_dir = manifest_path.parent();
    parse_and_validate(&content, base_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_manifest() {
        let toml = r#"
[package]
name = "my-plugin"
version = "0.1.0"
"#;
        let m = parse_and_validate(toml, None);
        assert!(m.is_ok());
        let m = m.ok();
        assert!(m.is_some());
        let m = m.as_ref();
        assert!(m.is_some_and(|m| m.package.is_some()));
    }

    #[test]
    fn parse_full_member_manifest() {
        let toml = r#"
[package]
name = "@company/ci-tools"
version = "1.2.3"
description = "CI automation skills"
type = "composite"
files = ["skills/", "hooks/", "README.md"]

[dependencies]
shared-lint = "^1.0"
core-hooks = { workspace = "^" }
heavy-analyzer = { version = "^1.0", optional = true }

[features]
default = ["basic"]
basic = []
deep = ["dep:heavy-analyzer"]

[components]
skills = ["skills/lint/SKILL.md"]
hooks = ["hooks/pre-push.json"]

[environment]
requires = ["git", "docker"]
aipm = ">=0.5.0"
platforms = ["linux-x64", "macos-arm64", "windows-x64"]
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_workspace_root_manifest() {
        let toml = r#"
[workspace]
members = ["claude-plugins/*"]
plugins_dir = "claude-plugins"

[workspace.dependencies]
common-skill = "^2.0"

[dependencies]
"@company/code-review" = "^1.0"

[overrides]
"vulnerable-lib" = "^2.0.0"

[catalog]
lint-skill = "^1.5.0"

[catalogs.stable]
framework = "^1.0.0"

[catalogs.next]
framework = "^2.0.0-beta.1"

[install]
allowed_build_scripts = ["native-tool"]
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn missing_name_fails() {
        let toml = r#"
[package]
name = ""
version = "0.1.0"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("missing required field: name")));
    }

    #[test]
    fn missing_version_fails() {
        let toml = r#"
[package]
name = "valid-name"
version = ""
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("missing required field: version")));
    }

    #[test]
    fn invalid_version_fails() {
        let toml = r#"
[package]
name = "my-plugin"
version = "not-a-version"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("invalid semver version")));
    }

    #[test]
    fn invalid_name_fails() {
        let toml = r#"
[package]
name = "Invalid_Name"
version = "0.1.0"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("invalid package name")));
    }

    #[test]
    fn invalid_plugin_type_fails() {
        let toml = r#"
[package]
name = "my-plugin"
version = "0.1.0"
type = "invalid-type"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("invalid plugin type")));
    }

    #[test]
    fn invalid_dependency_version_fails() {
        let toml = r#"
[package]
name = "my-plugin"
version = "0.1.0"

[dependencies]
broken = "???invalid"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e
            .to_string()
            .contains("invalid version requirement for dependency: broken")));
    }

    #[test]
    fn valid_dependency_versions() {
        let toml = r#"
[package]
name = "my-plugin"
version = "0.1.0"

[dependencies]
code-review = "^1.0.0"
lint-skill = "~0.2.3"
exact-pin = "=1.0.0"
any-version = "*"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn workspace_ref_dependency_valid() {
        let toml = r#"
[package]
name = "my-plugin"
version = "0.1.0"

[dependencies]
sibling = { workspace = "^" }
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn component_path_not_found() {
        let toml = r#"
[package]
name = "my-plugin"
version = "0.1.0"

[components]
skills = ["skills/nonexistent/SKILL.md"]
"#;
        // Use a temp directory that definitely won't have the path
        let temp = std::env::temp_dir();
        let result = parse_and_validate(toml, Some(&temp));
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("component not found")));
    }

    #[test]
    fn empty_manifest_is_valid() {
        // A manifest with neither [package] nor [workspace] is technically valid
        // (will be caught at a higher level by command-specific validation)
        let toml = "";
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn environment_section_parses() {
        let toml = r#"
[package]
name = "my-plugin"
version = "0.1.0"

[environment]
requires = ["git", "docker"]
aipm = ">=0.5.0"
platforms = ["linux-x64"]
strict = true

[environment.runtime]
node = ">=18.0.0"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn overrides_section_parses() {
        let toml = r#"
[workspace]
members = ["plugins/*"]

[overrides]
"vulnerable-lib" = "^2.0.0"
"skill-a>common-util" = "=2.1.0"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn features_section_parses() {
        let toml = r#"
[package]
name = "my-plugin"
version = "0.1.0"

[dependencies]
heavy = { version = "^1.0", optional = true }

[features]
default = ["basic"]
basic = []
advanced = ["dep:heavy"]
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn scoped_package_name_valid() {
        let toml = r#"
[package]
name = "@my-org/cool-plugin"
version = "1.0.0"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn all_plugin_types_valid() {
        for pt in &["skill", "agent", "mcp", "hook", "lsp", "composite"] {
            let toml = format!(
                r#"
[package]
name = "test"
version = "0.1.0"
type = "{pt}"
"#
            );
            let result = parse_and_validate(&toml, None);
            assert!(result.is_ok(), "plugin type '{pt}' should be valid");
        }
    }

    #[test]
    fn install_section_parses() {
        let toml = r#"
[workspace]
members = ["plugins/*"]

[install]
allowed_build_scripts = ["native-tool", "postinstall-script"]
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn catalogs_parse() {
        let toml = r#"
[workspace]
members = ["plugins/*"]

[catalog]
common = "^2.0.0"

[catalogs.stable]
framework = "^1.0.0"

[catalogs.next]
framework = "^2.0.0-beta.1"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn prerelease_version_valid() {
        let toml = r#"
[package]
name = "my-plugin"
version = "1.0.0-beta.1"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn version_with_build_metadata_valid() {
        let toml = r#"
[package]
name = "my-plugin"
version = "1.0.0+build.123"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn full_components_with_lsp_and_scripts() {
        let toml = r#"
[package]
name = "enterprise-plugin"
version = "1.0.0"
type = "composite"

[components]
skills = ["skills/code-review/SKILL.md"]
commands = ["commands/status.md"]
agents = ["agents/reviewer.md"]
hooks = ["hooks/hooks.json"]
mcp_servers = [".mcp.json"]
lsp_servers = [".lsp.json"]
scripts = ["scripts/format-code.sh"]
output_styles = ["styles/custom.css"]
settings = ["settings.json"]
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
        let m = result.ok();
        assert!(m.is_some());
        let components = m.and_then(|m| m.components);
        assert!(components.is_some());
        let c = components.as_ref();
        assert!(c.is_some_and(|c| c.lsp_servers.is_some()));
        assert!(c.is_some_and(|c| c.scripts.is_some()));
        assert!(c.is_some_and(|c| c.commands.is_some()));
        assert!(c.is_some_and(|c| c.output_styles.is_some()));
        assert!(c.is_some_and(|c| c.settings.is_some()));
    }

    #[test]
    fn lsp_plugin_type_valid() {
        let toml = r#"
[package]
name = "rust-lsp"
version = "0.1.0"
type = "lsp"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn multiple_errors_format_with_separator() {
        // Trigger Multiple error variant to cover format_errors branches (if i > 0)
        let toml = "[package]\nname = \"\"\nversion = \"\"";
        let result = parse_and_validate(toml, None);
        assert!(result.is_err());
        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        // Both "name" and "version" missing — errors joined by "; "
        assert!(err_msg.contains("name"), "expected name error in: {err_msg}");
        assert!(err_msg.contains("version"), "expected version error in: {err_msg}");
        assert!(err_msg.contains("; "), "expected '; ' separator in: {err_msg}");
    }

    #[test]
    fn edition_field_rejected() {
        let toml = r#"
[package]
name = "my-plugin"
version = "0.1.0"
edition = "2024"
"#;
        let result = parse_and_validate(toml, None);
        assert!(result.is_err());
    }
}
