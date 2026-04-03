//! Lint system for AI plugin quality validation.
//!
//! Validates plugin configurations across tool-specific source directories
//! (`.claude/`, `.github/`) and the `.ai/` marketplace. Uses the same
//! adapter architecture as `aipm migrate` — each source type gets its own
//! rule set behind the [`Rule`] trait.

pub mod config;
pub mod diagnostic;
pub mod reporter;
pub mod rule;
pub mod rules;

use std::path::PathBuf;

use crate::fs::Fs;

pub use diagnostic::{Diagnostic, Severity};
pub use rule::Rule;

/// Check if a file path matches any of the given glob ignore patterns.
fn is_ignored(path: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        match glob::Pattern::new(pattern) {
            Ok(pat) => {
                if pat.matches(path) {
                    return true;
                }
            },
            Err(e) => {
                tracing::warn!(
                    pattern = %pattern,
                    error = %e,
                    "ignoring invalid glob pattern in ignore list"
                );
            },
        }
    }
    false
}

/// Run rules for a given source type against a directory, collecting diagnostics.
fn run_rules_for_source(
    source_type: &str,
    scan_dir: &std::path::Path,
    project_root: &std::path::Path,
    fs: &dyn Fs,
    config: &config::Config,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Error> {
    let all_rules = rules::for_source(source_type, project_root);

    for rule in &all_rules {
        if config.is_suppressed(rule.id()) {
            continue;
        }

        let rule_diagnostics = rule.check(scan_dir, fs)?;

        let effective_severity =
            config.severity_override(rule.id()).unwrap_or_else(|| rule.default_severity());

        let rule_ignores = config.rule_ignore_paths(rule.id());

        for mut d in rule_diagnostics {
            let path_str = d.file_path.display().to_string();
            if is_ignored(&path_str, &config.ignore_paths) || is_ignored(&path_str, rule_ignores) {
                continue;
            }
            d.severity = effective_severity;
            diagnostics.push(d);
        }
    }

    Ok(())
}

/// Run the lint pipeline.
///
/// Discovers source directories recursively for `.claude/` and `.github/`
/// using the shared `discovery` module (gitignore-aware tree walk).
/// The `.ai/` marketplace is checked as a flat root-level directory.
///
/// # Errors
///
/// Returns an error if a critical I/O or discovery failure prevents scanning.
pub fn lint(opts: &Options, fs: &dyn Fs) -> Result<Outcome, Error> {
    let mut all_diagnostics = Vec::new();
    let mut sources_scanned = Vec::new();

    // Phase 1: Recursive discovery for .claude/.github source directories
    let source_patterns: Vec<&str> = match opts.source.as_deref() {
        Some(".ai") => vec![],
        Some(src) => vec![src],
        None => vec![".claude", ".github"],
    };

    if !source_patterns.is_empty() {
        let discovered =
            crate::discovery::discover_source_dirs(&opts.dir, &source_patterns, opts.max_depth)?;

        for src in &discovered {
            if !sources_scanned.contains(&src.source_type) {
                sources_scanned.push(src.source_type.clone());
            }
            run_rules_for_source(
                &src.source_type,
                &src.source_dir,
                &opts.dir,
                fs,
                &opts.config,
                &mut all_diagnostics,
            )?;
        }
    }

    // Phase 2: Flat marketplace check for .ai/
    let check_marketplace = match opts.source.as_deref() {
        Some(".ai") => true,
        Some(_) => false,
        None => fs.exists(&opts.dir.join(".ai")),
    };

    if check_marketplace {
        sources_scanned.push(".ai".to_string());
        let scan_dir = opts.dir.join(".ai");
        run_rules_for_source(".ai", &scan_dir, &opts.dir, fs, &opts.config, &mut all_diagnostics)?;
    }

    // Sort by file path for consistent output
    all_diagnostics.sort_by(|a, b| a.file_path.cmp(&b.file_path));

    let error_count = all_diagnostics.iter().filter(|d| d.severity == Severity::Error).count();
    let warning_count = all_diagnostics.iter().filter(|d| d.severity == Severity::Warning).count();

    Ok(Outcome { diagnostics: all_diagnostics, error_count, warning_count, sources_scanned })
}

/// Options for running the lint pipeline.
#[derive(Debug)]
pub struct Options {
    /// Directory to lint.
    pub dir: PathBuf,
    /// Optional filter to a specific source type (e.g., `".claude"`, `".ai"`).
    pub source: Option<String>,
    /// Lint configuration from `[workspace.lints]`.
    pub config: config::Config,
    /// Maximum directory traversal depth for `.claude`/`.github` discovery.
    pub max_depth: Option<usize>,
}

/// Outcome of a lint run.
#[derive(Debug)]
pub struct Outcome {
    /// All diagnostics found.
    pub diagnostics: Vec<Diagnostic>,
    /// Number of error-severity diagnostics.
    pub error_count: usize,
    /// Number of warning-severity diagnostics.
    pub warning_count: usize,
    /// Source types that were scanned.
    pub sources_scanned: Vec<String>,
}

/// Errors that can occur during linting.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// I/O error during filesystem access.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parse error.
    #[error("JSON parse error in {path}: {reason}")]
    JsonParse {
        /// Path to the file that failed to parse.
        path: PathBuf,
        /// Reason for the parse failure.
        reason: String,
    },

    /// Frontmatter parse error.
    #[error("frontmatter parse error in {path}: {reason}")]
    FrontmatterParse {
        /// Path to the file that failed to parse.
        path: PathBuf,
        /// Reason for the parse failure.
        reason: String,
    },

    /// Discovery failed during recursive directory walking.
    #[error(transparent)]
    DiscoveryFailed(#[from] crate::discovery::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_ignored tests ---

    #[test]
    fn is_ignored_suffix_glob() {
        let patterns = vec!["vendor/**".to_string()];
        assert!(is_ignored("vendor/foo/bar.md", &patterns));
        assert!(!is_ignored("src/main.rs", &patterns));
    }

    #[test]
    fn is_ignored_prefix_glob() {
        let patterns = vec!["**/hooks.json".to_string()];
        assert!(is_ignored(".ai/plugin/hooks/hooks.json", &patterns));
        assert!(!is_ignored(".ai/plugin/skills/SKILL.md", &patterns));
    }

    #[test]
    fn is_ignored_wildcard_pattern() {
        let patterns = vec!["**/legacy-plugin/**".to_string()];
        assert!(is_ignored(".ai/legacy-plugin/skills/SKILL.md", &patterns));
        assert!(!is_ignored(".ai/new-plugin/skills/SKILL.md", &patterns));
    }

    #[test]
    fn is_ignored_star_pattern() {
        let patterns = vec![".ai/legacy-*/**".to_string()];
        assert!(is_ignored(".ai/legacy-plugin/skills/SKILL.md", &patterns));
        assert!(!is_ignored(".ai/new-plugin/skills/SKILL.md", &patterns));
    }

    #[test]
    fn is_ignored_empty_patterns() {
        let patterns: Vec<String> = vec![];
        assert!(!is_ignored("any/path.md", &patterns));
    }

    #[test]
    fn is_ignored_no_match() {
        let patterns = vec!["vendor/**".to_string(), "**/test.md".to_string()];
        assert!(!is_ignored(".ai/plugin/skills/SKILL.md", &patterns));
    }

    #[test]
    fn is_ignored_invalid_pattern_skipped() {
        // An invalid glob pattern (unclosed bracket) should be silently skipped
        let patterns = vec!["[invalid".to_string()];
        assert!(!is_ignored("any/path", &patterns));
    }

    #[test]
    fn lint_outcome_default_counts() {
        let outcome = Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        };
        assert_eq!(outcome.error_count, 0);
        assert_eq!(outcome.warning_count, 0);
        assert!(outcome.diagnostics.is_empty());
    }

    #[test]
    fn lint_options_construction() {
        let opts = Options {
            dir: PathBuf::from("."),
            source: Some(".claude".to_string()),
            config: config::Config::default(),
            max_depth: None,
        };
        assert_eq!(opts.source.as_deref(), Some(".claude"));
    }

    #[test]
    fn error_display() {
        let err = Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "not found"));
        let msg = format!("{err}");
        assert!(msg.contains("I/O error"));

        let err = Error::JsonParse {
            path: PathBuf::from("hooks.json"),
            reason: "invalid json".to_string(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("hooks.json"));

        let err = Error::FrontmatterParse {
            path: PathBuf::from("SKILL.md"),
            reason: "missing delimiter".to_string(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("SKILL.md"));

        let err =
            Error::DiscoveryFailed(crate::discovery::Error::WalkFailed("access denied".into()));
        let msg = format!("{err}");
        assert!(msg.contains("discovery walk failed"));
    }

    #[test]
    fn lint_empty_dir_no_sources() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        assert!(outcome.sources_scanned.is_empty());
        assert!(outcome.diagnostics.is_empty());
    }

    #[test]
    fn lint_with_source_filter() {
        use crate::lint::rules::test_helpers::MockFs;

        let mut fs = MockFs::new();
        // .ai exists
        fs.add_existing("/project/.ai");
        fs.dirs.insert(PathBuf::from("/project/.ai"), vec![]);

        let opts = Options {
            dir: PathBuf::from("/project"),
            source: Some(".ai".to_string()),
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &fs);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        assert_eq!(outcome.sources_scanned, vec![".ai"]);
    }

    #[test]
    fn lint_config_allow_suppresses_rules() {
        use crate::lint::rules::test_helpers::MockFs;

        let mut fs = MockFs::new();
        fs.add_existing("/project/.ai");
        fs.add_skill("test-plugin", "default", "---\ndescription: no name\n---\nbody");

        // Adjust paths to be under /project
        let mut fs2 = MockFs::new();
        fs2.add_existing("/project/.ai");
        // Create plugin structure under /project/.ai
        let ai = PathBuf::from("/project/.ai");
        fs2.dirs.insert(
            ai.clone(),
            vec![crate::fs::DirEntry { name: "test-plugin".to_string(), is_dir: true }],
        );
        let skills = ai.join("test-plugin").join("skills");
        fs2.exists.insert(skills.clone());
        fs2.dirs.insert(
            skills.clone(),
            vec![crate::fs::DirEntry { name: "default".to_string(), is_dir: true }],
        );
        let skill_md = skills.join("default").join("SKILL.md");
        fs2.exists.insert(skill_md.clone());
        fs2.files.insert(skill_md, "---\ndescription: no name\n---\nbody".to_string());

        let mut config = config::Config::default();
        config.rule_overrides.insert("skill/missing-name".to_string(), config::RuleOverride::Allow);

        let opts = Options {
            dir: PathBuf::from("/project"),
            source: Some(".ai".to_string()),
            config,
            max_depth: None,
        };
        let result = lint(&opts, &fs2);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        // skill/missing-name should be suppressed
        assert!(!outcome.diagnostics.iter().any(|d| d.rule_id == "skill/missing-name"));
    }

    #[test]
    fn lint_auto_discovers_sources() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        // Create all three source directories on real FS
        assert!(std::fs::create_dir_all(root.join(".claude")).is_ok());
        assert!(std::fs::create_dir_all(root.join(".github")).is_ok());
        assert!(std::fs::create_dir_all(root.join(".ai")).is_ok());

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        assert!(outcome.sources_scanned.contains(&".claude".to_string()));
        assert!(outcome.sources_scanned.contains(&".github".to_string()));
        assert!(outcome.sources_scanned.contains(&".ai".to_string()));
    }

    #[test]
    fn lint_severity_override_applied() {
        use crate::lint::rules::test_helpers::MockFs;

        let mut fs = MockFs::new();
        fs.add_existing("/project/.ai");
        let ai = PathBuf::from("/project/.ai");
        fs.dirs
            .insert(ai.clone(), vec![crate::fs::DirEntry { name: "p".to_string(), is_dir: true }]);
        let skills = ai.join("p").join("skills");
        fs.exists.insert(skills.clone());
        fs.dirs.insert(
            skills.clone(),
            vec![crate::fs::DirEntry { name: "s".to_string(), is_dir: true }],
        );
        let skill_md = skills.join("s").join("SKILL.md");
        fs.exists.insert(skill_md.clone());
        fs.files.insert(skill_md, "---\nname: s\n---\nbody".to_string());

        let mut config = config::Config::default();
        // Override missing-description from warning to error
        config.rule_overrides.insert(
            "skill/missing-description".to_string(),
            config::RuleOverride::Level(Severity::Error),
        );

        let opts = Options {
            dir: PathBuf::from("/project"),
            source: Some(".ai".to_string()),
            config,
            max_depth: None,
        };
        let result = lint(&opts, &fs);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        // The missing-description diagnostic should now be Error
        let desc_diag =
            outcome.diagnostics.iter().find(|d| d.rule_id == "skill/missing-description");
        assert!(desc_diag.is_some());
        assert_eq!(desc_diag.map(|d| d.severity), Some(Severity::Error));
        assert!(outcome.error_count > 0);
    }

    // --- Recursive discovery integration tests ---

    #[test]
    fn lint_discovers_nested_claude_dirs() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        // Create .ai/ marketplace so misplaced-features rule fires
        assert!(std::fs::create_dir_all(root.join(".ai")).is_ok());
        // Create nested .claude/skills/ (misplaced feature)
        assert!(std::fs::create_dir_all(
            root.join("packages").join("auth").join(".claude").join("skills")
        )
        .is_ok());

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        // Should find misplaced-features in the nested .claude dir
        assert!(outcome.diagnostics.iter().any(|d| d.rule_id == "source/misplaced-features"));
        let diag = outcome.diagnostics.iter().find(|d| d.rule_id == "source/misplaced-features");
        assert!(diag.is_some());
        // The file_path should reference the nested location
        let path_str = diag.map(|d| d.file_path.display().to_string()).unwrap_or_default();
        assert!(path_str.contains("auth"));
    }

    #[test]
    fn lint_discovers_nested_github_dirs() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        assert!(std::fs::create_dir_all(root.join(".ai")).is_ok());
        assert!(std::fs::create_dir_all(
            root.join("packages").join("api").join(".github").join("hooks")
        )
        .is_ok());

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        assert!(outcome
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "source/misplaced-features" && d.source_type == ".github"));
    }

    #[test]
    fn lint_source_filter_claude_only() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        assert!(std::fs::create_dir_all(root.join(".ai")).is_ok());
        // Both .claude and .github have misplaced features
        assert!(std::fs::create_dir_all(root.join(".claude").join("skills")).is_ok());
        assert!(std::fs::create_dir_all(root.join(".github").join("skills")).is_ok());

        let opts = Options {
            dir: root.to_path_buf(),
            source: Some(".claude".to_string()),
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        // Only .claude diagnostics, no .github
        assert!(outcome.diagnostics.iter().all(|d| d.source_type == ".claude"));
        assert!(!outcome.diagnostics.is_empty());
    }

    #[test]
    fn lint_source_filter_ai_skips_discovery() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        assert!(std::fs::create_dir_all(root.join(".ai")).is_ok());
        // .claude has misplaced features but --source .ai should skip it
        assert!(std::fs::create_dir_all(root.join(".claude").join("skills")).is_ok());

        let opts = Options {
            dir: root.to_path_buf(),
            source: Some(".ai".to_string()),
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        // No misplaced-features diagnostics (those come from .claude, not .ai)
        assert!(!outcome.diagnostics.iter().any(|d| d.rule_id == "source/misplaced-features"));
        assert_eq!(outcome.sources_scanned, vec![".ai"]);
    }

    #[test]
    fn lint_max_depth_limits_discovery() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        assert!(std::fs::create_dir_all(root.join(".ai")).is_ok());
        // Root .claude/skills at depth 1
        assert!(std::fs::create_dir_all(root.join(".claude").join("skills")).is_ok());
        // Nested .claude/skills at depth 3
        assert!(std::fs::create_dir_all(
            root.join("packages").join("auth").join(".claude").join("skills")
        )
        .is_ok());

        // max_depth=1 should only find root .claude
        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: Some(1),
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        // Should have exactly 1 misplaced-features diagnostic (root only)
        let misplaced: Vec<_> = outcome
            .diagnostics
            .iter()
            .filter(|d| d.rule_id == "source/misplaced-features")
            .collect();
        assert_eq!(misplaced.len(), 1);
    }

    #[test]
    fn lint_no_sources_found_succeeds() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        // Only create src/ -- no .claude, .github, or .ai
        assert!(std::fs::create_dir_all(root.join("src")).is_ok());

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        assert!(outcome.sources_scanned.is_empty());
        assert!(outcome.diagnostics.is_empty());
    }

    #[test]
    fn lint_deduplicates_sources_scanned() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        // Multiple .claude dirs
        assert!(std::fs::create_dir_all(root.join(".claude")).is_ok());
        assert!(std::fs::create_dir_all(root.join("packages").join("a").join(".claude")).is_ok());
        assert!(std::fs::create_dir_all(root.join("packages").join("b").join(".claude")).is_ok());

        let opts = Options {
            dir: root.to_path_buf(),
            source: Some(".claude".to_string()),
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        // sources_scanned should have exactly one ".claude" entry despite 3 dirs
        let claude_count =
            outcome.sources_scanned.iter().filter(|s| s.as_str() == ".claude").count();
        assert_eq!(claude_count, 1);
    }

    #[test]
    fn lint_source_github_filters_discovery() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        assert!(std::fs::create_dir_all(root.join(".ai")).is_ok());
        assert!(std::fs::create_dir_all(root.join(".github").join("skills")).is_ok());

        let opts = Options {
            dir: root.to_path_buf(),
            source: Some(".github".to_string()),
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        assert!(outcome.diagnostics.iter().all(|d| d.source_type == ".github"));
        assert!(!outcome.diagnostics.is_empty());
        // .ai should NOT be scanned
        assert!(!outcome.sources_scanned.contains(&".ai".to_string()));
    }

    #[test]
    fn lint_ignore_paths_filter_source_diagnostics() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        assert!(std::fs::create_dir_all(root.join(".ai")).is_ok());
        assert!(std::fs::create_dir_all(root.join(".claude").join("skills")).is_ok());
        assert!(std::fs::create_dir_all(
            root.join("packages").join("ignored").join(".claude").join("skills")
        )
        .is_ok());

        let mut cfg = config::Config::default();
        cfg.ignore_paths = vec!["**/ignored/**".to_string()];

        let opts = Options { dir: root.to_path_buf(), source: None, config: cfg, max_depth: None };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        // The root .claude/skills should be found, but not the ignored one
        let misplaced: Vec<_> = outcome
            .diagnostics
            .iter()
            .filter(|d| d.rule_id == "source/misplaced-features")
            .collect();
        assert_eq!(misplaced.len(), 1);
        // Verify the ignored path is not in diagnostics
        assert!(misplaced.iter().all(|d| {
            let path_str = d.file_path.display().to_string();
            !path_str.contains("ignored")
        }));
    }

    #[test]
    fn lint_config_suppress_misplaced_features() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        assert!(std::fs::create_dir_all(root.join(".ai")).is_ok());
        assert!(std::fs::create_dir_all(root.join(".claude").join("skills")).is_ok());

        let mut cfg = config::Config::default();
        cfg.rule_overrides
            .insert("source/misplaced-features".to_string(), config::RuleOverride::Allow);

        let opts = Options { dir: root.to_path_buf(), source: None, config: cfg, max_depth: None };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        // misplaced-features should be suppressed
        assert!(!outcome.diagnostics.iter().any(|d| d.rule_id == "source/misplaced-features"));
    }

    #[test]
    fn lint_no_marketplace_no_source_findings() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        // .claude/skills exists but NO .ai — misplaced-features should not fire
        assert!(std::fs::create_dir_all(root.join(".claude").join("skills")).is_ok());

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });
        assert!(outcome.diagnostics.is_empty());
    }

    /// Covers the `is_ignored(&path_str, rule_ignores)` True branch in `run_rules_for_source`.
    ///
    /// When global `ignore_paths` is empty the first check returns False, so the
    /// second `is_ignored` call (per-rule ignore patterns from `RuleOverride::Detailed`)
    /// is the only gate.  A path matching that per-rule pattern must be skipped while
    /// a path that does not match still appears in the output.
    #[test]
    fn lint_rule_ignore_paths_filter_diagnostics() {
        let tmp = tempfile::tempdir();
        assert!(tmp.is_ok(), "tempdir creation must succeed");
        let tmp = tmp.ok();
        let root = tmp.as_ref().map(tempfile::TempDir::path);
        let root = root.as_ref().copied().unwrap_or(std::path::Path::new("."));

        // .ai/ must exist so misplaced-features fires
        assert!(std::fs::create_dir_all(root.join(".ai")).is_ok());
        // Root .claude/skills — should NOT be filtered (path doesn't contain "vendor")
        assert!(std::fs::create_dir_all(root.join(".claude").join("skills")).is_ok());
        // A nested .claude/skills under a "vendor" package — should be filtered by rule ignore
        assert!(std::fs::create_dir_all(
            root.join("packages").join("vendor").join(".claude").join("skills")
        )
        .is_ok());

        let mut cfg = config::Config::default();
        // No global ignore_paths — first is_ignored() always returns false.
        // Per-rule ignore for misplaced-features: suppress diagnostics under "vendor/".
        cfg.rule_overrides.insert(
            "source/misplaced-features".to_string(),
            config::RuleOverride::Detailed {
                level: Severity::Warning,
                ignore: vec!["**/vendor/**".to_string()],
            },
        );

        let opts = Options { dir: root.to_path_buf(), source: None, config: cfg, max_depth: None };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap_or_else(|_| Outcome {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
            sources_scanned: vec![],
        });

        let misplaced: Vec<_> = outcome
            .diagnostics
            .iter()
            .filter(|d| d.rule_id == "source/misplaced-features")
            .collect();

        // Root .claude/skills diagnostic must still be present
        assert!(
            misplaced.iter().any(|d| {
                let p = d.file_path.display().to_string();
                !p.contains("vendor")
            }),
            "root .claude/skills diagnostic should remain"
        );
        // The vendor path must be suppressed by the per-rule ignore
        assert!(
            !misplaced.iter().any(|d| {
                let p = d.file_path.display().to_string();
                p.contains("vendor")
            }),
            "vendor .claude/skills diagnostic should be filtered by rule ignore"
        );
    }
}
