//! Recursive `.claude/` directory discovery for the migrate command.
//!
//! Uses the `ignore` crate for gitignore-aware directory traversal.

use std::path::{Path, PathBuf};

use super::Error;

/// A discovered `.claude/` directory and its package context.
#[derive(Debug, Clone)]
pub struct DiscoveredSource {
    /// Absolute path to the `.claude/` directory.
    pub claude_dir: PathBuf,
    /// The package name derived from the parent directory.
    /// `None` if the `.claude/` dir is at the project root.
    pub package_name: Option<String>,
    /// Relative path from project root to the parent of `.claude/`.
    /// Empty for root-level `.claude/`.
    pub relative_path: PathBuf,
}

/// Walk the project tree and find all `.claude/` directories.
///
/// Uses the `ignore` crate for gitignore-aware traversal.
/// Skips the `.ai/` directory itself to avoid scanning marketplace plugins.
///
/// # Arguments
/// * `project_root` — The project root directory to scan from
/// * `max_depth` — Optional maximum traversal depth (`None` = unlimited)
///
/// # Returns
/// A sorted `Vec<DiscoveredSource>` (sorted by path for deterministic output).
pub fn discover_claude_dirs(
    project_root: &Path,
    max_depth: Option<usize>,
) -> Result<Vec<DiscoveredSource>, Error> {
    let mut builder = ignore::WalkBuilder::new(project_root);
    builder.hidden(false); // Must find .claude/ which is a hidden dir
    builder.git_ignore(true);
    builder.git_global(true);
    builder.git_exclude(true);

    if let Some(depth) = max_depth {
        builder.max_depth(Some(depth));
    }

    // Filter out .ai/ directory to avoid scanning marketplace plugins
    builder.filter_entry(|entry| {
        let file_name = entry.file_name().to_string_lossy();
        // Skip .ai/ and .git/ directories
        if entry.file_type().is_some_and(|ft| ft.is_dir()) && file_name == ".ai" {
            return false;
        }
        true
    });

    let mut discovered = Vec::new();

    for result in builder.build() {
        let entry = result.map_err(|e| Error::DiscoveryFailed(e.to_string()))?;

        // Only interested in directories named ".claude"
        let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
        if !is_dir {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy();
        if file_name != ".claude" {
            continue;
        }

        let claude_dir = entry.path().to_path_buf();

        // Derive package name and relative path
        let relative_to_root = claude_dir.strip_prefix(project_root).unwrap_or(&claude_dir);

        // relative_to_root is like ".claude" (root) or "packages/auth/.claude" (nested)
        let parent_of_claude = relative_to_root.parent().unwrap_or_else(|| Path::new(""));
        let relative_path = parent_of_claude.to_path_buf();

        let package_name = if parent_of_claude.as_os_str().is_empty() {
            // Root-level .claude/
            None
        } else {
            // Nested: use the immediate parent directory name as the package name
            parent_of_claude.file_name().map(|n| n.to_string_lossy().into_owned())
        };

        discovered.push(DiscoveredSource { claude_dir, package_name, relative_path });
    }

    // Sort by path for deterministic ordering
    discovered.sort_by(|a, b| a.claude_dir.cmp(&b.claude_dir));

    Ok(discovered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_finds_root_claude_dir() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(Path::new("."));

        // Create .claude/ at root
        let claude_dir = root.join(".claude");
        assert!(std::fs::create_dir_all(&claude_dir).is_ok());
        assert!(std::fs::write(claude_dir.join("settings.json"), "{}").is_ok());

        let result = discover_claude_dirs(root, None);
        assert!(result.is_ok());
        let sources = result.ok().unwrap_or_default();
        assert_eq!(sources.len(), 1);
        assert!(sources.first().is_some_and(|s| s.package_name.is_none()));
    }

    #[test]
    fn discover_finds_nested_claude_dirs() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(Path::new("."));

        // Create nested .claude/ dirs
        let auth_claude = root.join("packages").join("auth").join(".claude");
        let api_claude = root.join("packages").join("api").join(".claude");
        assert!(std::fs::create_dir_all(&auth_claude).is_ok());
        assert!(std::fs::create_dir_all(&api_claude).is_ok());

        let result = discover_claude_dirs(root, None);
        assert!(result.is_ok());
        let sources = result.ok().unwrap_or_default();
        assert_eq!(sources.len(), 2);

        // Should have package names
        let names: Vec<_> = sources.iter().filter_map(|s| s.package_name.as_deref()).collect();
        assert!(names.contains(&"api"));
        assert!(names.contains(&"auth"));
    }

    #[test]
    fn discover_assigns_correct_package_name() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(Path::new("."));

        // Deeply nested: a/b/c/mypackage/.claude
        let deep = root.join("a").join("b").join("c").join("mypackage").join(".claude");
        assert!(std::fs::create_dir_all(&deep).is_ok());

        let result = discover_claude_dirs(root, None);
        assert!(result.is_ok());
        let sources = result.ok().unwrap_or_default();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources.first().and_then(|s| s.package_name.as_deref()), Some("mypackage"));
    }

    #[test]
    fn discover_returns_none_package_for_root() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(Path::new("."));

        assert!(std::fs::create_dir_all(root.join(".claude")).is_ok());

        let result = discover_claude_dirs(root, None);
        assert!(result.is_ok());
        let sources = result.ok().unwrap_or_default();
        assert!(sources.first().is_some_and(|s| s.package_name.is_none()));
        assert!(sources.first().is_some_and(|s| s.relative_path.as_os_str().is_empty()));
    }

    #[test]
    fn discover_respects_max_depth() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(Path::new("."));

        // Root .claude at depth 1
        assert!(std::fs::create_dir_all(root.join(".claude")).is_ok());
        // Nested .claude at depth 3 (packages/auth/.claude)
        assert!(std::fs::create_dir_all(root.join("packages").join("auth").join(".claude")).is_ok());

        // max_depth=1 should only find root .claude
        let result = discover_claude_dirs(root, Some(1));
        assert!(result.is_ok());
        let sources = result.ok().unwrap_or_default();
        assert_eq!(sources.len(), 1);
        assert!(sources.first().is_some_and(|s| s.package_name.is_none()));
    }

    #[test]
    fn discover_excludes_ai_directory() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(Path::new("."));

        // .claude in .ai/ should be excluded
        assert!(std::fs::create_dir_all(root.join(".ai").join("starter").join(".claude")).is_ok());
        // Normal .claude should be found
        assert!(std::fs::create_dir_all(root.join(".claude")).is_ok());

        let result = discover_claude_dirs(root, None);
        assert!(result.is_ok());
        let sources = result.ok().unwrap_or_default();
        assert_eq!(sources.len(), 1);
        assert!(sources.first().is_some_and(|s| s.package_name.is_none()));
    }

    #[test]
    fn discover_returns_empty_when_no_claude_dirs() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(Path::new("."));

        // No .claude/ directories at all
        assert!(std::fs::create_dir_all(root.join("src")).is_ok());

        let result = discover_claude_dirs(root, None);
        assert!(result.is_ok());
        assert!(result.ok().unwrap_or_default().is_empty());
    }

    #[test]
    fn discover_returns_sorted_results() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(Path::new("."));

        assert!(
            std::fs::create_dir_all(root.join("packages").join("zebra").join(".claude")).is_ok()
        );
        assert!(
            std::fs::create_dir_all(root.join("packages").join("alpha").join(".claude")).is_ok()
        );
        assert!(std::fs::create_dir_all(root.join(".claude")).is_ok());

        let result = discover_claude_dirs(root, None);
        assert!(result.is_ok());
        let sources = result.ok().unwrap_or_default();
        assert_eq!(sources.len(), 3);

        // Verify sorted by path
        for i in 0..sources.len() - 1 {
            assert!(
                sources.get(i).map(|s| &s.claude_dir) <= sources.get(i + 1).map(|s| &s.claude_dir)
            );
        }
    }

    #[test]
    fn discover_with_gitignore_skips_ignored() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(Path::new("."));

        // Initialize a git repo so .gitignore is respected by the ignore crate
        assert!(std::fs::create_dir_all(root.join(".git")).is_ok());
        // Create .gitignore that ignores node_modules
        assert!(std::fs::write(root.join(".gitignore"), "node_modules/\n").is_ok());
        // Create .claude inside node_modules (should be skipped)
        assert!(
            std::fs::create_dir_all(root.join("node_modules").join("pkg").join(".claude")).is_ok()
        );
        // Create normal .claude (should be found)
        assert!(std::fs::create_dir_all(root.join(".claude")).is_ok());

        let result = discover_claude_dirs(root, None);
        assert!(result.is_ok());
        let sources = result.ok().unwrap_or_default();
        assert_eq!(sources.len(), 1);
        assert!(sources.first().is_some_and(|s| s.package_name.is_none()));
    }
}
