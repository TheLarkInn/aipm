//! Recursive directory discovery for AI tool source directories.
//!
//! Uses the `ignore` crate for gitignore-aware directory traversal.
//! Shared by both the `lint` and `migrate` pipelines.

use std::path::{Path, PathBuf};

/// The kind of AI plugin feature discovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureKind {
    /// A skill file (`SKILL.md` inside a `skills/` directory).
    Skill,
    /// An agent file (`*.md` inside an `agents/` directory).
    Agent,
    /// A hook file (`hooks.json` inside a `hooks/` directory).
    Hook,
    /// A plugin manifest (`aipm.toml` inside a `.ai/<plugin>/` directory).
    Plugin,
    /// A marketplace manifest (`marketplace.json` at `.ai/.claude-plugin/marketplace.json`).
    Marketplace,
    /// A plugin JSON manifest (`plugin.json` at `.ai/<plugin>/.claude-plugin/plugin.json`).
    PluginJson,
}

/// The source directory context for a discovered feature.
#[derive(Debug, Clone)]
pub struct SourceContext {
    /// The recognized source directory type (e.g., `".ai"`, `".claude"`, `".github"`).
    pub source_type: String,
    /// The plugin name, derived from the `.ai/<plugin>/` path segment. `None` for non-`.ai/` sources.
    pub plugin_name: Option<String>,
}

/// A discovered AI plugin feature file and its context.
#[derive(Debug, Clone)]
pub struct DiscoveredFeature {
    /// Absolute path to the feature file (e.g., `.github/skills/default/SKILL.md`).
    pub file_path: PathBuf,
    /// The kind of feature this file represents.
    pub kind: FeatureKind,
    /// The source directory context, if the file lives inside a recognized source dir.
    /// `None` for features found outside any recognized source directory.
    pub source_context: Option<SourceContext>,
    /// Relative path from project root to the feature file.
    pub relative_path: PathBuf,
}

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

// Keep the old field name accessible for backwards compatibility in dry_run.rs
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

/// Known directories that are not worth descending into when looking for features.
const SKIP_DIRS: &[&str] =
    &["node_modules", "target", ".git", "vendor", "__pycache__", "dist", "build"];

/// Classify the source context of a file path by inspecting its ancestor components.
///
/// Returns `Some(SourceContext)` if the path contains a recognized source directory
/// (`.ai/`, `.claude/`, `.github/`), or `None` otherwise.
fn classify_source_context(file_path: &Path, project_root: &Path) -> Option<SourceContext> {
    let relative = file_path.strip_prefix(project_root).unwrap_or(file_path);
    let components: Vec<&std::ffi::OsStr> =
        relative.components().map(std::path::Component::as_os_str).collect();

    // Scan components for a recognized source dir
    for (i, component) in components.iter().enumerate() {
        let name = component.to_string_lossy();
        match name.as_ref() {
            ".ai" => {
                // plugin_name is the component immediately after .ai/
                let plugin_name = components.get(i + 1).map(|n| n.to_string_lossy().into_owned());
                return Some(SourceContext { source_type: ".ai".to_string(), plugin_name });
            },
            ".claude" => {
                return Some(SourceContext {
                    source_type: ".claude".to_string(),
                    plugin_name: None,
                });
            },
            ".github" => {
                return Some(SourceContext {
                    source_type: ".github".to_string(),
                    plugin_name: None,
                });
            },
            _ => {},
        }
    }
    None
}

/// Walk the project tree and find all AI plugin feature files.
///
/// Uses a single unified recursive walk (gitignore-aware) that discovers
/// skills (`SKILL.md`), agents (`*.md` in `agents/`), hooks (`hooks.json`
/// in `hooks/`), and plugin manifests (`aipm.toml` in `.ai/<plugin>/`).
///
/// Unlike [`discover_source_dirs`], this function:
/// - Walks `.ai/` instead of skipping it
/// - Matches individual **files**, not directories
/// - Requires both file name AND parent directory name to match (avoids false positives)
/// - Classifies each file's source context from its path
///
/// # Arguments
/// * `project_root` — The project root directory to scan from
/// * `max_depth` — Optional maximum traversal depth (`None` = unlimited)
///
/// # Returns
/// A sorted `Vec<DiscoveredFeature>` (sorted by file path for deterministic output).
/// Classify a file path into a `FeatureKind`, or `None` if it is not a recognized feature.
fn classify_feature_kind(file_path: &Path) -> Option<FeatureKind> {
    let file_name = file_path.file_name()?.to_string_lossy();

    let parent_name = file_path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let grandparent_name = file_path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    if file_name == "SKILL.md" {
        // Standard: skills/<name>/SKILL.md; flat: skills/SKILL.md
        if parent_name == "skills" || grandparent_name == "skills" {
            return Some(FeatureKind::Skill);
        }
    } else if file_name.ends_with(".md") && parent_name == "agents" {
        return Some(FeatureKind::Agent);
    } else if file_name == "hooks.json" && parent_name == "hooks" {
        return Some(FeatureKind::Hook);
    } else if file_name == "aipm.toml" && grandparent_name == ".ai" {
        return Some(FeatureKind::Plugin);
    } else if file_name == "marketplace.json"
        && parent_name == ".claude-plugin"
        && grandparent_name == ".ai"
    {
        return Some(FeatureKind::Marketplace);
    } else if file_name == "plugin.json" && parent_name == ".claude-plugin" {
        let great_grandparent = file_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if great_grandparent == ".ai" {
            return Some(FeatureKind::PluginJson);
        }
    }
    None
}

pub fn discover_features(
    project_root: &Path,
    max_depth: Option<usize>,
) -> Result<Vec<DiscoveredFeature>, Error> {
    let mut builder = ignore::WalkBuilder::new(project_root);
    builder.hidden(false); // Must find hidden dirs like .ai/, .claude/, .github/
    builder.git_ignore(true);
    builder.git_global(true);
    builder.git_exclude(true);

    if let Some(depth) = max_depth {
        builder.max_depth(Some(depth));
    }

    // Skip directories that cannot contain AI plugin features
    builder.filter_entry(|entry| {
        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
            let name = entry.file_name().to_string_lossy();
            if SKIP_DIRS.iter().any(|&skip| name == skip) {
                tracing::trace!(dir = %entry.path().display(), reason = "skip-list", "skipping directory");
                return false;
            }
        }
        true
    });

    let mut features = Vec::new();
    let mut dirs_walked: usize = 0;

    for result in builder.build() {
        let entry = result.map_err(|e| Error::WalkFailed(e.to_string()))?;

        let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
        if is_dir {
            dirs_walked += 1;
            tracing::trace!(dir = %entry.path().display(), "entering directory");
            continue;
        }

        let file_path = entry.path();
        let Some(kind) = classify_feature_kind(file_path) else { continue };

        let source_context = classify_source_context(file_path, project_root);
        let relative_path = file_path.strip_prefix(project_root).unwrap_or(file_path).to_path_buf();

        tracing::trace!(
            file = %file_path.display(),
            kind = ?kind,
            source = ?source_context.as_ref().map(|c| &c.source_type),
            "feature detected"
        );

        features.push(DiscoveredFeature {
            file_path: file_path.to_path_buf(),
            kind,
            source_context,
            relative_path,
        });
    }

    tracing::trace!(
        total_features = features.len(),
        total_dirs_walked = dirs_walked,
        "walk complete"
    );

    // Sort by file path for deterministic ordering
    features.sort_by(|a, b| a.file_path.cmp(&b.file_path));

    Ok(features)
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
                sources.get(i).map(|s| &s.source_dir) <= sources.get(i + 1).map(|s| &s.source_dir)
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

    #[test]
    fn discover_source_dirs_finds_both_claude_and_github() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(Path::new("."));

        assert!(std::fs::create_dir_all(root.join(".claude")).is_ok());
        assert!(std::fs::create_dir_all(root.join(".github")).is_ok());

        let result = discover_source_dirs(root, &[".claude", ".github"], None);
        assert!(result.is_ok());
        let sources = result.ok().unwrap_or_default();
        assert_eq!(sources.len(), 2);

        let types: Vec<&str> = sources.iter().map(|s| s.source_type.as_str()).collect();
        assert!(types.contains(&".claude"));
        assert!(types.contains(&".github"));
    }

    #[test]
    fn discover_source_dirs_sets_correct_source_type() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(Path::new("."));

        assert!(std::fs::create_dir_all(root.join("packages").join("auth").join(".github")).is_ok());

        let result = discover_source_dirs(root, &[".github"], None);
        assert!(result.is_ok());
        let sources = result.ok().unwrap_or_default();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources.first().map(|s| s.source_type.as_str()), Some(".github"));
        assert_eq!(sources.first().and_then(|s| s.package_name.as_deref()), Some("auth"));
    }

    #[test]
    fn discover_source_dirs_root_github_has_none_package_name() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(Path::new("."));

        assert!(std::fs::create_dir_all(root.join(".github")).is_ok());

        let result = discover_source_dirs(root, &[".github"], None);
        assert!(result.is_ok());
        let sources = result.ok().unwrap_or_default();
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

    // ---- discover_features() tests ----

    fn make_tmp() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().to_path_buf();
        (tmp, root)
    }

    #[test]
    fn discover_features_finds_skill_in_ai() {
        let (_tmp, root) = make_tmp();
        let path = root.join(".ai").join("test-plugin").join("skills").join("default");
        assert!(std::fs::create_dir_all(&path).is_ok());
        assert!(std::fs::write(path.join("SKILL.md"), "---\nname: test\n---\n").is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert_eq!(features.len(), 1);
        let f = &features[0];
        assert_eq!(f.kind, FeatureKind::Skill);
        assert!(f.source_context.as_ref().is_some_and(|c| c.source_type == ".ai"));
        assert!(f
            .source_context
            .as_ref()
            .is_some_and(|c| c.plugin_name.as_deref() == Some("test-plugin")));
    }

    #[test]
    fn discover_features_finds_skill_in_claude() {
        let (_tmp, root) = make_tmp();
        let path = root.join(".claude").join("skills").join("default");
        assert!(std::fs::create_dir_all(&path).is_ok());
        assert!(std::fs::write(path.join("SKILL.md"), "---\nname: test\n---\n").is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].kind, FeatureKind::Skill);
        assert!(features[0].source_context.as_ref().is_some_and(|c| c.source_type == ".claude"));
    }

    #[test]
    fn discover_features_finds_skill_in_github() {
        let (_tmp, root) = make_tmp();
        let path = root.join(".github").join("skills").join("default");
        assert!(std::fs::create_dir_all(&path).is_ok());
        assert!(std::fs::write(path.join("SKILL.md"), "---\nname: test\n---\n").is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].kind, FeatureKind::Skill);
        assert!(features[0].source_context.as_ref().is_some_and(|c| c.source_type == ".github"));
    }

    #[test]
    fn discover_features_finds_hooks_json() {
        let (_tmp, root) = make_tmp();
        let path = root.join(".ai").join("test-plugin").join("hooks");
        assert!(std::fs::create_dir_all(&path).is_ok());
        assert!(std::fs::write(path.join("hooks.json"), r#"{"hooks":[]}"#).is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].kind, FeatureKind::Hook);
        assert!(features[0].source_context.as_ref().is_some_and(|c| c.source_type == ".ai"));
    }

    #[test]
    fn discover_features_finds_agent_md() {
        let (_tmp, root) = make_tmp();
        let path = root.join(".ai").join("test-plugin").join("agents");
        assert!(std::fs::create_dir_all(&path).is_ok());
        assert!(std::fs::write(path.join("reviewer.md"), "---\nname: reviewer\n---\n").is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].kind, FeatureKind::Agent);
        assert!(features[0].source_context.as_ref().is_some_and(|c| c.source_type == ".ai"));
    }

    #[test]
    fn discover_features_classifies_source_context_none_for_unknown() {
        let (_tmp, root) = make_tmp();
        // SKILL.md inside some_folder/skills/default/ — no recognized source dir
        let path = root.join("some_folder").join("skills").join("default");
        assert!(std::fs::create_dir_all(&path).is_ok());
        assert!(std::fs::write(path.join("SKILL.md"), "---\nname: test\n---\n").is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].kind, FeatureKind::Skill);
        assert!(features[0].source_context.is_none());
    }

    #[test]
    fn discover_features_respects_max_depth() {
        let (_tmp, root) = make_tmp();
        // Shallow skill at: .ai/plugin/skills/default/SKILL.md
        // Directory depths: .ai=1, plugin=2, skills=3, default=4 — file is inside depth-4 dir
        let shallow = root.join(".ai").join("plugin").join("skills").join("default");
        assert!(std::fs::create_dir_all(&shallow).is_ok());
        assert!(std::fs::write(shallow.join("SKILL.md"), "---\nname: s\n---\n").is_ok());
        // Deep skill at: packages/auth/.ai/plugin/skills/default/SKILL.md
        // Directory depths: packages=1, auth=2, .ai=3, plugin=4, skills=5, default=6
        let deep = root
            .join("packages")
            .join("auth")
            .join(".ai")
            .join("plugin")
            .join("skills")
            .join("default");
        assert!(std::fs::create_dir_all(&deep).is_ok());
        assert!(std::fs::write(deep.join("SKILL.md"), "---\nname: d\n---\n").is_ok());

        // SKILL.md in the shallow path is at depth 5 (.ai=1,plugin=2,skills=3,default=4,file=5)
        // SKILL.md in the deep path is at depth 7 (packages=1,auth=2,.ai=3,plugin=4,skills=5,default=6,file=7)
        // max_depth=5 reaches shallow but not deep
        let features = discover_features(&root, Some(5)).expect("discover_features");
        assert_eq!(features.len(), 1);
        assert!(features[0].file_path.starts_with(&shallow));
    }

    #[test]
    fn discover_features_returns_sorted_output() {
        let (_tmp, root) = make_tmp();
        let z = root.join(".ai").join("z-plugin").join("skills").join("default");
        let a = root.join(".ai").join("a-plugin").join("skills").join("default");
        assert!(std::fs::create_dir_all(&z).is_ok());
        assert!(std::fs::create_dir_all(&a).is_ok());
        assert!(std::fs::write(z.join("SKILL.md"), "---\nname: z\n---\n").is_ok());
        assert!(std::fs::write(a.join("SKILL.md"), "---\nname: a\n---\n").is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert_eq!(features.len(), 2);
        assert!(features[0].file_path < features[1].file_path);
    }

    #[test]
    fn discover_features_ignores_skill_md_outside_skills_dir() {
        let (_tmp, root) = make_tmp();
        // SKILL.md directly in docs/ — not inside skills/
        let path = root.join("docs");
        assert!(std::fs::create_dir_all(&path).is_ok());
        assert!(std::fs::write(path.join("SKILL.md"), "---\nname: test\n---\n").is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert!(features.is_empty());
    }

    #[test]
    fn discover_features_ignores_random_md_outside_agents_dir() {
        let (_tmp, root) = make_tmp();
        // README.md in root — not inside agents/
        assert!(std::fs::write(root.join("README.md"), "# project").is_ok());
        // some.md in skills/ — inside skills but not agents/
        let skills = root.join(".ai").join("plugin").join("skills").join("default");
        assert!(std::fs::create_dir_all(&skills).is_ok());
        assert!(std::fs::write(skills.join("extra.md"), "extra").is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert!(features.is_empty());
    }

    #[test]
    fn discover_features_skips_node_modules_and_target() {
        let (_tmp, root) = make_tmp();
        // SKILL.md inside node_modules — should be skipped
        let nm = root.join("node_modules").join("pkg").join("skills").join("default");
        assert!(std::fs::create_dir_all(&nm).is_ok());
        assert!(std::fs::write(nm.join("SKILL.md"), "---\nname: nm\n---\n").is_ok());
        // SKILL.md inside target — should be skipped
        let tgt = root.join("target").join("debug").join("skills").join("default");
        assert!(std::fs::create_dir_all(&tgt).is_ok());
        assert!(std::fs::write(tgt.join("SKILL.md"), "---\nname: tgt\n---\n").is_ok());
        // Legitimate SKILL.md
        let real = root.join(".ai").join("plugin").join("skills").join("default");
        assert!(std::fs::create_dir_all(&real).is_ok());
        assert!(std::fs::write(real.join("SKILL.md"), "---\nname: real\n---\n").is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert_eq!(features.len(), 1);
        assert!(features[0].file_path.starts_with(&real));
    }

    #[test]
    fn discover_features_plugin_manifest_at_correct_depth_is_classified() {
        let (_tmp, root) = make_tmp();
        // Valid: .ai/<plugin>/aipm.toml — grandparent is ".ai"
        let plugin_dir = root.join(".ai").join("my-plugin");
        assert!(std::fs::create_dir_all(&plugin_dir).is_ok());
        assert!(std::fs::write(plugin_dir.join("aipm.toml"), "[package]\nname = \"my-plugin\"\n")
            .is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].kind, FeatureKind::Plugin);
    }

    #[test]
    fn discover_features_plugin_manifest_nested_too_deep_is_ignored() {
        let (_tmp, root) = make_tmp();
        // Invalid: .ai/<plugin>/nested/aipm.toml — grandparent is "nested", not ".ai"
        let nested_dir = root.join(".ai").join("my-plugin").join("nested");
        assert!(std::fs::create_dir_all(&nested_dir).is_ok());
        assert!(std::fs::write(nested_dir.join("aipm.toml"), "[package]\nname = \"bad\"\n").is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert!(features.is_empty(), "nested aipm.toml should not be classified as Plugin");
    }

    #[test]
    fn discover_features_finds_skill_in_flat_layout() {
        // Flat layout: .claude/skills/SKILL.md — parent is "skills" directly (no subdirectory).
        // This covers the `parent_name == "skills"` branch in the classifier, which the standard
        // `skills/<name>/SKILL.md` layout (grandparent == "skills") does not exercise.
        let (_tmp, root) = make_tmp();
        let skills_dir = root.join(".claude").join("skills");
        assert!(std::fs::create_dir_all(&skills_dir).is_ok());
        assert!(std::fs::write(skills_dir.join("SKILL.md"), "---\nname: flat-skill\n---\n").is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert_eq!(features.len(), 1, "expected exactly one feature from flat layout");
        assert_eq!(features[0].kind, FeatureKind::Skill);
        assert!(
            features[0].source_context.as_ref().is_some_and(|c| c.source_type == ".claude"),
            "source_type should be .claude"
        );
    }

    #[test]
    fn discover_features_marketplace_json_classified_correctly() {
        let (_tmp, root) = make_tmp();
        let claude_plugin = root.join(".ai").join(".claude-plugin");
        assert!(std::fs::create_dir_all(&claude_plugin).is_ok());
        assert!(std::fs::write(
            claude_plugin.join("marketplace.json"),
            r#"{"name":"local","plugins":[]}"#
        )
        .is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].kind, FeatureKind::Marketplace);
    }

    #[test]
    fn discover_features_marketplace_json_outside_ai_ignored() {
        let (_tmp, root) = make_tmp();
        // marketplace.json not under .ai/ — should not be classified
        let other = root.join("other").join(".claude-plugin");
        assert!(std::fs::create_dir_all(&other).is_ok());
        assert!(std::fs::write(other.join("marketplace.json"), r#"{"plugins":[]}"#).is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert!(features.is_empty());
    }

    #[test]
    fn discover_features_plugin_json_classified_correctly() {
        let (_tmp, root) = make_tmp();
        let plugin_claude = root.join(".ai").join("my-plugin").join(".claude-plugin");
        assert!(std::fs::create_dir_all(&plugin_claude).is_ok());
        assert!(std::fs::write(
            plugin_claude.join("plugin.json"),
            r#"{"name":"my-plugin","version":"0.1.0"}"#
        )
        .is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].kind, FeatureKind::PluginJson);
    }

    #[test]
    fn discover_features_plugin_json_outside_ai_ignored() {
        let (_tmp, root) = make_tmp();
        // plugin.json not under .ai/<plugin>/.claude-plugin/ — should not be classified
        let bad_path = root.join("packages").join("my-plugin").join(".claude-plugin");
        assert!(std::fs::create_dir_all(&bad_path).is_ok());
        assert!(std::fs::write(bad_path.join("plugin.json"), r#"{"name":"x"}"#).is_ok());

        let features = discover_features(&root, None).expect("discover_features");
        assert!(features.is_empty());
    }
}
