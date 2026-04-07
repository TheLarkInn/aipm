//! Manifest validation logic.
//!
//! Validates a parsed `Manifest` against the aipm schema rules:
//! required fields, name format, semver version, dependency versions,
//! plugin type, and component path existence.

use std::path::Path;

use super::error::Error;
use super::types::{DependencySpec, Manifest, PluginType};

/// Regex-style check for valid package names.
/// Must match: `^(@[a-z0-9-]+/)?[a-z0-9][a-z0-9-]*$`
fn is_valid_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let to_check = if let Some(rest) = name.strip_prefix('@') {
        // Scoped name: @scope/name
        let Some(slash_pos) = rest.find('/') else {
            return false;
        };
        let scope = &rest[..slash_pos];
        let pkg = &rest[slash_pos + 1..];
        if scope.is_empty() || pkg.is_empty() {
            return false;
        }
        if !is_valid_segment(scope) || !is_valid_segment(pkg) {
            return false;
        }
        return true;
    } else {
        name
    };

    is_valid_segment(to_check)
}

/// Check a single name segment: lowercase alphanumeric + hyphens, must start with alnum.
fn is_valid_segment(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    // Must start with lowercase letter or digit
    if !bytes.first().is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit()) {
        return false;
    }
    // Rest must be lowercase letter, digit, or hyphen
    bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// Validate a version requirement string (caret, tilde, exact, wildcard, range).
fn is_valid_version_req(req: &str) -> bool {
    // Handle wildcard (used in [workspace.dependencies] version specs)
    if req == "*" {
        return true;
    }

    // Handle catalog references
    if req.starts_with("catalog:") || req == "catalog:" {
        return true;
    }

    // Try parsing as a semver requirement
    semver::VersionReq::parse(req).is_ok()
}

/// Validate a parsed manifest, optionally checking component paths against a base directory.
///
/// # Errors
///
/// Returns `Error` if validation fails — missing fields, invalid names,
/// bad versions, invalid dependency requirements, or missing component paths.
pub fn validate(manifest: &Manifest, base_dir: Option<&Path>) -> Result<(), Error> {
    let mut errors = Vec::new();

    // Validate [package] section if present
    if let Some(pkg) = &manifest.package {
        validate_package(pkg, &mut errors);
    }

    // Validate [dependencies]
    if let Some(deps) = &manifest.dependencies {
        validate_dependencies(deps, &mut errors);
    }

    // Validate [workspace.dependencies]
    if let Some(ws) = &manifest.workspace {
        if let Some(ws_deps) = &ws.dependencies {
            validate_dependencies(ws_deps, &mut errors);
        }
    }

    // Validate [components] paths if base_dir provided
    if let Some(components) = &manifest.components {
        if let Some(dir) = base_dir {
            validate_component_paths(components, dir, &mut errors);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else if errors.len() == 1 {
        // Single error — return it directly rather than wrapping in Multiple.
        errors.into_iter().next().map_or(Ok(()), Err)
    } else {
        Err(Error::Multiple(errors))
    }
}

fn validate_package(pkg: &super::types::Package, errors: &mut Vec<Error>) {
    // Name is required and must be valid
    if pkg.name.is_empty() {
        errors.push(Error::MissingField { field: "name".to_string() });
    } else if !is_valid_name(&pkg.name) {
        errors.push(Error::InvalidName {
            name: pkg.name.clone(),
            reason: "must be lowercase alphanumeric with hyphens, optionally scoped with @org/"
                .to_string(),
        });
    }

    // Version is required and must be valid semver
    if pkg.version.is_empty() {
        errors.push(Error::MissingField { field: "version".to_string() });
    } else if semver::Version::parse(&pkg.version).is_err() {
        errors.push(Error::InvalidVersion { version: pkg.version.clone() });
    }

    // Plugin type must be valid if specified
    if let Some(ref pt) = pkg.plugin_type {
        if pt.parse::<PluginType>().is_err() {
            errors.push(Error::InvalidPluginType { value: pt.clone() });
        }
    }
}

fn validate_dependencies(
    deps: &std::collections::BTreeMap<String, DependencySpec>,
    errors: &mut Vec<Error>,
) {
    for (name, spec) in deps {
        let version_str = match spec {
            DependencySpec::Simple(v) => Some(v.as_str()),
            DependencySpec::Detailed(d) => {
                if let Some(ref ws) = d.workspace {
                    if ws != "*" {
                        errors.push(Error::InvalidWorkspaceProtocol {
                            dependency: name.clone(),
                            protocol: ws.clone(),
                        });
                    }
                    continue;
                }
                d.version.as_deref()
            },
        };

        if let Some(v) = version_str {
            if !is_valid_version_req(v) {
                errors.push(Error::InvalidDependencyVersion {
                    dependency: name.clone(),
                    version: v.to_string(),
                });
            }
        }
    }
}

fn validate_component_paths(
    components: &super::types::Components,
    base_dir: &Path,
    errors: &mut Vec<Error>,
) {
    let all_paths = [
        components.skills.as_deref(),
        components.commands.as_deref(),
        components.agents.as_deref(),
        components.hooks.as_deref(),
        components.mcp_servers.as_deref(),
        components.lsp_servers.as_deref(),
        components.scripts.as_deref(),
        components.output_styles.as_deref(),
        components.settings.as_deref(),
    ];

    for paths in all_paths.iter().flatten() {
        for p in *paths {
            let full = base_dir.join(p);
            if !full.exists() {
                errors.push(Error::ComponentNotFound { path: p.into() });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_simple_name() {
        assert!(is_valid_name("my-plugin"));
        assert!(is_valid_name("plugin123"));
        assert!(is_valid_name("a"));
    }

    #[test]
    fn valid_scoped_name() {
        assert!(is_valid_name("@company/my-plugin"));
        assert!(is_valid_name("@org/tool"));
    }

    #[test]
    fn invalid_names() {
        assert!(!is_valid_name(""));
        assert!(!is_valid_name("My-Plugin")); // uppercase
        assert!(!is_valid_name("my_plugin")); // underscore
        assert!(!is_valid_name("-starts-dash")); // starts with dash
        assert!(!is_valid_name("@/no-scope")); // empty scope
        assert!(!is_valid_name("@scope/")); // empty name after scope
        assert!(!is_valid_name("has spaces")); // spaces
                                               // Branch coverage: scoped name without slash
        assert!(!is_valid_name("@noslash"));
        // Branch coverage: invalid scope segment
        assert!(!is_valid_name("@UPPER/pkg"));
        // Branch coverage: invalid pkg segment
        assert!(!is_valid_name("@org/UPPER"));
    }

    #[test]
    fn valid_version_reqs() {
        assert!(is_valid_version_req("^1.0"));
        assert!(is_valid_version_req("^1.0.0"));
        assert!(is_valid_version_req("~0.2.3"));
        assert!(is_valid_version_req("=1.0.0"));
        assert!(is_valid_version_req("*"));
        assert!(is_valid_version_req(">=1.0.0, <2.0.0"));
    }

    #[test]
    fn invalid_version_reqs() {
        assert!(!is_valid_version_req("???invalid"));
        assert!(!is_valid_version_req("not-a-version"));
        assert!(!is_valid_version_req(""));
    }

    #[test]
    fn workspace_protocol_valid() {
        // Only "*" is accepted as a valid standalone version req (for workspace deps)
        assert!(is_valid_version_req("*"));
        // "^" and "=" are no longer valid as standalone symbols
        assert!(!is_valid_version_req("^"));
        assert!(!is_valid_version_req("="));
    }

    #[test]
    fn catalog_refs_valid() {
        assert!(is_valid_version_req("catalog:"));
        assert!(is_valid_version_req("catalog:stable"));
    }

    #[test]
    fn dependency_with_no_version_is_accepted() {
        // Detailed dependency with no version and no workspace — version_str is None
        let toml = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
some-dep = {}
"#;
        let result = crate::manifest::parse_and_validate(toml, None);
        // Should succeed — missing version is not an error (just no version constraint)
        assert!(result.is_ok(), "dependency with no version should be valid: {result:?}");
    }

    #[test]
    fn segment_starting_with_digit_is_valid() {
        // Covers the `b.is_ascii_digit()` branch in `is_valid_segment` —
        // when the first byte is a digit, `is_ascii_lowercase()` is false and
        // the `||` falls through to evaluate `is_ascii_digit()`.
        assert!(is_valid_name("1plugin"));
        assert!(is_valid_name("@scope/1tool"));
        assert!(is_valid_segment("123abc"));
    }

    #[test]
    fn is_valid_segment_empty_returns_false() {
        // Covers the `if s.is_empty()` True branch in `is_valid_segment`.
        // The callers in `is_valid_name` guard against empty scope/pkg before
        // calling `is_valid_segment`, so this branch is only reachable directly.
        assert!(!is_valid_segment(""));
    }
}
