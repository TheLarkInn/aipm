//! Legacy directory enumeration helpers used by the migrate hybrid.
//!
//! `discovery::discover` is the unified file-classification path; this module
//! still owns the per-source-directory enumeration helpers used by
//! `migrate::unified::run` to drive the deferred-kind legacy detectors and
//! to identify package-scoped sources.
//!
//! The previous file-classification walker (`discover_features`) and the
//! legacy `DiscoveredFeature` / `SourceContext` shapes were removed when the
//! `legacy_compat` adapter went away.

use std::path::{Path, PathBuf};

/// Errors from directory discovery.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A walk entry produced an I/O error.
    #[error("discovery walk failed: {0}")]
    WalkFailed(String),
}

/// A discovered source directory and its package context.
#[derive(Debug, Clone)]
pub struct DiscoveredSource {
    /// Absolute path to the source directory (e.g., `.claude/` or `.github/`).
    pub source_dir: PathBuf,
    /// Which source type this is (e.g., ".claude", ".github").
    pub source_type: String,
    /// The package name derived from the parent directory.
    /// `None` if the source dir is at the project root.
    pub package_name: Option<String>,
    /// Relative path from project root to the parent of the source dir.
    /// Empty for root-level source dirs.
    pub relative_path: PathBuf,
}

// Backwards-compatible accessor still referenced by `dry_run.rs`.
impl DiscoveredSource {
    /// Alias for `source_dir` — backwards compatibility.
    pub fn claude_dir(&self) -> &Path {
        &self.source_dir
    }
}

/// Walk the project tree and find all `.claude/` directories.
///
/// Delegates to `discover_source_dirs` with `[".claude"]` patterns.
pub fn discover_claude_dirs(
    project_root: &Path,
    max_depth: Option<usize>,
) -> Result<Vec<DiscoveredSource>, Error> {
    discover_source_dirs(project_root, &[".claude"], max_depth)
}

/// Walk the project tree and find all source directories matching the given patterns.
///
/// Uses the `ignore` crate for gitignore-aware traversal.
/// Skips the `.ai/` directory itself to avoid scanning marketplace plugins.
///
/// # Arguments
/// * `project_root` — The project root directory to scan from
/// * `patterns` — Directory name patterns to match (e.g., `&[".claude", ".github"]`)
/// * `max_depth` — Optional maximum traversal depth (`None` = unlimited)
///
/// # Returns
/// A sorted `Vec<DiscoveredSource>` (sorted by path for deterministic output).
pub fn discover_source_dirs(
    project_root: &Path,
    patterns: &[&str],
    max_depth: Option<usize>,
) -> Result<Vec<DiscoveredSource>, Error> {
    let mut builder = ignore::WalkBuilder::new(project_root);
    builder.hidden(false); // Must find hidden dirs like .claude/ and .github/
    builder.git_ignore(true);
    builder.git_global(true);
    builder.git_exclude(true);

    if let Some(depth) = max_depth {
        builder.max_depth(Some(depth));
    }

    // Filter out .ai/ directory to avoid scanning marketplace plugins
    builder.filter_entry(|entry| {
        let file_name = entry.file_name().to_string_lossy();
        if entry.file_type().is_some_and(|ft| ft.is_dir()) && file_name == ".ai" {
            return false;
        }
        true
    });

    let mut discovered = Vec::new();

    tracing::trace!("starting source directory discovery");

    for result in builder.build() {
        let entry = result.map_err(|e| Error::WalkFailed(e.to_string()))?;

        let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
        if !is_dir {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy();

        // Check if this directory matches any of the patterns
        let matched_pattern = patterns.iter().find(|&&p| file_name == p);
        let Some(&source_type_str) = matched_pattern else {
            continue;
        };

        let source_dir = entry.path().to_path_buf();
        tracing::trace!(dir = %source_dir.display(), source_type = source_type_str, "discovered source directory");

        // Derive package name and relative path
        let relative_to_root = source_dir.strip_prefix(project_root).unwrap_or(&source_dir);
        let parent_of_source = relative_to_root.parent().unwrap_or_else(|| Path::new(""));
        let relative_path = parent_of_source.to_path_buf();

        let package_name = if parent_of_source.as_os_str().is_empty() {
            None
        } else {
            parent_of_source.file_name().map(|n| n.to_string_lossy().into_owned())
        };

        discovered.push(DiscoveredSource {
            source_dir,
            source_type: source_type_str.to_string(),
            package_name,
            relative_path,
        });
    }

    tracing::trace!(total = discovered.len(), "source directory discovery complete");

    // Sort by path for deterministic ordering
    discovered.sort_by(|a, b| a.source_dir.cmp(&b.source_dir));

    Ok(discovered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_finds_root_claude_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        let claude_dir = root.join(".claude");
        std::fs::create_dir_all(&claude_dir).expect("create .claude");
        std::fs::write(claude_dir.join("settings.json"), "{}").expect("write settings.json");

        let sources = discover_claude_dirs(root, None).expect("ok");
        assert_eq!(sources.len(), 1);
        assert!(sources.first().is_some_and(|s| s.package_name.is_none()));
    }

    #[test]
    fn discover_finds_nested_claude_dirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        let auth_claude = root.join("packages").join("auth").join(".claude");
        let api_claude = root.join("packages").join("api").join(".claude");
        std::fs::create_dir_all(&auth_claude).expect("create");
        std::fs::create_dir_all(&api_claude).expect("create");

        let sources = discover_claude_dirs(root, None).expect("ok");
        assert_eq!(sources.len(), 2);
        let names: Vec<_> = sources.iter().filter_map(|s| s.package_name.as_deref()).collect();
        assert!(names.contains(&"api"));
        assert!(names.contains(&"auth"));
    }

    #[test]
    fn discover_assigns_correct_package_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        let deep = root.join("a").join("b").join("c").join("mypackage").join(".claude");
        std::fs::create_dir_all(&deep).expect("create");

        let sources = discover_claude_dirs(root, None).expect("ok");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources.first().and_then(|s| s.package_name.as_deref()), Some("mypackage"));
    }

    #[test]
    fn discover_returns_none_package_for_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        std::fs::create_dir_all(root.join(".claude")).expect("create");

        let sources = discover_claude_dirs(root, None).expect("ok");
        assert!(sources.first().is_some_and(|s| s.package_name.is_none()));
        assert!(sources.first().is_some_and(|s| s.relative_path.as_os_str().is_empty()));
    }

    #[test]
    fn discover_respects_max_depth() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        std::fs::create_dir_all(root.join(".claude")).expect("create root .claude");
        std::fs::create_dir_all(root.join("packages").join("auth").join(".claude"))
            .expect("create nested .claude");

        // max_depth=1 should only find root .claude
        let sources = discover_claude_dirs(root, Some(1)).expect("ok");
        assert_eq!(sources.len(), 1);
        assert!(sources.first().is_some_and(|s| s.package_name.is_none()));
    }

    #[test]
    fn discover_excludes_ai_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        std::fs::create_dir_all(root.join(".ai").join("starter").join(".claude"))
            .expect("create .ai/starter/.claude");
        std::fs::create_dir_all(root.join(".claude")).expect("create .claude");

        let sources = discover_claude_dirs(root, None).expect("ok");
        assert_eq!(sources.len(), 1);
        assert!(sources.first().is_some_and(|s| s.package_name.is_none()));
    }

    #[test]
    fn discover_returns_empty_when_no_claude_dirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        std::fs::create_dir_all(root.join("src")).expect("create src");

        let sources = discover_claude_dirs(root, None).expect("ok");
        assert!(sources.is_empty());
    }

    #[test]
    fn discover_returns_sorted_results() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        std::fs::create_dir_all(root.join("packages").join("zebra").join(".claude"))
            .expect("zebra");
        std::fs::create_dir_all(root.join("packages").join("alpha").join(".claude"))
            .expect("alpha");
        std::fs::create_dir_all(root.join(".claude")).expect("root");

        let sources = discover_claude_dirs(root, None).expect("ok");
        assert_eq!(sources.len(), 3);
        for i in 0..sources.len() - 1 {
            assert!(
                sources.get(i).map(|s| &s.source_dir) <= sources.get(i + 1).map(|s| &s.source_dir)
            );
        }
    }

    #[test]
    fn discover_with_gitignore_skips_ignored() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        std::fs::create_dir_all(root.join(".git")).expect(".git");
        std::fs::write(root.join(".gitignore"), "node_modules/\n").expect(".gitignore");
        std::fs::create_dir_all(root.join("node_modules").join("pkg").join(".claude"))
            .expect("nm/pkg/.claude");
        std::fs::create_dir_all(root.join(".claude")).expect("root .claude");

        let sources = discover_claude_dirs(root, None).expect("ok");
        assert_eq!(sources.len(), 1);
        assert!(sources.first().is_some_and(|s| s.package_name.is_none()));
    }

    #[test]
    fn discover_source_dirs_finds_both_claude_and_github() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        std::fs::create_dir_all(root.join(".claude")).expect(".claude");
        std::fs::create_dir_all(root.join(".github")).expect(".github");

        let sources = discover_source_dirs(root, &[".claude", ".github"], None).expect("ok");
        assert_eq!(sources.len(), 2);
        let types: Vec<&str> = sources.iter().map(|s| s.source_type.as_str()).collect();
        assert!(types.contains(&".claude"));
        assert!(types.contains(&".github"));
    }

    #[test]
    fn discover_source_dirs_sets_correct_source_type() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        std::fs::create_dir_all(root.join("packages").join("auth").join(".github"))
            .expect("auth/.github");

        let sources = discover_source_dirs(root, &[".github"], None).expect("ok");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources.first().map(|s| s.source_type.as_str()), Some(".github"));
        assert_eq!(sources.first().and_then(|s| s.package_name.as_deref()), Some("auth"));
    }

    #[test]
    fn discover_source_dirs_root_github_has_none_package_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        std::fs::create_dir_all(root.join(".github")).expect(".github");

        let sources = discover_source_dirs(root, &[".github"], None).expect("ok");
        assert_eq!(sources.len(), 1);
        assert!(sources.first().is_some_and(|s| s.package_name.is_none()));
    }

    #[test]
    fn error_display() {
        let err = Error::WalkFailed("permission denied".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("discovery walk failed"));
        assert!(msg.contains("permission denied"));
    }

    #[test]
    fn discovered_source_claude_dir_alias_matches_source_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        std::fs::create_dir_all(root.join(".claude")).expect("create .claude");

        let sources = discover_claude_dirs(root, None).expect("ok");
        for s in &sources {
            assert_eq!(s.claude_dir(), s.source_dir.as_path());
        }
        assert!(!sources.is_empty(), "expected at least one source");
    }
}
