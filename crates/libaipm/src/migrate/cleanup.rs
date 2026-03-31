//! Post-migration source file cleanup.
//!
//! Removes successfully-migrated source files from `.claude/` directories
//! and prunes any resulting empty parent directories.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{Action, Outcome};
use crate::fs::Fs;

/// File names that are never deleted because they contain shared configuration.
const SKIP_FILENAMES: &[&str] = &["settings.json", ".mcp.json"];

/// Returns `true` if a source path should be skipped during cleanup.
///
/// Used internally and by the dry-run report to identify shared config files.
pub fn should_skip_for_report(path: &Path) -> bool {
    should_skip(path)
}

/// Returns `true` if a source path should be skipped during cleanup.
fn should_skip(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|name| SKIP_FILENAMES.contains(&name))
}

/// Remove successfully-migrated source files and prune empty parent directories.
///
/// Only paths from `PluginCreated` actions are candidates for removal.
/// Paths whose file name matches [`SKIP_FILENAMES`] (e.g., `settings.json`,
/// `.mcp.json`) are excluded because they may contain unrelated configuration.
///
/// After file/directory removal, empty parent directories are pruned bottom-up
/// (deepest first).
pub fn remove_migrated_sources(
    outcome: &Outcome,
    fs: &dyn Fs,
) -> Result<Vec<Action>, std::io::Error> {
    let mut actions = Vec::new();
    let mut dirs_to_check: BTreeSet<PathBuf> = BTreeSet::new();

    for (source_path, is_dir) in outcome.migrated_sources() {
        if should_skip(source_path) {
            continue;
        }

        if is_dir {
            fs.remove_dir_all(source_path)?;
            actions.push(Action::SourceDirRemoved { path: source_path.to_path_buf() });
        } else {
            fs.remove_file(source_path)?;
            actions.push(Action::SourceFileRemoved { path: source_path.to_path_buf() });
        }

        if let Some(parent) = source_path.parent() {
            dirs_to_check.insert(parent.to_path_buf());
        }
    }

    // Prune empty parent directories, deepest first.
    let mut sorted_dirs: Vec<PathBuf> = dirs_to_check.into_iter().collect();
    sorted_dirs.sort_by_key(|p| std::cmp::Reverse(p.components().count()));

    for dir in sorted_dirs {
        if let Ok(entries) = fs.read_dir(&dir) {
            if entries.is_empty() {
                fs.remove_dir_all(&dir)?;
                actions.push(Action::EmptyDirPruned { path: dir });
            }
        }
    }

    Ok(actions)
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    use super::*;
    use crate::fs::DirEntry;

    /// Mock filesystem that tracks removals and can simulate directory/file state.
    struct MockFs {
        /// Paths that exist as directories (read_dir returns their entries).
        dirs: HashMap<PathBuf, Vec<DirEntry>>,
        /// Paths that exist as files (read_dir returns NotFound for these).
        files: HashSet<PathBuf>,
        /// Tracks calls to remove_file.
        removed_files: Mutex<Vec<PathBuf>>,
        /// Tracks calls to remove_dir_all.
        removed_dirs: Mutex<Vec<PathBuf>>,
        /// If set, remove_file returns this error for matching paths.
        fail_remove_file: Mutex<Option<PathBuf>>,
        /// If set, remove_dir_all returns a PermissionDenied error for matching paths.
        fail_remove_dir_all: Mutex<Option<PathBuf>>,
    }

    impl MockFs {
        fn new() -> Self {
            Self {
                dirs: HashMap::new(),
                files: HashSet::new(),
                removed_files: Mutex::new(Vec::new()),
                removed_dirs: Mutex::new(Vec::new()),
                fail_remove_file: Mutex::new(None),
                fail_remove_dir_all: Mutex::new(None),
            }
        }
    }

    impl Fs for MockFs {
        fn exists(&self, path: &Path) -> bool {
            self.dirs.contains_key(path) || self.files.contains(path)
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Ok(())
        }

        fn write_file(&self, _: &Path, _: &[u8]) -> std::io::Result<()> {
            Ok(())
        }

        fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "not found"))
        }

        fn read_dir(&self, path: &Path) -> std::io::Result<Vec<DirEntry>> {
            self.dirs
                .get(path)
                .cloned()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "not a directory"))
        }

        fn remove_file(&self, path: &Path) -> std::io::Result<()> {
            let fail =
                self.fail_remove_file.lock().map_err(|_| std::io::Error::other("lock poisoned"))?;
            if let Some(ref fail_path) = *fail {
                if path == fail_path {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "permission denied",
                    ));
                }
            }
            self.removed_files
                .lock()
                .map_err(|_| std::io::Error::other("lock poisoned"))?
                .push(path.to_path_buf());
            Ok(())
        }

        fn remove_dir_all(&self, path: &Path) -> std::io::Result<()> {
            let fail = self
                .fail_remove_dir_all
                .lock()
                .map_err(|_| std::io::Error::other("lock poisoned"))?;
            if let Some(ref fail_path) = *fail {
                if path == fail_path {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "permission denied",
                    ));
                }
            }
            self.removed_dirs
                .lock()
                .map_err(|_| std::io::Error::other("lock poisoned"))?
                .push(path.to_path_buf());
            Ok(())
        }
    }

    fn make_outcome(actions: Vec<Action>) -> Outcome {
        Outcome { actions }
    }

    fn plugin_created(name: &str, source: &str, plugin_type: &str, source_is_dir: bool) -> Action {
        Action::PluginCreated {
            name: name.to_string(),
            source: PathBuf::from(source),
            plugin_type: plugin_type.to_string(),
            source_is_dir,
        }
    }

    #[test]
    fn empty_outcome_produces_no_actions() {
        let fs = MockFs::new();
        let outcome = make_outcome(Vec::new());
        let result = remove_migrated_sources(&outcome, &fs);
        assert!(result.is_ok());
        assert!(result.ok().is_some_and(|a| a.is_empty()));
    }

    #[test]
    fn skill_directory_is_removed_via_remove_dir_all() {
        let mut fs = MockFs::new();
        // The skill source is a directory
        fs.dirs.insert(
            PathBuf::from("/p/.claude/skills/deploy"),
            vec![DirEntry { name: "SKILL.md".to_string(), is_dir: false }],
        );
        // Parent dir still has content after removal (so it won't be pruned)
        fs.dirs.insert(
            PathBuf::from("/p/.claude/skills"),
            vec![DirEntry { name: "other-skill".to_string(), is_dir: true }],
        );

        let outcome =
            make_outcome(vec![plugin_created("deploy", "/p/.claude/skills/deploy", "skill", true)]);

        let result = remove_migrated_sources(&outcome, &fs);
        assert!(result.is_ok());
        let actions = result.ok().unwrap_or_default();
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], Action::SourceDirRemoved { path } if path == Path::new("/p/.claude/skills/deploy"))
        );

        let removed_dirs = fs.removed_dirs.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(removed_dirs.len(), 1);
        assert_eq!(removed_dirs[0], PathBuf::from("/p/.claude/skills/deploy"));
    }

    #[test]
    fn command_file_is_removed_via_remove_file() {
        let mut fs = MockFs::new();
        // The command source is a file (read_dir returns NotFound)
        fs.files.insert(PathBuf::from("/p/.claude/commands/review.md"));
        // Parent dir still has content
        fs.dirs.insert(
            PathBuf::from("/p/.claude/commands"),
            vec![DirEntry { name: "other.md".to_string(), is_dir: false }],
        );

        let outcome = make_outcome(vec![plugin_created(
            "review",
            "/p/.claude/commands/review.md",
            "skill",
            false,
        )]);

        let result = remove_migrated_sources(&outcome, &fs);
        assert!(result.is_ok());
        let actions = result.ok().unwrap_or_default();
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], Action::SourceFileRemoved { path } if path == Path::new("/p/.claude/commands/review.md"))
        );
    }

    #[test]
    fn settings_json_is_skipped() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/p/.claude/settings.json"));

        let outcome = make_outcome(vec![plugin_created(
            "project-hooks",
            "/p/.claude/settings.json",
            "hook",
            false,
        )]);

        let result = remove_migrated_sources(&outcome, &fs);
        assert!(result.is_ok());
        assert!(result.ok().is_some_and(|a| a.is_empty()));
    }

    #[test]
    fn mcp_json_is_skipped() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/p/.mcp.json"));

        let outcome =
            make_outcome(vec![plugin_created("project-mcp-servers", "/p/.mcp.json", "mcp", false)]);

        let result = remove_migrated_sources(&outcome, &fs);
        assert!(result.is_ok());
        assert!(result.ok().is_some_and(|a| a.is_empty()));
    }

    #[test]
    fn empty_parent_directory_is_pruned() {
        let mut fs = MockFs::new();
        // Skill directory exists
        fs.dirs.insert(
            PathBuf::from("/p/.claude/skills/deploy"),
            vec![DirEntry { name: "SKILL.md".to_string(), is_dir: false }],
        );
        // Parent dir is empty after removal
        fs.dirs.insert(PathBuf::from("/p/.claude/skills"), Vec::new());

        let outcome =
            make_outcome(vec![plugin_created("deploy", "/p/.claude/skills/deploy", "skill", true)]);

        let result = remove_migrated_sources(&outcome, &fs);
        assert!(result.is_ok());
        let actions = result.ok().unwrap_or_default();
        // Should have: SourceDirRemoved for deploy + EmptyDirPruned for empty skills/
        assert_eq!(actions.len(), 2);
        assert!(
            matches!(&actions[0], Action::SourceDirRemoved { path } if path == Path::new("/p/.claude/skills/deploy"))
        );
        assert!(
            matches!(&actions[1], Action::EmptyDirPruned { path } if path == Path::new("/p/.claude/skills"))
        );
    }

    #[test]
    fn non_empty_parent_directory_is_not_pruned() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from("/p/.claude/skills/deploy"),
            vec![DirEntry { name: "SKILL.md".to_string(), is_dir: false }],
        );
        // Parent still has another skill
        fs.dirs.insert(
            PathBuf::from("/p/.claude/skills"),
            vec![DirEntry { name: "lint".to_string(), is_dir: true }],
        );

        let outcome =
            make_outcome(vec![plugin_created("deploy", "/p/.claude/skills/deploy", "skill", true)]);

        let result = remove_migrated_sources(&outcome, &fs);
        assert!(result.is_ok());
        let actions = result.ok().unwrap_or_default();
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], Action::SourceDirRemoved { path } if path == Path::new("/p/.claude/skills/deploy"))
        );
    }

    #[test]
    fn remove_file_error_propagates() {
        let mut fs = MockFs::new();
        fs.files.insert(PathBuf::from("/p/.claude/commands/review.md"));
        *fs.fail_remove_file.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(PathBuf::from("/p/.claude/commands/review.md"));

        let outcome = make_outcome(vec![plugin_created(
            "review",
            "/p/.claude/commands/review.md",
            "skill",
            false,
        )]);

        let result = remove_migrated_sources(&outcome, &fs);
        assert!(result.is_err());
        let err = result.err().unwrap_or_else(|| std::io::Error::other("unexpected"));
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn mixed_artifacts_with_skip() {
        let mut fs = MockFs::new();
        // Skill dir
        fs.dirs.insert(
            PathBuf::from("/p/.claude/skills/deploy"),
            vec![DirEntry { name: "SKILL.md".to_string(), is_dir: false }],
        );
        fs.dirs.insert(
            PathBuf::from("/p/.claude/skills"),
            vec![DirEntry { name: "deploy".to_string(), is_dir: true }],
        );
        // Command file
        fs.files.insert(PathBuf::from("/p/.claude/commands/review.md"));
        fs.dirs.insert(PathBuf::from("/p/.claude/commands"), Vec::new());
        // settings.json (should be skipped)
        fs.files.insert(PathBuf::from("/p/.claude/settings.json"));

        let outcome = make_outcome(vec![
            plugin_created("deploy", "/p/.claude/skills/deploy", "skill", true),
            plugin_created("review", "/p/.claude/commands/review.md", "skill", false),
            plugin_created("project-hooks", "/p/.claude/settings.json", "hook", false),
        ]);

        let result = remove_migrated_sources(&outcome, &fs);
        assert!(result.is_ok());
        let actions = result.ok().unwrap_or_default();

        // Should have:
        // 1. SourceDirRemoved for deploy dir
        // 2. SourceFileRemoved for review.md
        // 3. SourceDirRemoved for empty commands/
        // settings.json is skipped
        // skills/ still has deploy entry in mock (not actually removed), but
        // we check its mock state which still shows content → no prune
        let dir_removed =
            actions.iter().filter(|a| matches!(a, Action::SourceDirRemoved { .. })).count();
        let file_removed =
            actions.iter().filter(|a| matches!(a, Action::SourceFileRemoved { .. })).count();
        // At least 1 dir removed (deploy) + 1 file removed (review.md)
        assert!(dir_removed >= 1);
        assert!(file_removed >= 1);
        // settings.json should NOT appear in any action
        let has_settings = actions.iter().any(|a| match a {
            Action::SourceFileRemoved { path } | Action::SourceDirRemoved { path } => {
                path.file_name().is_some_and(|n| n == "settings.json")
            },
            _ => false,
        });
        assert!(!has_settings);
    }

    #[test]
    fn parent_dir_read_error_is_silently_skipped() {
        let mut fs = MockFs::new();
        // Skill dir exists
        fs.dirs.insert(
            PathBuf::from("/p/.claude/skills/deploy"),
            vec![DirEntry { name: "SKILL.md".to_string(), is_dir: false }],
        );
        // Parent dir NOT in dirs map → read_dir returns Err → pruning skips it

        let outcome =
            make_outcome(vec![plugin_created("deploy", "/p/.claude/skills/deploy", "skill", true)]);

        let result = remove_migrated_sources(&outcome, &fs);
        assert!(result.is_ok());
        let actions = result.ok().unwrap_or_default();
        // Only the skill dir itself is removed; parent is not pruned (read_dir fails)
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], Action::SourceDirRemoved { path } if path == Path::new("/p/.claude/skills/deploy"))
        );
    }

    #[test]
    fn should_skip_for_report_matches_settings_json() {
        assert!(super::should_skip_for_report(Path::new("/p/.claude/settings.json")));
    }

    #[test]
    fn should_skip_for_report_matches_mcp_json() {
        assert!(super::should_skip_for_report(Path::new("/p/.mcp.json")));
    }

    #[test]
    fn should_skip_for_report_does_not_match_regular_file() {
        assert!(!super::should_skip_for_report(Path::new("/p/.claude/skills/deploy/SKILL.md")));
    }

    #[test]
    fn should_skip_for_report_does_not_match_root_path() {
        assert!(!super::should_skip_for_report(Path::new("/")));
    }

    #[test]
    fn source_at_root_path_has_no_parent_dir_to_check() {
        // Path::new("/").parent() returns None on Unix; verify that remove_migrated_sources
        // handles this gracefully and does not attempt to queue a parent dir for pruning.
        let fs = MockFs::new();
        let outcome = make_outcome(vec![plugin_created("root-file", "/", "hook", false)]);
        let result = remove_migrated_sources(&outcome, &fs);
        assert!(result.is_ok());
        let actions = result.ok().unwrap_or_default();
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], Action::SourceFileRemoved { path } if path == Path::new("/"))
        );
    }

    #[test]
    fn remove_dir_all_error_on_source_dir_propagates() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from("/p/.claude/skills/deploy"),
            vec![DirEntry { name: "SKILL.md".to_string(), is_dir: false }],
        );
        *fs.fail_remove_dir_all.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(PathBuf::from("/p/.claude/skills/deploy"));

        let outcome =
            make_outcome(vec![plugin_created("deploy", "/p/.claude/skills/deploy", "skill", true)]);

        let result = remove_migrated_sources(&outcome, &fs);
        assert!(result.is_err());
        let err = result.err().unwrap_or_else(|| std::io::Error::other("unexpected"));
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn remove_dir_all_error_on_prune_propagates() {
        let mut fs = MockFs::new();
        // Skill dir exists as a directory
        fs.dirs.insert(
            PathBuf::from("/p/.claude/skills/deploy"),
            vec![DirEntry { name: "SKILL.md".to_string(), is_dir: false }],
        );
        // Parent dir is empty — pruning will try to remove_dir_all on it
        fs.dirs.insert(PathBuf::from("/p/.claude/skills"), Vec::new());
        // Fail only on the parent, so the source dir removal succeeds
        *fs.fail_remove_dir_all.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(PathBuf::from("/p/.claude/skills"));

        let outcome =
            make_outcome(vec![plugin_created("deploy", "/p/.claude/skills/deploy", "skill", true)]);

        let result = remove_migrated_sources(&outcome, &fs);
        assert!(result.is_err());
        let err = result.err().unwrap_or_else(|| std::io::Error::other("unexpected"));
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }
}
