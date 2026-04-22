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
pub mod error;
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

pub use error::Error;

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
        let result = migrate(&opts, &fs).unwrap();
        assert_eq!(result.actions.len(), 1);
        assert!(matches!(result.actions.first(), Some(Action::DryRunReport { .. })));
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
    fn migrate_other_files_with_no_artifacts_skips_emit() {
        // Exercises the None arm of `if let Some(first_entry) = registered_entries.first()`
        // — the case where other_files is non-empty but no plugin artifacts were found,
        // so there is no plugin directory to attach the unclaimed files to.
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.ai"));
        fs.exists.insert(PathBuf::from("/project/.claude"));
        // A plain file that no detector recognises — becomes an unclaimed "other file"
        fs.exists.insert(PathBuf::from("/project/.claude/readme.txt"));

        // .ai/ dir listing for collect_existing_plugin_names
        fs.dirs.insert(PathBuf::from("/project/.ai"), Vec::new());
        // .claude/ dir listing for the reconciler
        fs.dirs.insert(
            PathBuf::from("/project/.claude"),
            vec![crate::fs::DirEntry { name: "readme.txt".to_string(), is_dir: false }],
        );

        let opts = Options {
            dir: Path::new("/project"),
            source: Some(".claude"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: true,
        };

        // All detectors return empty (no skills/, commands/, agents/, etc., no settings.json).
        // The reconciler still finds readme.txt as an unclaimed file, making other_files
        // non-empty. registered_entries stays empty, so the `if let Some` at the
        // "emit other files" block yields None and the body is correctly skipped.
        let result = migrate(&opts, &fs);
        assert!(result.is_ok(), "migrate should succeed with unclaimed files and no artifacts");
        let outcome = result.ok();
        assert!(outcome.is_some_and(|o| o.actions.is_empty()), "no actions expected");
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
        let outcome = migrate(&opts, &fs).unwrap();
        let plugin_created_count =
            outcome.actions.iter().filter(|a| matches!(a, Action::PluginCreated { .. })).count();
        let marketplace_count = outcome
            .actions
            .iter()
            .filter(|a| matches!(a, Action::MarketplaceRegistered { .. }))
            .count();
        assert_eq!(plugin_created_count, 2);
        assert_eq!(marketplace_count, 2);

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
        let outcome = migrate(&opts, &fs).unwrap();
        let other_migrated = outcome
            .actions
            .iter()
            .filter(|a| matches!(a, Action::OtherFileMigrated { .. }))
            .count();
        assert!(other_migrated > 0, "should have OtherFileMigrated actions");
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
        let outcome = migrate(&opts, &fs).unwrap();
        // No plugins created and no marketplace entries because no artifacts were detected.
        assert!(!outcome.actions.iter().any(|a| matches!(a, Action::PluginCreated { .. })));
        assert!(!outcome.actions.iter().any(|a| matches!(a, Action::MarketplaceRegistered { .. })));
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

    #[test]
    fn emit_and_register_plan_with_no_artifacts_skips_plugin_emission() {
        // Covers the False branch of `else if let Some(artifact) = plan.artifacts.first()`
        // (line 590 in emit_and_register): when is_package_scoped is false and artifacts is
        // empty, neither emit function is called but the plan is still registered in
        // marketplace.json.
        let mut fs = MockFs::new();
        fs.files.insert(
            PathBuf::from("/ai/.claude-plugin/marketplace.json"),
            r#"{"plugins":[]}"#.to_string(),
        );

        let plan = PluginPlan {
            name: "empty-plugin".to_string(),
            artifacts: Vec::new(),
            is_package_scoped: false,
            source_dir: PathBuf::from("/src"),
            other_files: Vec::new(),
        };

        let result = emit_and_register(vec![plan], HashSet::new(), Path::new("/ai"), false, &fs);
        assert!(result.is_ok());
        let outcome = result.expect("emit_and_register should succeed");
        // No PluginCreated action since no artifacts were emitted
        assert!(!outcome.has_migrated_artifacts());
        // The plan is still registered in marketplace.json
        assert!(outcome.actions.iter().any(
            |a| matches!(a, Action::MarketplaceRegistered { name } if name == "empty-plugin")
        ));
    }

    #[test]
    fn migrated_sources_includes_other_file_migrated() {
        // Covers the `OtherFileMigrated` arm of `migrated_sources()` which was previously
        // untested — the existing test only exercised `PluginCreated` and wildcard arms.
        let outcome = Outcome {
            actions: vec![
                Action::OtherFileMigrated {
                    path: PathBuf::from("/project/.claude/README.md"),
                    destination: PathBuf::from("/project/.ai/my-plugin/README.md"),
                    associated_artifact: Some("my-plugin".to_string()),
                },
                Action::MarketplaceRegistered { name: "my-plugin".to_string() },
            ],
        };
        let sources = outcome.migrated_sources();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources.first().map(|s| s.0), Some(Path::new("/project/.claude/README.md")));
        // OtherFileMigrated always reports is_dir = false
        assert!(sources.first().is_some_and(|s| !s.1));
    }

    /// Covers the `if detectors.is_empty()` True branch in `migrate_single_source`
    /// (line 317): when an unrecognized source type is provided, `detectors_for_source`
    /// returns an empty list and the function falls back to running all detectors.
    /// With an empty custom-source directory no artifacts are produced, so the
    /// migration succeeds and returns an empty `Outcome`.
    #[test]
    fn migrate_with_unrecognized_source_type_uses_all_detectors_as_fallback() {
        let mut fs = MockFs::new();
        // .ai/ dir must exist for the initial guard check
        fs.exists.insert(PathBuf::from("/project/.ai"));
        // A custom source dir that is NOT ".claude" or ".github" — forces the
        // fallback branch in `migrate_single_source` (detectors.is_empty() == true).
        fs.exists.insert(PathBuf::from("/project/custom-source"));
        // Empty directory listings: no subdirectories → all detectors find nothing
        fs.dirs.insert(PathBuf::from("/project/custom-source"), vec![]);
        fs.dirs.insert(PathBuf::from("/project/.ai"), vec![]);

        let opts = Options {
            dir: Path::new("/project"),
            source: Some("custom-source"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };

        let result = migrate(&opts, &fs);
        assert!(result.is_ok(), "expected Ok for unrecognized source type");
        let outcome = result.unwrap_or_else(|_| Outcome { actions: Vec::new() });
        assert!(
            outcome.actions.is_empty(),
            "expected no actions when custom source has no detectable artifacts"
        );
    }

    #[test]
    fn artifact_kind_to_type_string_all_variants() {
        // Covers all match arms in ArtifactKind::to_type_string().
        assert_eq!(ArtifactKind::Skill.to_type_string(), "skill");
        assert_eq!(ArtifactKind::Command.to_type_string(), "skill");
        assert_eq!(ArtifactKind::Agent.to_type_string(), "agent");
        assert_eq!(ArtifactKind::McpServer.to_type_string(), "mcp");
        assert_eq!(ArtifactKind::Hook.to_type_string(), "hook");
        assert_eq!(ArtifactKind::OutputStyle.to_type_string(), "composite");
        assert_eq!(ArtifactKind::Extension.to_type_string(), "composite");
        assert_eq!(ArtifactKind::LspServer.to_type_string(), "lsp");
    }

    #[test]
    fn migrate_recursive_returns_empty_when_no_source_dirs_exist() {
        // Covers the `if discovered.is_empty()` True branch in `migrate_recursive`.
        // When opts.source is None and no .claude/.github dirs exist, the function
        // returns Ok with an empty actions list without error.
        let tmp = tempfile::tempdir().expect("tempdir");
        let project_dir = tmp.path();

        // Create .ai/ dir (required for the initial existence check)
        std::fs::create_dir_all(project_dir.join(".ai")).expect("create .ai");

        // No .claude/ or .github/ dirs — discover_source_dirs will return empty.
        let opts = Options {
            dir: project_dir,
            source: None,
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };

        let result = migrate(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.expect("migrate should succeed");
        assert!(outcome.actions.is_empty(), "expected no actions when no source dirs found");
    }

    #[test]
    fn migrate_recursive_dry_run_generates_report() {
        // Covers the `if dry_run` True branch in `migrate_recursive`, as well as the
        // `if discovered.is_empty()` False branch.
        let tmp = tempfile::tempdir().expect("tempdir");
        let project_dir = tmp.path();

        // Create .ai/ dir
        std::fs::create_dir_all(project_dir.join(".ai")).expect("create .ai");
        // Create an empty .claude/ dir — discover_source_dirs will find it.
        std::fs::create_dir_all(project_dir.join(".claude")).expect("create .claude");

        let opts = Options {
            dir: project_dir,
            source: None,
            dry_run: true,
            destructive: false,
            max_depth: None,
            manifest: false,
        };

        let result = migrate(&opts, &crate::fs::Real);
        assert!(result.is_ok());
        let outcome = result.expect("migrate should succeed");
        assert!(
            outcome.actions.iter().any(|a| matches!(a, Action::DryRunReport { .. })),
            "expected a DryRunReport action in recursive dry-run mode"
        );
    }

    #[test]
    fn migrate_recursive_non_dry_run_no_artifacts_succeeds() {
        // Covers the `if dry_run` False branch in `migrate_recursive` (the actual
        // migration path). With no artifacts detected, `emit_and_register` is called
        // with an empty plan list and returns Ok with no actions.
        let tmp = tempfile::tempdir().expect("tempdir");
        let project_dir = tmp.path();

        // `.ai/` must exist so `collect_existing_plugin_names` can read it
        std::fs::create_dir_all(project_dir.join(".ai")).expect("create .ai");
        // `.claude/` must exist so `discover_source_dirs` finds a source to scan
        std::fs::create_dir_all(project_dir.join(".claude")).expect("create .claude");

        let opts = Options {
            dir: project_dir,
            source: None,   // triggers migrate_recursive
            dry_run: false, // exercises the False branch of `if dry_run`
            destructive: false,
            max_depth: None,
            manifest: false,
        };

        let result = migrate(&opts, &crate::fs::Real);
        assert!(result.is_ok(), "migrate should succeed with no artifacts");
        let outcome = result.expect("migrate should succeed");
        // No artifacts means no PluginCreated or MarketplaceRegistered actions
        assert!(
            !outcome.actions.iter().any(|a| matches!(a, Action::PluginCreated { .. })),
            "expected no PluginCreated actions when source is empty"
        );
    }

    #[test]
    fn migrate_recursive_root_skill_attaches_other_files_to_first_plan() {
        // Covers the `if let Some(first) = plans.first_mut()` True branch (line 444)
        // and the `else if let Some(artifact) = plan.artifacts.first()` True branch
        // (line 528) in `migrate_recursive`.
        //
        // When .claude/ is at the project root (package_name = None), each artifact
        // becomes its own PluginPlan. With a real skill file, plans is non-empty,
        // so plans.first_mut() returns Some and other_files are attached.
        let tmp = tempfile::tempdir().expect("tempdir");
        let project_dir = tmp.path();

        // Initialise .ai/ with a valid marketplace.json so registrar can update it.
        let ai_dir = project_dir.join(".ai");
        let claude_plugin_dir = ai_dir.join(".claude-plugin");
        std::fs::create_dir_all(&claude_plugin_dir).expect("create .ai/.claude-plugin");
        std::fs::write(
            claude_plugin_dir.join("marketplace.json"),
            crate::generate::marketplace::create("test-marketplace", &[]),
        )
        .expect("write marketplace.json");

        // Create a skill so that detection yields at least one artifact.
        let skill_dir = project_dir.join(".claude").join("skills").join("my-skill");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: Test skill\n---\nDo something",
        )
        .expect("write SKILL.md");

        let opts = Options {
            dir: project_dir,
            source: None,
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };

        let result = migrate(&opts, &crate::fs::Real);
        assert!(result.is_ok(), "migrate should succeed");
        let outcome = result.expect("migrate should succeed");
        assert!(
            outcome.actions.iter().any(|a| matches!(a, Action::PluginCreated { .. })),
            "expected PluginCreated action for root-level skill"
        );
    }

    #[test]
    fn migrate_recursive_package_scoped_source_triggers_is_package_scoped() {
        // Covers the `if let Some(ref pkg_name) = src.package_name` True branch
        // (line 421) and the `if plan.is_package_scoped` True branch (line 519)
        // in `migrate_recursive`.
        //
        // When .claude/ is inside a subdirectory (e.g., mypkg/.claude/), the
        // discovery assigns package_name = Some("mypkg"), which routes to the
        // package-scoped branch: all artifacts from that source are grouped under
        // one PluginPlan named "mypkg".
        let tmp = tempfile::tempdir().expect("tempdir");
        let project_dir = tmp.path();

        // Initialise .ai/ with a valid marketplace.json.
        let ai_dir = project_dir.join(".ai");
        let claude_plugin_dir = ai_dir.join(".claude-plugin");
        std::fs::create_dir_all(&claude_plugin_dir).expect("create .ai/.claude-plugin");
        std::fs::write(
            claude_plugin_dir.join("marketplace.json"),
            crate::generate::marketplace::create("test-marketplace", &[]),
        )
        .expect("write marketplace.json");

        // Place .claude/ inside a subdirectory so package_name becomes Some("mypkg").
        let skill_dir = project_dir.join("mypkg").join(".claude").join("skills").join("pkg-skill");
        std::fs::create_dir_all(&skill_dir).expect("create nested skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: pkg-skill\ndescription: Package skill\n---\nDo something",
        )
        .expect("write SKILL.md");

        let opts = Options {
            dir: project_dir,
            source: None,
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };

        let result = migrate(&opts, &crate::fs::Real);
        assert!(result.is_ok(), "migrate should succeed");
        let outcome = result.expect("migrate should succeed");
        // The package-scoped plan name is "mypkg" (from the parent directory).
        assert!(
            outcome.actions.iter().any(|a| matches!(
                a,
                Action::PluginCreated { name, .. } if name == "mypkg"
            )),
            "expected PluginCreated with name 'mypkg' for package-scoped migration"
        );
    }

    /// Covers the `if detectors.is_empty()` True branch in `migrate_single_source`.
    ///
    /// When the source type is not `.claude` or `.github`,
    /// `detectors_for_source` returns an empty Vec. The function then falls back
    /// to running all Claude + Copilot detectors. With an empty source directory
    /// none of them find any artifacts, and the dry-run report is still produced.
    #[test]
    fn migrate_single_source_unknown_type_falls_back_to_all_detectors() {
        let mut fs = MockFs::new();
        fs.exists.insert(PathBuf::from("/project/.ai"));
        fs.exists.insert(PathBuf::from("/project/.vscode"));
        // Source dir listing must be present so the reconciler can enumerate files.
        fs.dirs.insert(PathBuf::from("/project/.vscode"), Vec::new());
        // .ai/ dir listing needed by collect_existing_plugin_names.
        fs.dirs.insert(PathBuf::from("/project/.ai"), Vec::new());

        let opts = Options {
            dir: Path::new("/project"),
            source: Some(".vscode"),
            dry_run: true,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let result = migrate(&opts, &fs);
        assert!(result.is_ok(), "migrate with unknown source type should succeed");
        let outcome = result.ok();
        let actions = outcome.map(|r| r.actions).unwrap_or_default();
        assert_eq!(actions.len(), 1, "should produce exactly one DryRunReport action");
        assert!(
            matches!(actions.first(), Some(Action::DryRunReport { .. })),
            "action should be DryRunReport"
        );
    }

    #[test]
    fn migrate_recursive_other_files_emitted_via_emit_and_register() {
        // Covers the `if !plan.other_files.is_empty()` True branch (line 535)
        // inside `emit_and_register`, which is only reached via `migrate_recursive`
        // (i.e. `source: None`). When the root `.claude/` directory contains both
        // a skill artifact and a non-artifact file (e.g. README.md), the reconciler
        // assigns the extra file to `plan.other_files`, triggering the branch.
        let tmp = tempfile::tempdir().expect("tempdir");
        let project_dir = tmp.path();

        // Initialise .ai/ with a valid marketplace.json so registrar can update it.
        let ai_dir = project_dir.join(".ai");
        let claude_plugin_dir = ai_dir.join(".claude-plugin");
        std::fs::create_dir_all(&claude_plugin_dir).expect("create .ai/.claude-plugin");
        std::fs::write(
            claude_plugin_dir.join("marketplace.json"),
            crate::generate::marketplace::create("test-marketplace", &[]),
        )
        .expect("write marketplace.json");

        // Create a skill so detection yields at least one artifact.
        let skill_dir = project_dir.join(".claude").join("skills").join("deploy");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: deploy\ndescription: Deploy skill\n---\nDeploy",
        )
        .expect("write SKILL.md");

        // Add a non-artifact file so that `plan.other_files` is non-empty.
        let claude_dir = project_dir.join(".claude");
        std::fs::write(claude_dir.join("README.md"), "# Notes").expect("write README.md");

        let opts = Options {
            dir: project_dir,
            source: None,
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };

        let result = migrate(&opts, &crate::fs::Real);
        assert!(result.is_ok(), "migrate should succeed");
        let outcome = result.expect("migrate should succeed");
        assert!(
            outcome.actions.iter().any(|a| matches!(a, Action::OtherFileMigrated { .. })),
            "expected OtherFileMigrated action when non-artifact files exist in recursive mode"
        );
    }

    /// Exercises the `None` (False) branch of
    /// `if let Some(first_entry) = registered_entries.first()` (line 371).
    ///
    /// When a source directory contains only non-artifact files (so no artifacts
    /// are detected and `registered_entries` stays empty) but the reconciler still
    /// produces `other_files`, the `if let Some(...)` evaluates to `None` and the
    /// other-file emission is silently skipped.
    #[test]
    fn migrate_other_files_skipped_when_no_artifacts() {
        let mut fs = MockFs::new();
        // .ai/ must exist for the initial marketplace check
        fs.exists.insert(PathBuf::from("/project/.ai"));
        // Source dir must exist so migrate_single_source doesn't return SourceNotFound
        fs.exists.insert(PathBuf::from("/project/.vscode"));
        // The plain file must be in fs.exists so MockFs::is_file() (which defaults to
        // self.exists()) returns true, making collect_files_recursive include it in
        // other_files.
        fs.exists.insert(PathBuf::from("/project/.vscode/workspace.txt"));
        // Source dir has one plain file that no detector will claim
        fs.dirs.insert(
            PathBuf::from("/project/.vscode"),
            vec![crate::fs::DirEntry { name: "workspace.txt".to_string(), is_dir: false }],
        );
        // .ai/ dir listing needed by collect_existing_plugin_names
        fs.dirs.insert(PathBuf::from("/project/.ai"), Vec::new());

        let opts = Options {
            dir: Path::new("/project"),
            source: Some(".vscode"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        // ".vscode" is an unknown source type → falls back to all detectors, none of
        // which find any artifacts → registered_entries is empty → other_files has
        // workspace.txt → if let Some(first_entry) = registered_entries.first() is None.
        let result = migrate(&opts, &fs);
        assert!(result.is_ok(), "migrate should succeed even with no artifacts");
        // No OtherFileMigrated because registered_entries was empty (nowhere to put them)
        assert!(
            result.ok().is_some_and(|o| {
                !o.actions.iter().any(|a| matches!(a, Action::OtherFileMigrated { .. }))
            }),
            "other-file emission should be skipped when there are no registered plugins"
        );
    }
}
