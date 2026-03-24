//! Dry-run report generator for the migrate command.

use std::collections::HashSet;
use std::fmt::Write;
use std::hash::BuildHasher;

use super::discovery::DiscoveredSource;
use super::{Artifact, ArtifactKind, PluginPlan};

/// Generate a dry-run report as markdown.
pub fn generate_report<S: BuildHasher>(
    artifacts: &[Artifact],
    existing_plugins: &HashSet<String, S>,
    source_name: &str,
    manifest: bool,
) -> String {
    let mut report = String::new();

    // Header
    let _ = writeln!(report, "# aipm migrate — Dry Run Report\n");
    let _ = writeln!(report, "**Source:** {source_name}/");
    let _ = writeln!(report, "**Artifacts found:** {}\n", artifacts.len());

    // Group by kind
    let skills: Vec<_> = artifacts.iter().filter(|a| a.kind == ArtifactKind::Skill).collect();
    let commands: Vec<_> = artifacts.iter().filter(|a| a.kind == ArtifactKind::Command).collect();
    let agents: Vec<_> = artifacts.iter().filter(|a| a.kind == ArtifactKind::Agent).collect();
    let mcp: Vec<_> = artifacts.iter().filter(|a| a.kind == ArtifactKind::McpServer).collect();
    let hooks: Vec<_> = artifacts.iter().filter(|a| a.kind == ArtifactKind::Hook).collect();
    let output_styles: Vec<_> =
        artifacts.iter().filter(|a| a.kind == ArtifactKind::OutputStyle).collect();

    let mut rename_counter = 0u32;
    let mut used_names: HashSet<String> = existing_plugins.iter().cloned().collect();
    let mut total_conflicts = 0u32;
    let mut total_hooks = 0u32;

    let sections: &[(&str, &[&Artifact])] = &[
        ("Skills", &skills),
        ("Legacy Commands", &commands),
        ("Agents", &agents),
        ("MCP Servers", &mcp),
        ("Hooks", &hooks),
        ("Output Styles", &output_styles),
    ];

    for (title, items) in sections {
        if !items.is_empty() {
            let _ = writeln!(report, "## {title}\n");
            for artifact in *items {
                write_artifact_section(
                    &mut report,
                    artifact,
                    &mut used_names,
                    &mut rename_counter,
                    &mut total_conflicts,
                    &mut total_hooks,
                    manifest,
                );
            }
        }
    }

    // Summary table
    let _ = writeln!(report, "## Summary\n");
    let _ = writeln!(report, "| Action | Count |");
    let _ = writeln!(report, "|--------|-------|");
    let _ = writeln!(report, "| Plugins to create | {} |", artifacts.len());
    let _ = writeln!(report, "| Marketplace entries to add | {} |", artifacts.len());
    let _ = writeln!(report, "| Name conflicts (auto-renamed) | {total_conflicts} |");
    let _ = writeln!(report, "| Hooks to extract | {total_hooks} |");

    report
}

/// Generate a dry-run report for recursive discovery mode.
pub fn generate_recursive_report<S: BuildHasher>(
    discovered: &[DiscoveredSource],
    plugin_plans: &[PluginPlan],
    existing_plugins: &HashSet<String, S>,
) -> String {
    let mut report = String::new();

    let _ = writeln!(report, "# aipm migrate — Dry Run Report\n");
    let _ = writeln!(report, "**Mode:** Recursive discovery");
    let _ = writeln!(report, "**Discovered {} `.claude/` directories:**\n", discovered.len());

    // Discovery table
    let _ = writeln!(
        report,
        "| Location | Package Name | Skills | Commands | Agents | MCP | Hooks | Styles |"
    );
    let _ = writeln!(
        report,
        "|----------|-------------|--------|----------|--------|-----|-------|--------|"
    );

    for src in discovered {
        let location = if src.relative_path.as_os_str().is_empty() {
            "./.claude".to_string()
        } else {
            format!("./{}/{}", src.relative_path.display(), ".claude")
        };
        let pkg_name = src.package_name.as_deref().unwrap_or("(root)");

        // Count artifacts by kind from plans matching this specific source dir
        let (skills, commands, agents, mcp, hooks, styles) = plugin_plans
            .iter()
            .filter(|p| p.source_dir == src.claude_dir)
            .flat_map(|p| &p.artifacts)
            .fold((0u32, 0u32, 0u32, 0u32, 0u32, 0u32), |(s, c, ag, m, h, st), a| match a.kind {
                ArtifactKind::Skill => (s + 1, c, ag, m, h, st),
                ArtifactKind::Command => (s, c + 1, ag, m, h, st),
                ArtifactKind::Agent => (s, c, ag + 1, m, h, st),
                ArtifactKind::McpServer => (s, c, ag, m + 1, h, st),
                ArtifactKind::Hook => (s, c, ag, m, h + 1, st),
                ArtifactKind::OutputStyle => (s, c, ag, m, h, st + 1),
            });

        let _ = writeln!(
            report,
            "| {location} | {pkg_name} | {skills} | {commands} | {agents} | {mcp} | {hooks} | {styles} |"
        );
    }

    let _ = writeln!(report);

    // Planned plugins
    let _ = writeln!(report, "## Planned Plugins\n");

    let mut rename_counter = 0u32;
    let mut used_names: HashSet<String> = existing_plugins.iter().cloned().collect();
    let mut conflicts = Vec::new();

    for plan in plugin_plans {
        // Re-check for collisions in a loop (the generated name itself could collide)
        let final_name = if used_names.contains(&plan.name) {
            let mut new_name;
            loop {
                rename_counter += 1;
                new_name = format!("{}-renamed-{rename_counter}", plan.name);
                if !used_names.contains(&new_name) {
                    break;
                }
            }
            conflicts.push((plan.name.clone(), new_name.clone()));
            new_name
        } else {
            plan.name.clone()
        };
        used_names.insert(final_name.clone());

        // Composite when 2+ distinct artifact kinds are present
        let distinct_kinds: HashSet<&ArtifactKind> =
            plan.artifacts.iter().map(|a| &a.kind).collect();
        let type_str = if distinct_kinds.len() > 1 {
            "composite"
        } else {
            plan.artifacts.first().map_or("composite", |a| a.kind.to_type_string())
        };

        let source_label = if plan.is_package_scoped {
            format!("from {}", plan.name)
        } else {
            "from root .claude".to_string()
        };

        let _ = writeln!(report, "### Plugin: `{final_name}` ({source_label})");
        let _ = writeln!(report, "- Type: {type_str}");
        if plan.artifacts.len() == 1 {
            if let Some(a) = plan.artifacts.first() {
                let _ = writeln!(report, "- Components: {}", component_path(a));
            }
        } else {
            let _ = writeln!(report, "- Components:");
            for a in &plan.artifacts {
                let suffix =
                    if a.kind == ArtifactKind::Command { " (converted from command)" } else { "" };
                let _ = writeln!(report, "  - {}{suffix}", component_path(a));
            }
        }
        let _ = writeln!(report);
    }

    // Name conflicts section
    let _ = writeln!(report, "## Name Conflicts");
    if conflicts.is_empty() {
        let _ = writeln!(report, "(none)");
    } else {
        for (original, renamed) in &conflicts {
            let _ = writeln!(report, "- `{original}` → `{renamed}`");
        }
    }

    report
}

/// Returns the component path for display in the dry-run report.
fn component_path(artifact: &Artifact) -> String {
    match artifact.kind {
        ArtifactKind::Skill | ArtifactKind::Command => {
            format!("skills/{}/SKILL.md", artifact.name)
        },
        ArtifactKind::Agent => format!("agents/{}.md", artifact.name),
        ArtifactKind::McpServer => ".mcp.json".to_string(),
        ArtifactKind::Hook => "hooks/hooks.json".to_string(),
        ArtifactKind::OutputStyle => format!("{}.md", artifact.name),
    }
}

/// Write a section for a single artifact in the dry-run report.
fn write_artifact_section(
    report: &mut String,
    artifact: &Artifact,
    used_names: &mut HashSet<String>,
    rename_counter: &mut u32,
    total_conflicts: &mut u32,
    total_hooks: &mut u32,
    manifest: bool,
) {
    let _ = writeln!(report, "### {}\n", artifact.name);
    let _ = writeln!(report, "- **Source:** {}", artifact.source_path.display());

    // Resolve name for display
    let (target_name, conflict) = if used_names.contains(&artifact.name) {
        *rename_counter += 1;
        *total_conflicts += 1;
        let new_name = format!("{}-renamed-{rename_counter}", artifact.name);
        (new_name.clone(), Some(new_name))
    } else {
        (artifact.name.clone(), None)
    };
    used_names.insert(target_name.clone());

    let _ = writeln!(report, "- **Target:** .ai/{target_name}/");

    // Files to copy
    if !artifact.files.is_empty() {
        let _ = writeln!(report, "- **Files to copy:**");
        for file in &artifact.files {
            let _ = writeln!(report, "  - {}", file.display());
        }
    }

    // Manifest changes
    let _ = writeln!(report, "- **Manifest changes:**");
    if manifest {
        let _ = writeln!(
            report,
            "  - New aipm.toml with type = \"{}\"",
            artifact.kind.to_type_string()
        );
    } else {
        let _ = writeln!(report, "  - No aipm.toml (pass --manifest to generate)");
    }
    let _ = writeln!(report, "  - New .claude-plugin/plugin.json");

    // Marketplace entry
    let _ = writeln!(report, "- **marketplace.json:** append entry \"{target_name}\"");

    // Path rewrites
    if !artifact.referenced_scripts.is_empty() {
        let _ = writeln!(
            report,
            "- **Path rewrites:** ${{CLAUDE_SKILL_DIR}}/scripts/ → ${{CLAUDE_SKILL_DIR}}/../../scripts/"
        );
    }

    // Hooks
    let has_hooks = artifact.metadata.hooks.is_some();
    if has_hooks {
        *total_hooks += 1;
    }
    let _ = writeln!(report, "- **Hooks extracted:** {}", if has_hooks { "yes" } else { "no" });

    // Conflict
    if let Some(new_name) = conflict {
        let _ = writeln!(report, "- **Conflict:** renamed to {new_name}");
    } else {
        let _ = writeln!(report, "- **Conflict:** none");
    }

    let _ = writeln!(report);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::migrate::ArtifactMetadata;

    fn make_artifact(name: &str, kind: ArtifactKind) -> Artifact {
        Artifact {
            kind,
            name: name.to_string(),
            source_path: PathBuf::from(format!(".claude/skills/{name}/")),
            files: vec![PathBuf::from("SKILL.md")],
            referenced_scripts: Vec::new(),
            metadata: ArtifactMetadata::default(),
        }
    }

    #[test]
    fn dry_run_report_lists_all_artifacts() {
        let artifacts = vec![
            make_artifact("deploy", ArtifactKind::Skill),
            make_artifact("lint", ArtifactKind::Skill),
        ];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", true);

        assert!(report.contains("### deploy"));
        assert!(report.contains("### lint"));
    }

    #[test]
    fn dry_run_report_shows_conflict_renames() {
        let artifacts = vec![make_artifact("deploy", ArtifactKind::Skill)];
        let mut existing = HashSet::new();
        existing.insert("deploy".to_string());
        let report = generate_report(&artifacts, &existing, ".claude", true);

        assert!(report.contains("deploy-renamed-1"));
        assert!(report.contains("Name conflicts (auto-renamed) | 1"));
    }

    #[test]
    fn dry_run_report_shows_file_list() {
        let mut artifact = make_artifact("deploy", ArtifactKind::Skill);
        artifact.files = vec![PathBuf::from("SKILL.md"), PathBuf::from("scripts/run.sh")];
        let artifacts = vec![artifact];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", true);

        assert!(report.contains("SKILL.md"));
        assert!(report.contains("scripts/run.sh"));
    }

    #[test]
    fn dry_run_report_summary_table() {
        let artifacts = vec![
            make_artifact("deploy", ArtifactKind::Skill),
            make_artifact("review", ArtifactKind::Command),
        ];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", true);

        assert!(report.contains("## Summary"));
        assert!(report.contains("Plugins to create | 2"));
        assert!(report.contains("Marketplace entries to add | 2"));
    }

    #[test]
    fn dry_run_report_empty_artifacts() {
        let artifacts: Vec<Artifact> = Vec::new();
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", true);

        assert!(report.contains("**Artifacts found:** 0"));
        assert!(report.contains("Plugins to create | 0"));
    }

    #[test]
    fn dry_run_report_with_hooks() {
        let mut artifact = make_artifact("deploy", ArtifactKind::Skill);
        artifact.metadata.hooks = Some("PreToolUse: check".to_string());
        let artifacts = vec![artifact];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", true);

        assert!(report.contains("**Hooks extracted:** yes"));
        assert!(report.contains("Hooks to extract | 1"));
    }

    #[test]
    fn dry_run_report_with_script_references() {
        let mut artifact = make_artifact("deploy", ArtifactKind::Skill);
        artifact.referenced_scripts = vec![PathBuf::from("scripts/run.sh")];
        let artifacts = vec![artifact];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", true);

        assert!(report.contains("**Path rewrites:**"));
    }

    #[test]
    fn dry_run_report_commands_section() {
        let artifacts = vec![make_artifact("review", ArtifactKind::Command)];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", true);

        assert!(report.contains("## Legacy Commands"));
    }

    #[test]
    fn recursive_report_shows_discovery_table() {
        let discovered = vec![
            DiscoveredSource {
                claude_dir: PathBuf::from("/project/.claude"),
                package_name: None,
                relative_path: PathBuf::new(),
            },
            DiscoveredSource {
                claude_dir: PathBuf::from("/project/packages/auth/.claude"),
                package_name: Some("auth".to_string()),
                relative_path: PathBuf::from("packages/auth"),
            },
        ];

        let plugin_plans = vec![
            PluginPlan {
                name: "deploy".to_string(),
                artifacts: vec![make_artifact("deploy", ArtifactKind::Skill)],
                is_package_scoped: false,
                source_dir: PathBuf::from("/project/.claude"),
            },
            PluginPlan {
                name: "auth".to_string(),
                artifacts: vec![
                    make_artifact("lint", ArtifactKind::Skill),
                    make_artifact("review", ArtifactKind::Command),
                ],
                is_package_scoped: true,
                source_dir: PathBuf::from("/project/packages/auth/.claude"),
            },
        ];

        let existing = HashSet::new();
        let report = generate_recursive_report(&discovered, &plugin_plans, &existing);

        assert!(report.contains("Recursive discovery"));
        assert!(report.contains("(root)"));
        assert!(report.contains("auth"));
        assert!(report.contains("Plugin: `deploy`"));
        assert!(report.contains("Plugin: `auth`"));
        assert!(report.contains("composite"));
        assert!(report.contains("(none)"));
    }

    #[test]
    fn recursive_report_shows_name_conflicts() {
        let discovered = vec![DiscoveredSource {
            claude_dir: PathBuf::from("/project/packages/auth/.claude"),
            package_name: Some("auth".to_string()),
            relative_path: PathBuf::from("packages/auth"),
        }];

        let plugin_plans = vec![PluginPlan {
            name: "auth".to_string(),
            artifacts: vec![make_artifact("deploy", ArtifactKind::Skill)],
            is_package_scoped: true,
            source_dir: PathBuf::from("/project/packages/auth/.claude"),
        }];

        let mut existing = HashSet::new();
        existing.insert("auth".to_string());
        let report = generate_recursive_report(&discovered, &plugin_plans, &existing);

        assert!(report.contains("auth-renamed-1"));
    }

    #[test]
    fn recursive_report_empty_discovery() {
        let discovered: Vec<DiscoveredSource> = Vec::new();
        let plugin_plans: Vec<PluginPlan> = Vec::new();
        let existing = HashSet::new();
        let report = generate_recursive_report(&discovered, &plugin_plans, &existing);

        assert!(report.contains("Discovered 0"));
        assert!(report.contains("(none)"));
    }

    #[test]
    fn recursive_report_single_artifact_plan() {
        let discovered = vec![DiscoveredSource {
            claude_dir: PathBuf::from("/project/packages/api/.claude"),
            package_name: Some("api".to_string()),
            relative_path: PathBuf::from("packages/api"),
        }];

        let plugin_plans = vec![PluginPlan {
            name: "api".to_string(),
            artifacts: vec![make_artifact("deploy", ArtifactKind::Skill)],
            is_package_scoped: true,
            source_dir: PathBuf::from("/project/packages/api/.claude"),
        }];

        let existing = HashSet::new();
        let report = generate_recursive_report(&discovered, &plugin_plans, &existing);

        assert!(report.contains("Plugin: `api`"));
        assert!(report.contains("Type: skill"));
    }

    #[test]
    fn dry_run_report_no_manifest_shows_hint() {
        let artifacts = vec![make_artifact("deploy", ArtifactKind::Skill)];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", false);

        assert!(report.contains("No aipm.toml (pass --manifest to generate)"));
        assert!(!report.contains("New aipm.toml with type"));
    }

    #[test]
    fn dry_run_report_with_manifest_shows_aipm_toml() {
        let artifacts = vec![make_artifact("deploy", ArtifactKind::Skill)];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", true);

        assert!(report.contains("New aipm.toml with type"));
        assert!(!report.contains("No aipm.toml (pass --manifest to generate)"));
    }

    #[test]
    fn dry_run_report_agents_section() {
        let artifacts = vec![make_artifact("reviewer", ArtifactKind::Agent)];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", true);
        assert!(report.contains("## Agents"));
        assert!(report.contains("### reviewer"));
    }

    #[test]
    fn dry_run_report_mcp_section() {
        let artifacts = vec![make_artifact("project-mcp-servers", ArtifactKind::McpServer)];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", true);
        assert!(report.contains("## MCP Servers"));
        assert!(report.contains("### project-mcp-servers"));
    }

    #[test]
    fn dry_run_report_hooks_section() {
        let artifacts = vec![make_artifact("project-hooks", ArtifactKind::Hook)];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", true);
        assert!(report.contains("## Hooks"));
        assert!(report.contains("### project-hooks"));
    }

    #[test]
    fn dry_run_report_output_styles_section() {
        let artifacts = vec![make_artifact("concise", ArtifactKind::OutputStyle)];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", true);
        assert!(report.contains("## Output Styles"));
        assert!(report.contains("### concise"));
    }

    #[test]
    fn dry_run_report_all_types() {
        let artifacts = vec![
            make_artifact("deploy", ArtifactKind::Skill),
            make_artifact("review", ArtifactKind::Command),
            make_artifact("reviewer", ArtifactKind::Agent),
            make_artifact("mcp", ArtifactKind::McpServer),
            make_artifact("hooks", ArtifactKind::Hook),
            make_artifact("concise", ArtifactKind::OutputStyle),
        ];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude", true);
        assert!(report.contains("**Artifacts found:** 6"));
        assert!(report.contains("## Skills"));
        assert!(report.contains("## Legacy Commands"));
        assert!(report.contains("## Agents"));
        assert!(report.contains("## MCP Servers"));
        assert!(report.contains("## Hooks"));
        assert!(report.contains("## Output Styles"));
    }

    #[test]
    fn recursive_report_new_type_counts() {
        let discovered = vec![DiscoveredSource {
            claude_dir: PathBuf::from("/project/.claude"),
            package_name: None,
            relative_path: PathBuf::new(),
        }];

        let plugin_plans = vec![
            PluginPlan {
                name: "reviewer".to_string(),
                artifacts: vec![make_artifact("reviewer", ArtifactKind::Agent)],
                is_package_scoped: false,
                source_dir: PathBuf::from("/project/.claude"),
            },
            PluginPlan {
                name: "mcp".to_string(),
                artifacts: vec![make_artifact("mcp", ArtifactKind::McpServer)],
                is_package_scoped: false,
                source_dir: PathBuf::from("/project/.claude"),
            },
        ];

        let existing = HashSet::new();
        let report = generate_recursive_report(&discovered, &plugin_plans, &existing);

        // Table should show agent and MCP counts
        assert!(report.contains("Plugin: `reviewer`"));
        assert!(report.contains("Plugin: `mcp`"));
        assert!(report.contains("Type: agent"));
        assert!(report.contains("Type: mcp"));
    }

    #[test]
    fn recursive_report_composite_with_new_types() {
        let discovered = vec![DiscoveredSource {
            claude_dir: PathBuf::from("/project/packages/auth/.claude"),
            package_name: Some("auth".to_string()),
            relative_path: PathBuf::from("packages/auth"),
        }];

        let plugin_plans = vec![PluginPlan {
            name: "auth".to_string(),
            artifacts: vec![
                make_artifact("deploy", ArtifactKind::Skill),
                make_artifact("reviewer", ArtifactKind::Agent),
            ],
            is_package_scoped: true,
            source_dir: PathBuf::from("/project/packages/auth/.claude"),
        }];

        let existing = HashSet::new();
        let report = generate_recursive_report(&discovered, &plugin_plans, &existing);

        assert!(report.contains("Type: composite"));
        assert!(report.contains("agents/reviewer.md"));
    }

    #[test]
    fn component_path_all_kinds() {
        let skill = make_artifact("deploy", ArtifactKind::Skill);
        assert_eq!(component_path(&skill), "skills/deploy/SKILL.md");

        let cmd = make_artifact("review", ArtifactKind::Command);
        assert_eq!(component_path(&cmd), "skills/review/SKILL.md");

        let agent = make_artifact("reviewer", ArtifactKind::Agent);
        assert_eq!(component_path(&agent), "agents/reviewer.md");

        let mcp = make_artifact("mcp", ArtifactKind::McpServer);
        assert_eq!(component_path(&mcp), ".mcp.json");

        let hook = make_artifact("hooks", ArtifactKind::Hook);
        assert_eq!(component_path(&hook), "hooks/hooks.json");

        let style = make_artifact("concise", ArtifactKind::OutputStyle);
        assert_eq!(component_path(&style), "concise.md");
    }
}
