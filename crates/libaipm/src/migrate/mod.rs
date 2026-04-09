//! Migration pipeline: scan AI tool configurations and convert to marketplace plugins.

pub mod agent_detector;
pub mod cleanup;
pub mod command_detector;
pub mod copilot_agent_detector;
pub mod copilot_extension_detector;
pub mod copilot_hook_detector;
pub mod copilot_lsp_detector;
pub mod copilot_mcp_detector;
pub mod copilot_skill_detector;
pub mod detector;
pub mod dry_run;
pub mod emitter;
pub mod hook_detector;
pub mod mcp_detector;
pub mod output_style_detector;
pub mod reconciler;
pub mod registrar;
pub mod skill_common;
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
    /// LSP server config from `lsp.json`.
    LspServer,
    /// An extension from `.github/extensions/<name>/`.
    Extension,
}

impl ArtifactKind {
    /// Returns a human-readable type string for display and manifest generation.
    pub const fn to_type_string(&self) -> &'static str {
        match self {
            Self::Skill | Self::Command => "skill",
            Self::Agent => "agent",
            Self::McpServer => "mcp",
            Self::Hook => "hook",
            // OutputStyle has no standalone PluginType; always composite when mixed.
            // Extensions bundle as composite plugins.
            Self::OutputStyle | Self::Extension => "composite",
            Self::LspServer => "lsp",
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

/// A file in the source directory not claimed by any detector.
#[derive(Debug, Clone)]
pub struct OtherFile {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// Path relative to the source directory.
    pub relative_path: PathBuf,
    /// Name of the artifact that references this file (if any).
    pub associated_artifact: Option<String>,
    /// Whether the file is outside the source directory boundary.
    pub is_external: bool,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// A plugin directory was created.
    PluginCreated {
        /// Final plugin name.
        name: String,
        /// Source path of the original artifact.
        source: PathBuf,
        /// Plugin type (e.g., "skill").
        plugin_type: String,
        /// Whether the source is a directory (true for skills) or a file (commands, agents, etc.).
        source_is_dir: bool,
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
    /// A migrated source directory was removed during cleanup.
    SourceDirRemoved {
        /// Path to the removed directory.
        path: PathBuf,
    },
    /// An empty parent directory was pruned after its children were removed.
    EmptyDirPruned {
        /// Path to the pruned directory.
        path: PathBuf,
    },
    /// An "other file" was migrated alongside its parent artifact.
    OtherFileMigrated {
        /// Source path of the file.
        path: PathBuf,
        /// Destination path in the plugin directory.
        destination: PathBuf,
        /// Name of the artifact this file is associated with (if any).
        associated_artifact: Option<String>,
    },
    /// An external file reference was detected in migrated artifact content.
    ExternalReferenceDetected {
        /// Path to the external file.
        path: PathBuf,
        /// Name of the artifact that references this file.
        referenced_by: String,
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

    /// Returns the source paths and directory flags of all successfully migrated artifacts
    /// and other files.
    pub fn migrated_sources(&self) -> Vec<(&Path, bool)> {
        self.actions
            .iter()
            .filter_map(|a| match a {
                Action::PluginCreated { source, source_is_dir, .. } => {
                    Some((source.as_path(), *source_is_dir))
                },
                Action::OtherFileMigrated { path, .. } => Some((path.as_path(), false)),
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
    #[error("unsupported source type '{0}' — supported sources: .claude, .github")]
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
    #[error("failed to discover source directories: {0}")]
    DiscoveryFailed(#[from] crate::discovery::Error),

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
    /// Files not claimed by any detector in this source directory.
    pub other_files: Vec<OtherFile>,
}

/// Run the migration pipeline.
pub fn migrate(opts: &Options<'_>, fs: &dyn Fs) -> Result<Outcome, Error> {
    tracing::debug!(
        source = ?opts.source,
        dry_run = opts.dry_run,
        destructive = opts.destructive,
        "starting migration"
    );
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

    let mut detectors = detector::detectors_for_source(source);
    if detectors.is_empty() {
        // Unknown source type — run all detectors as a fallback
        detectors = detector::claude_detectors();
        detectors.extend(detector::copilot_detectors());
    }

    let mut all_artifacts = Vec::new();
    for det in &detectors {
        let artifacts = det.detect(&source_dir, fs)?;
        all_artifacts.extend(artifacts);
    }

    let other_files = reconciler::reconcile(&source_dir, &all_artifacts, fs)?;

    let existing_plugins = collect_existing_plugin_names(ai_dir, fs)?;

    if dry_run {
        let report = dry_run::generate_report(
            &all_artifacts,
            &existing_plugins,
            source,
            manifest,
            destructive,
            &other_files,
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

    // Emit other files into the first created plugin's directory
    if !other_files.is_empty() {
        if let Some(first_entry) = registered_entries.first() {
            let plugin_dir = ai_dir.join(&first_entry.name);
            let other_actions = emitter::emit_other_files(&other_files, &plugin_dir, fs)?;
            actions.extend(other_actions);
        }
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

    let discovered =
        crate::discovery::discover_source_dirs(dir, &[".claude", ".github"], max_depth)?;
    tracing::debug!(
        discovered = discovered.len(),
        "discovered source directories for recursive migration"
    );
    if discovered.is_empty() {
        return Ok(Outcome { actions: Vec::new() });
    }

    // Parallel detection: run detectors across discovered dirs concurrently
    let detection_results: Vec<Result<Vec<PluginPlan>, Error>> = discovered
        .par_iter()
        .map(|src| {
            let detectors = detector::detectors_for_source(&src.source_type);
            let mut all_artifacts = Vec::new();
            for det in &detectors {
                let artifacts = det.detect(&src.source_dir, fs)?;
                all_artifacts.extend(artifacts);
            }

            let other_files = reconciler::reconcile(&src.source_dir, &all_artifacts, fs)?;

            if let Some(ref pkg_name) = src.package_name {
                // Package-scoped: merge all artifacts under one plugin
                Ok(vec![PluginPlan {
                    name: pkg_name.clone(),
                    artifacts: all_artifacts,
                    is_package_scoped: true,
                    source_dir: src.source_dir.clone(),
                    other_files,
                }])
            } else {
                // Root-level: each artifact becomes its own plugin.
                // Attach other_files to the first plan so they appear in reports.
                let source = src.source_dir.clone();
                let mut plans: Vec<PluginPlan> = all_artifacts
                    .into_iter()
                    .map(|a| PluginPlan {
                        name: a.name.clone(),
                        artifacts: vec![a],
                        is_package_scoped: false,
                        source_dir: source.clone(),
                        other_files: Vec::new(),
                    })
                    .collect();
                if let Some(first) = plans.first_mut() {
                    first.other_files = other_files;
                }
                Ok(plans)
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
    tracing::debug!(plans = plugin_plans.len(), "detection complete");

    let existing_plugins = collect_existing_plugin_names(ai_dir, fs)?;

    if dry_run {
        tracing::debug!("dry-run mode — generating report");
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

    tracing::debug!("emitting plugins");
    emit_and_register(plugin_plans, existing_plugins, ai_dir, manifest, fs)
}

/// Resolve names, emit plugins in parallel, and register in marketplace.json.
fn emit_and_register(
    plugin_plans: Vec<PluginPlan>,
    existing_plugins: HashSet<String>,
    ai_dir: &Path,
    manifest: bool,
    fs: &dyn Fs,
) -> Result<Outcome, Error> {
    use rayon::prelude::*;

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

    tracing::debug!(plans = resolved.len(), "starting plugin emission");

    let emission_results: Vec<Result<_, Error>> = resolved
        .par_iter()
        .map(|(plan, final_name)| {
            tracing::trace!(
                plugin = final_name.as_str(),
                artifacts = plan.artifacts.len(),
                other_files = plan.other_files.len(),
                "emitting plugin"
            );
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
                let emit_actions =
                    emitter::emit_plugin_with_name(artifact, final_name, ai_dir, manifest, fs)?;
                actions.extend(emit_actions);
            }

            // Emit other files into the plugin directory
            if !plan.other_files.is_empty() {
                let plugin_dir = ai_dir.join(final_name);
                let other_actions = emitter::emit_other_files(&plan.other_files, &plugin_dir, fs)?;
                actions.extend(other_actions);
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

    registrar::register_plugins(ai_dir, &registered_entries, fs)?;
    for entry in &registered_entries {
        all_actions.push(Action::MarketplaceRegistered { name: entry.name.clone() });
    }

    tracing::debug!(
        emitted = all_actions.len(),
        registered = registered_entries.len(),
        "emission and registration complete"
    );

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
        // Source dir listing (for reconciler)
        fs.dirs.insert(
            PathBuf::from("/project/.claude"),
            vec![
                crate::fs::DirEntry { name: "skills".to_string(), is_dir: true },
                crate::fs::DirEntry { name: "commands".to_string(), is_dir: true },
            ],
        );
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
        fs.dirs.insert(
            PathBuf::from("/project/.claude"),
            vec![
                crate::fs::DirEntry { name: "skills".to_string(), is_dir: true },
                crate::fs::DirEntry { name: "commands".to_string(), is_dir: true },
            ],
        );
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

        // Source dir listing (for reconciler)
        fs.dirs.insert(
            PathBuf::from("/project/.claude"),
            vec![
                crate::fs::DirEntry { name: "skills".to_string(), is_dir: true },
                crate::fs::DirEntry { name: "commands".to_string(), is_dir: true },
            ],
        );

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
                    source_is_dir: true,
                },
            ],
        };
        assert!(outcome.has_migrated_artifacts());
    }

    #[test]
    fn migrated_sources_empty() {
        let outcome = Outcome { actions: Vec::new() };
        assert!(outcome.migrated_sources().is_empty());
    }

    #[test]
    fn migrated_sources_filters_correctly() {
        let outcome = Outcome {
            actions: vec![
                Action::PluginCreated {
                    name: "deploy".to_string(),
                    source: PathBuf::from("/project/.claude/skills/deploy"),
                    plugin_type: "skill".to_string(),
                    source_is_dir: true,
                },
                Action::Skipped { name: "x".to_string(), reason: "test".to_string() },
                Action::PluginCreated {
                    name: "review".to_string(),
                    source: PathBuf::from("/project/.claude/commands/review.md"),
                    plugin_type: "skill".to_string(),
                    source_is_dir: false,
                },
                Action::MarketplaceRegistered { name: "deploy".to_string() },
            ],
        };
        let sources = outcome.migrated_sources();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].0, Path::new("/project/.claude/skills/deploy"));
        assert!(sources[0].1); // is_dir
        assert_eq!(sources[1].0, Path::new("/project/.claude/commands/review.md"));
        assert!(!sources[1].1); // not is_dir
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

    #[test]
    fn migrate_unknown_source_runs_all_detectors() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.ai"));
        fs.exists.insert(PathBuf::from("/project/.custom"));
        fs.dirs.insert(PathBuf::from("/project/.custom"), Vec::new());
        fs.dirs.insert(PathBuf::from("/project/.ai"), Vec::new());
        fs.files.insert(
            PathBuf::from("/project/.ai/.claude-plugin/marketplace.json"),
            r#"{"plugins":[]}"#.to_string(),
        );

        let opts = Options {
            dir: Path::new("/project"),
            source: Some(".custom"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let result = migrate(&opts, &fs);
        // Should not error — unknown source falls back to all detectors
        assert!(result.is_ok());
    }

    #[test]
    fn migrate_github_source_accepted() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.ai"));
        fs.exists.insert(PathBuf::from("/project/.github"));
        fs.dirs.insert(PathBuf::from("/project/.github"), Vec::new());
        fs.dirs.insert(PathBuf::from("/project/.ai"), Vec::new());
        fs.files.insert(
            PathBuf::from("/project/.ai/.claude-plugin/marketplace.json"),
            r#"{"plugins":[]}"#.to_string(),
        );

        let opts = Options {
            dir: Path::new("/project"),
            source: Some(".github"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let result = migrate(&opts, &fs);
        assert!(result.is_ok());
    }

    #[test]
    fn migrate_with_other_files_emits_them() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.ai"));
        fs.exists.insert(PathBuf::from("/project/.claude"));
        fs.exists.insert(PathBuf::from("/project/.claude/skills"));
        fs.exists.insert(PathBuf::from("/project/.claude/skills/deploy/SKILL.md"));
        fs.exists.insert(PathBuf::from("/project/.claude/README.md"));

        // Source dir listing includes an unclaimed file
        fs.dirs.insert(
            PathBuf::from("/project/.claude"),
            vec![
                crate::fs::DirEntry { name: "skills".to_string(), is_dir: true },
                crate::fs::DirEntry { name: "README.md".to_string(), is_dir: false },
            ],
        );
        fs.dirs.insert(
            PathBuf::from("/project/.claude/skills"),
            vec![crate::fs::DirEntry { name: "deploy".to_string(), is_dir: true }],
        );
        fs.dirs.insert(
            PathBuf::from("/project/.claude/skills/deploy"),
            vec![crate::fs::DirEntry { name: "SKILL.md".to_string(), is_dir: false }],
        );
        fs.files.insert(
            PathBuf::from("/project/.claude/skills/deploy/SKILL.md"),
            "---\nname: deploy\n---\nDeploy".to_string(),
        );
        fs.files.insert(PathBuf::from("/project/.claude/README.md"), "# Notes".to_string());
        fs.dirs.insert(
            PathBuf::from("/project/.ai"),
            vec![crate::fs::DirEntry { name: ".claude-plugin".to_string(), is_dir: true }],
        );
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
            manifest: false,
        };
        let result = migrate(&opts, &fs);
        assert!(result.is_ok());

        if let Some(result) = result.ok() {
            let other_migrated = result
                .actions
                .iter()
                .filter(|a| matches!(a, Action::OtherFileMigrated { .. }))
                .count();
            assert!(other_migrated > 0, "should have OtherFileMigrated actions");
        }
    }

    #[test]
    fn migrate_other_files_skipped_when_no_artifacts_detected() {
        // Covers the `None` branch of `if let Some(first_entry) = registered_entries.first()`
        // (line 427): when other_files is non-empty but no artifacts were detected so
        // registered_entries is empty, the emit step is silently skipped.
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.ai"));
        fs.exists.insert(PathBuf::from("/project/.claude"));
        // Source dir contains only a plain README — no detectable artifacts.
        fs.dirs.insert(
            PathBuf::from("/project/.claude"),
            vec![crate::fs::DirEntry { name: "README.md".to_string(), is_dir: false }],
        );
        // README.md must be in fs.exists so that is_file() returns true and
        // collect_files_recursive includes it in other_files.
        fs.exists.insert(PathBuf::from("/project/.claude/README.md"));
        fs.files.insert(PathBuf::from("/project/.claude/README.md"), "# Notes".to_string());
        // .ai directory is empty (no existing plugins).
        fs.dirs.insert(PathBuf::from("/project/.ai"), Vec::new());

        let opts = Options {
            dir: Path::new("/project"),
            source: Some(".claude"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let result = migrate(&opts, &fs);
        assert!(result.is_ok());
        if let Ok(outcome) = result {
            // No plugins created and no marketplace entries because no artifacts were detected.
            assert!(!outcome.actions.iter().any(|a| matches!(a, Action::PluginCreated { .. })));
            assert!(!outcome
                .actions
                .iter()
                .any(|a| matches!(a, Action::MarketplaceRegistered { .. })));
        }
    }

    #[test]
    fn migrate_unknown_source_falls_back_to_all_detectors() {
        // Covers the `if detectors.is_empty()` True branch in `migrate_single_source`:
        // when the source name is not ".claude" or ".github", `detectors_for_source`
        // returns an empty Vec and the code falls back to running all detectors.
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.ai"));
        fs.exists.insert(PathBuf::from("/project/.custom"));
        // Empty source dir — no detectable artifacts.
        fs.dirs.insert(PathBuf::from("/project/.custom"), Vec::new());
        fs.dirs.insert(PathBuf::from("/project/.ai"), Vec::new());

        let opts = Options {
            dir: Path::new("/project"),
            source: Some(".custom"),
            dry_run: true,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        // An unknown source type must succeed (falling back to all detectors)
        // and produce a dry-run report with no detected artifacts.
        let result = migrate(&opts, &fs);
        assert!(result.is_ok());
        let actions = result.ok().map(|o| o.actions).unwrap_or_default();
        assert!(actions.iter().any(|a| matches!(a, Action::DryRunReport { .. })));
    }
}
