//! Unified migrate orchestrator: `discovery::discover` + adapters pipeline,
//! with a per-source-dir legacy-detector fallback for kinds the unified
//! discovery does not yet classify.
//!
//! This is the **default and only** migrate path — the alpha release flipped
//! the previous opt-in `AIPM_UNIFIED_DISCOVERY` env var to unconditionally on
//! and removed the legacy `migrate_recursive` dispatch in `migrate::migrate`.
//!
//! # Pipeline
//!
//! 1. Walk the project tree once via [`crate::discovery::discover`] to get
//!    the full [`crate::discovery::DiscoveredSet`] (features, scanned dirs,
//!    skip reasons).
//! 2. For each [`crate::discovery::DiscoveredFeature`], select the first
//!    adapter from [`super::adapters::all`] whose `applies_to` returns
//!    `true`, and call its `to_artifact`.
//! 3. **Hybrid fallback**: enumerate `.claude` / `.github` source dirs via
//!    [`crate::discovery::discover_source_dirs`] and, for each one, run the
//!    legacy detectors that produce kinds the adapter pipeline doesn't yet
//!    cover (Claude embedded `settings.json` hooks, MCP, Extension, LSP,
//!    Command, `OutputStyle`). The legacy Skill/Agent/Copilot-Hook
//!    detectors are deliberately skipped — those are already produced by
//!    the unified path's adapters and double-running would create
//!    duplicates.
//! 4. Group artifacts by source dir. For package-scoped sources
//!    (`DiscoveredSource::package_name == Some(_)`) all artifacts merge
//!    into a single plugin named after the package. For root-level sources
//!    each artifact becomes its own plugin (the legacy contract).
//! 5. Per source dir: run [`super::reconciler::reconcile`] then
//!    [`super::emitter::emit_plugin_with_name`] (package-scoped) or
//!    [`super::emitter::emit_plugin`] (root-level) for each artifact.
//! 6. Register the resulting plugin entries in `marketplace.json`.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::discovery::{DiscoverOptions, DiscoveredSet, DiscoveredSource};
use crate::fs::Fs;

use super::adapters;
use super::detector::Detector;
use super::dry_run;
use super::emitter;
use super::reconciler;
use super::registrar;
use super::{
    Action, Artifact, ArtifactKind, Error, Options, OtherFile, Outcome, PluginEntry, PluginPlan,
};

/// Run the unified migrate pipeline.
///
/// `ai_dir` must already be validated to exist by the caller.
///
/// When `opts.source` is `Some(name)`, only artifacts under the literal
/// `opts.dir/<name>` source directory are kept (legacy single-path
/// semantics) — sub-package `.claude/` / `.github/` directories at deeper
/// nesting are excluded. When `opts.source` is `None`, every
/// `.claude` / `.github` source dir found by `discover_source_dirs`
/// participates.
pub fn run(opts: &Options<'_>, ai_dir: &Path, fs: &dyn Fs) -> Result<Outcome, Error> {
    let discover_opts = DiscoverOptions::from(opts);
    let mut discovered = crate::discovery::discover(opts.dir, &discover_opts, fs)?;
    if let Some(source) = opts.source {
        let pinned_root = opts.dir.join(source);
        discovered.features.retain(|f| f.source_root == pinned_root);
    }
    tracing::debug!(
        features = discovered.features.len(),
        scanned_dirs = discovered.scanned_dirs.len(),
        "unified discovery complete"
    );

    let adapter_artifacts = group_adapter_artifacts(&discovered, fs)?;

    // Enumerate source dirs (.claude / .github) so we can:
    //  - run deferred legacy detectors per source dir for kinds the
    //    adapter pipeline doesn't cover, and
    //  - know which sources are package-scoped (need merged plugin name).
    let sources = enumerate_sources(opts)?;
    let plans = build_plugin_plans(&adapter_artifacts, &sources, fs)?;

    if opts.dry_run {
        return write_dry_run_report(opts, ai_dir, &discovered, &sources, &plans, fs);
    }

    emit_and_register(plans, ai_dir, opts.manifest, &discovered, fs)
}

/// Walk the discovered set and route each feature through `adapters::all()`,
/// grouping resulting artifacts by their `source_root`.
///
/// Uses [`BTreeMap`] (not `HashMap`) so iteration order is deterministic
/// across runs — migrate emits actions in source-root order, and within
/// each root the artifacts are sorted by `(kind, name)`. Without this,
/// `PluginCreated` / `MarketplaceRegistered` ordering would shuffle
/// between invocations and snapshot tests would flake.
fn group_adapter_artifacts(
    discovered: &DiscoveredSet,
    fs: &dyn Fs,
) -> Result<BTreeMap<PathBuf, Vec<Artifact>>, Error> {
    let registry = adapters::all();
    let mut by_root: BTreeMap<PathBuf, Vec<Artifact>> = BTreeMap::new();
    for feat in &discovered.features {
        for adapter in &registry {
            if adapter.applies_to(feat) {
                let artifact = adapter.to_artifact(feat, fs)?;
                tracing::trace!(
                    name = %artifact.name,
                    kind = ?artifact.kind,
                    "unified adapter produced artifact"
                );
                by_root.entry(feat.source_root.clone()).or_default().push(artifact);
                break;
            }
        }
    }
    // Dedup Agent artifacts that share a name. Mirrors the legacy
    // `CopilotAgentDetector` precedence rule where `.agent.md` wins over
    // `.md` for the same stem. Without this, a repo containing both
    // `agents/foo.md` and `agents/foo.agent.md` would emit two artifacts
    // with the same derived name and trigger spurious `Renamed` actions.
    for artifacts in by_root.values_mut() {
        dedup_agent_artifacts(artifacts);
    }
    Ok(by_root)
}

/// Dedup `Agent` artifacts that share a name. When both `foo.md` and
/// `foo.agent.md` were discovered (the unified pipeline produces a
/// `DiscoveredFeature` per file), keep only the one whose `source_path`
/// ends with `.agent.md`. Mirrors the legacy `CopilotAgentDetector`
/// precedence rule.
fn dedup_agent_artifacts(artifacts: &mut Vec<Artifact>) {
    use std::collections::BTreeMap;
    let mut chosen: BTreeMap<String, usize> = BTreeMap::new();
    let mut drop_indices: Vec<usize> = Vec::new();
    for (idx, artifact) in artifacts.iter().enumerate() {
        if artifact.kind != ArtifactKind::Agent {
            continue;
        }
        match chosen.get(&artifact.name) {
            None => {
                chosen.insert(artifact.name.clone(), idx);
            },
            Some(&existing_idx) => {
                let new_is_dot_agent = is_dot_agent_md(&artifact.source_path);
                let existing_is_dot_agent =
                    artifacts.get(existing_idx).is_some_and(|a| is_dot_agent_md(&a.source_path));
                if new_is_dot_agent && !existing_is_dot_agent {
                    // New `.agent.md` wins; drop the existing `.md`.
                    drop_indices.push(existing_idx);
                    chosen.insert(artifact.name.clone(), idx);
                } else {
                    // Existing wins (already `.agent.md` or both `.md`);
                    // drop the new one.
                    drop_indices.push(idx);
                }
            },
        }
    }
    if drop_indices.is_empty() {
        return;
    }
    drop_indices.sort_unstable();
    drop_indices.dedup();
    // Drop in reverse order so earlier indices stay valid as we remove.
    for idx in drop_indices.into_iter().rev() {
        artifacts.remove(idx);
    }
}

/// `true` if the path's filename ends with `.agent.md` (case-insensitive).
fn is_dot_agent_md(path: &Path) -> bool {
    path.file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|n| n.ends_with(".agent.md"))
}

/// Enumerate `.claude` / `.github` source directories under `opts.dir` so
/// the hybrid orchestrator can run deferred legacy detectors per source
/// and assign package names for package-scoped sources.
///
/// When `opts.source` is set, only that one literal source directory at
/// the project root is returned (the legacy `--source <name>` single-path
/// semantics — no recursive sub-package discovery). Sub-package
/// `.claude/` / `.github/` directories are intentionally excluded so
/// `aipm migrate --source .claude` does not accidentally pull in
/// `mypkg/.claude/` artifacts.
fn enumerate_sources(opts: &Options<'_>) -> Result<Vec<DiscoveredSource>, Error> {
    if let Some(source) = opts.source {
        let source_dir = opts.dir.join(source);
        if !source_dir.exists() {
            return Ok(Vec::new());
        }
        return Ok(vec![DiscoveredSource {
            source_dir,
            source_type: source.to_string(),
            package_name: None,
            relative_path: PathBuf::new(),
        }]);
    }
    let sources =
        crate::discovery::discover_source_dirs(opts.dir, &[".claude", ".github"], opts.max_depth)?;
    Ok(sources)
}

/// Returns the legacy detectors whose artifact kinds are NOT covered by the
/// adapter pipeline. The unified path produces Skill, Agent, and Copilot
/// Hook directly via adapters; this fallback covers everything else
/// (Claude embedded `settings.json` hook, MCP, Extension, LSP, Command,
/// `OutputStyle`).
fn deferred_legacy_detectors(source_type: &str) -> Vec<Box<dyn Detector>> {
    use super::{
        command_detector::CommandDetector, copilot_extension_detector::CopilotExtensionDetector,
        copilot_lsp_detector::CopilotLspDetector, copilot_mcp_detector::CopilotMcpDetector,
        hook_detector::HookDetector, mcp_detector::McpDetector,
        output_style_detector::OutputStyleDetector,
    };
    match source_type {
        ".claude" => vec![
            Box::new(CommandDetector),
            Box::new(McpDetector),
            Box::new(HookDetector),
            Box::new(OutputStyleDetector),
        ],
        ".github" => vec![
            Box::new(CopilotMcpDetector),
            Box::new(CopilotExtensionDetector),
            Box::new(CopilotLspDetector),
        ],
        _ => Vec::new(),
    }
}

/// For each enumerated source, combine adapter-produced artifacts (already
/// grouped by source root) with artifacts from the deferred legacy detectors,
/// then form `PluginPlan`s honoring the package-scoped vs root-level
/// distinction.
fn build_plugin_plans(
    adapter_artifacts: &BTreeMap<PathBuf, Vec<Artifact>>,
    sources: &[DiscoveredSource],
    fs: &dyn Fs,
) -> Result<Vec<PluginPlan>, Error> {
    let mut plans = Vec::new();
    let mut handled_roots: HashSet<PathBuf> = HashSet::new();

    for src in sources {
        handled_roots.insert(src.source_dir.clone());
        let mut artifacts = adapter_artifacts.get(&src.source_dir).cloned().unwrap_or_default();

        for det in deferred_legacy_detectors(&src.source_type) {
            let extra = det.detect(&src.source_dir, fs)?;
            artifacts.extend(extra);
        }

        // Stable per-source ordering: sort by (kind, name) so emitted
        // actions are deterministic regardless of detector order.
        artifacts.sort_by(|a, b| {
            (format!("{:?}", a.kind), &a.name).cmp(&(format!("{:?}", b.kind), &b.name))
        });

        if artifacts.is_empty() {
            continue;
        }

        let other_files = reconciler::reconcile(&src.source_dir, &artifacts, fs)?;

        if let Some(ref pkg_name) = src.package_name {
            plans.push(PluginPlan {
                name: pkg_name.clone(),
                artifacts,
                is_package_scoped: true,
                source_dir: src.source_dir.clone(),
                other_files,
            });
        } else {
            // Root-level: each artifact becomes its own plugin.
            // Attach other_files to the first plan so they appear in
            // reports and migrated output.
            let source_dir = src.source_dir.clone();
            let mut per_artifact: Vec<PluginPlan> = artifacts
                .into_iter()
                .map(|a| PluginPlan {
                    name: a.name.clone(),
                    artifacts: vec![a],
                    is_package_scoped: false,
                    source_dir: source_dir.clone(),
                    other_files: Vec::new(),
                })
                .collect();
            if let Some(first) = per_artifact.first_mut() {
                first.other_files = other_files;
            }
            plans.extend(per_artifact);
        }
    }

    // Adapter artifacts with a source_root that did NOT come back from
    // discover_source_dirs (e.g. nested layouts with no engine ancestor
    // matching ".claude" / ".github" by literal name) still need a plan
    // so their plugins get emitted. Treat them as root-level.
    for (root, artifacts) in adapter_artifacts {
        if handled_roots.contains(root) {
            continue;
        }
        let mut sorted = artifacts.clone();
        sorted.sort_by(|a, b| {
            (format!("{:?}", a.kind), &a.name).cmp(&(format!("{:?}", b.kind), &b.name))
        });
        let other_files = reconciler::reconcile(root, &sorted, fs)?;
        let source_dir = root.clone();
        let mut per_artifact: Vec<PluginPlan> = sorted
            .into_iter()
            .map(|a| PluginPlan {
                name: a.name.clone(),
                artifacts: vec![a],
                is_package_scoped: false,
                source_dir: source_dir.clone(),
                other_files: Vec::new(),
            })
            .collect();
        if let Some(first) = per_artifact.first_mut() {
            first.other_files = other_files;
        }
        plans.extend(per_artifact);
    }

    Ok(plans)
}

/// Resolve names, emit plugins, and register entries in marketplace.json.
fn emit_and_register(
    plans: Vec<PluginPlan>,
    ai_dir: &Path,
    manifest: bool,
    discovered: &DiscoveredSet,
    fs: &dyn Fs,
) -> Result<Outcome, Error> {
    let existing_plugins: HashSet<String> = super::collect_existing_plugin_names(ai_dir, fs)?;

    let mut known_names = existing_plugins;
    let mut rename_counter: u32 = 0;
    let mut rename_actions: Vec<Action> = Vec::new();
    let mut resolved: Vec<(PluginPlan, String)> = Vec::new();
    for plan in plans {
        let final_name = emitter::resolve_plugin_name(
            &plan.name,
            &known_names,
            &mut rename_counter,
            &mut rename_actions,
        );
        known_names.insert(final_name.clone());
        resolved.push((plan, final_name));
    }

    let mut all_actions = rename_actions;
    let mut registered_entries: Vec<PluginEntry> = Vec::new();

    for (plan, final_name) in &resolved {
        let mut emit_actions = Vec::new();
        if plan.is_package_scoped {
            emit_actions.extend(emitter::emit_package_plugin(
                final_name,
                &plan.artifacts,
                ai_dir,
                manifest,
                fs,
            )?);
        } else if let Some(artifact) = plan.artifacts.first() {
            emit_actions.extend(emitter::emit_plugin_with_name(
                artifact, final_name, ai_dir, manifest, fs,
            )?);
        }

        if !plan.other_files.is_empty() {
            let plugin_dir = ai_dir.join(final_name);
            emit_actions.extend(emitter::emit_other_files(&plan.other_files, &plugin_dir, fs)?);
        }

        all_actions.extend(emit_actions);

        let description = plan.artifacts.first().and_then(|a| a.metadata.description.clone());
        registered_entries.push(PluginEntry { name: final_name.clone(), description });
    }

    registrar::register_plugins(ai_dir, &registered_entries, fs)?;
    for entry in &registered_entries {
        all_actions.push(Action::MarketplaceRegistered { name: entry.name.clone() });
    }

    Ok(Outcome {
        actions: all_actions,
        scan_counts: discovered.counts(),
        scanned_dirs: discovered.scanned_dirs.clone(),
    })
}

/// Write a dry-run report. Emits the recursive (per-source-directory) report
/// shape when `--source` is not set so the "Recursive discovery" / "Other
/// Files" sections are present, and the legacy single-source report when
/// `--source` is given so the "Other Files" section format matches.
fn write_dry_run_report(
    opts: &Options<'_>,
    ai_dir: &Path,
    discovered: &DiscoveredSet,
    sources: &[DiscoveredSource],
    plans: &[PluginPlan],
    fs: &dyn Fs,
) -> Result<Outcome, Error> {
    let existing_plugins = super::collect_existing_plugin_names(ai_dir, fs)?;

    let report = if opts.source.is_some() {
        let all_artifacts: Vec<Artifact> =
            plans.iter().flat_map(|p| p.artifacts.iter().cloned()).collect();
        let all_other_files: Vec<OtherFile> =
            plans.iter().flat_map(|p| p.other_files.iter().cloned()).collect();
        dry_run::generate_report(
            &all_artifacts,
            &existing_plugins,
            opts.source.unwrap_or("unified"),
            opts.manifest,
            opts.destructive,
            &all_other_files,
        )
    } else {
        dry_run::generate_recursive_report(sources, plans, &existing_plugins, opts.destructive)
    };

    let report_path = opts.dir.join("aipm-migrate-dryrun-report.md");
    fs.write_file(&report_path, report.as_bytes())?;
    Ok(Outcome {
        actions: vec![Action::DryRunReport { path: report_path }],
        scan_counts: discovered.counts(),
        scanned_dirs: discovered.scanned_dirs.clone(),
    })
}

impl<'a> From<&'a Options<'a>> for DiscoverOptions {
    fn from(opts: &'a Options<'a>) -> Self {
        Self {
            max_depth: opts.max_depth,
            source_filter: opts.source.map(String::from),
            follow_symlinks: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Real;

    /// Build a minimal `.ai/` so MarketplaceNotFound doesn't fire.
    fn init_marketplace(root: &Path) {
        let claude_plugin = root.join(".ai/.claude-plugin");
        std::fs::create_dir_all(&claude_plugin).expect("create .ai/.claude-plugin");
        std::fs::write(claude_plugin.join("marketplace.json"), r#"{"name":"test","plugins":[]}"#)
            .expect("write marketplace.json");
    }

    #[test]
    fn unified_migrate_finds_issue_725_skills() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        init_marketplace(root);

        for name in ["skill-alpha", "skill-beta", "skill-gamma"] {
            let dir = root.join(format!(".github/copilot/skills/{name}"));
            std::fs::create_dir_all(&dir).expect("create skill dir");
            std::fs::write(
                dir.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: For {name}\n---\n# {name}\n"),
            )
            .expect("write SKILL.md");
        }

        let opts = Options {
            dir: root,
            source: None,
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let ai_dir = root.join(".ai");
        let outcome = run(&opts, &ai_dir, &Real).expect("migrate succeeds");
        let plugin_created_count =
            outcome.actions.iter().filter(|a| matches!(a, Action::PluginCreated { .. })).count();
        assert_eq!(plugin_created_count, 3, "expected 3 plugins migrated");
        assert_eq!(outcome.scan_counts.skills, 3);
    }

    #[test]
    fn unified_migrate_dry_run_writes_report() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        init_marketplace(root);
        let dir = root.join(".github/copilot/skills/skill-alpha");
        std::fs::create_dir_all(&dir).expect("create");
        std::fs::write(
            dir.join("SKILL.md"),
            "---\nname: skill-alpha\ndescription: Alpha\n---\n# alpha\n",
        )
        .expect("write");

        let opts = Options {
            dir: root,
            source: None,
            dry_run: true,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let ai_dir = root.join(".ai");
        let outcome = run(&opts, &ai_dir, &Real).expect("dry run succeeds");
        assert!(matches!(outcome.actions.first(), Some(Action::DryRunReport { .. })));
        assert!(root.join("aipm-migrate-dryrun-report.md").exists());
        assert_eq!(outcome.scan_counts.skills, 1);
    }

    #[test]
    fn unified_migrate_empty_tree_returns_empty_outcome() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        init_marketplace(root);
        let opts = Options {
            dir: root,
            source: None,
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let ai_dir = root.join(".ai");
        let outcome = run(&opts, &ai_dir, &Real).expect("ok");
        assert_eq!(outcome.scan_counts.total(), 1, ".ai marketplace.json counts");
        let _ = outcome.actions;
    }

    #[test]
    fn discover_options_from_options_maps_correctly() {
        let opts = Options {
            dir: Path::new("/repo"),
            source: Some(".github"),
            dry_run: false,
            destructive: false,
            max_depth: Some(5),
            manifest: false,
        };
        let do_opts: DiscoverOptions = (&opts).into();
        assert_eq!(do_opts.max_depth, Some(5));
        assert_eq!(do_opts.source_filter, Some(".github".to_string()));
        assert!(!do_opts.follow_symlinks);
    }

    #[test]
    fn deferred_detectors_for_claude_includes_settings_hook_and_command() {
        let detectors = deferred_legacy_detectors(".claude");
        let names: Vec<&str> = detectors.iter().map(|d| d.name()).collect();
        assert!(names.contains(&"command"));
        assert!(names.contains(&"hook"));
        assert!(names.contains(&"mcp"));
        assert!(names.contains(&"output-style"));
        // Skill/Agent are deliberately excluded — adapter pipeline covers them.
        assert!(!names.contains(&"skill"));
        assert!(!names.contains(&"agent"));
    }

    #[test]
    fn deferred_detectors_for_github_excludes_skill_agent_hook() {
        let detectors = deferred_legacy_detectors(".github");
        let names: Vec<&str> = detectors.iter().map(|d| d.name()).collect();
        assert!(names.contains(&"copilot-mcp"));
        assert!(names.contains(&"copilot-extension"));
        assert!(names.contains(&"copilot-lsp"));
        // Adapter pipeline covers these — must NOT double-detect.
        assert!(!names.contains(&"copilot-skill"));
        assert!(!names.contains(&"copilot-agent"));
        assert!(!names.contains(&"copilot-hook"));
    }

    #[test]
    fn deferred_detectors_for_unknown_source_is_empty() {
        assert!(deferred_legacy_detectors(".unknown").is_empty());
    }

    #[test]
    fn unified_migrate_package_scoped_merges_skill_and_command() {
        // Package-scoped: a sub-package's .claude/ with both a skill and a
        // command must merge into a single plugin named after the package.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        init_marketplace(root);

        let skill_dir = root.join("auth/.claude/skills/deploy");
        std::fs::create_dir_all(&skill_dir).expect("create");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: deploy\ndescription: Deploy\n---\nDeploy\n",
        )
        .expect("write SKILL.md");

        let cmd_dir = root.join("auth/.claude/commands");
        std::fs::create_dir_all(&cmd_dir).expect("create commands");
        std::fs::write(cmd_dir.join("review.md"), "Review the code").expect("write command");

        let opts = Options {
            dir: root,
            source: None,
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let ai_dir = root.join(".ai");
        let outcome = run(&opts, &ai_dir, &Real).expect("migrate succeeds");
        // ONE plugin should be created — named "auth" — merging both
        // artifacts (the skill from the adapter, the command from the
        // legacy detector fallback).
        let auth_creates: Vec<&Action> = outcome
            .actions
            .iter()
            .filter(|a| matches!(a, Action::PluginCreated { name, .. } if name == "auth"))
            .collect();
        assert!(!auth_creates.is_empty(), "expected at least one PluginCreated for 'auth'");
        // The auth plugin directory should contain BOTH a skill and a
        // converted command sub-directory.
        let deploy_skill = root.join(".ai/auth/skills/deploy/SKILL.md");
        let review_skill = root.join(".ai/auth/skills/review/SKILL.md");
        assert!(deploy_skill.exists(), "expected merged skill at {}", deploy_skill.display());
        assert!(review_skill.exists(), "expected merged command at {}", review_skill.display());
    }

    #[test]
    fn unified_migrate_root_settings_json_hook_is_detected() {
        // Claude embedded hook (`.claude/settings.json` with hooks) is a
        // deferred kind — only the legacy HookDetector picks it up.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        init_marketplace(root);

        let claude_dir = root.join(".claude");
        std::fs::create_dir_all(&claude_dir).expect("create .claude");
        std::fs::write(
            claude_dir.join("settings.json"),
            r#"{"hooks":{"PreToolUse":[{"matcher":"x","hooks":[{"type":"command","command":"echo hi"}]}]}}"#,
        )
        .expect("write settings.json");

        let opts = Options {
            dir: root,
            source: None,
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let ai_dir = root.join(".ai");
        let outcome = run(&opts, &ai_dir, &Real).expect("migrate succeeds");
        // The Claude embedded hook becomes an artifact named "project-hooks".
        assert!(
            outcome.actions.iter().any(
                |a| matches!(a, Action::PluginCreated { name, .. } if name == "project-hooks")
            ),
            "expected a project-hooks plugin from .claude/settings.json"
        );
    }

    #[test]
    fn unified_migrate_recursive_dry_run_contains_recursive_discovery_section() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        init_marketplace(root);
        let skill_dir = root.join("auth/.claude/skills/deploy");
        std::fs::create_dir_all(&skill_dir).expect("create");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: deploy\ndescription: Deploy\n---\nDeploy\n",
        )
        .expect("write SKILL.md");

        let opts = Options {
            dir: root,
            source: None,
            dry_run: true,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let ai_dir = root.join(".ai");
        run(&opts, &ai_dir, &Real).expect("dry-run ok");
        let report =
            std::fs::read_to_string(root.join("aipm-migrate-dryrun-report.md")).expect("read");
        assert!(
            report.contains("Recursive discovery"),
            "report should contain 'Recursive discovery' header; got:\n{report}"
        );
    }

    /// Covers the `if !source_dir.exists()` early-return branch in
    /// `enumerate_sources`: when `opts.source` points to a directory that
    /// does not exist the function returns an empty Vec and `run` produces
    /// an empty `Outcome` with no actions.
    #[test]
    fn unified_migrate_source_dir_not_found_returns_empty_outcome() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        init_marketplace(root);

        // ".nonexistent" is never created, so source_dir.exists() == false
        let opts = Options {
            dir: root,
            source: Some(".nonexistent"),
            dry_run: false,
            destructive: false,
            max_depth: None,
            manifest: false,
        };
        let ai_dir = root.join(".ai");
        let outcome = run(&opts, &ai_dir, &Real)
            .expect("run should succeed even when the named source dir is absent");

        assert_eq!(outcome.actions.len(), 0, "no artifacts — source directory did not exist");
        assert_eq!(
            outcome.scan_counts.total(),
            0,
            "no features after filtering by the pinned (nonexistent) source root"
        );
    }

    /// Smoke check that artifact ordering is stable across repeated runs
    /// by virtue of the per-source `(kind, name)` sort.
    #[test]
    fn artifacts_sorted_by_kind_then_name_for_stability() {
        let a1 = Artifact {
            kind: super::super::ArtifactKind::Skill,
            name: "z".into(),
            source_path: PathBuf::new(),
            files: Vec::new(),
            referenced_scripts: Vec::new(),
            metadata: Default::default(),
        };
        let mut a2 = a1.clone();
        a2.name = "a".into();
        let mut v = vec![a1.clone(), a2.clone()];
        v.sort_by(|x, y| {
            (format!("{:?}", x.kind), &x.name).cmp(&(format!("{:?}", y.kind), &y.name))
        });
        assert_eq!(v[0].name, "a");
        assert_eq!(v[1].name, "z");
    }

    /// Covers `dedup_agent_artifacts`: when two Agent artifacts share a name
    /// and one ends with `.agent.md`, only the `.agent.md` version survives.
    #[test]
    fn dedup_agent_artifacts_dot_agent_md_wins_over_plain_md() {
        use super::super::ArtifactKind;

        let plain = Artifact {
            kind: ArtifactKind::Agent,
            name: "foo".into(),
            source_path: PathBuf::from("/repo/.claude/agents/foo.md"),
            files: Vec::new(),
            referenced_scripts: Vec::new(),
            metadata: Default::default(),
        };
        let dot_agent = Artifact {
            kind: ArtifactKind::Agent,
            name: "foo".into(),
            source_path: PathBuf::from("/repo/.claude/agents/foo.agent.md"),
            files: Vec::new(),
            referenced_scripts: Vec::new(),
            metadata: Default::default(),
        };
        let mut artifacts = vec![plain, dot_agent];
        dedup_agent_artifacts(&mut artifacts);
        assert_eq!(artifacts.len(), 1, "deduplication must keep exactly one artifact");
        assert!(
            artifacts[0]
                .source_path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".agent.md")),
            "the .agent.md variant must survive deduplication"
        );
    }

    /// Covers `is_dot_agent_md`: detects `.agent.md` suffix case-insensitively.
    #[test]
    fn is_dot_agent_md_detects_suffix() {
        assert!(is_dot_agent_md(Path::new("/repo/.claude/agents/foo.agent.md")));
        assert!(is_dot_agent_md(Path::new("FOO.AGENT.MD")));
        assert!(!is_dot_agent_md(Path::new("/repo/.claude/agents/foo.md")));
        assert!(!is_dot_agent_md(Path::new("/repo/.claude/agents/agent.md")));
    }
}
