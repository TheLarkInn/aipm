//! Dry-run report generator for the migrate command.

use std::collections::HashSet;
use std::fmt::Write;
use std::hash::BuildHasher;

use super::{Artifact, ArtifactKind};

/// Generate a dry-run report as markdown.
pub fn generate_report<S: BuildHasher>(
    artifacts: &[Artifact],
    existing_plugins: &HashSet<String, S>,
    source_name: &str,
) -> String {
    let mut report = String::new();

    // Header
    let _ = writeln!(report, "# aipm migrate — Dry Run Report\n");
    let _ = writeln!(report, "**Source:** {source_name}/");
    let _ = writeln!(report, "**Artifacts found:** {}\n", artifacts.len());

    // Group by kind
    let skills: Vec<_> = artifacts.iter().filter(|a| a.kind == ArtifactKind::Skill).collect();
    let commands: Vec<_> = artifacts.iter().filter(|a| a.kind == ArtifactKind::Command).collect();

    let mut rename_counter = 0u32;
    let mut used_names: HashSet<String> = existing_plugins.iter().cloned().collect();
    let mut total_conflicts = 0u32;
    let mut total_hooks = 0u32;

    if !skills.is_empty() {
        let _ = writeln!(report, "## Skills\n");
        for artifact in &skills {
            write_artifact_section(
                &mut report,
                artifact,
                &mut used_names,
                &mut rename_counter,
                &mut total_conflicts,
                &mut total_hooks,
            );
        }
    }

    if !commands.is_empty() {
        let _ = writeln!(report, "## Legacy Commands\n");
        for artifact in &commands {
            write_artifact_section(
                &mut report,
                artifact,
                &mut used_names,
                &mut rename_counter,
                &mut total_conflicts,
                &mut total_hooks,
            );
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

/// Write a section for a single artifact in the dry-run report.
fn write_artifact_section(
    report: &mut String,
    artifact: &Artifact,
    used_names: &mut HashSet<String>,
    rename_counter: &mut u32,
    total_conflicts: &mut u32,
    total_hooks: &mut u32,
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
    let _ =
        writeln!(report, "  - New aipm.toml with type = \"{}\"", artifact.kind.to_type_string());
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
        let report = generate_report(&artifacts, &existing, ".claude");

        assert!(report.contains("### deploy"));
        assert!(report.contains("### lint"));
    }

    #[test]
    fn dry_run_report_shows_conflict_renames() {
        let artifacts = vec![make_artifact("deploy", ArtifactKind::Skill)];
        let mut existing = HashSet::new();
        existing.insert("deploy".to_string());
        let report = generate_report(&artifacts, &existing, ".claude");

        assert!(report.contains("deploy-renamed-1"));
        assert!(report.contains("Name conflicts (auto-renamed) | 1"));
    }

    #[test]
    fn dry_run_report_shows_file_list() {
        let mut artifact = make_artifact("deploy", ArtifactKind::Skill);
        artifact.files = vec![PathBuf::from("SKILL.md"), PathBuf::from("scripts/run.sh")];
        let artifacts = vec![artifact];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude");

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
        let report = generate_report(&artifacts, &existing, ".claude");

        assert!(report.contains("## Summary"));
        assert!(report.contains("Plugins to create | 2"));
        assert!(report.contains("Marketplace entries to add | 2"));
    }

    #[test]
    fn dry_run_report_empty_artifacts() {
        let artifacts: Vec<Artifact> = Vec::new();
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude");

        assert!(report.contains("**Artifacts found:** 0"));
        assert!(report.contains("Plugins to create | 0"));
    }

    #[test]
    fn dry_run_report_with_hooks() {
        let mut artifact = make_artifact("deploy", ArtifactKind::Skill);
        artifact.metadata.hooks = Some("PreToolUse: check".to_string());
        let artifacts = vec![artifact];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude");

        assert!(report.contains("**Hooks extracted:** yes"));
        assert!(report.contains("Hooks to extract | 1"));
    }

    #[test]
    fn dry_run_report_with_script_references() {
        let mut artifact = make_artifact("deploy", ArtifactKind::Skill);
        artifact.referenced_scripts = vec![PathBuf::from("scripts/run.sh")];
        let artifacts = vec![artifact];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude");

        assert!(report.contains("**Path rewrites:**"));
    }

    #[test]
    fn dry_run_report_commands_section() {
        let artifacts = vec![make_artifact("review", ArtifactKind::Command)];
        let existing = HashSet::new();
        let report = generate_report(&artifacts, &existing, ".claude");

        assert!(report.contains("## Legacy Commands"));
    }
}
