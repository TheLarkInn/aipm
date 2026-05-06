//! Shared path-containment guard for lint rules that join paths from
//! PR-author-controlled file content.
//!
//! Validates path strings *before* any `Path::join`, rejecting parent-dir
//! traversal, absolute roots, and Windows drive prefixes. Used by lint
//! rules that read paths derived from `marketplace.json`, `aipm.toml`,
//! or other PR-controlled inputs to make sure those reads cannot escape
//! the workspace under inspection (issue #793 Finding 2).
//!
//! Existing user: `lint::rules::import_resolver` (the helper was originally
//! defined there; this module is the shared lint-layer home introduced
//! in #793).

use std::path::{Component, Path};

/// Returns `true` when every component of `path` is `CurDir` or `Normal`.
///
/// Returns `false` when any component is `ParentDir` (`..`), `RootDir`
/// (the Unix `/` root), or `Prefix(_)` (a Windows drive or UNC prefix) —
/// i.e. when the path could escape its base directory after a
/// `Path::join`. Cross-platform: `Path::components()` normalises both
/// `/`- and `\`-separated paths.
pub fn is_safe_path(path: &str) -> bool {
    Path::new(path)
        .components()
        .all(|c| !matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(..)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_safe_path_rejects_parent_dir_traversal() {
        assert!(!is_safe_path("../../etc/passwd"));
    }

    #[test]
    fn is_safe_path_rejects_absolute_unix_path() {
        assert!(!is_safe_path("/etc/passwd"));
    }

    #[test]
    fn is_safe_path_accepts_relative_path() {
        assert!(is_safe_path("shared/context.md"));
    }

    #[test]
    fn is_safe_path_accepts_curdir_prefix() {
        // `./shared/context.md` parses as a sequence of CurDir + Normal
        // components, neither of which is rejected.
        assert!(is_safe_path("./shared/context.md"));
    }

    #[test]
    fn is_safe_path_accepts_simple_filename() {
        assert!(is_safe_path("a.md"));
    }

    #[test]
    fn is_safe_path_rejects_parent_dir_segment_anywhere() {
        // `..` anywhere in the path is a Component::ParentDir and rejected.
        assert!(!is_safe_path("a/../b"));
        assert!(!is_safe_path("a/b/.."));
    }

    #[test]
    fn is_safe_path_rejects_windows_prefix() {
        // `\\?\C:\Windows` parses with a `Component::Prefix` head on
        // Windows targets. On Unix the same string parses as a single
        // Normal component and would be accepted — this assertion is
        // therefore guarded to the Windows target where the helper's
        // Prefix-rejection branch is exercised.
        #[cfg(windows)]
        assert!(!is_safe_path(r"\\?\C:\Windows"));
        #[cfg(windows)]
        assert!(!is_safe_path(r"C:\Windows"));
    }

    #[test]
    fn is_safe_path_accepts_dotted_filename() {
        // `foo..bar` is a single Normal component, not a ParentDir —
        // the helper accepts it. (The encoded-traversal hardening lives
        // in `path_security::ValidatedPath`, which scopes a different
        // threat model; lint-layer rules only need component-shape
        // safety.)
        assert!(is_safe_path("foo..bar"));
    }

    #[test]
    fn is_safe_path_rejects_empty_after_traversal_normalization() {
        // Edge: a single `..` is a single ParentDir component.
        assert!(!is_safe_path(".."));
    }
}
