//! Shared scanning utilities for lint rules.
//!
//! Provides helpers to read individual feature files and iterate over
//! plugin directories without duplicating filesystem logic across rules.

use std::path::{Path, PathBuf};

use crate::frontmatter::Frontmatter;
use crate::fs::Fs;

/// A skill found during scanning.
pub struct FoundSkill {
    /// Path to the SKILL.md file.
    pub path: PathBuf,
    /// Parsed frontmatter (if any).
    pub frontmatter: Option<Frontmatter>,
    /// Raw content of the file.
    pub content: String,
}

/// An agent found during scanning.
pub struct FoundAgent {
    /// Path to the agent .md file.
    pub path: PathBuf,
    /// Parsed frontmatter (if any).
    pub frontmatter: Option<Frontmatter>,
}

/// Derive the source type string from a file path by scanning its components.
///
/// Returns `".ai"`, `".claude"`, `".github"`, or `"other"` depending on which
/// recognized source directory ancestor the file lives under.
pub fn source_type_from_path(file_path: &Path) -> &'static str {
    for component in file_path.components() {
        let name = component.as_os_str().to_string_lossy();
        match name.as_ref() {
            ".ai" => return ".ai",
            ".claude" => return ".claude",
            ".github" => return ".github",
            _ => {},
        }
    }
    "other"
}

/// Read and parse a single `SKILL.md` file by absolute path.
///
/// Returns `None` if the file cannot be read.
pub fn read_skill(path: &Path, fs: &dyn Fs) -> Option<FoundSkill> {
    let content = match fs.read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(path = %path.display(), error = %e, "could not read SKILL.md");
            return None;
        },
    };
    let frontmatter = crate::frontmatter::parse(&content).ok().flatten();
    Some(FoundSkill { path: path.to_path_buf(), frontmatter, content })
}

/// Read and parse a single agent `.md` file by absolute path.
///
/// Returns `None` if the file cannot be read.
pub fn read_agent(path: &Path, fs: &dyn Fs) -> Option<FoundAgent> {
    let content = match fs.read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(path = %path.display(), error = %e, "could not read agent markdown file");
            return None;
        },
    };
    let frontmatter = crate::frontmatter::parse(&content).ok().flatten();
    Some(FoundAgent { path: path.to_path_buf(), frontmatter })
}

/// Read a single `hooks.json` file by absolute path.
///
/// Returns `None` if the file cannot be read.
pub fn read_hook(path: &Path, fs: &dyn Fs) -> Option<(PathBuf, String)> {
    let content = match fs.read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(path = %path.display(), error = %e, "could not read hooks.json");
            return None;
        },
    };
    Some((path.to_path_buf(), content))
}

/// List plugin directory names under `.ai/`, excluding `.claude-plugin`.
///
/// Returns an empty `Vec` if `ai_dir` cannot be read. Skips non-directory
/// entries and the internal `.claude-plugin` metadata directory.
pub fn list_plugin_dirs(ai_dir: &Path, fs: &dyn Fs) -> Vec<String> {
    let Ok(entries) = fs.read_dir(ai_dir) else {
        return vec![];
    };
    entries.into_iter().filter(|e| e.is_dir && e.name != ".claude-plugin").map(|e| e.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::test_helpers::MockFs;

    // --- source_type_from_path tests ---

    #[test]
    fn source_type_ai() {
        assert_eq!(source_type_from_path(Path::new(".ai/plugin/skills/s/SKILL.md")), ".ai");
    }

    #[test]
    fn source_type_claude() {
        assert_eq!(source_type_from_path(Path::new(".claude/skills/s/SKILL.md")), ".claude");
    }

    #[test]
    fn source_type_github() {
        assert_eq!(source_type_from_path(Path::new(".github/skills/s/SKILL.md")), ".github");
    }

    #[test]
    fn source_type_other() {
        assert_eq!(source_type_from_path(Path::new("some/random/path.md")), "other");
    }

    // --- read_skill tests ---

    #[test]
    fn read_skill_returns_some_for_existing_file() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        fs.files.insert(path.clone(), "---\nname: s\n---\nbody".to_string());
        let skill = read_skill(&path, &fs);
        assert!(skill.is_some());
        assert!(skill.as_ref().and_then(|s| s.frontmatter.as_ref()).is_some());
    }

    #[test]
    fn read_skill_returns_none_for_missing_file() {
        let fs = MockFs::new();
        let path = PathBuf::from(".ai/p/skills/s/SKILL.md");
        assert!(read_skill(&path, &fs).is_none());
    }

    // --- read_agent tests ---

    #[test]
    fn read_agent_returns_some_for_existing_file() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/agents/reviewer.md");
        fs.files.insert(path.clone(), "---\nname: reviewer\n---\nprompt".to_string());
        let agent = read_agent(&path, &fs);
        assert!(agent.is_some());
        assert!(agent.as_ref().and_then(|a| a.frontmatter.as_ref()).is_some());
    }

    #[test]
    fn read_agent_returns_none_for_missing_file() {
        let fs = MockFs::new();
        let path = PathBuf::from(".ai/p/agents/reviewer.md");
        assert!(read_agent(&path, &fs).is_none());
    }

    // --- read_hook tests ---

    #[test]
    fn read_hook_returns_some_for_existing_file() {
        let mut fs = MockFs::new();
        let path = PathBuf::from(".ai/p/hooks/hooks.json");
        fs.files.insert(path.clone(), r#"{"PreToolUse": []}"#.to_string());
        let hook = read_hook(&path, &fs);
        assert!(hook.is_some());
    }

    #[test]
    fn read_hook_returns_none_for_missing_file() {
        let fs = MockFs::new();
        let path = PathBuf::from(".ai/p/hooks/hooks.json");
        assert!(read_hook(&path, &fs).is_none());
    }

    // --- list_plugin_dirs tests ---

    #[test]
    fn list_plugin_dirs_returns_dir_names() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"plugins":[]}"#);
        fs.add_plugin_json("foo", r#"{"name":"foo"}"#);
        fs.add_plugin_json("bar", r#"{"name":"bar"}"#);
        let mut dirs = list_plugin_dirs(Path::new(".ai"), &fs);
        dirs.sort();
        assert_eq!(dirs, vec!["bar", "foo"]);
    }

    #[test]
    fn list_plugin_dirs_excludes_claude_plugin() {
        let mut fs = MockFs::new();
        fs.add_marketplace_json(r#"{"plugins":[]}"#);
        let dirs = list_plugin_dirs(Path::new(".ai"), &fs);
        assert!(!dirs.contains(&".claude-plugin".to_string()));
    }

    #[test]
    fn list_plugin_dirs_skips_file_entries() {
        let mut fs = MockFs::new();
        fs.dirs.insert(
            PathBuf::from(".ai"),
            vec![
                crate::fs::DirEntry { name: "plugin".to_string(), is_dir: true },
                crate::fs::DirEntry { name: "README.md".to_string(), is_dir: false },
            ],
        );
        let dirs = list_plugin_dirs(Path::new(".ai"), &fs);
        assert_eq!(dirs, vec!["plugin"]);
        assert!(!dirs.contains(&"README.md".to_string()));
    }

    #[test]
    fn list_plugin_dirs_nonexistent_dir_returns_empty() {
        let fs = MockFs::new();
        let dirs = list_plugin_dirs(Path::new(".ai"), &fs);
        assert!(dirs.is_empty());
    }
}
