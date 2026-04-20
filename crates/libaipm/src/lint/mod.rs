//! Lint system for AI plugin quality validation.
//!
//! Validates plugin configurations with a single unified recursive walk of the
//! project tree (gitignore-aware). Each discovered feature file is linted
//! against quality rules for its kind, plus `source/misplaced-features` if it
//! lives outside `.ai/`.

pub mod config;
pub mod diagnostic;
pub mod error;
pub mod reporter;
pub mod rule;
pub mod rules;

pub use error::Error;

use std::path::PathBuf;

use crate::discovery::{DiscoveredFeature, FeatureKind};
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

/// Apply diagnostic results from one rule to the accumulator, respecting
/// severity overrides, help text, and ignore paths from the config.
fn apply_rule_diagnostics(
    rule: &dyn Rule,
    rule_diagnostics: Vec<Diagnostic>,
    config: &config::Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let effective_severity =
        config.severity_override(rule.id()).unwrap_or_else(|| rule.default_severity());
    let rule_ignores = config.rule_ignore_paths(rule.id());

    for mut d in rule_diagnostics {
        let path_str = d.file_path.display().to_string();
        if is_ignored(&path_str, &config.ignore_paths) || is_ignored(&path_str, rule_ignores) {
            continue;
        }
        d.severity = effective_severity;
        d.help_text = rule.help_text().map(String::from);
        d.help_url = rule.help_url().map(String::from);
        diagnostics.push(d);
    }
}

/// Run all applicable rules for a single discovered feature file.
fn run_rules_for_feature(
    feature: &DiscoveredFeature,
    ai_exists: bool,
    fs: &dyn Fs,
    config: &config::Config,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Error> {
    tracing::trace!(
        feature = %feature.file_path.display(),
        kind = ?feature.kind,
        source = ?feature.source_context.as_ref().map(|c| &c.source_type),
        "dispatching rules for feature"
    );

    let is_inside_ai = feature.source_context.as_ref().is_some_and(|ctx| ctx.source_type == ".ai");

    // 1. Quality rules — run on ALL features regardless of location.
    let quality_rules = rules::quality_rules_for_kind(&feature.kind, config);
    for rule in &quality_rules {
        if config.is_suppressed(rule.id()) {
            continue;
        }
        let rule_diagnostics = rule.check_file(&feature.file_path, fs)?;
        apply_rule_diagnostics(rule.as_ref(), rule_diagnostics, config, diagnostics);
    }

    // 2. Misplaced-features — run on features NOT inside .ai/, but NOT on instruction files.
    // Instruction files (CLAUDE.md, AGENTS.md, etc.) live at the repo root by design and
    // are not plugin features — flagging them as misplaced would always be a false positive.
    if !is_inside_ai && feature.kind != FeatureKind::Instructions {
        let rule = rules::misplaced_features_rule(feature, ai_exists);
        if !config.is_suppressed(rule.id()) {
            let rule_diagnostics = rule.check_file(&feature.file_path, fs)?;
            apply_rule_diagnostics(&rule, rule_diagnostics, config, diagnostics);
        }
    }

    Ok(())
}

/// Run the lint pipeline.
///
/// Performs a single unified recursive walk of the project tree
/// (gitignore-aware) to discover all AI plugin feature files, then runs
/// applicable quality rules and `source/misplaced-features` on each.
///
/// # Errors
///
/// Returns an error if a critical I/O or discovery failure prevents scanning.
pub fn lint(opts: &Options, fs: &dyn Fs) -> Result<Outcome, Error> {
    let mut all_diagnostics = Vec::new();
    let mut sources_scanned = Vec::new();

    // Single-pass: discover all feature files in the project tree.
    let features = crate::discovery::discover_features(&opts.dir, opts.max_depth)?;

    // Apply --source filter if provided.
    let features: Vec<_> = if let Some(ref source_filter) = opts.source {
        features
            .into_iter()
            .filter(|f| {
                f.source_context.as_ref().is_some_and(|ctx| ctx.source_type == *source_filter)
            })
            .collect()
    } else {
        features
    };

    // Track which source types were scanned (deduplicated).
    for f in &features {
        let src = f.source_context.as_ref().map_or("other", |ctx| ctx.source_type.as_str());
        if !sources_scanned.contains(&src.to_string()) {
            sources_scanned.push(src.to_string());
        }
    }

    // Determine whether a .ai/ marketplace exists (affects misplaced-features help text).
    let ai_exists = fs.exists(&opts.dir.join(".ai"));

    // Run rules per discovered feature.
    for feature in &features {
        run_rules_for_feature(feature, ai_exists, fs, &opts.config, &mut all_diagnostics)?;
    }

    // Sort by file path, then line, then column for consistent output.
    all_diagnostics.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.col.cmp(&b.col))
    });

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
    /// Maximum directory traversal depth for feature discovery.
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

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::Path;

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

    // --- apply_rule_diagnostics unit tests ---

    #[test]
    fn apply_rule_diagnostics_rule_ignore_path_filters_diagnostic() {
        // Covers the True branch of `is_ignored(&path_str, rule_ignores)` at the `||`
        // expression in `apply_rule_diagnostics`: when global `ignore_paths` is empty
        // but the per-rule ignore pattern matches the diagnostic's file path, the
        // diagnostic must be suppressed.
        struct StubRule;
        impl Rule for StubRule {
            fn id(&self) -> &'static str {
                "stub/per-rule-ignore"
            }
            fn name(&self) -> &'static str {
                "stub"
            }
            fn default_severity(&self) -> Severity {
                Severity::Warning
            }
            fn check_file(
                &self,
                _: &std::path::Path,
                _: &dyn crate::fs::Fs,
            ) -> Result<Vec<Diagnostic>, Error> {
                Ok(vec![])
            }
        }

        let make_diag = |path: &str| Diagnostic {
            rule_id: "stub/per-rule-ignore".into(),
            severity: Severity::Warning,
            message: "test".into(),
            file_path: PathBuf::from(path),
            line: None,
            col: None,
            end_line: None,
            end_col: None,
            source_type: ".claude".into(),
            help_text: None,
            help_url: None,
        };

        let mut cfg = config::Config::default();
        cfg.rule_overrides.insert(
            "stub/per-rule-ignore".to_string(),
            config::RuleOverride::Detailed {
                level: Some(Severity::Warning),
                ignore: vec!["vendor/**".to_string()],
                options: std::collections::BTreeMap::new(),
            },
        );

        let rule = StubRule;
        let diagnostics = vec![
            make_diag("vendor/foo/SKILL.md"), // matches rule ignore → filtered
            make_diag("src/bar/SKILL.md"),    // does not match → kept
        ];
        let mut output = Vec::new();
        apply_rule_diagnostics(&rule, diagnostics, &cfg, &mut output);

        assert_eq!(output.len(), 1, "vendor diagnostic should be filtered by per-rule ignore");
        assert_eq!(output[0].file_path, PathBuf::from("src/bar/SKILL.md"));
    }

    // --- Struct and error tests ---

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

    // --- Helper: write a minimal valid SKILL.md ---
    fn write_skill_md(dir: &Path, name: &str) {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join("SKILL.md");
        let mut f = std::fs::File::create(path).unwrap();
        writeln!(f, "---\nname: {name}\ndescription: test skill\n---\nbody").unwrap();
    }

    // --- Helper: write a minimal valid hooks.json ---
    fn write_hooks_json(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join("hooks.json");
        let mut f = std::fs::File::create(path).unwrap();
        writeln!(f, r#"{{"PreToolUse": []}}"#).unwrap();
    }

    // --- Integration tests ---

    #[test]
    fn lint_empty_dir_no_sources() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        assert!(outcome.sources_scanned.is_empty());
        assert!(outcome.diagnostics.is_empty());
    }

    #[test]
    fn lint_auto_discovers_sources() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create feature files in all three source dirs
        write_skill_md(&root.join(".claude").join("skills").join("default"), "claude-skill");
        write_skill_md(&root.join(".github").join("skills").join("default"), "github-skill");
        write_skill_md(&root.join(".ai").join("plugin").join("skills").join("default"), "ai-skill");

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        assert!(outcome.sources_scanned.contains(&".claude".to_string()));
        assert!(outcome.sources_scanned.contains(&".github".to_string()));
        assert!(outcome.sources_scanned.contains(&".ai".to_string()));
    }

    #[test]
    fn lint_with_source_filter() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        write_skill_md(&root.join(".ai").join("plugin").join("skills").join("default"), "test");

        let opts = Options {
            dir: root.to_path_buf(),
            source: Some(".ai".to_string()),
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        assert_eq!(outcome.sources_scanned, vec![".ai"]);
    }

    #[test]
    fn lint_sources_scanned_deduplicates_same_source_type() {
        // Two skills from the same `.ai` source → the `sources_scanned.contains`
        // duplicate-guard (the false/skip branch) must fire on the second feature.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        write_skill_md(
            &root.join(".ai").join("plugin-a").join("skills").join("default"),
            "skill-a",
        );
        write_skill_md(
            &root.join(".ai").join("plugin-b").join("skills").join("default"),
            "skill-b",
        );
        // Add a .claude skill so sources_scanned contains both ".ai" and ".claude".
        write_skill_md(&root.join(".claude").join("skills").join("default"), "claude-skill");

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let outcome = lint(&opts, &crate::fs::Real).unwrap();

        // Both .ai features share the ".ai" source — it must appear exactly once.
        assert!(outcome.sources_scanned.contains(&".ai".to_string()));
        assert!(outcome.sources_scanned.contains(&".claude".to_string()));
        let ai_count = outcome.sources_scanned.iter().filter(|s| s.as_str() == ".ai").count();
        assert_eq!(ai_count, 1, "'.ai' source should appear exactly once in sources_scanned");
    }

    #[test]
    fn lint_config_allow_suppresses_rules() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // SKILL.md without a name field → triggers skill/missing-name
        let skill_dir = root.join(".ai").join("plugin").join("skills").join("default");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let path = skill_dir.join("SKILL.md");
        let mut f = std::fs::File::create(path).unwrap();
        writeln!(f, "---\ndescription: no name here\n---\nbody").unwrap();

        let mut config = config::Config::default();
        config.rule_overrides.insert("skill/missing-name".to_string(), config::RuleOverride::Allow);

        let opts = Options {
            dir: root.to_path_buf(),
            source: Some(".ai".to_string()),
            config,
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        // skill/missing-name should be suppressed
        assert!(!outcome.diagnostics.iter().any(|d| d.rule_id == "skill/missing-name"));
    }

    #[test]
    fn lint_severity_override_applied() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // SKILL.md without description → triggers skill/missing-description (Warning by default)
        let skill_dir = root.join(".ai").join("plugin").join("skills").join("default");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let path = skill_dir.join("SKILL.md");
        let mut f = std::fs::File::create(path).unwrap();
        writeln!(f, "---\nname: test\n---\nbody").unwrap();

        let mut config = config::Config::default();
        // Override missing-description from warning to error
        config.rule_overrides.insert(
            "skill/missing-description".to_string(),
            config::RuleOverride::Level(Severity::Error),
        );

        let opts = Options {
            dir: root.to_path_buf(),
            source: Some(".ai".to_string()),
            config,
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        let desc_diag =
            outcome.diagnostics.iter().find(|d| d.rule_id == "skill/missing-description");
        assert!(desc_diag.is_some());
        assert_eq!(desc_diag.map(|d| d.severity), Some(Severity::Error));
        assert!(outcome.error_count > 0);
    }

    // --- Recursive discovery integration tests ---

    #[test]
    fn lint_discovers_nested_claude_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create a SKILL.md nested inside .claude/ — should trigger misplaced-features
        write_skill_md(
            &root.join("packages").join("auth").join(".claude").join("skills").join("default"),
            "nested-skill",
        );

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        assert!(outcome.diagnostics.iter().any(|d| d.rule_id == "source/misplaced-features"));
        let diag =
            outcome.diagnostics.iter().find(|d| d.rule_id == "source/misplaced-features").unwrap();
        let path_str = diag.file_path.display().to_string();
        assert!(path_str.contains("auth"));
    }

    #[test]
    fn lint_discovers_nested_github_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        write_hooks_json(&root.join("packages").join("api").join(".github").join("hooks"));

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        assert!(outcome
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "source/misplaced-features" && d.source_type == ".github"));
    }

    #[test]
    fn lint_source_filter_claude_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        write_skill_md(&root.join(".claude").join("skills").join("default"), "claude-skill");
        write_skill_md(&root.join(".github").join("skills").join("default"), "github-skill");

        let opts = Options {
            dir: root.to_path_buf(),
            source: Some(".claude".to_string()),
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        // Only .claude diagnostics should appear
        assert!(outcome.diagnostics.iter().all(|d| d.source_type == ".claude"));
        assert!(!outcome.diagnostics.is_empty());
    }

    #[test]
    fn lint_source_filter_ai_skips_discovery() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        write_skill_md(&root.join(".ai").join("plugin").join("skills").join("default"), "ai-skill");
        write_skill_md(&root.join(".claude").join("skills").join("default"), "claude-skill");

        let opts = Options {
            dir: root.to_path_buf(),
            source: Some(".ai".to_string()),
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        // No misplaced-features diagnostics (those come from .claude, which is filtered)
        assert!(!outcome.diagnostics.iter().any(|d| d.rule_id == "source/misplaced-features"));
        assert_eq!(outcome.sources_scanned, vec![".ai"]);
    }

    #[test]
    fn lint_max_depth_limits_discovery() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Root-level SKILL.md at depth 4 from project root:
        //   .claude/skills/default/SKILL.md
        write_skill_md(&root.join(".claude").join("skills").join("default"), "root-skill");

        // Deep SKILL.md at depth 6 from project root:
        //   packages/auth/.claude/skills/default/SKILL.md
        write_skill_md(
            &root.join("packages").join("auth").join(".claude").join("skills").join("default"),
            "nested-skill",
        );

        // max_depth=5 should find depth-4 file but not depth-6 file
        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: Some(5),
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        let misplaced: Vec<_> = outcome
            .diagnostics
            .iter()
            .filter(|d| d.rule_id == "source/misplaced-features")
            .collect();
        assert_eq!(misplaced.len(), 1);
    }

    #[test]
    fn lint_no_sources_found_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Only create src/ -- no .claude, .github, or .ai
        std::fs::create_dir_all(root.join("src")).unwrap();

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        assert!(outcome.sources_scanned.is_empty());
        assert!(outcome.diagnostics.is_empty());
    }

    #[test]
    fn lint_deduplicates_sources_scanned() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Multiple .claude dirs — add a SKILL.md in one of them
        write_skill_md(&root.join(".claude").join("skills").join("default"), "root-skill");
        write_skill_md(
            &root.join("packages").join("a").join(".claude").join("skills").join("default"),
            "a-skill",
        );
        write_skill_md(
            &root.join("packages").join("b").join(".claude").join("skills").join("default"),
            "b-skill",
        );

        let opts = Options {
            dir: root.to_path_buf(),
            source: Some(".claude".to_string()),
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        // sources_scanned should have exactly one ".claude" entry despite 3 dirs
        let claude_count =
            outcome.sources_scanned.iter().filter(|s| s.as_str() == ".claude").count();
        assert_eq!(claude_count, 1);
    }

    #[test]
    fn lint_source_github_filters_discovery() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        write_skill_md(&root.join(".github").join("skills").join("default"), "github-skill");
        write_skill_md(&root.join(".ai").join("plugin").join("skills").join("default"), "ai-skill");

        let opts = Options {
            dir: root.to_path_buf(),
            source: Some(".github".to_string()),
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        assert!(outcome.diagnostics.iter().all(|d| d.source_type == ".github"));
        assert!(!outcome.diagnostics.is_empty());
        // .ai should NOT be scanned
        assert!(!outcome.sources_scanned.contains(&".ai".to_string()));
    }

    #[test]
    fn lint_ignore_paths_filter_source_diagnostics() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        write_skill_md(&root.join(".claude").join("skills").join("default"), "root-skill");
        write_skill_md(
            &root.join("packages").join("ignored").join(".claude").join("skills").join("default"),
            "ignored-skill",
        );

        let mut cfg = config::Config::default();
        cfg.ignore_paths = vec!["**/ignored/**".to_string()];

        let opts = Options { dir: root.to_path_buf(), source: None, config: cfg, max_depth: None };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        let misplaced: Vec<_> = outcome
            .diagnostics
            .iter()
            .filter(|d| d.rule_id == "source/misplaced-features")
            .collect();
        // Root .claude/skills should be found, but not the ignored one
        assert_eq!(misplaced.len(), 1);
        assert!(misplaced.iter().all(|d| {
            let path_str = d.file_path.display().to_string();
            !path_str.contains("ignored")
        }));
    }

    #[test]
    fn lint_config_suppress_misplaced_features() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        write_skill_md(&root.join(".claude").join("skills").join("default"), "claude-skill");

        let mut cfg = config::Config::default();
        cfg.rule_overrides
            .insert("source/misplaced-features".to_string(), config::RuleOverride::Allow);

        let opts = Options { dir: root.to_path_buf(), source: None, config: cfg, max_depth: None };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        // misplaced-features should be suppressed
        assert!(!outcome.diagnostics.iter().any(|d| d.rule_id == "source/misplaced-features"));
    }

    /// Bug fix for issue #208: misplaced-features must fire even when `.ai/` does NOT exist.
    #[test]
    fn lint_no_marketplace_fires_misplaced_features() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // .claude/skills/default/SKILL.md exists but NO .ai — rule must still fire
        write_skill_md(&root.join(".claude").join("skills").join("default"), "claude-skill");

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        // The bug fix: misplaced-features fires regardless of .ai/ existence
        assert!(outcome.diagnostics.iter().any(|d| d.rule_id == "source/misplaced-features"));
        // Help text should mention aipm init (since .ai/ doesn't exist)
        let diag =
            outcome.diagnostics.iter().find(|d| d.rule_id == "source/misplaced-features").unwrap();
        let help = diag.help_text.as_deref().unwrap_or("");
        assert!(help.contains("aipm init"));
    }

    /// Instruction files (CLAUDE.md, AGENTS.md, etc.) live at the repo root by design and
    /// must NOT trigger `source/misplaced-features` even though they are outside `.ai/`.
    ///
    /// A skill outside `.ai/` (in `.claude/`) MUST still trigger the rule — this ensures
    /// the exemption is narrowly scoped to `FeatureKind::Instructions`, not all outside-`.ai/`
    /// features. The test covers both branches of the `kind != FeatureKind::Instructions` guard.
    #[test]
    fn lint_instruction_files_not_flagged_as_misplaced() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // CLAUDE.md at the repo root — Instructions feature outside .ai/ (must NOT be flagged)
        std::fs::write(root.join("CLAUDE.md"), "# Project Rules\n\nSome rules here.\n").unwrap();

        // A skill in .claude/ — Skill feature outside .ai/ (MUST still be flagged)
        write_skill_md(&root.join(".claude").join("skills").join("misplaced"), "misplaced-skill");

        // .ai/ exists so the help text uses "aipm migrate" path
        std::fs::create_dir_all(root.join(".ai").join(".claude-plugin")).unwrap();

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();

        // The skill in .claude/ must still be flagged as misplaced
        assert!(
            outcome.diagnostics.iter().any(|d| d.rule_id == "source/misplaced-features"),
            "source/misplaced-features must still fire for skills outside .ai/"
        );

        // CLAUDE.md must NOT appear in any misplaced-features diagnostic
        let misplaced_on_claude: Vec<_> = outcome
            .diagnostics
            .iter()
            .filter(|d| {
                d.rule_id == "source/misplaced-features"
                    && d.file_path.file_name().is_some_and(|n| n == "CLAUDE.md")
            })
            .collect();
        assert!(
            misplaced_on_claude.is_empty(),
            "source/misplaced-features must not fire on CLAUDE.md (instruction files are not plugin features)"
        );
    }

    /// Covers the `is_ignored(&path_str, rule_ignores)` True branch.
    ///
    /// When global `ignore_paths` is empty the first check returns False, so the
    /// second `is_ignored` call (per-rule ignore patterns from `RuleOverride::Detailed`)
    /// is the only gate.  A path matching that per-rule pattern must be skipped while
    /// a path that does not match still appears in the output.
    #[test]
    fn lint_rule_ignore_paths_filter_diagnostics() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Root .claude/skills — should NOT be filtered
        write_skill_md(&root.join(".claude").join("skills").join("default"), "root-skill");
        // A nested .claude/skills under a "vendor" package — should be filtered by rule ignore
        write_skill_md(
            &root.join("packages").join("vendor").join(".claude").join("skills").join("default"),
            "vendor-skill",
        );

        let mut cfg = config::Config::default();
        // Per-rule ignore for misplaced-features: suppress diagnostics under "vendor/".
        cfg.rule_overrides.insert(
            "source/misplaced-features".to_string(),
            config::RuleOverride::Detailed {
                level: Some(Severity::Warning),
                ignore: vec!["**/vendor/**".to_string()],
                options: std::collections::BTreeMap::new(),
            },
        );

        let opts = Options { dir: root.to_path_buf(), source: None, config: cfg, max_depth: None };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();

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

    /// Covers the `is_ignored(&path_str, &config.ignore_paths)` True branch at
    /// line 60 of `apply_rule_diagnostics`.
    ///
    /// When `config.ignore_paths` matches a diagnostic's file path the first
    /// `is_ignored` call returns `true`, short-circuiting the `||` and skipping
    /// that diagnostic before the per-rule check is ever reached.
    #[test]
    fn lint_global_ignore_paths_filter_diagnostics() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Skill outside .ai/ under "vendor/" → would normally trigger misplaced-features
        write_skill_md(
            &root.join("vendor").join(".claude").join("skills").join("default"),
            "vendor-skill",
        );
        // Skill outside .ai/ under ".claude/" → should still appear in diagnostics
        write_skill_md(&root.join(".claude").join("skills").join("default"), "root-skill");

        let mut cfg = config::Config::default();
        // Global ignore: suppress ALL diagnostics whose path contains "vendor".
        cfg.ignore_paths = vec!["**/vendor/**".to_string()];

        let opts = Options { dir: root.to_path_buf(), source: None, config: cfg, max_depth: None };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok(), "lint should succeed: {:?}", result.err());
        let outcome = result.unwrap();

        let misplaced: Vec<_> = outcome
            .diagnostics
            .iter()
            .filter(|d| d.rule_id == "source/misplaced-features")
            .collect();

        // Root .claude/skills must still appear
        assert!(
            misplaced.iter().any(|d| !d.file_path.display().to_string().contains("vendor")),
            "root .claude/skills diagnostic should remain"
        );
        // Vendor path must be suppressed by the global ignore_paths
        assert!(
            !misplaced.iter().any(|d| d.file_path.display().to_string().contains("vendor")),
            "vendor diagnostic should be filtered by global ignore_paths"
        );
    }

    // --- Helpers for marketplace/plugin integration tests ---

    fn write_marketplace_json(dir: &Path, content: &str) {
        let mp_dir = dir.join(".ai").join(".claude-plugin");
        std::fs::create_dir_all(&mp_dir).unwrap();
        std::fs::write(mp_dir.join("marketplace.json"), content).unwrap();
    }

    fn write_plugin_json(dir: &Path, plugin_name: &str, content: &str) {
        let pj_dir = dir.join(".ai").join(plugin_name).join(".claude-plugin");
        std::fs::create_dir_all(&pj_dir).unwrap();
        std::fs::write(pj_dir.join("plugin.json"), content).unwrap();
    }

    // --- Integration tests for marketplace/plugin rules ---

    #[test]
    fn lint_valid_marketplace_and_plugin_no_new_rule_diagnostics() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        write_marketplace_json(
            root,
            r#"{"plugins":[{"name":"my-plugin","source":"./my-plugin"}]}"#,
        );
        write_plugin_json(
            root,
            "my-plugin",
            r#"{"name":"my-plugin","version":"0.1.0","description":"A plugin","author":{"name":"Dev","email":"dev@example.com"}}"#,
        );

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let outcome = lint(&opts, &crate::fs::Real).unwrap();
        let new_rule_ids = [
            "marketplace/source-resolve",
            "marketplace/plugin-field-mismatch",
            "plugin/missing-manifest",
            "plugin/missing-registration",
            "plugin/required-fields",
        ];
        assert!(
            !outcome.diagnostics.iter().any(|d| new_rule_ids.contains(&d.rule_id.as_str())),
            "got unexpected diagnostics: {:?}",
            outcome.diagnostics.iter().map(|d| &d.rule_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn lint_marketplace_source_not_found_emits_diagnostic() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        write_marketplace_json(
            root,
            r#"{"plugins":[{"name":"missing-plugin","source":"./missing-plugin"}]}"#,
        );

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let outcome = lint(&opts, &crate::fs::Real).unwrap();
        assert!(outcome.diagnostics.iter().any(|d| d.rule_id == "marketplace/source-resolve"));
    }

    #[test]
    fn lint_plugin_json_missing_required_fields_emits_diagnostics() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // name is missing, author is missing
        write_plugin_json(root, "my-plugin", r#"{"version":"0.1.0","description":"A plugin"}"#);

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let outcome = lint(&opts, &crate::fs::Real).unwrap();
        let diags: Vec<_> =
            outcome.diagnostics.iter().filter(|d| d.rule_id == "plugin/required-fields").collect();
        assert!(!diags.is_empty());
        assert!(diags.iter().any(|d| d.message.contains("name")));
    }

    #[test]
    fn lint_plugin_missing_manifest_emits_diagnostic() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Plugin dir exists but no plugin.json inside it
        std::fs::create_dir_all(root.join(".ai").join("my-plugin")).unwrap();
        write_marketplace_json(
            root,
            r#"{"plugins":[{"name":"my-plugin","source":"./my-plugin"}]}"#,
        );

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let outcome = lint(&opts, &crate::fs::Real).unwrap();
        assert!(outcome.diagnostics.iter().any(|d| d.rule_id == "plugin/missing-manifest"));
    }

    #[test]
    fn lint_plugin_missing_registration_emits_diagnostic() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Plugin dir exists but marketplace.json doesn't register it
        std::fs::create_dir_all(root.join(".ai").join("unregistered")).unwrap();
        write_marketplace_json(root, r#"{"plugins":[]}"#);

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let outcome = lint(&opts, &crate::fs::Real).unwrap();
        assert!(outcome.diagnostics.iter().any(|d| d.rule_id == "plugin/missing-registration"));
    }

    #[test]
    fn lint_marketplace_field_mismatch_emits_diagnostic() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        write_marketplace_json(
            root,
            r#"{"plugins":[{"name":"foo","description":"wrong","source":"./foo"}]}"#,
        );
        write_plugin_json(
            root,
            "foo",
            r#"{"name":"foo","version":"0.1.0","description":"right","author":{"name":"Dev","email":"dev@example.com"}}"#,
        );

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let outcome = lint(&opts, &crate::fs::Real).unwrap();
        assert!(outcome
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "marketplace/plugin-field-mismatch"));
    }

    #[test]
    fn lint_new_rules_configurable_via_allow() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // plugin.json missing required fields — would normally trigger plugin/required-fields
        write_plugin_json(root, "my-plugin", r#"{"version":"0.1.0"}"#);

        let mut config = config::Config::default();
        config
            .rule_overrides
            .insert("plugin/required-fields".to_string(), config::RuleOverride::Allow);

        let opts = Options { dir: root.to_path_buf(), source: None, config, max_depth: None };
        let outcome = lint(&opts, &crate::fs::Real).unwrap();
        assert!(
            !outcome.diagnostics.iter().any(|d| d.rule_id == "plugin/required-fields"),
            "plugin/required-fields should be suppressed by RuleOverride::Allow"
        );
    }

    // --- Sorting tests ---

    #[test]
    fn diagnostics_sort_by_file_then_line_then_col() {
        let mut diags = vec![
            Diagnostic {
                rule_id: "r1".into(),
                severity: Severity::Warning,
                message: "m".into(),
                file_path: PathBuf::from("b.md"),
                line: Some(5),
                col: Some(10),
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: None,
                help_url: None,
            },
            Diagnostic {
                rule_id: "r2".into(),
                severity: Severity::Error,
                message: "m".into(),
                file_path: PathBuf::from("a.md"),
                line: Some(3),
                col: None,
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: None,
                help_url: None,
            },
            Diagnostic {
                rule_id: "r3".into(),
                severity: Severity::Warning,
                message: "m".into(),
                file_path: PathBuf::from("a.md"),
                line: Some(1),
                col: Some(5),
                end_line: None,
                end_col: None,
                source_type: ".ai".into(),
                help_text: None,
                help_url: None,
            },
        ];

        diags.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.line.cmp(&b.line))
                .then_with(|| a.col.cmp(&b.col))
        });

        assert_eq!(diags[0].rule_id, "r3"); // a.md:1:5
        assert_eq!(diags[1].rule_id, "r2"); // a.md:3
        assert_eq!(diags[2].rule_id, "r1"); // b.md:5:10
    }

    #[test]
    fn lint_misplaced_features_suppressed_by_allow_config() {
        // Covers the False branch of `if !config.is_suppressed(rule.id())` in
        // `dispatch_rules_for_feature` (lint/mod.rs line 97): when the
        // `source/misplaced-features` rule is set to Allow, the branch body is
        // skipped and no diagnostic is emitted for an out-of-place feature.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create a SKILL.md outside .ai/ so it would normally trigger
        // source/misplaced-features.
        write_skill_md(&root.join(".claude").join("skills").join("default"), "misplaced-skill");

        let mut config = config::Config::default();
        config
            .rule_overrides
            .insert("source/misplaced-features".to_string(), config::RuleOverride::Allow);

        let opts = Options {
            dir: root.to_path_buf(),
            source: Some(".claude".to_string()),
            config,
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        assert!(
            !outcome.diagnostics.iter().any(|d| d.rule_id == "source/misplaced-features"),
            "source/misplaced-features should be suppressed when set to Allow"
        );
    }

    // --- instructions/oversized integration tests ---

    #[test]
    fn lint_discovers_oversized_instruction_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create a CLAUDE.md with >100 lines
        let content: String = (0..110).map(|i| format!("line {i}\n")).collect();
        std::fs::write(root.join("CLAUDE.md"), &content).unwrap();
        // Create .ai/ marker
        std::fs::create_dir_all(root.join(".ai")).unwrap();

        let opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        assert!(
            outcome.diagnostics.iter().any(|d| d.rule_id == "instructions/oversized"),
            "should detect oversized CLAUDE.md"
        );
    }

    #[test]
    fn lint_config_overrides_thresholds() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // 60 lines — under default 100, over custom 50
        let content: String = (0..60).map(|i| format!("line {i}\n")).collect();
        std::fs::write(root.join("CLAUDE.md"), &content).unwrap();
        std::fs::create_dir_all(root.join(".ai")).unwrap();

        let mut cfg = config::Config::default();
        let mut opts_map = std::collections::BTreeMap::new();
        opts_map.insert("lines".to_string(), toml::Value::Integer(50));
        cfg.rule_overrides.insert(
            "instructions/oversized".to_string(),
            config::RuleOverride::Detailed {
                level: Some(Severity::Warning),
                ignore: vec![],
                options: opts_map,
            },
        );

        let opts = Options { dir: root.to_path_buf(), source: None, config: cfg, max_depth: None };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        assert!(
            outcome.diagnostics.iter().any(|d| d.rule_id == "instructions/oversized"),
            "custom threshold of 50 lines should trigger on 60-line file"
        );
    }

    #[test]
    fn lint_config_allow_suppresses_instructions_oversized() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Oversized CLAUDE.md
        let content: String = (0..110).map(|i| format!("line {i}\n")).collect();
        std::fs::write(root.join("CLAUDE.md"), &content).unwrap();
        std::fs::create_dir_all(root.join(".ai")).unwrap();

        let mut cfg = config::Config::default();
        cfg.rule_overrides
            .insert("instructions/oversized".to_string(), config::RuleOverride::Allow);

        let opts = Options { dir: root.to_path_buf(), source: None, config: cfg, max_depth: None };
        let result = lint(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.unwrap();
        assert!(
            !outcome.diagnostics.iter().any(|d| d.rule_id == "instructions/oversized"),
            "instructions/oversized should be suppressed by allow"
        );
    }

    // --- init-then-lint integration test ---

    #[test]
    fn lint_after_init_produces_zero_diagnostics() {
        // Verifies that `aipm init` (with marketplace + manifest) produces a
        // workspace that passes `aipm lint` with zero diagnostics (#356).
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Step 1: initialise workspace with marketplace + starter plugin
        let init_opts = crate::workspace_init::Options {
            dir: root,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
        };
        let adaptors = crate::workspace_init::adaptors::defaults();
        crate::workspace_init::init(&init_opts, &adaptors, &crate::fs::Real).unwrap();

        // Step 2: lint the initialised workspace
        let lint_opts = Options {
            dir: root.to_path_buf(),
            source: None,
            config: config::Config::default(),
            max_depth: None,
        };
        let outcome = lint(&lint_opts, &crate::fs::Real).unwrap();

        // Step 3: assert zero diagnostics
        assert!(
            outcome.diagnostics.is_empty(),
            "freshly initialised workspace should produce zero lint diagnostics, got: {:#?}",
            outcome.diagnostics,
        );

        // Step 4: verify starter plugin has skills and agents fields in plugin.json
        let plugin_json_path =
            root.join(".ai").join("starter-aipm-plugin").join(".claude-plugin").join("plugin.json");
        let plugin_json_content = std::fs::read_to_string(&plugin_json_path).unwrap();
        let plugin_json: serde_json::Value = serde_json::from_str(&plugin_json_content).unwrap();
        assert!(plugin_json.get("skills").is_some(), "plugin.json should contain a 'skills' field");
        assert!(
            plugin_json.get("agents").is_some(),
            "plugin.json should contain an 'agents' field"
        );

        // Step 5: verify starter skill and agent have proper frontmatter
        let skill_path = root
            .join(".ai")
            .join("starter-aipm-plugin")
            .join("skills")
            .join("scaffold-plugin")
            .join("SKILL.md");
        assert!(skill_path.exists(), "starter skill SKILL.md should exist");
        let skill_content = std::fs::read_to_string(&skill_path).unwrap();
        assert!(skill_content.starts_with("---"), "starter skill should have YAML frontmatter");

        let agent_path = root
            .join(".ai")
            .join("starter-aipm-plugin")
            .join("agents")
            .join("marketplace-scanner.md");
        assert!(agent_path.exists(), "starter agent .md should exist");
        let agent_content = std::fs::read_to_string(&agent_path).unwrap();
        assert!(agent_content.starts_with("---"), "starter agent should have YAML frontmatter");
    }
}
