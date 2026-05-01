//! Filesystem walker for the unified discovery module.
//!
//! Wraps `ignore::WalkBuilder` with the project's standard configuration:
//! - Hidden files **are** descended into (so `.claude/`, `.github/`, `.ai/`
//!   are visible).
//! - Gitignore files (project, global, exclude) are honored.
//! - Symlinks are not followed by default.
//! - Directories whose name appears in [`SKIP_DIRS`] are pruned and recorded
//!   as [`SkipReason::SkipDirByName`].
//!
//! The walker does NOT classify files — it returns paths and skip reasons
//! that the higher-level `discover()` (added in a later spec feature) feeds
//! into `classify::classify`. Source filtering is also applied
//! post-classification, not in the walker.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::discovery_legacy::Error;

use super::scan_report::SkipReason;
use super::DiscoverOptions;

/// Directory names that are pruned during the walk because they cannot
/// contain AI plugin features.
///
/// Mirrors the legacy `discovery_legacy::SKIP_DIRS` constant verbatim.
pub const SKIP_DIRS: &[&str] =
    &["node_modules", "target", ".git", "vendor", "__pycache__", "dist", "build"];

/// The result of a single walk.
#[derive(Debug, Default, Clone)]
pub struct WalkResult {
    /// All file paths the walker visited (after `SKIP_DIRS` pruning, after
    /// gitignore, and within the configured `max_depth`).
    pub files: Vec<PathBuf>,
    /// All directories the walker descended into.
    pub scanned_dirs: Vec<PathBuf>,
    /// Skip reasons recorded during the walk.
    pub skipped: Vec<SkipReason>,
}

/// Walk `project_root` according to `opts` and return the discovered file
/// paths, directories visited, and skip reasons.
///
/// Errors from the underlying `ignore::Walk` iterator are converted to
/// [`Error::WalkFailed`] with the error's `Display` text.
///
/// # Errors
///
/// Returns `Error::WalkFailed` if the underlying walker yields an I/O or
/// gitignore-parse failure. Skip-list pruning is not an error.
pub fn walk(project_root: &Path, opts: &DiscoverOptions) -> Result<WalkResult, Error> {
    let mut builder = ignore::WalkBuilder::new(project_root);
    builder.hidden(false); // Must descend into .claude/, .github/, .ai/.
    builder.git_ignore(true);
    builder.git_global(true);
    builder.git_exclude(true);
    builder.follow_links(opts.follow_symlinks);

    if let Some(depth) = opts.max_depth {
        builder.max_depth(Some(depth));
    }

    let skipped: Arc<Mutex<Vec<SkipReason>>> = Arc::new(Mutex::new(Vec::new()));
    let skipped_for_filter = Arc::clone(&skipped);

    builder.filter_entry(move |entry| {
        if !entry.file_type().is_some_and(|ft| ft.is_dir()) {
            return true;
        }
        let name = entry.file_name().to_string_lossy();
        if let Some(skip_name) = SKIP_DIRS.iter().find(|&&s| name == s) {
            tracing::trace!(dir = %entry.path().display(), reason = "skip-list", "skipping directory");
            if let Ok(mut guard) = skipped_for_filter.lock() {
                guard.push(SkipReason::SkipDirByName {
                    path: entry.path().to_path_buf(),
                    name: (*skip_name).to_string(),
                });
            }
            return false;
        }
        true
    });

    let mut files = Vec::new();
    let mut scanned_dirs = Vec::new();

    for result in builder.build() {
        let entry = result.map_err(|e| Error::WalkFailed(e.to_string()))?;
        let path = entry.path().to_path_buf();
        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
            tracing::trace!(dir = %path.display(), "entering directory");
            scanned_dirs.push(path);
        } else {
            files.push(path);
        }
    }

    // Sort for deterministic output ordering.
    files.sort();
    scanned_dirs.sort();

    let skipped = take_skipped(&skipped);

    tracing::trace!(
        files = files.len(),
        dirs = scanned_dirs.len(),
        skipped = skipped.len(),
        "walk complete"
    );

    Ok(WalkResult { files, scanned_dirs, skipped })
}

/// Drain the skipped vec out of the shared `Arc<Mutex<...>>`. Falls back to
/// a clone if the Arc still has outstanding references (which shouldn't
/// happen — the closure was dropped when `builder` went out of scope — but
/// the fallback keeps the function infallible).
fn take_skipped(shared: &Arc<Mutex<Vec<SkipReason>>>) -> Vec<SkipReason> {
    shared.lock().map(|g| g.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, "").expect("touch file");
    }

    #[test]
    fn empty_dir_returns_empty_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let result = walk(root, &DiscoverOptions::default()).expect("walk should succeed");
        // The root itself is visited as a directory.
        assert!(result.files.is_empty(), "no files should be found in empty dir");
    }

    #[test]
    fn walk_finds_files_and_descends_dirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join("a.txt"));
        touch(&root.join("subdir/b.md"));
        let result = walk(root, &DiscoverOptions::default()).expect("walk should succeed");
        assert!(result.files.iter().any(|p| p.ends_with("a.txt")));
        assert!(result.files.iter().any(|p| p.ends_with("b.md")));
        assert!(result.scanned_dirs.iter().any(|p| p.ends_with("subdir")));
    }

    #[test]
    fn skip_dirs_pruned_and_recorded() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join("node_modules/foo/index.js"));
        touch(&root.join("target/debug/build.log"));
        touch(&root.join("src/main.rs"));
        let result = walk(root, &DiscoverOptions::default()).expect("walk should succeed");
        // node_modules and target should be pruned; main.rs visible.
        assert!(result.files.iter().any(|p| p.ends_with("main.rs")));
        assert!(!result.files.iter().any(|p| p.to_string_lossy().contains("node_modules")));
        assert!(!result.files.iter().any(|p| p.to_string_lossy().contains("target")));
        // SkipReason recorded for both pruned dirs.
        let names: Vec<&str> = result
            .skipped
            .iter()
            .map(|r| match r {
                SkipReason::SkipDirByName { name, .. } => name.as_str(),
            })
            .collect();
        assert!(names.contains(&"node_modules"), "expected node_modules in skipped: {names:?}");
        assert!(names.contains(&"target"), "expected target in skipped: {names:?}");
    }

    #[test]
    fn max_depth_honored() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join("level1.md"));
        touch(&root.join("a/level2.md"));
        touch(&root.join("a/b/level3.md"));
        let opts = DiscoverOptions { max_depth: Some(1), ..DiscoverOptions::default() };
        let result = walk(root, &opts).expect("walk should succeed");
        assert!(result.files.iter().any(|p| p.ends_with("level1.md")));
        // depth=1 means root + 1 layer; level3.md (depth 3) must be excluded.
        assert!(!result.files.iter().any(|p| p.ends_with("level3.md")));
    }

    #[test]
    fn finds_dot_directories() {
        // The walker must descend into hidden dirs (.claude, .github, .ai).
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join(".claude/skills/x/SKILL.md"));
        touch(&root.join(".github/copilot/skills/y/SKILL.md"));
        touch(&root.join(".ai/.claude-plugin/marketplace.json"));
        let result = walk(root, &DiscoverOptions::default()).expect("walk should succeed");
        assert!(result.files.iter().any(|p| p.ends_with(".claude/skills/x/SKILL.md")));
        assert!(result.files.iter().any(|p| p.ends_with(".github/copilot/skills/y/SKILL.md")));
        assert!(result.files.iter().any(|p| p.ends_with(".ai/.claude-plugin/marketplace.json")));
    }

    #[test]
    fn issue_725_tree_visible_to_walker() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        for name in ["skill-alpha", "skill-beta", "skill-gamma"] {
            touch(&root.join(format!(".github/copilot/skills/{name}/SKILL.md")));
        }
        touch(&root.join(".github/copilot/copilot-instructions.md"));
        let result = walk(root, &DiscoverOptions::default()).expect("walk should succeed");
        // All four files must be in the walker's output (classification happens later).
        assert!(result.files.iter().any(|p| p.ends_with("skill-alpha/SKILL.md")));
        assert!(result.files.iter().any(|p| p.ends_with("skill-beta/SKILL.md")));
        assert!(result.files.iter().any(|p| p.ends_with("skill-gamma/SKILL.md")));
        assert!(result.files.iter().any(|p| p.ends_with("copilot-instructions.md")));
    }

    #[test]
    fn gitignored_files_excluded() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        // The `ignore` crate only honors `.gitignore` when it can find a `.git/`
        // directory in an ancestor — create an empty one. (`.git` is also in
        // SKIP_DIRS so it won't be descended into.)
        fs::create_dir_all(root.join(".git")).expect("create .git dir");
        fs::write(root.join(".gitignore"), "ignored.md\n").expect("write gitignore");
        touch(&root.join("kept.md"));
        touch(&root.join("ignored.md"));
        let result = walk(root, &DiscoverOptions::default()).expect("walk should succeed");
        assert!(result.files.iter().any(|p| p.ends_with("kept.md")));
        assert!(
            !result.files.iter().any(|p| p.ends_with("ignored.md")),
            "gitignored file leaked into results: {:?}",
            result.files
        );
    }

    #[test]
    fn follow_symlinks_default_off() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join("real/SKILL.md"));
        // Make a symlink dir 'link' -> 'real'. On Windows this might fail without
        // privileges; skip the test gracefully there.
        #[cfg(unix)]
        {
            let link = root.join("link");
            std::os::unix::fs::symlink(root.join("real"), &link).expect("create symlink");
            let result = walk(root, &DiscoverOptions::default()).expect("walk should succeed");
            // The real path is visited but the symlink is not followed.
            assert!(result.files.iter().any(|p| p.ends_with("real/SKILL.md")));
            // No file path under the link dir should appear.
            assert!(!result.files.iter().any(|p| p.to_string_lossy().contains("/link/")));
        }
        #[cfg(not(unix))]
        {
            let _ = root;
        }
    }

    #[test]
    fn skip_reason_records_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join("node_modules/dummy.js"));
        let result = walk(root, &DiscoverOptions::default()).expect("walk should succeed");
        let recorded = result
            .skipped
            .iter()
            .map(|r| match r {
                SkipReason::SkipDirByName { path, .. } => path.clone(),
            })
            .next()
            .expect("at least one SkipDirByName record");
        assert!(recorded.ends_with("node_modules"));
    }

    #[test]
    fn discover_options_default_walks_full_tree() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        touch(&root.join("a/b/c/deep.md"));
        let result = walk(root, &DiscoverOptions::default()).expect("walk should succeed");
        // No max_depth → deep.md should be visible.
        assert!(result.files.iter().any(|p| p.ends_with("deep.md")));
    }

    #[test]
    fn results_are_sorted() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        for n in ["c.md", "a.md", "b.md"] {
            touch(&root.join(n));
        }
        let result = walk(root, &DiscoverOptions::default()).expect("walk should succeed");
        let mut sorted = result.files.clone();
        sorted.sort();
        assert_eq!(result.files, sorted);
    }
}
