//! Engine-root inference for the unified discovery module.
//!
//! Given a feature file path and the project root, walks the path's ancestors
//! upward and identifies the innermost engine source root (`.claude`,
//! `.github`, or `.ai`) along with the engine it represents.
//!
//! Replaces `discovery_legacy::classify_source_context` and
//! `lint::rules::scan::source_type_from_path` — both of which encode the same
//! decision in slightly different shapes.

use std::path::{Path, PathBuf};

use super::types::Engine;

/// Walk `path`'s ancestors up to (but not including) `project_root` and return
/// the innermost engine source root encountered, along with the engine it
/// represents.
///
/// Recognized source-root directory names:
/// - `.claude` → [`Engine::Claude`]
/// - `.github` → [`Engine::Copilot`]
/// - `.ai`     → [`Engine::Ai`]
///
/// Returns `None` if no engine ancestor exists between `path` and
/// `project_root` (or if `path == project_root`).
///
/// For nested layouts like `.ai/<plugin>/.claude/skills/<x>/SKILL.md`, the
/// innermost ancestor wins — this returns
/// `(Engine::Claude, ".ai/<plugin>/.claude")`, not `(Engine::Ai, ".ai")`. This
/// mirrors the unified module's intent: each feature is associated with the
/// engine that authored it, not the marketplace host that contains it.
///
/// Non-UTF-8 path components are handled via [`std::path::Path::file_name`]
/// returning [`Option`] and [`std::ffi::OsStr::to_string_lossy`] — invalid
/// UTF-8 simply does not match any known engine root.
#[must_use]
pub fn infer_engine_root(path: &Path, project_root: &Path) -> Option<(Engine, PathBuf)> {
    for ancestor in path.ancestors() {
        if ancestor == project_root {
            break;
        }
        let Some(name) = ancestor.file_name() else {
            continue;
        };
        let lossy = name.to_string_lossy();
        match lossy.as_ref() {
            ".claude" => return Some((Engine::Claude, ancestor.to_path_buf())),
            ".github" => return Some((Engine::Copilot, ancestor.to_path_buf())),
            ".ai" => return Some((Engine::Ai, ancestor.to_path_buf())),
            _ => {},
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_skills_canonical_layout() {
        let project = Path::new("/repo");
        let path = Path::new("/repo/.claude/skills/my-skill/SKILL.md");
        let (engine, root) = infer_engine_root(path, project).expect("should match");
        assert_eq!(engine, Engine::Claude);
        assert_eq!(root, PathBuf::from("/repo/.claude"));
    }

    #[test]
    fn copilot_subroot_with_skills_layout() {
        // The customer's #725 layout.
        let project = Path::new("/repo");
        let path = Path::new("/repo/.github/copilot/skills/skill-alpha/SKILL.md");
        let (engine, root) = infer_engine_root(path, project).expect("should match");
        assert_eq!(engine, Engine::Copilot);
        assert_eq!(root, PathBuf::from("/repo/.github"));
    }

    #[test]
    fn copilot_canonical_skills_layout() {
        let project = Path::new("/repo");
        let path = Path::new("/repo/.github/skills/my-skill/SKILL.md");
        let (engine, root) = infer_engine_root(path, project).expect("should match");
        assert_eq!(engine, Engine::Copilot);
        assert_eq!(root, PathBuf::from("/repo/.github"));
    }

    #[test]
    fn ai_plugin_layout() {
        let project = Path::new("/repo");
        let path = Path::new("/repo/.ai/my-plugin/skills/my-skill/SKILL.md");
        let (engine, root) = infer_engine_root(path, project).expect("should match");
        assert_eq!(engine, Engine::Ai);
        assert_eq!(root, PathBuf::from("/repo/.ai"));
    }

    #[test]
    fn ai_nested_claude_returns_innermost() {
        // Nested authoring tree: the .claude/ inside .ai/<plugin>/ wins because
        // ancestors are walked from leaf upward.
        let project = Path::new("/repo");
        let path = Path::new("/repo/.ai/my-plugin/.claude/skills/my-skill/SKILL.md");
        let (engine, root) = infer_engine_root(path, project).expect("should match");
        assert_eq!(engine, Engine::Claude);
        assert_eq!(root, PathBuf::from("/repo/.ai/my-plugin/.claude"));
    }

    #[test]
    fn no_engine_ancestor_returns_none() {
        let project = Path::new("/repo");
        let path = Path::new("/repo/src/lib.rs");
        assert!(infer_engine_root(path, project).is_none());
    }

    #[test]
    fn path_equals_project_root_returns_none() {
        let project = Path::new("/repo");
        let path = Path::new("/repo");
        assert!(infer_engine_root(path, project).is_none());
    }

    #[test]
    fn project_root_is_engine_dir_does_not_match_itself() {
        // If the user invokes inference on the project root itself, we don't
        // wrap it as its own engine — we break before reading its name.
        let project = Path::new("/repo/.claude");
        let path = Path::new("/repo/.claude");
        assert!(infer_engine_root(path, project).is_none());
    }

    #[test]
    fn relative_paths_work_too() {
        let project = Path::new(".");
        let path = Path::new("./.github/skills/my-skill/SKILL.md");
        let (engine, root) = infer_engine_root(path, project).expect("should match");
        assert_eq!(engine, Engine::Copilot);
        assert_eq!(root, PathBuf::from("./.github"));
    }

    #[test]
    fn relative_paths_without_dot_prefix() {
        let project = Path::new("");
        let path = Path::new(".claude/skills/my-skill/SKILL.md");
        let (engine, root) = infer_engine_root(path, project).expect("should match");
        assert_eq!(engine, Engine::Claude);
        assert_eq!(root, PathBuf::from(".claude"));
    }

    #[test]
    fn deeper_nested_layout_picks_innermost() {
        // Layered .ai > .github > .claude — the innermost (.claude) still wins.
        let project = Path::new("/repo");
        let path = Path::new("/repo/.ai/p/.github/.claude/skills/x/SKILL.md");
        let (engine, root) = infer_engine_root(path, project).expect("should match");
        assert_eq!(engine, Engine::Claude);
        assert_eq!(root, PathBuf::from("/repo/.ai/p/.github/.claude"));
    }

    #[test]
    fn unrelated_dotted_directory_is_ignored() {
        // A directory named `.foo` is not an engine root — the walk continues
        // until it finds one that is.
        let project = Path::new("/repo");
        let path = Path::new("/repo/.foo/.github/skills/x/SKILL.md");
        let (engine, root) = infer_engine_root(path, project).expect("should match");
        assert_eq!(engine, Engine::Copilot);
        assert_eq!(root, PathBuf::from("/repo/.foo/.github"));
    }

    #[test]
    fn no_match_when_path_outside_project_root() {
        // path is not under project_root at all — walks ancestors up to the
        // filesystem root without ever encountering project_root, but also
        // never encountering a recognized engine name. Returns None.
        let project = Path::new("/some/other/dir");
        let path = Path::new("/repo/src/lib.rs");
        assert!(infer_engine_root(path, project).is_none());
    }

    #[test]
    fn project_root_at_filesystem_root_works() {
        let project = Path::new("/");
        let path = Path::new("/.github/skills/x/SKILL.md");
        let (engine, root) = infer_engine_root(path, project).expect("should match");
        assert_eq!(engine, Engine::Copilot);
        assert_eq!(root, PathBuf::from("/.github"));
    }
}
