//! Migration pipeline: scan AI tool configurations and convert to marketplace plugins.

pub mod adapters;
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
pub mod unified;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::fs::Fs;

pub use error::Error;

// Re-export legacy aliases that downstream call sites and dry-run report
// helpers still import. Keeping these visible from `migrate::*` after the
// internal `migrate_recursive` / `migrate_single_source` paths were retired
// so that nothing outside `unified::run` had to be rewritten.

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
#[derive(Default)]
pub struct Outcome {
    /// Actions taken during migration.
    pub actions: Vec<Action>,
    /// Aggregated counts of features the discovery walker classified.
    /// Populated when the unified migrate path runs (`AIPM_UNIFIED_DISCOVERY=1`);
    /// `Default::default()` when the legacy detector path runs.
    pub scan_counts: crate::discovery::ScanCounts,
    /// Directories the discovery walker descended into. Populated under the
    /// unified path; empty under legacy.
    pub scanned_dirs: Vec<std::path::PathBuf>,
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
///
/// All migration is delegated to [`unified::run`] — the unified discovery +
/// adapters pipeline (with a per-source-dir legacy-detector fallback for the
/// kinds the adapter pipeline does not yet cover). The pre-alpha
/// `AIPM_UNIFIED_DISCOVERY` env-var dispatch and the legacy
/// `migrate_recursive` / `migrate_single_source` paths were retired in this
/// release.
pub fn migrate(opts: &Options<'_>, fs: &dyn Fs) -> Result<Outcome, Error> {
    tracing::debug!(
        source = ?opts.source,
        dry_run = opts.dry_run,
        destructive = opts.destructive,
        "starting migration"
    );
    let ai_dir = opts.dir.join(".ai");

    // 1. Validate .ai/ exists.
    if !fs.exists(&ai_dir) {
        return Err(Error::MarketplaceNotFound(ai_dir));
    }

    // 2. When `--source <name>` is given, validate the named source dir
    //    exists. Preserves the legacy CLI contract of erroring out when the
    //    user explicitly asked for a source that isn't there.
    if let Some(source) = opts.source {
        let source_dir = opts.dir.join(source);
        if !fs.exists(&source_dir) {
            return Err(Error::SourceNotFound(source_dir));
        }
    }

    unified::run(opts, &ai_dir, fs)
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

    /// Lightweight in-memory `Fs` used only by tests that exercise the
    /// outer guards in `migrate` (MarketplaceNotFound / SourceNotFound).
    /// Tests that need to reach the real walker must use a `tempdir`
    /// instead — `crate::discovery::discover` and `discover_source_dirs`
    /// both walk the real filesystem via `ignore::WalkBuilder`.
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

    /// Replacement for the legacy MockFs `migrate_dry_run_writes_report` test.
    /// Uses a real tempdir because the unified path's walker reads the real
    /// filesystem.
    #[test]
    fn migrate_dry_run_writes_report() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".ai/.claude-plugin"))
            .expect("create .ai/.claude-plugin");
        std::fs::write(
            root.join(".ai/.claude-plugin/marketplace.json"),
            r#"{"name":"t","plugins":[]}"#,
        )
        .expect("write marketplace.json");
        std::fs::create_dir_all(root.join(".claude/skills/deploy")).expect("create skill");
        std::fs::write(
            root.join(".claude/skills/deploy/SKILL.md"),
            "---\nname: deploy\ndescription: Deploy app\n---\nDeploy",
        )
        .expect("write SKILL.md");

        let opts = Options {
            dir: root,
            source: Some(".claude"),
            dry_run: true,
            destructive: false,
            max_depth: None,
            manifest: true,
        };
        let outcome = migrate(&opts, &crate::fs::Real).expect("dry-run");
        assert_eq!(outcome.actions.len(), 1);
        assert!(matches!(outcome.actions.first(), Some(Action::DryRunReport { .. })));
        assert!(root.join("aipm-migrate-dryrun-report.md").exists());
    }

    /// Empty `.claude/` source — migration succeeds with no actions emitted.
    #[test]
    fn migrate_empty_source() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".ai/.claude-plugin"))
            .expect("create .ai/.claude-plugin");
        std::fs::write(
            root.join(".ai/.claude-plugin/marketplace.json"),
            r#"{"name":"t","plugins":[]}"#,
        )
        .expect("write marketplace.json");
        std::fs::create_dir_all(root.join(".claude")).expect("create empty .claude");

        let opts = Options {
            dir: root,
            source: Some(".claude"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: true,
        };
        let outcome = migrate(&opts, &crate::fs::Real).expect("ok");
        // No artifacts → no PluginCreated / MarketplaceRegistered actions.
        assert!(!outcome.actions.iter().any(|a| matches!(a, Action::PluginCreated { .. })));
    }

    /// Skill + command in the same root `.claude/` directory: both produce
    /// individual plugins; descriptions plumbed correctly into
    /// marketplace.json.
    #[test]
    fn migrate_full_flow() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".ai/.claude-plugin"))
            .expect("create .ai/.claude-plugin");
        std::fs::write(
            root.join(".ai/.claude-plugin/marketplace.json"),
            r#"{"name":"t","plugins":[]}"#,
        )
        .expect("write marketplace.json");

        std::fs::create_dir_all(root.join(".claude/skills/deploy")).expect("skill dir");
        std::fs::write(
            root.join(".claude/skills/deploy/SKILL.md"),
            "---\nname: deploy\ndescription: Deploy app\n---\nDeploy",
        )
        .expect("SKILL.md");
        std::fs::create_dir_all(root.join(".claude/commands")).expect("commands dir");
        std::fs::write(root.join(".claude/commands/review.md"), "Review the code carefully")
            .expect("review.md");

        let opts = Options {
            dir: root,
            source: Some(".claude"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: true,
        };
        let outcome = migrate(&opts, &crate::fs::Real).expect("migrate ok");
        let plugin_created_count =
            outcome.actions.iter().filter(|a| matches!(a, Action::PluginCreated { .. })).count();
        let marketplace_count = outcome
            .actions
            .iter()
            .filter(|a| matches!(a, Action::MarketplaceRegistered { .. }))
            .count();
        assert_eq!(plugin_created_count, 2, "expected 2 plugins (deploy + review)");
        assert_eq!(marketplace_count, 2);

        // Verify marketplace.json contents.
        let content = std::fs::read_to_string(root.join(".ai/.claude-plugin/marketplace.json"))
            .expect("read marketplace.json");
        let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid JSON");
        let plugins = parsed.get("plugins").and_then(|v| v.as_array()).expect("plugins array");

        let deploy = plugins
            .iter()
            .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("deploy"))
            .expect("deploy entry");
        assert_eq!(
            deploy.get("description").and_then(serde_json::Value::as_str),
            Some("Deploy app"),
            "deploy description should match SKILL.md frontmatter"
        );

        let review = plugins
            .iter()
            .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("review"))
            .expect("review entry");
        assert_eq!(
            review.get("description").and_then(serde_json::Value::as_str),
            Some("Migrated from .claude/ configuration"),
            "review fallback description"
        );
    }

    // =========================================================================
    // Outcome helper method tests
    // =========================================================================

    #[test]
    fn has_migrated_artifacts_empty() {
        let outcome = Outcome { actions: Vec::new(), ..Outcome::default() };
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
            ..Outcome::default()
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
            ..Outcome::default()
        };
        assert!(outcome.has_migrated_artifacts());
    }

    #[test]
    fn migrated_sources_empty() {
        let outcome = Outcome { actions: Vec::new(), ..Outcome::default() };
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
            ..Outcome::default()
        };
        let sources = outcome.migrated_sources();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].0, Path::new("/project/.claude/skills/deploy"));
        assert!(sources[0].1); // is_dir
        assert_eq!(sources[1].0, Path::new("/project/.claude/commands/review.md"));
        assert!(!sources[1].1); // not is_dir
    }

    /// Unknown / non-engine source names like `.custom` or `.vscode` are
    /// passed through `migrate()` after the `SourceNotFound` guard. The
    /// unified path doesn't enumerate them as `.claude` / `.github` source
    /// dirs, so no detection happens and the migration succeeds with no
    /// emitted actions.
    #[test]
    fn migrate_unknown_source_succeeds_with_no_actions() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".ai/.claude-plugin"))
            .expect("create .ai/.claude-plugin");
        std::fs::write(
            root.join(".ai/.claude-plugin/marketplace.json"),
            r#"{"name":"t","plugins":[]}"#,
        )
        .expect("write marketplace.json");
        std::fs::create_dir_all(root.join(".custom")).expect("create .custom");

        let opts = Options {
            dir: root,
            source: Some(".custom"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        assert!(migrate(&opts, &crate::fs::Real).is_ok());
    }

    /// `--source .github` is accepted; an empty `.github/` produces no
    /// actions.
    #[test]
    fn migrate_github_source_accepted() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".ai/.claude-plugin"))
            .expect("create .ai/.claude-plugin");
        std::fs::write(
            root.join(".ai/.claude-plugin/marketplace.json"),
            r#"{"name":"t","plugins":[]}"#,
        )
        .expect("write marketplace.json");
        std::fs::create_dir_all(root.join(".github")).expect("create .github");

        let opts = Options {
            dir: root,
            source: Some(".github"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        assert!(migrate(&opts, &crate::fs::Real).is_ok());
    }

    /// Skill + unclaimed README in the same `.claude/`. The reconciler
    /// attaches README to the plugin, producing an `OtherFileMigrated`
    /// action.
    #[test]
    fn migrate_with_other_files_emits_them() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".ai/.claude-plugin"))
            .expect("create .ai/.claude-plugin");
        std::fs::write(
            root.join(".ai/.claude-plugin/marketplace.json"),
            r#"{"name":"t","plugins":[]}"#,
        )
        .expect("write marketplace.json");
        std::fs::create_dir_all(root.join(".claude/skills/deploy")).expect("skill dir");
        std::fs::write(
            root.join(".claude/skills/deploy/SKILL.md"),
            "---\nname: deploy\ndescription: Deploy\n---\nDeploy",
        )
        .expect("SKILL.md");
        std::fs::write(root.join(".claude/README.md"), "# Notes").expect("README.md");

        let opts = Options {
            dir: root,
            source: Some(".claude"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let outcome = migrate(&opts, &crate::fs::Real).expect("migrate ok");
        assert!(
            outcome.actions.iter().any(|a| matches!(a, Action::OtherFileMigrated { .. })),
            "expected OtherFileMigrated when README sits next to a skill"
        );
    }

    /// `.claude/` containing only a non-artifact file: no plugins emitted,
    /// no marketplace entries.
    #[test]
    fn migrate_other_files_skipped_when_no_artifacts_detected() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".ai/.claude-plugin"))
            .expect("create .ai/.claude-plugin");
        std::fs::write(
            root.join(".ai/.claude-plugin/marketplace.json"),
            r#"{"name":"t","plugins":[]}"#,
        )
        .expect("write marketplace.json");
        std::fs::create_dir_all(root.join(".claude")).expect("create .claude");
        std::fs::write(root.join(".claude/README.md"), "# Notes").expect("README.md");

        let opts = Options {
            dir: root,
            source: Some(".claude"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let outcome = migrate(&opts, &crate::fs::Real).expect("ok");
        assert!(!outcome.actions.iter().any(|a| matches!(a, Action::PluginCreated { .. })));
        assert!(!outcome.actions.iter().any(|a| matches!(a, Action::MarketplaceRegistered { .. })));
    }

    /// Unknown source type with `--dry-run` still produces a dry-run report
    /// (zero artifacts inside).
    #[test]
    fn migrate_unknown_source_dry_run_writes_report() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".ai/.claude-plugin"))
            .expect("create .ai/.claude-plugin");
        std::fs::write(
            root.join(".ai/.claude-plugin/marketplace.json"),
            r#"{"name":"t","plugins":[]}"#,
        )
        .expect("write marketplace.json");
        std::fs::create_dir_all(root.join(".custom")).expect("create .custom");

        let opts = Options {
            dir: root,
            source: Some(".custom"),
            dry_run: true,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let outcome = migrate(&opts, &crate::fs::Real).expect("dry-run");
        assert!(outcome.actions.iter().any(|a| matches!(a, Action::DryRunReport { .. })));
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
            ..Outcome::default()
        };
        let sources = outcome.migrated_sources();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources.first().map(|s| s.0), Some(Path::new("/project/.claude/README.md")));
        // OtherFileMigrated always reports is_dir = false
        assert!(sources.first().is_some_and(|s| !s.1));
    }

    /// Unrecognized source name `custom-source` exists on disk: migration
    /// passes the SourceNotFound guard, the unified pipeline finds nothing
    /// in `.claude` / `.github`, and the outcome is empty.
    #[test]
    fn migrate_with_unrecognized_source_type_succeeds_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".ai/.claude-plugin"))
            .expect("create .ai/.claude-plugin");
        std::fs::write(
            root.join(".ai/.claude-plugin/marketplace.json"),
            r#"{"name":"t","plugins":[]}"#,
        )
        .expect("write marketplace.json");
        std::fs::create_dir_all(root.join("custom-source")).expect("create custom-source");

        let opts = Options {
            dir: root,
            source: Some("custom-source"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let outcome = migrate(&opts, &crate::fs::Real).expect("ok");
        assert!(!outcome.actions.iter().any(|a| matches!(a, Action::PluginCreated { .. })));
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

    /// `--source` not given and no source dirs exist → empty outcome.
    #[test]
    fn migrate_recursive_returns_empty_when_no_source_dirs_exist() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let project_dir = tmp.path();
        std::fs::create_dir_all(project_dir.join(".ai")).expect("create .ai");

        let opts = Options {
            dir: project_dir,
            source: None,
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let outcome = migrate(&opts, &crate::fs::Real).expect("ok");
        assert!(outcome.actions.is_empty(), "expected no actions when no source dirs found");
    }

    /// Empty `.claude/` + recursive dry-run still produces a DryRunReport.
    #[test]
    fn migrate_recursive_dry_run_generates_report() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let project_dir = tmp.path();
        std::fs::create_dir_all(project_dir.join(".ai")).expect("create .ai");
        std::fs::create_dir_all(project_dir.join(".claude")).expect("create .claude");

        let opts = Options {
            dir: project_dir,
            source: None,
            dry_run: true,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let outcome = migrate(&opts, &crate::fs::Real).expect("ok");
        assert!(
            outcome.actions.iter().any(|a| matches!(a, Action::DryRunReport { .. })),
            "expected DryRunReport in recursive dry-run mode"
        );
    }

    /// Recursive non-dry-run with empty source: success, no actions.
    #[test]
    fn migrate_recursive_non_dry_run_no_artifacts_succeeds() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let project_dir = tmp.path();
        std::fs::create_dir_all(project_dir.join(".ai")).expect("create .ai");
        std::fs::create_dir_all(project_dir.join(".claude")).expect("create .claude");

        let opts = Options {
            dir: project_dir,
            source: None,
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let outcome = migrate(&opts, &crate::fs::Real).expect("ok");
        assert!(!outcome.actions.iter().any(|a| matches!(a, Action::PluginCreated { .. })));
    }

    /// Root-level `.claude/` skill: produces a per-artifact PluginCreated.
    #[test]
    fn migrate_recursive_root_skill_creates_plugin() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let project_dir = tmp.path();
        let ai_dir = project_dir.join(".ai");
        let claude_plugin_dir = ai_dir.join(".claude-plugin");
        std::fs::create_dir_all(&claude_plugin_dir).expect("create .ai/.claude-plugin");
        std::fs::write(
            claude_plugin_dir.join("marketplace.json"),
            crate::generate::marketplace::create("test-marketplace", &[]),
        )
        .expect("write marketplace.json");

        let skill_dir = project_dir.join(".claude/skills/my-skill");
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
        let outcome = migrate(&opts, &crate::fs::Real).expect("ok");
        assert!(
            outcome.actions.iter().any(|a| matches!(a, Action::PluginCreated { .. })),
            "expected PluginCreated action for root-level skill"
        );
    }

    /// Package-scoped `mypkg/.claude/` skill: PluginCreated named after
    /// the package, exercising the merge path.
    #[test]
    fn migrate_recursive_package_scoped_source_triggers_is_package_scoped() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let project_dir = tmp.path();
        let ai_dir = project_dir.join(".ai");
        let claude_plugin_dir = ai_dir.join(".claude-plugin");
        std::fs::create_dir_all(&claude_plugin_dir).expect("create .ai/.claude-plugin");
        std::fs::write(
            claude_plugin_dir.join("marketplace.json"),
            crate::generate::marketplace::create("test-marketplace", &[]),
        )
        .expect("write marketplace.json");

        let skill_dir = project_dir.join("mypkg/.claude/skills/pkg-skill");
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
        let outcome = migrate(&opts, &crate::fs::Real).expect("ok");
        assert!(
            outcome.actions.iter().any(|a| matches!(
                a,
                Action::PluginCreated { name, .. } if name == "mypkg"
            )),
            "expected PluginCreated with name 'mypkg' for package-scoped migration"
        );
    }

    /// Recursive `.claude/skills/<name>/SKILL.md` + `.claude/README.md`:
    /// the reconciler finds README and emits an OtherFileMigrated action
    /// alongside the plugin.
    #[test]
    fn migrate_recursive_other_files_emitted() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let project_dir = tmp.path();
        let ai_dir = project_dir.join(".ai");
        let claude_plugin_dir = ai_dir.join(".claude-plugin");
        std::fs::create_dir_all(&claude_plugin_dir).expect("create .ai/.claude-plugin");
        std::fs::write(
            claude_plugin_dir.join("marketplace.json"),
            crate::generate::marketplace::create("test-marketplace", &[]),
        )
        .expect("write marketplace.json");

        let skill_dir = project_dir.join(".claude/skills/deploy");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: deploy\ndescription: Deploy skill\n---\nDeploy",
        )
        .expect("write SKILL.md");
        std::fs::write(project_dir.join(".claude/README.md"), "# Notes").expect("README.md");

        let opts = Options {
            dir: project_dir,
            source: None,
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let outcome = migrate(&opts, &crate::fs::Real).expect("ok");
        assert!(
            outcome.actions.iter().any(|a| matches!(a, Action::OtherFileMigrated { .. })),
            "expected OtherFileMigrated action"
        );
    }
}
