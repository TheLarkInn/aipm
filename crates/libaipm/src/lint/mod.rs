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
        if let Ok(pat) = glob::Pattern::new(pattern) {
            if pat.matches(path) {
                return true;
            }
        }
    }
    false
}

/// Run the lint pipeline.
///
/// Discovers source directories, selects rules per source type, applies
/// configuration overrides (including ignore paths), executes rules
/// sequentially, and collects diagnostics.
///
/// # Errors
///
/// Returns an error if a critical I/O failure prevents scanning.
pub fn lint(opts: &Options, fs: &dyn Fs) -> Result<Outcome, Error> {
    let mut all_diagnostics = Vec::new();
    let mut sources_scanned = Vec::new();

    // Determine which sources to scan
    let source_types: Vec<&str> = opts.source.as_deref().map_or_else(
        || {
            // Auto-discover: check which source dirs exist
            let mut found = Vec::new();
            if fs.exists(&opts.dir.join(".claude")) {
                found.push(".claude");
            }
            if fs.exists(&opts.dir.join(".github")) {
                found.push(".github");
            }
            if fs.exists(&opts.dir.join(".ai")) {
                found.push(".ai");
            }
            found
        },
        |s| vec![s],
    );

    for source_type in &source_types {
        sources_scanned.push((*source_type).to_string());

        let source_dir = opts.dir.join(source_type.trim_start_matches('.'));
        // For .ai, the dir is just .ai; for .claude, it's .claude
        let scan_dir = opts.dir.join(source_type);

        let all_rules = rules::for_source(source_type);

        for rule in &all_rules {
            // Skip rules suppressed by config
            if opts.config.is_suppressed(rule.id()) {
                continue;
            }

            let rule_diagnostics = rule.check(&scan_dir, fs)?;

            // Apply severity overrides from config
            let effective_severity =
                opts.config.severity_override(rule.id()).unwrap_or_else(|| rule.default_severity());

            // Collect ignore patterns for this rule
            let rule_ignores = opts.config.rule_ignore_paths(rule.id());

            for mut d in rule_diagnostics {
                // Apply global and per-rule ignore path filtering
                let path_str = d.file_path.display().to_string();
                if is_ignored(&path_str, &opts.config.ignore_paths)
                    || is_ignored(&path_str, rule_ignores)
                {
                    continue;
                }
                d.severity = effective_severity;
                all_diagnostics.push(d);
            }
        }

        let _ = source_dir;
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
    /// Maximum directory traversal depth (reserved for future use).
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
    }

    #[test]
    fn lint_empty_dir_no_sources() {
        use crate::lint::rules::test_helpers::MockFs;

        let fs = MockFs::new();
        let opts = Options {
            dir: PathBuf::from("/tmp/empty"),
            source: None,
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
        use crate::lint::rules::test_helpers::MockFs;

        let mut fs = MockFs::new();
        fs.add_existing("/project/.claude");
        fs.add_existing("/project/.github");
        fs.add_existing("/project/.ai");
        // Empty dirs
        fs.dirs.insert(PathBuf::from("/project/.ai"), vec![]);

        let opts = Options {
            dir: PathBuf::from("/project"),
            source: None,
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
}
