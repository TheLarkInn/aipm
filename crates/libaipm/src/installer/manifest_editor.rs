//! Manifest editing using `toml_edit` for comment and formatting preservation.
//!
//! When `aipm install <pkg>` adds a new dependency, the manifest file must be
//! updated without losing existing comments, whitespace, or ordering.

use std::path::Path;

use super::error::Error;

/// Add a dependency to `aipm.toml`, preserving comments and formatting.
///
/// If the `[dependencies]` table does not exist, it is created.
/// If the dependency already exists, its version requirement is updated.
///
/// # Arguments
///
/// * `manifest_path` - Path to the `aipm.toml` file.
/// * `name` - The package name to add.
/// * `version_req` - The version requirement string (e.g. `"^1.0"`).
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read or written.
/// Returns [`Error::Manifest`] if the TOML is invalid.
pub fn add_dependency(manifest_path: &Path, name: &str, version_req: &str) -> Result<(), Error> {
    let content = std::fs::read_to_string(manifest_path)?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| Error::Manifest { reason: e.to_string() })?;

    // Ensure [dependencies] table exists and get a mutable reference to it.
    if doc.get("dependencies").is_none() {
        doc.insert("dependencies", toml_edit::Item::Table(toml_edit::Table::new()));
    }

    let deps = doc.get_mut("dependencies").and_then(|d| d.as_table_mut()).ok_or_else(|| {
        Error::Manifest { reason: "could not access [dependencies] table".to_string() }
    })?;

    // Set the dependency.
    deps.insert(name, toml_edit::value(version_req));

    std::fs::write(manifest_path, doc.to_string())?;
    Ok(())
}

/// Remove a dependency from `aipm.toml`, preserving comments and formatting.
///
/// No-op if the dependency does not exist.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be read or written.
/// Returns [`Error::Manifest`] if the TOML is invalid.
pub fn remove_dependency(manifest_path: &Path, name: &str) -> Result<(), Error> {
    let content = std::fs::read_to_string(manifest_path)?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| Error::Manifest { reason: e.to_string() })?;

    if let Some(deps) = doc.get_mut("dependencies").and_then(|d| d.as_table_mut()) {
        deps.remove(name);
    }

    std::fs::write(manifest_path, doc.to_string())?;
    Ok(())
}

/// Parse a `name@version` string into `(name, version_req)`.
///
/// If no `@version` suffix is present, defaults to `"*"`.
pub fn parse_package_spec(spec: &str) -> (String, String) {
    match spec.split_once('@') {
        Some((name, version)) => (name.to_string(), version.to_string()),
        None => (spec.to_string(), "*".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MANIFEST: &str = r#"# My project
[package]
name = "my-project"
version = "0.1.0"

# Existing deps
[dependencies]
existing-pkg = "^1.0"
"#;

    #[test]
    fn add_dependency_to_existing_table() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manifest = tmp.path().join("aipm.toml");
        std::fs::write(&manifest, SAMPLE_MANIFEST).expect("write");

        let result = add_dependency(&manifest, "new-pkg", "^2.0");
        assert!(result.is_ok(), "add_dependency failed: {result:?}");

        let content = std::fs::read_to_string(&manifest).expect("read");
        assert!(content.contains("new-pkg"));
        assert!(content.contains("^2.0"));
        // Preserves existing entries.
        assert!(content.contains("existing-pkg"));
        // Preserves comments.
        assert!(content.contains("# My project"));
        assert!(content.contains("# Existing deps"));
    }

    #[test]
    fn add_dependency_creates_table_if_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manifest = tmp.path().join("aipm.toml");
        std::fs::write(&manifest, "[package]\nname = \"test\"\n").expect("write");

        let result = add_dependency(&manifest, "new-pkg", "^1.0");
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&manifest).expect("read");
        assert!(content.contains("[dependencies]"));
        assert!(content.contains("new-pkg"));
    }

    #[test]
    fn add_dependency_updates_existing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manifest = tmp.path().join("aipm.toml");
        std::fs::write(&manifest, SAMPLE_MANIFEST).expect("write");

        let result = add_dependency(&manifest, "existing-pkg", "^2.0");
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&manifest).expect("read");
        assert!(content.contains("^2.0"));
        assert!(!content.contains("^1.0"));
    }

    #[test]
    fn remove_dependency_removes_entry() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manifest = tmp.path().join("aipm.toml");
        std::fs::write(&manifest, SAMPLE_MANIFEST).expect("write");

        let result = remove_dependency(&manifest, "existing-pkg");
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&manifest).expect("read");
        assert!(!content.contains("existing-pkg"));
        // Preserves other content.
        assert!(content.contains("# My project"));
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let manifest = tmp.path().join("aipm.toml");
        std::fs::write(&manifest, SAMPLE_MANIFEST).expect("write");

        let result = remove_dependency(&manifest, "nonexistent");
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&manifest).expect("read");
        assert!(content.contains("existing-pkg"));
    }

    #[test]
    fn parse_package_spec_with_version() {
        let (name, version) = parse_package_spec("my-pkg@^1.0");
        assert_eq!(name, "my-pkg");
        assert_eq!(version, "^1.0");
    }

    #[test]
    fn parse_package_spec_without_version() {
        let (name, version) = parse_package_spec("my-pkg");
        assert_eq!(name, "my-pkg");
        assert_eq!(version, "*");
    }

    #[test]
    fn parse_package_spec_exact_version() {
        let (name, version) = parse_package_spec("my-pkg@=1.2.3");
        assert_eq!(name, "my-pkg");
        assert_eq!(version, "=1.2.3");
    }

    #[test]
    fn add_dependency_fails_when_dependencies_is_scalar() {
        // When `dependencies` key exists but is a scalar value (not a table),
        // `as_table_mut()` returns None → ok_or_else error path (lines 37-38) is triggered.
        let tmp = tempfile::tempdir().expect("tempdir");
        let manifest = tmp.path().join("aipm.toml");
        // Put `dependencies` at the DOCUMENT ROOT (not inside [package]) so that
        // `doc.get("dependencies")` finds it as a scalar and `as_table_mut()` returns None.
        std::fs::write(&manifest, "dependencies = \"not-a-table\"\n").expect("write");

        let result = add_dependency(&manifest, "new-pkg", "^1.0");
        assert!(result.is_err());
        let err = format!("{result:?}");
        assert!(err.contains("dependencies"));
    }

    #[test]
    fn remove_dependency_no_deps_table_is_noop() {
        // Covers the None branch of `if let Some(deps) = doc.get_mut("dependencies")…`
        // in remove_dependency — when the manifest has no [dependencies] table at all.
        let tmp = tempfile::tempdir().expect("tempdir");
        let manifest = tmp.path().join("aipm.toml");
        std::fs::write(&manifest, "[package]\nname = \"test\"\nversion = \"0.1.0\"\n")
            .expect("write");

        let result = remove_dependency(&manifest, "nonexistent-pkg");
        assert!(result.is_ok());

        // File should be unchanged (still no [dependencies])
        let content = std::fs::read_to_string(&manifest).expect("read");
        assert!(!content.contains("[dependencies]"));
    }
}
