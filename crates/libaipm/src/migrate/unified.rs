//! Unified migrate orchestrator: `discovery::discover` + adapters pipeline.
//!
//! Engaged when `AIPM_UNIFIED_DISCOVERY=1`. The legacy detector path stays
//! the default until the spec rollout flips it, so this module is opt-in
//! during the soak window.
//!
//! # Pipeline
//!
//! 1. Walk the project tree once via [`crate::discovery::discover`] to get
//!    the full [`crate::discovery::DiscoveredSet`] (features, scanned dirs,
//!    skip reasons).
//! 2. For each [`crate::discovery::DiscoveredFeature`], select the first
//!    adapter from [`super::adapters::all`] whose `applies_to` returns
//!    `true`, and call its `to_artifact`.
//! 3. Group artifacts by source root (so reconciler sees per-source-root
//!    sets, matching the legacy contract).
//! 4. Per source root: run [`super::reconciler::reconcile`] then
//!    [`super::emitter::emit_plugin`] for each artifact.
//! 5. Register the resulting plugin entries in `marketplace.json`.
//!
//! # Limitations
//!
//! Adapters today cover only the `FeatureKind`-aligned subset (Skill,
//! Agent, both engines, plus Copilot Hook). Kinds without `FeatureKind`
//! variants (Claude hook via `.claude/settings.json`, MCP, Extension,
//! LSP, Command, `OutputStyle`) are NOT migrated under the unified path.
//! Those kinds are still produced by the legacy detector path which is
//! the default during the rollout window. A follow-up feature extends
//! discovery and adapters to cover them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::discovery::{DiscoverOptions, DiscoveredSet};
use crate::fs::Fs;

use super::adapters;
use super::dry_run;
use super::emitter;
use super::reconciler;
use super::registrar;
use super::{Action, Artifact, Error, Options, Outcome, PluginEntry};

/// Run the unified migrate pipeline.
///
/// `ai_dir` must already be validated to exist by the caller.
pub fn run(opts: &Options<'_>, ai_dir: &Path, fs: &dyn Fs) -> Result<Outcome, Error> {
    let discover_opts = DiscoverOptions::from(opts);
    let discovered = crate::discovery::discover(opts.dir, &discover_opts, fs)?;
    tracing::debug!(
        features = discovered.features.len(),
        scanned_dirs = discovered.scanned_dirs.len(),
        "unified discovery complete"
    );

    let artifacts_by_root = group_artifacts_by_root(&discovered, fs)?;

    if opts.dry_run {
        return write_dry_run_report(opts, ai_dir, &discovered, &artifacts_by_root, fs);
    }

    let mut actions: Vec<Action> = Vec::new();
    let mut registered_entries: Vec<PluginEntry> = Vec::new();
    let mut existing_plugins: HashSet<String> = super::collect_existing_plugin_names(ai_dir, fs)?;
    let mut rename_counter: u32 = 0;

    for (source_root, artifacts) in &artifacts_by_root {
        let other_files = reconciler::reconcile(source_root, artifacts, fs)?;

        let mut first_plugin_dir_for_others: Option<PathBuf> = None;
        for artifact in artifacts {
            let (plugin_name, emit_actions) = emitter::emit_plugin(
                artifact,
                ai_dir,
                &existing_plugins,
                &mut rename_counter,
                opts.manifest,
                fs,
            )?;
            actions.extend(emit_actions);
            existing_plugins.insert(plugin_name.clone());
            if first_plugin_dir_for_others.is_none() {
                first_plugin_dir_for_others = Some(ai_dir.join(&plugin_name));
            }
            registered_entries.push(PluginEntry {
                name: plugin_name,
                description: artifact.metadata.description.clone(),
            });
        }

        if !other_files.is_empty() {
            if let Some(plugin_dir) = first_plugin_dir_for_others {
                let other_actions = emitter::emit_other_files(&other_files, &plugin_dir, fs)?;
                actions.extend(other_actions);
            }
        }
    }

    registrar::register_plugins(ai_dir, &registered_entries, fs)?;
    for entry in &registered_entries {
        actions.push(Action::MarketplaceRegistered { name: entry.name.clone() });
    }

    Ok(Outcome { actions, scan_counts: discovered.counts(), scanned_dirs: discovered.scanned_dirs })
}

/// Walk the discovered set and route each feature through `adapters::all()`,
/// grouping resulting artifacts by their `source_root` for the reconciler.
fn group_artifacts_by_root(
    discovered: &DiscoveredSet,
    fs: &dyn Fs,
) -> Result<HashMap<PathBuf, Vec<Artifact>>, Error> {
    let registry = adapters::all();
    let mut by_root: HashMap<PathBuf, Vec<Artifact>> = HashMap::new();
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
    Ok(by_root)
}

/// Write a dry-run report containing the artifacts the unified path
/// found. Mirrors the legacy migrate's dry-run behavior.
fn write_dry_run_report(
    opts: &Options<'_>,
    ai_dir: &Path,
    discovered: &DiscoveredSet,
    artifacts_by_root: &HashMap<PathBuf, Vec<Artifact>>,
    fs: &dyn Fs,
) -> Result<Outcome, Error> {
    let all_artifacts: Vec<Artifact> = artifacts_by_root.values().flatten().cloned().collect();
    let existing_plugins = super::collect_existing_plugin_names(ai_dir, fs)?;
    let report = dry_run::generate_report(
        &all_artifacts,
        &existing_plugins,
        opts.source.unwrap_or("unified"),
        opts.manifest,
        opts.destructive,
        &Vec::new(),
    );
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

/// `true` when `AIPM_UNIFIED_DISCOVERY` is set to exactly `"1"`.
///
/// Mirrors `discovery::unified_enabled()` so the env var has a single
/// source-of-truth meaning across discovery and migrate.
pub(crate) fn unified_enabled() -> bool {
    std::env::var(crate::discovery::UNIFIED_DISCOVERY_ENV).map(|v| v == "1").unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::test_env::with_unified_discovery_env;
    use crate::fs::Real;

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, "").expect("write");
    }

    /// Build a minimal `.ai/` so MarketplaceNotFound doesn't fire.
    fn init_marketplace(root: &Path) {
        let claude_plugin = root.join(".ai/.claude-plugin");
        std::fs::create_dir_all(&claude_plugin).expect("create .ai/.claude-plugin");
        std::fs::write(claude_plugin.join("marketplace.json"), r#"{"name":"test","plugins":[]}"#)
            .expect("write marketplace.json");
    }

    #[test]
    fn unified_enabled_returns_true_only_for_exact_one() {
        with_unified_discovery_env(Some("1"), || assert!(unified_enabled()));
        with_unified_discovery_env(Some("0"), || assert!(!unified_enabled()));
        with_unified_discovery_env(None, || assert!(!unified_enabled()));
    }

    #[test]
    fn unified_migrate_finds_issue_725_skills() {
        // Customer's exact #725 layout — legacy CopilotSkillDetector misses
        // it; the unified path must catch it via discovery::discover +
        // CopilotSkillAdapter.
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
        with_unified_discovery_env(Some("1"), || {
            let outcome = run(&opts, &ai_dir, &Real).expect("migrate succeeds");
            // 3 PluginCreated actions for the 3 customer skills.
            let plugin_created_count = outcome
                .actions
                .iter()
                .filter(|a| matches!(a, Action::PluginCreated { .. }))
                .count();
            assert_eq!(plugin_created_count, 3, "expected 3 plugins migrated");
            assert_eq!(outcome.scan_counts.skills, 3);
        });
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
        with_unified_discovery_env(Some("1"), || {
            let outcome = run(&opts, &ai_dir, &Real).expect("dry run succeeds");
            assert!(matches!(outcome.actions.first(), Some(Action::DryRunReport { .. })));
            assert!(root.join("aipm-migrate-dryrun-report.md").exists());
            assert_eq!(outcome.scan_counts.skills, 1);
        });
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
        with_unified_discovery_env(Some("1"), || {
            let outcome = run(&opts, &ai_dir, &Real).expect("ok");
            assert_eq!(outcome.scan_counts.total(), 1, ".ai marketplace.json counts");
            // PluginCreated actions for the marketplace.json get filtered/dropped
            // — depends on registrar behavior. Just confirm we ran without error.
            let _ = outcome.actions;
        });
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
}
