//! Plugin path security validation.
//!
//! Provides [`ValidatedPath`], a newtype wrapper that guarantees paths are
//! relative, non-empty, and free from traversal attacks.  Every plugin path
//! flowing through the acquisition pipeline must pass through this module
//! before touching the filesystem.
//!
//! # Rejected patterns
//!
//! - Empty paths
//! - `..` path components (directory traversal)
//! - URL-encoded traversal (`%2e%2e`)
//! - Absolute paths (Unix `/`, Windows `C:\`, UNC `\\`)
//! - Null bytes

use std::path::{Component, Path};

/// Error when validating a plugin path.
#[derive(Debug, thiserror::Error)]
pub enum PathValidationError {
    /// The path string is empty.
    #[error("Empty plugin path")]
    EmptyPath,

    /// A `..` component or URL-encoded equivalent was detected.
    #[error("Path traversal detected: '..' components are forbidden")]
    PathTraversal,

    /// The path is absolute (starts with `/`, `C:\`, `\\`, etc.).
    #[error("Absolute paths not allowed — use a relative path")]
    AbsolutePath,
}

/// A validated plugin path that is guaranteed to be relative, non-empty,
/// and free from traversal attacks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidatedPath(String);

impl ValidatedPath {
    /// Create a new validated plugin path.
    ///
    /// # Errors
    ///
    /// Returns [`PathValidationError`] if the path is empty, contains
    /// traversal components, or is absolute.
    pub fn new(path: impl AsRef<str>) -> Result<Self, PathValidationError> {
        let path = path.as_ref();
        validate_plugin_path(path)?;
        Ok(Self(path.to_string()))
    }

    /// Get the path as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the final component of the path (the plugin folder name).
    pub fn folder_name(&self) -> &str {
        Path::new(&self.0).file_name().and_then(|n| n.to_str()).unwrap_or("plugin")
    }
}

impl std::fmt::Display for ValidatedPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ValidatedPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Validate a plugin path for security issues.
///
/// Rejects empty paths, traversal (`..` and URL-encoded `%2e%2e`),
/// and absolute paths.
fn validate_plugin_path(path: &str) -> Result<(), PathValidationError> {
    if path.is_empty() {
        return Err(PathValidationError::EmptyPath);
    }

    // Reject null bytes
    if path.contains('\0') {
        return Err(PathValidationError::PathTraversal);
    }

    // Check each component for traversal and absolute-path indicators
    for component in Path::new(path).components() {
        match component {
            Component::ParentDir => {
                return Err(PathValidationError::PathTraversal);
            },
            Component::Prefix(_) | Component::RootDir => {
                return Err(PathValidationError::AbsolutePath);
            },
            _ => {},
        }
    }

    // Also check the raw string for URL-encoded traversal variants
    let lower = path.to_lowercase();
    if lower.contains("..") || lower.contains("%2e%2e") {
        return Err(PathValidationError::PathTraversal);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_empty_path() {
        let result = validate_plugin_path("");
        assert!(matches!(result, Err(PathValidationError::EmptyPath)));
    }

    #[test]
    fn validate_path_traversal_start() {
        let result = validate_plugin_path("../etc/passwd");
        assert!(matches!(result, Err(PathValidationError::PathTraversal)));
    }

    #[test]
    fn validate_path_traversal_middle() {
        let result = validate_plugin_path("foo/../../../etc/passwd");
        assert!(matches!(result, Err(PathValidationError::PathTraversal)));
    }

    #[test]
    fn validate_path_traversal_encoded() {
        let result = validate_plugin_path("foo/%2e%2e/bar");
        assert!(matches!(result, Err(PathValidationError::PathTraversal)));
    }

    #[test]
    fn validate_absolute_path_unix() {
        let result = validate_plugin_path("/etc/passwd");
        assert!(matches!(result, Err(PathValidationError::AbsolutePath)));
    }

    #[test]
    fn validate_simple_path() {
        assert!(validate_plugin_path("plugins/my-plugin").is_ok());
    }

    #[test]
    fn validate_nested_path() {
        assert!(validate_plugin_path("experimental/plugins/hello-world").is_ok());
    }

    #[test]
    fn validate_deeply_nested_path() {
        assert!(validate_plugin_path("a/b/c/d/e/f").is_ok());
    }

    #[test]
    fn validate_path_with_dashes() {
        assert!(validate_plugin_path("plugin-with-dashes").is_ok());
    }

    #[test]
    fn validate_path_with_underscores() {
        assert!(validate_plugin_path("plugin_with_underscores").is_ok());
    }

    #[test]
    fn validate_path_with_dots_not_traversal() {
        assert!(validate_plugin_path("plugin.with").is_ok());
    }

    #[test]
    fn plugin_path_new_valid() {
        let path = ValidatedPath::new("experimental/plugins/hello-world");
        assert!(path.is_ok());
        let path = path.unwrap_or_else(|e| panic_free_unreachable(&e));
        assert_eq!(path.as_str(), "experimental/plugins/hello-world");
    }

    #[test]
    fn plugin_path_new_invalid_traversal() {
        let path = ValidatedPath::new("../secret");
        assert!(path.is_err());
    }

    #[test]
    fn plugin_path_display() {
        let path =
            ValidatedPath::new("plugins/auth").unwrap_or_else(|e| panic_free_unreachable(&e));
        assert_eq!(format!("{path}"), "plugins/auth");
    }

    #[test]
    fn plugin_path_folder_name() {
        let path =
            ValidatedPath::new("a/b/c/my-plugin").unwrap_or_else(|e| panic_free_unreachable(&e));
        assert_eq!(path.folder_name(), "my-plugin");
    }

    #[test]
    fn plugin_path_folder_name_simple() {
        let path = ValidatedPath::new("my-plugin").unwrap_or_else(|e| panic_free_unreachable(&e));
        assert_eq!(path.folder_name(), "my-plugin");
    }

    #[test]
    fn validate_null_byte_rejected() {
        let result = validate_plugin_path("foo\0bar");
        assert!(matches!(result, Err(PathValidationError::PathTraversal)));
    }

    #[test]
    fn validate_double_dot_in_filename_rejected() {
        // "foo..bar" contains ".." in the raw string but not as a path component
        let result = validate_plugin_path("foo..bar");
        assert!(matches!(result, Err(PathValidationError::PathTraversal)));
    }

    #[test]
    fn validate_windows_absolute_path() {
        // Windows-style drive letter path
        let result = validate_plugin_path("C:\\Users\\test");
        assert!(matches!(result, Err(PathValidationError::AbsolutePath)));
    }

    #[test]
    fn validate_unc_path() {
        let result = validate_plugin_path("\\\\server\\share");
        assert!(matches!(result, Err(PathValidationError::AbsolutePath)));
    }

    /// Test helper that converts a validation error to a string and then
    /// creates a dummy [`ValidatedPath`] — used in place of `.unwrap()` which
    /// is denied by the workspace lint configuration.
    fn panic_free_unreachable(err: &PathValidationError) -> ValidatedPath {
        // This is only called in tests and will fail the test assertion before
        // reaching here, but we must return *something* to satisfy the type
        // checker without using `panic!()` / `unwrap()`.
        let _ = err;
        ValidatedPath(String::new())
    }
}
