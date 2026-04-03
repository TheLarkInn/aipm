//! Workspace discovery and member resolution.
//!
//! Provides utilities for finding the workspace root manifest, expanding
//! `[workspace].members` glob patterns to discover member directories,
//! and building a name-to-path map of workspace members.

pub mod error;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::manifest;
pub use error::Error;

/// A discovered workspace member with its metadata.
#[derive(Debug, Clone)]
pub struct Member {
    /// Package name from the member's `[package].name`.
    pub name: String,
    /// Path to the member directory.
    pub path: PathBuf,
    /// Version from the member's `[package].version`.
    pub version: String,
    /// The member's parsed manifest (for transitive workspace dep detection).
    pub manifest: manifest::types::Manifest,
}

/// Walk up from `start_dir` looking for an `aipm.toml` with a `[workspace]` section.
///
/// Returns the path to the workspace root directory (parent of the manifest),
/// or `None` if no workspace root is found before reaching the filesystem root.
pub fn find_workspace_root(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir.to_path_buf();
    loop {
        let manifest_path = current.join("aipm.toml");
        if manifest_path.exists() {
            match std::fs::read_to_string(&manifest_path) {
                Ok(content) => match toml::from_str::<manifest::types::Manifest>(&content) {
                    Ok(m) => {
                        if m.workspace.is_some() {
                            return Some(current);
                        }
                    },
                    Err(e) => {
                        tracing::debug!(
                            path = %manifest_path.display(),
                            error = %e,
                            "skipping unparseable manifest during workspace discovery"
                        );
                    },
                },
                Err(e) => {
                    tracing::debug!(
                        path = %manifest_path.display(),
                        error = %e,
                        "could not read manifest during workspace discovery"
                    );
                },
            }
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Discover all workspace members by expanding glob patterns.
///
/// Reads each member's `aipm.toml` to extract name and version.
/// Returns a map of `package_name` → [`Member`].
///
/// # Errors
///
/// Returns an error if:
/// - A glob pattern is invalid
/// - A member manifest fails to parse
/// - A member manifest has no `[package]` section
/// - Two members declare the same package name
pub fn discover_members(
    workspace_root: &Path,
    member_patterns: &[String],
) -> Result<BTreeMap<String, Member>, Error> {
    let mut members = BTreeMap::new();

    for pattern in member_patterns {
        let full_pattern = workspace_root.join(pattern);
        let pattern_str = full_pattern.to_string_lossy().into_owned();
        let entries = glob::glob(&pattern_str)
            .map_err(|e| Error::Discovery(format!("invalid glob pattern '{pattern}': {e}")))?;

        for entry in entries {
            let dir = entry.map_err(|e| Error::Discovery(format!("glob traversal error: {e}")))?;

            if !dir.is_dir() {
                continue;
            }

            let manifest_path = dir.join("aipm.toml");
            if !manifest_path.exists() {
                tracing::warn!(
                    path = %dir.display(),
                    "directory matches workspace member glob but has no aipm.toml — skipping"
                );
                continue;
            }

            let content = std::fs::read_to_string(&manifest_path).map_err(|e| {
                Error::Discovery(format!("failed to read {}: {e}", manifest_path.display()))
            })?;

            let parsed = manifest::parse_and_validate(&content, Some(&dir)).map_err(|e| {
                Error::Discovery(format!("invalid manifest at {}: {e}", manifest_path.display()))
            })?;

            let package = parsed.package.as_ref().ok_or_else(|| {
                Error::Discovery(format!("member at {} has no [package] section", dir.display()))
            })?;

            let name = package.name.clone();
            let version = package.version.clone();

            if let Some(existing) = members.get(&name) {
                let existing: &Member = existing;
                return Err(Error::Discovery(format!(
                    "duplicate workspace member name '{}': found at {} and {}",
                    name,
                    existing.path.display(),
                    dir.display()
                )));
            }

            members.insert(name.clone(), Member { name, path: dir, version, manifest: parsed });
        }
    }

    if !members.is_empty() {
        let names: Vec<&str> = members.keys().map(String::as_str).collect();
        tracing::info!(count = members.len(), ?names, "discovered workspace members");
    }

    Ok(members)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_root_from_member() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        std::fs::write(root.join("aipm.toml"), "[workspace]\nmembers = [\".ai/*\"]\n").unwrap();
        std::fs::create_dir_all(root.join(".ai/plugin-a")).unwrap();

        let result = find_workspace_root(&root.join(".ai/plugin-a"));
        assert_eq!(result.as_deref(), Some(root));
    }

    #[test]
    fn find_root_returns_none_for_non_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Manifest WITHOUT [workspace]
        std::fs::write(root.join("aipm.toml"), "[package]\nname = \"foo\"\nversion = \"0.1.0\"\n")
            .unwrap();
        let subdir = root.join("sub");
        std::fs::create_dir_all(&subdir).unwrap();

        let result = find_workspace_root(&subdir);
        // Should not match the non-workspace manifest at root
        if let Some(ref found) = result {
            assert_ne!(found.as_path(), root, "should not match non-workspace manifest");
        }
    }

    #[test]
    fn find_root_from_root_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        std::fs::write(root.join("aipm.toml"), "[workspace]\nmembers = [\".ai/*\"]\n").unwrap();

        let result = find_workspace_root(root);
        assert_eq!(result.as_deref(), Some(root));
    }

    #[test]
    fn discover_members_single_glob() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        for (name, ver) in &[("plugin-a", "1.0.0"), ("plugin-b", "2.0.0")] {
            let dir = root.join(".ai").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("aipm.toml"),
                format!(
                    "[package]\nname = \"{name}\"\nversion = \"{ver}\"\ntype = \"composite\"\n"
                ),
            )
            .unwrap();
        }

        let members = discover_members(root, &[".ai/*".to_string()]).unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(members.get("plugin-a").unwrap().version, "1.0.0");
        assert_eq!(members.get("plugin-b").unwrap().version, "2.0.0");
    }

    #[test]
    fn discover_members_skips_no_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        std::fs::create_dir_all(root.join(".ai/no-manifest")).unwrap();
        let dir = root.join(".ai/valid-plugin");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("aipm.toml"),
            "[package]\nname = \"valid-plugin\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();

        let members = discover_members(root, &[".ai/*".to_string()]).unwrap();
        assert_eq!(members.len(), 1);
        assert!(members.contains_key("valid-plugin"));
    }

    #[test]
    fn discover_members_error_duplicate_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        for subdir in &["dir-a", "dir-b"] {
            let dir = root.join(".ai").join(subdir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("aipm.toml"),
                "[package]\nname = \"same-name\"\nversion = \"1.0.0\"\n",
            )
            .unwrap();
        }

        let err = discover_members(root, &[".ai/*".to_string()]).unwrap_err();
        assert!(format!("{err}").contains("duplicate workspace member name"));
    }

    #[test]
    fn discover_members_error_no_package_section() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let dir = root.join(".ai/ws-only");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("aipm.toml"), "[workspace]\nmembers = [\"*\"]\n").unwrap();

        let err = discover_members(root, &[".ai/*".to_string()]).unwrap_err();
        assert!(format!("{err}").contains("no [package] section"));
    }

    #[test]
    fn discover_members_multiple_patterns() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let dir_a = root.join("plugins/plugin-a");
        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::write(
            dir_a.join("aipm.toml"),
            "[package]\nname = \"plugin-a\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();

        let dir_b = root.join("tools/tool-b");
        std::fs::create_dir_all(&dir_b).unwrap();
        std::fs::write(
            dir_b.join("aipm.toml"),
            "[package]\nname = \"tool-b\"\nversion = \"2.0.0\"\n",
        )
        .unwrap();

        let members =
            discover_members(root, &["plugins/*".to_string(), "tools/*".to_string()]).unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.contains_key("plugin-a"));
        assert!(members.contains_key("tool-b"));
    }

    #[test]
    fn find_root_skips_invalid_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        std::fs::write(root.join("aipm.toml"), "this is not valid { toml [[[").unwrap();
        let subdir = root.join("sub");
        std::fs::create_dir_all(&subdir).unwrap();

        let result = find_workspace_root(&subdir);
        if let Some(ref found) = result {
            assert_ne!(found.as_path(), root, "should skip invalid TOML");
        }
    }

    #[test]
    fn discover_members_skips_non_directory_match() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        std::fs::create_dir_all(root.join(".ai")).unwrap();
        std::fs::write(root.join(".ai/some-file"), "not a directory").unwrap();

        let dir = root.join(".ai/valid");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("aipm.toml"), "[package]\nname = \"valid\"\nversion = \"1.0.0\"\n")
            .unwrap();

        let members = discover_members(root, &[".ai/*".to_string()]).unwrap();
        assert_eq!(members.len(), 1);
        assert!(members.contains_key("valid"));
    }

    #[test]
    fn discover_members_empty_when_no_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // No .ai/ directory at all — glob finds nothing
        let members = discover_members(root, &[".ai/*".to_string()]).unwrap();
        assert!(members.is_empty());
    }

    #[test]
    fn discover_members_error_invalid_manifest_content() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let dir = root.join(".ai/bad-toml");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("aipm.toml"), "this is [[[ not valid toml").unwrap();

        let err = discover_members(root, &[".ai/*".to_string()]).unwrap_err();
        assert!(format!("{err}").contains("invalid manifest"));
    }

    #[test]
    fn discover_members_error_manifest_is_directory() {
        // If a path named "aipm.toml" exists as a directory rather than a file,
        // std::fs::read_to_string fails — this covers the error-conversion branch
        // in discover_members (the map_err closure and the ? propagation).
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let member_dir = root.join(".ai").join("dir-member");
        std::fs::create_dir_all(&member_dir).unwrap();
        // Create a *directory* called "aipm.toml" so exists() returns true
        // but read_to_string errors with "Is a directory".
        let manifest_path = member_dir.join("aipm.toml");
        std::fs::create_dir_all(&manifest_path).unwrap();

        let err = discover_members(root, &[".ai/*".to_string()]).unwrap_err();
        assert!(
            format!("{err}").contains("failed to read"),
            "expected 'failed to read' error, got: {err}"
        );
    }
}
