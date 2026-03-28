//! Migration pipeline: scan AI tool configurations and convert to marketplace plugins.

pub mod agent_detector;
pub mod cleanup;
pub mod command_detector;
pub mod detector;
pub mod discovery;
pub mod dry_run;
pub mod emitter;
pub mod hook_detector;
pub mod mcp_detector;
pub mod output_style_detector;
pub mod registrar;
pub mod skill_detector;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::fs::Fs;

/// What kind of artifact was detected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArtifactKind {
    /// A skill from `.claude/skills/<name>/`.
    Skill,
    /// A legacy command from `.claude/commands/<name>.md`.
    Command,
    /// A subagent from `.claude/agents/<name>.md`.
    Agent,
    /// MCP server configs from `.mcp.json` at the project root.
    McpServer,
    /// Hooks extracted from `.claude/settings.json`.
    Hook,
    /// An output style from `.claude/output-styles/<name>.md`.
    OutputStyle,
}

impl ArtifactKind {
    /// Returns a human-readable type string for display and manifest generation.
    pub const fn to_type_string(&self) -> &'static str {
        match self {
            Self::Skill | Self::Command => "skill",
            Self::Agent => "agent",
            Self::McpServer => "mcp",
            Self::Hook => "hook",
            // OutputStyle has no standalone PluginType; always composite when mixed
            Self::OutputStyle => "composite",
        }
    }
}

/// Metadata extracted from a skill's YAML frontmatter.
#[derive(Debug, Clone, Default)]
pub struct ArtifactMetadata {
    /// Explicit name from frontmatter.
    pub name: Option<String>,
    /// Description from frontmatter.
    pub description: Option<String>,
    /// Raw YAML/JSON hooks block from frontmatter.
    pub hooks: Option<String>,
    /// Whether model invocation should be disabled (always true for commands).
    pub model_invocation_disabled: bool,
    /// Raw file content for config-based artifacts (MCP JSON, hooks JSON, etc.).
    /// Used by the emitter for pass-through without re-serialization.
    pub raw_content: Option<String>,
}

/// Strip matching surrounding YAML quote delimiters from a scalar value.
///
/// Handles both double-quoted (`"..."`) and single-quoted (`'...'`) YAML scalars.
/// Returns the inner content if delimiters match, otherwise returns the input unchanged.
pub(crate) fn strip_yaml_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    match (bytes.first(), bytes.last()) {
        (Some(b'"'), Some(b'"')) | (Some(b'\''), Some(b'\'')) if bytes.len() >= 2 => {
            &s[1..s.len() - 1]
        },
        _ => s,
    }
}

/// A single detected artifact from a source folder.
#[derive(Debug, Clone)]
pub struct Artifact {
    /// What kind of artifact this is.
    pub kind: ArtifactKind,
    /// Artifact name (e.g., "deploy", "lint-fix").
    pub name: String,
    /// Absolute path to the source (e.g., `.claude/skills/deploy/`).
    pub source_path: PathBuf,
    /// All files relative to `source_path`.
    pub files: Vec<PathBuf>,
    /// Script paths referenced in the body.
    pub referenced_scripts: Vec<PathBuf>,
    /// Parsed metadata.
    pub metadata: ArtifactMetadata,
}

/// Options for the migrate command.
pub struct Options<'a> {
    /// Project root directory.
    pub dir: &'a Path,
    /// Source folder name (e.g., ".claude").
    /// When `None`, recursive discovery is used.
    /// When `Some`, only that single directory under `dir` is scanned (legacy behavior).
    pub source: Option<&'a str>,
    /// Whether to run in dry-run mode (report only, no writes).
    pub dry_run: bool,
    /// Whether `--destructive` was passed. Affects the dry-run report only;
    /// actual cleanup is handled at the CLI layer.
    pub destructive: bool,
    /// Maximum directory traversal depth for recursive discovery.
    /// `None` means unlimited. Ignored when `source` is `Some`.
    pub max_depth: Option<usize>,
    /// Generate `aipm.toml` plugin manifests (opt-in).
    pub manifest: bool,
}

/// A single action taken (or planned) during migration.
#[derive(Debug, Clone)]
pub enum Action {
    /// A plugin directory was created.
    PluginCreated {
        /// Final plugin name.
        name: String,
        /// Source path of the original artifact.
        source: PathBuf,
        /// Plugin type (e.g., "skill").
        plugin_type: String,
    },
    /// A plugin was registered in marketplace.json.
    MarketplaceRegistered {
        /// Plugin name.
        name: String,
    },
    /// A plugin was renamed due to a name conflict.
    Renamed {
        /// Original artifact name.
        original_name: String,
        /// New plugin name after renaming.
        new_name: String,
        /// Reason for the rename.
        reason: String,
    },
    /// An artifact was skipped.
    Skipped {
        /// Artifact name.
        name: String,
        /// Reason for skipping.
        reason: String,
    },
    /// A dry-run report was generated.
    DryRunReport {
        /// Path to the generated report file.
        path: PathBuf,
    },
    /// A migrated source file was removed during cleanup.
    SourceFileRemoved {
        /// Path to the removed file.
        path: PathBuf,
    },
    /// An empty source directory was removed after cleanup.
    SourceDirRemoved {
        /// Path to the removed directory.
        path: PathBuf,
    },
}

/// Result of migration.
pub struct Outcome {
    /// Actions taken during migration.
    pub actions: Vec<Action>,
}

impl Outcome {
    /// Returns `true` if at least one `PluginCreated` action exists.
    pub fn has_migrated_artifacts(&self) -> bool {
        self.actions.iter().any(|a| matches!(a, Action::PluginCreated { .. }))
    }

    /// Returns the source paths of all successfully migrated artifacts.
    pub fn migrated_source_paths(&self) -> Vec<&Path> {
        self.actions
            .iter()
            .filter_map(|a| match a {
                Action::PluginCreated { source, .. } => Some(source.as_path()),
                _ => None,
            })
            .collect()
    }
}

/// Errors specific to migration.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The `.ai/` marketplace directory does not exist.
    #[error("marketplace directory does not exist at {0} — run `aipm init --marketplace` first")]
    MarketplaceNotFound(PathBuf),

    /// The source directory does not exist.
    #[error("source directory does not exist: {0}")]
    SourceNotFound(PathBuf),

    /// The source type is not supported.
    #[error("unsupported source type '{0}' — supported sources: .claude")]
    UnsupportedSource(String),

    /// Failed to parse marketplace.json.
    #[error("failed to parse marketplace.json at {path}: {source}")]
    MarketplaceJsonParse {
        /// Path to the marketplace.json file.
        path: PathBuf,
        /// The underlying parse error.
        source: serde_json::Error,
    },

    /// Failed to parse SKILL.md frontmatter.
    #[error("failed to parse SKILL.md frontmatter in {path}: {reason}")]
    FrontmatterParse {
        /// Path to the SKILL.md file.
        path: PathBuf,
        /// Description of the parse failure.
        reason: String,
    },

    /// Failed to parse a JSON configuration file.
    #[error("failed to parse {path}: {reason}")]
    ConfigParse {
        /// Path to the configuration file.
        path: PathBuf,
        /// Description of the parse failure.
        reason: String,
    },

    /// Discovery failed during recursive directory walking.
    #[error("failed to discover .claude directories: {0}")]
    DiscoveryFailed(String),

    /// An I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Data needed to register a plugin in `marketplace.json`.
#[derive(Debug, Clone)]
pub struct PluginEntry {
    /// Plugin name.
    pub name: String,
    /// Plugin description (from artifact metadata).
    pub description: Option<String>,
}

/// A planned plugin to emit, which may contain artifacts from multiple detectors.
#[derive(Debug, Clone)]
pub struct PluginPlan {
    /// The plugin name (package name or individual artifact name).
    pub name: String,
    /// All artifacts to include in this plugin.
    pub artifacts: Vec<Artifact>,
    /// Whether this was merged from a package (true) or is a single artifact (false).
    pub is_package_scoped: bool,
    /// The `.claude/` directory this plan originated from (for report accuracy).
    pub source_dir: PathBuf,
}

/// Run the migration pipeline.
pub fn migrate(opts: &Options<'_>, fs: &dyn Fs) -> Result<Outcome, Error> {
    let ai_dir = opts.dir.join(".ai");

    // 1. Validate .ai/ exists
    if !fs.exists(&ai_dir) {
        return Err(Error::MarketplaceNotFound(ai_dir));
    }

    opts.source.map_or_else(
        || {
            migrate_recursive(
                opts.dir,
                opts.max_depth,
                opts.dry_run,
                opts.destructive,
                opts.manifest,
                &ai_dir,
                fs,
            )
        },
        |source| {
            migrate_single_source(
                opts.dir,
                source,
                opts.dry_run,
                opts.destructive,
                opts.manifest,
                &ai_dir,
                fs,
            )
        },
    )
}

/// Legacy single-path migration mode (when `--source` is explicitly provided).
fn migrate_single_source(
    dir: &Path,
    source: &str,
    dry_run: bool,
    destructive: bool,
    manifest: bool,
    ai_dir: &Path,
    fs: &dyn Fs,
) -> Result<Outcome, Error> {
    let source_dir = dir.join(source);

    if !fs.exists(&source_dir) {
        return Err(Error::SourceNotFound(source_dir));
    }

    let detectors = match source {
        ".claude" => detector::claude_detectors(),
        other => return Err(Error::UnsupportedSource(other.to_string())),
    };

    let mut all_artifacts = Vec::new();
    for det in &detectors {
        let artifacts = det.detect(&source_dir, fs)?;
        all_artifacts.extend(artifacts);
    }

    let existing_plugins = collect_existing_plugin_names(ai_dir, fs)?;

    if dry_run {
        let report = dry_run::generate_report(
            &all_artifacts,
            &existing_plugins,
            source,
            manifest,
            destructive,
        );
        let report_path = dir.join("aipm-migrate-dryrun-report.md");
        fs.write_file(&report_path, report.as_bytes())?;
        return Ok(Outcome { actions: vec![Action::DryRunReport { path: report_path }] });
    }

    let mut actions = Vec::new();
    let mut registered_entries = Vec::new();
    let mut known_names = existing_plugins;
    let mut rename_counter = 0u32;

    for artifact in &all_artifacts {
        let (plugin_name, emit_actions) = emitter::emit_plugin(
            artifact,
            ai_dir,
            &known_names,
            &mut rename_counter,
            manifest,
            fs,
        )?;
        actions.extend(emit_actions);
        known_names.insert(plugin_name.clone());
        registered_entries.push(PluginEntry {
            name: plugin_name,
            description: artifact.metadata.description.clone(),
        });
    }

    registrar::register_plugins(ai_dir, &registered_entries, fs)?;
    for entry in &registered_entries {
        actions.push(Action::MarketplaceRegistered { name: entry.name.clone() });
    }

    Ok(Outcome { actions })
}

/// Recursive discovery migration mode (when `--source` is not provided).
fn migrate_recursive(
    dir: &Path,
    max_depth: Option<usize>,
    dry_run: bool,
    destructive: bool,
    manifest: bool,
    ai_dir: &Path,
    fs: &dyn Fs,
) -> Result<Outcome, Error> {
    use rayon::prelude::*;

    let discovered = discovery::discover_claude_dirs(dir, max_depth)?;
    if discovered.is_empty() {
        return Ok(Outcome { actions: Vec::new() });
    }

    // Parallel detection: run detectors across discovered dirs concurrently
    let detection_results: Vec<Result<Vec<PluginPlan>, Error>> = discovered
        .par_iter()
        .map(|src| {
            let detectors = detector::claude_detectors();
            let mut all_artifacts = Vec::new();
            for det in &detectors {
                let artifacts = det.detect(&src.claude_dir, fs)?;
                all_artifacts.extend(artifacts);
            }

            if let Some(ref pkg_name) = src.package_name {
                // Package-scoped: merge all artifacts under one plugin
                Ok(vec![PluginPlan {
                    name: pkg_name.clone(),
                    artifacts: all_artifacts,
                    is_package_scoped: true,
                    source_dir: src.claude_dir.clone(),
                }])
            } else {
                // Root-level: each artifact becomes its own plugin
                let source = src.claude_dir.clone();
                Ok(all_artifacts
                    .into_iter()
                    .map(|a| PluginPlan {
                        name: a.name.clone(),
                        artifacts: vec![a],
                        is_package_scoped: false,
                        source_dir: source.clone(),
                    })
                    .collect())
            }
        })
        .collect();

    // Collect results, propagating errors
    let mut plugin_plans = Vec::new();
    for result in detection_results {
        plugin_plans.extend(result?);
    }

    // Filter out empty plans
    plugin_plans.retain(|p| !p.artifacts.is_empty());

    let existing_plugins = collect_existing_plugin_names(ai_dir, fs)?;

    if dry_run {
        let report = dry_run::generate_recursive_report(
            &discovered,
            &plugin_plans,
            &existing_plugins,
            destructive,
        );
        let report_path = dir.join("aipm-migrate-dryrun-report.md");
        fs.write_file(&report_path, report.as_bytes())?;
        return Ok(Outcome { actions: vec![Action::DryRunReport { path: report_path }] });
    }

    // Sequential name resolution
    let mut known_names = existing_plugins;
    let mut rename_counter = 0u32;
    let mut rename_actions = Vec::new();
    let mut resolved: Vec<(PluginPlan, String)> = Vec::new();
    for plan in plugin_plans {
        let final_name = emitter::resolve_plugin_name(
            &plan.name,
            &known_names,
            &mut rename_counter,
            &mut rename_actions,
        );
        known_names.insert(final_name.clone());
        resolved.push((plan, final_name));
    }

    // Parallel emission
    let emission_results: Vec<Result<_, Error>> = resolved
        .par_iter()
        .map(|(plan, final_name)| {
            let mut actions = Vec::new();

            if plan.is_package_scoped {
                let emit_actions = emitter::emit_package_plugin(
                    final_name,
                    &plan.artifacts,
                    ai_dir,
                    manifest,
                    fs,
                )?;
                actions.extend(emit_actions);
            } else if let Some(artifact) = plan.artifacts.first() {
                // Single artifact — use existing emit logic but with pre-resolved name
                let emit_actions =
                    emitter::emit_plugin_with_name(artifact, final_name, ai_dir, manifest, fs)?;
                actions.extend(emit_actions);
            }

            let description = plan.artifacts.first().and_then(|a| a.metadata.description.clone());
            Ok((actions, final_name.clone(), description))
        })
        .collect();

    let mut all_actions = rename_actions;
    let mut registered_entries = Vec::new();
    for result in emission_results {
        let (actions, name, description) = result?;
        all_actions.extend(actions);
        registered_entries.push(PluginEntry { name, description });
    }

    // Register all in marketplace.json
    registrar::register_plugins(ai_dir, &registered_entries, fs)?;
    for entry in &registered_entries {
        all_actions.push(Action::MarketplaceRegistered { name: entry.name.clone() });
    }

    Ok(Outcome { actions: all_actions })
}

/// Collect names of existing plugins in .ai/ directory.
fn collect_existing_plugin_names(ai_dir: &Path, fs: &dyn Fs) -> Result<HashSet<String>, Error> {
    let entries = fs.read_dir(ai_dir)?;
    Ok(entries.into_iter().filter(|e| e.is_dir).map(|e| e.name).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockFs {
        exists: HashSet<PathBuf>,
        dirs: HashMap<PathBuf, Vec<crate::fs::DirEntry>>,
        files: HashMap<PathBuf, String>,
        written: Mutex<HashMap<PathBuf, Vec<u8>>>,
    }

    impl MockFs {
        fn new() -> Self {
            Self {
                exists: HashSet::new(),
                dirs: HashMap::new(),
                files: HashMap::new(),
                written: Mutex::new(HashMap::new()),
            }
        }
    }

    impl crate::fs::Fs for MockFs {
        fn exists(&self, path: &Path) -> bool {
            self.exists.contains(path)
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Ok(())
        }

        fn write_file(&self, path: &Path, content: &[u8]) -> std::io::Result<()> {
            self.written
                .lock()
                .expect("MockFs::write_file: mutex poisoned")
                .insert(path.to_path_buf(), content.to_vec());
            Ok(())
        }

        fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
            self.files.get(path).cloned().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("not found: {}", path.display()),
                )
            })
        }

        fn read_dir(&self, path: &Path) -> std::io::Result<Vec<crate::fs::DirEntry>> {
            self.dirs.get(path).cloned().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("dir not found: {}", path.display()),
                )
            })
        }
    }

    #[test]
    fn migrate_errors_if_no_ai_dir() {
        let fs = MockFs::new();
        let opts = Options {
            dir: Path::new("/project"),
            source: Some(".claude"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: true,
        };
        let result = migrate(&opts, &fs);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| matches!(e, Error::MarketplaceNotFound(_))));
    }

    #[test]
    fn migrate_errors_if_no_source_dir() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.ai"));
        let opts = Options {
            dir: Path::new("/project"),
            source: Some(".claude"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: true,
        };
        let result = migrate(&opts, &fs);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| matches!(e, Error::SourceNotFound(_))));
    }

    #[test]
    fn migrate_dry_run_writes_report() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.ai"));
        fs.exists.insert(PathBuf::from("/project/.claude"));
        // Empty skills and commands dirs
        fs.dirs.insert(PathBuf::from("/project/.claude/skills"), Vec::new());
        fs.dirs.insert(PathBuf::from("/project/.claude/commands"), Vec::new());
        fs.dirs.insert(PathBuf::from("/project/.ai"), Vec::new());

        let opts = Options {
            dir: Path::new("/project"),
            source: Some(".claude"),
            dry_run: true,
            destructive: false,
            max_depth: None,
            manifest: true,
        };
        let result = migrate(&opts, &fs);
        assert!(result.is_ok());
        let result = result.ok();
        assert!(result.is_some_and(|r| {
            r.actions.len() == 1 && matches!(&r.actions.first(), Some(Action::DryRunReport { .. }))
        }));
        // Verify report file was written
        assert!(fs
            .written
            .lock()
            .expect("mutex poisoned")
            .contains_key(Path::new("/project/aipm-migrate-dryrun-report.md")));
    }

    #[test]
    fn migrate_empty_source() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.ai"));
        fs.exists.insert(PathBuf::from("/project/.claude"));
        fs.dirs.insert(PathBuf::from("/project/.claude/skills"), Vec::new());
        fs.dirs.insert(PathBuf::from("/project/.claude/commands"), Vec::new());
        fs.dirs.insert(PathBuf::from("/project/.ai"), Vec::new());
        // Need marketplace.json for registrar
        fs.files.insert(
            PathBuf::from("/project/.ai/.claude-plugin/marketplace.json"),
            r#"{"plugins":[]}"#.to_string(),
        );

        let opts = Options {
            dir: Path::new("/project"),
            source: Some(".claude"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: true,
        };
        let result = migrate(&opts, &fs);
        assert!(result.is_ok());
        let result = result.ok();
        assert!(result.is_some_and(|r| r.actions.is_empty()));
    }

    #[test]
    fn migrate_full_flow() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.ai"));
        fs.exists.insert(PathBuf::from("/project/.claude"));
        fs.exists.insert(PathBuf::from("/project/.claude/skills"));
        fs.exists.insert(PathBuf::from("/project/.claude/skills/deploy/SKILL.md"));
        fs.exists.insert(PathBuf::from("/project/.claude/commands"));

        // AI dir entries (no existing plugins)
        fs.dirs.insert(
            PathBuf::from("/project/.ai"),
            vec![crate::fs::DirEntry { name: ".claude-plugin".to_string(), is_dir: true }],
        );

        // Skills dir entries
        fs.dirs.insert(
            PathBuf::from("/project/.claude/skills"),
            vec![crate::fs::DirEntry { name: "deploy".to_string(), is_dir: true }],
        );

        // Deploy skill dir entries
        fs.dirs.insert(
            PathBuf::from("/project/.claude/skills/deploy"),
            vec![crate::fs::DirEntry { name: "SKILL.md".to_string(), is_dir: false }],
        );

        // SKILL.md content
        fs.files.insert(
            PathBuf::from("/project/.claude/skills/deploy/SKILL.md"),
            "---\nname: deploy\ndescription: Deploy app\n---\nDeploy instructions here".to_string(),
        );

        // Commands dir entries
        fs.dirs.insert(
            PathBuf::from("/project/.claude/commands"),
            vec![crate::fs::DirEntry { name: "review.md".to_string(), is_dir: false }],
        );

        // Command content
        fs.files.insert(
            PathBuf::from("/project/.claude/commands/review.md"),
            "Review the code carefully".to_string(),
        );

        // Marketplace JSON
        fs.files.insert(
            PathBuf::from("/project/.ai/.claude-plugin/marketplace.json"),
            r#"{"plugins":[]}"#.to_string(),
        );

        let opts = Options {
            dir: Path::new("/project"),
            source: Some(".claude"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: true,
        };
        let result = migrate(&opts, &fs);
        assert!(result.is_ok());

        if let Some(result) = result.ok() {
            let plugin_created_count =
                result.actions.iter().filter(|a| matches!(a, Action::PluginCreated { .. })).count();
            let marketplace_count = result
                .actions
                .iter()
                .filter(|a| matches!(a, Action::MarketplaceRegistered { .. }))
                .count();
            assert_eq!(plugin_created_count, 2);
            assert_eq!(marketplace_count, 2);
        }

        // Verify marketplace.json descriptions match plugin.json descriptions
        let marketplace_bytes = fs
            .written
            .lock()
            .expect("mutex poisoned")
            .get(Path::new("/project/.ai/.claude-plugin/marketplace.json"))
            .expect("marketplace.json should have been written")
            .clone();
        let content =
            String::from_utf8(marketplace_bytes).expect("marketplace.json must be valid UTF-8");
        let parsed: serde_json::Value =
            serde_json::from_str(&content).expect("marketplace.json must contain valid JSON");
        let plugins = parsed.get("plugins").and_then(|v| v.as_array());

        // "deploy" skill has description "Deploy app" in its SKILL.md frontmatter
        let deploy = plugins.and_then(|a| {
            a.iter().find(|p| p.get("name").and_then(|n| n.as_str()) == Some("deploy"))
        });
        assert_eq!(
            deploy.and_then(|p| p.get("description")).and_then(serde_json::Value::as_str),
            Some("Deploy app"),
            "deploy marketplace description should match SKILL.md frontmatter"
        );

        // "review" command has no frontmatter description — should get fallback
        let review = plugins.and_then(|a| {
            a.iter().find(|p| p.get("name").and_then(|n| n.as_str()) == Some("review"))
        });
        assert_eq!(
            review.and_then(|p| p.get("description")).and_then(serde_json::Value::as_str),
            Some("Migrated from .claude/ configuration"),
            "review marketplace description should use fallback when no frontmatter"
        );
    }

    // =========================================================================
    // Outcome helper method tests
    // =========================================================================

    #[test]
    fn has_migrated_artifacts_empty() {
        let outcome = Outcome { actions: Vec::new() };
        assert!(!outcome.has_migrated_artifacts());
    }

    #[test]
    fn has_migrated_artifacts_only_skipped() {
        let outcome = Outcome {
            actions: vec![
                Action::Skipped { name: "x".to_string(), reason: "test".to_string() },
                Action::Renamed {
                    original_name: "a".to_string(),
                    new_name: "b".to_string(),
                    reason: "conflict".to_string(),
                },
                Action::MarketplaceRegistered { name: "y".to_string() },
            ],
        };
        assert!(!outcome.has_migrated_artifacts());
    }

    #[test]
    fn has_migrated_artifacts_with_plugin_created() {
        let outcome = Outcome {
            actions: vec![
                Action::Skipped { name: "x".to_string(), reason: "test".to_string() },
                Action::PluginCreated {
                    name: "deploy".to_string(),
                    source: PathBuf::from("/project/.claude/skills/deploy"),
                    plugin_type: "skill".to_string(),
                },
            ],
        };
        assert!(outcome.has_migrated_artifacts());
    }

    #[test]
    fn migrated_source_paths_empty() {
        let outcome = Outcome { actions: Vec::new() };
        assert!(outcome.migrated_source_paths().is_empty());
    }

    #[test]
    fn migrated_source_paths_filters_correctly() {
        let outcome = Outcome {
            actions: vec![
                Action::PluginCreated {
                    name: "deploy".to_string(),
                    source: PathBuf::from("/project/.claude/skills/deploy"),
                    plugin_type: "skill".to_string(),
                },
                Action::Skipped { name: "x".to_string(), reason: "test".to_string() },
                Action::PluginCreated {
                    name: "review".to_string(),
                    source: PathBuf::from("/project/.claude/commands/review.md"),
                    plugin_type: "skill".to_string(),
                },
                Action::MarketplaceRegistered { name: "deploy".to_string() },
            ],
        };
        let paths = outcome.migrated_source_paths();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], Path::new("/project/.claude/skills/deploy"));
        assert_eq!(paths[1], Path::new("/project/.claude/commands/review.md"));
    }

    #[test]
    fn strip_yaml_quotes_double() {
        assert_eq!(strip_yaml_quotes(r#""hello""#), "hello");
    }

    #[test]
    fn strip_yaml_quotes_single() {
        assert_eq!(strip_yaml_quotes("'hello'"), "hello");
    }

    #[test]
    fn strip_yaml_quotes_no_quotes() {
        assert_eq!(strip_yaml_quotes("hello"), "hello");
    }

    #[test]
    fn strip_yaml_quotes_mismatched() {
        assert_eq!(strip_yaml_quotes("\"hello'"), "\"hello'");
    }

    #[test]
    fn strip_yaml_quotes_empty_quoted() {
        assert_eq!(strip_yaml_quotes("\"\""), "");
    }

    #[test]
    fn strip_yaml_quotes_single_char() {
        assert_eq!(strip_yaml_quotes("x"), "x");
    }

    #[test]
    fn strip_yaml_quotes_empty() {
        assert_eq!(strip_yaml_quotes(""), "");
    }

    #[test]
    fn strip_yaml_quotes_lone_quote_char_unchanged() {
        // A string containing only a single quote character (either '"' or '\''):
        // first == last == quote, but bytes.len() == 1 < 2, so the guard fails and
        // the input is returned as-is.
        assert_eq!(strip_yaml_quotes("\""), "\"");
        assert_eq!(strip_yaml_quotes("'"), "'");
    }
}
