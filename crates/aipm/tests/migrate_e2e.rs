//! E2E tests for `aipm migrate`.
//!
//! These tests exercise the compiled binary via `assert_cmd`,
//! using `tempfile` for isolated test directories.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use predicates::prelude::*;

fn aipm() -> assert_cmd::Command {
    Command::cargo_bin("aipm").expect("aipm binary should be built")
}

/// Set up a workspace with marketplace (runs `aipm init`).
fn init_workspace(dir: &std::path::Path) {
    aipm().args(["init", &dir.display().to_string()]).assert().success();
}

/// Create a skill at `.claude/skills/<name>/SKILL.md`.
fn create_skill(dir: &std::path::Path, name: &str, content: &str) {
    let skill_dir = dir.join(".claude").join("skills").join(name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
}

/// Create a command at `.claude/commands/<name>.md`.
fn create_command(dir: &std::path::Path, name: &str, content: &str) {
    let cmd_dir = dir.join(".claude").join("commands");
    std::fs::create_dir_all(&cmd_dir).unwrap();
    std::fs::write(cmd_dir.join(format!("{name}.md")), content).unwrap();
}

// =========================================================================
// Scenario: Migrate a single skill creates a plugin
// =========================================================================
#[test]
fn migrate_skill_creates_plugin() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(
        &dir,
        "deploy",
        "---\nname: deploy\ndescription: Deploy app\n---\nDeploy instructions",
    );

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    assert!(dir.join(".ai/deploy/aipm.toml").exists(), "aipm.toml should exist");
    let toml_content = std::fs::read_to_string(dir.join(".ai/deploy/aipm.toml")).unwrap();
    assert!(toml_content.contains("name = \"deploy\""));
    assert!(toml_content.contains("type = \"skill\""));
}

// =========================================================================
// Scenario: Migrate a command creates a plugin with disable-model-invocation
// =========================================================================
#[test]
fn migrate_command_creates_plugin() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_command(&dir, "review", "Review the code carefully");

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    let skill_md = dir.join(".ai/review/skills/review/SKILL.md");
    assert!(skill_md.exists(), "SKILL.md should exist for converted command");
    let content = std::fs::read_to_string(skill_md).unwrap();
    assert!(content.contains("disable-model-invocation: true"));
}

// =========================================================================
// Scenario: Migrate registers in marketplace.json
// =========================================================================
#[test]
fn migrate_registers_in_marketplace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(&dir, "deploy", "---\nname: deploy\n---\nDeploy");

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    let mp_path = dir.join(".ai/.claude-plugin/marketplace.json");
    let content = std::fs::read_to_string(mp_path).unwrap();
    assert!(content.contains("\"deploy\""), "marketplace.json should contain plugin name");
}

// =========================================================================
// Scenario: Migrated plugins are NOT auto-enabled
// =========================================================================
#[test]
fn migrate_does_not_enable_plugin() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(&dir, "deploy", "---\nname: deploy\n---\nDeploy");

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    let settings_path = dir.join(".claude/settings.json");
    if settings_path.exists() {
        let content = std::fs::read_to_string(settings_path).unwrap();
        assert!(
            !content.contains("deploy@local-repo-plugins"),
            "settings.json should NOT contain enabledPlugins for deploy"
        );
    }
}

// =========================================================================
// Scenario: Original files are preserved after migration
// =========================================================================
#[test]
fn migrate_preserves_originals() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(&dir, "deploy", "---\nname: deploy\n---\nDeploy");

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    assert!(
        dir.join(".claude/skills/deploy/SKILL.md").exists(),
        "original SKILL.md should still exist"
    );
}

// =========================================================================
// Scenario: Name conflict triggers auto-rename
// =========================================================================
#[test]
fn migrate_handles_name_conflict() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);

    // Pre-create a plugin directory with the same name
    std::fs::create_dir_all(dir.join(".ai/deploy")).unwrap();

    create_skill(&dir, "deploy", "---\nname: deploy\n---\nDeploy");

    aipm()
        .args(["migrate", &dir.display().to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("renamed"));

    assert!(dir.join(".ai/deploy-renamed-1").exists(), "renamed plugin directory should exist");
}

// =========================================================================
// Scenario: Dry run creates report but no plugins
// =========================================================================
#[test]
fn migrate_dry_run_creates_report() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(&dir, "deploy", "---\nname: deploy\n---\nDeploy");

    aipm().args(["migrate", "--dry-run", &dir.display().to_string()]).assert().success();

    assert!(dir.join("aipm-migrate-dryrun-report.md").exists(), "dry run report should exist");
    assert!(
        !dir.join(".ai/deploy/aipm.toml").exists(),
        "plugin should NOT be created in dry-run mode"
    );
}

// =========================================================================
// Scenario: Dry run has no side effects on marketplace
// =========================================================================
#[test]
fn migrate_dry_run_no_side_effects() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(&dir, "deploy", "---\nname: deploy\n---\nDeploy");

    let mp_before =
        std::fs::read_to_string(dir.join(".ai/.claude-plugin/marketplace.json")).unwrap();

    aipm().args(["migrate", "--dry-run", &dir.display().to_string()]).assert().success();

    let mp_after =
        std::fs::read_to_string(dir.join(".ai/.claude-plugin/marketplace.json")).unwrap();
    assert_eq!(mp_before, mp_after, "marketplace.json should be unchanged after dry-run");
}

// =========================================================================
// Scenario: Error when .ai/ directory is missing
// =========================================================================
#[test]
fn migrate_no_ai_dir_errors() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    std::fs::create_dir_all(&dir).unwrap();
    // Create .claude dir but no .ai/
    create_skill(&dir, "deploy", "---\nname: deploy\n---\nDeploy");

    aipm()
        .args(["migrate", &dir.display().to_string()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("aipm init"));
}

// =========================================================================
// Scenario: Error when source dir is missing
// =========================================================================
#[test]
fn migrate_no_source_dir_errors() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    // No .claude/ directory at all (init creates .claude/settings.json so remove it)
    let claude_dir = dir.join(".claude");
    if claude_dir.exists() {
        std::fs::remove_dir_all(&claude_dir).unwrap();
    }

    // With explicit --source, missing .claude/ is an error
    aipm()
        .args(["migrate", "--source", ".claude", &dir.display().to_string()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("source directory"));

    // Without --source (recursive mode), no .claude/ dirs is a quiet success
    aipm().args(["migrate", &dir.display().to_string()]).assert().success();
}

// =========================================================================
// Scenario: Empty skills dir succeeds with no plugins
// =========================================================================
#[test]
fn migrate_empty_skills_dir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    std::fs::create_dir_all(dir.join(".claude/skills")).unwrap();

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();
}

// =========================================================================
// Scenario: Multiple skills create multiple plugins
// =========================================================================
#[test]
fn migrate_multiple_skills() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(&dir, "deploy", "---\nname: deploy\n---\nDeploy");
    create_skill(&dir, "lint", "---\nname: lint\n---\nLint");

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    assert!(dir.join(".ai/deploy/aipm.toml").exists());
    assert!(dir.join(".ai/lint/aipm.toml").exists());

    let mp = std::fs::read_to_string(dir.join(".ai/.claude-plugin/marketplace.json")).unwrap();
    assert!(mp.contains("\"deploy\""));
    assert!(mp.contains("\"lint\""));
}

// =========================================================================
// Scenario: Skill with scripts copies scripts
// =========================================================================
#[test]
fn migrate_skill_with_scripts() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);

    let skill_dir = dir.join(".claude/skills/deploy");
    std::fs::create_dir_all(skill_dir.join("scripts")).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: deploy\n---\nRun ${CLAUDE_SKILL_DIR}/scripts/deploy.sh",
    )
    .unwrap();
    std::fs::write(skill_dir.join("scripts/deploy.sh"), "#!/bin/bash\necho deploy").unwrap();

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    assert!(
        dir.join(".ai/deploy/scripts/deploy.sh").exists(),
        "scripts should be copied to plugin"
    );
}

// =========================================================================
// Scenario: Help output shows expected flags
// =========================================================================
#[test]
fn migrate_help_output() {
    aipm()
        .args(["migrate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--source"));
}
