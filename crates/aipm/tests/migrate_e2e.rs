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

/// Create an agent at `.claude/agents/<name>.md`.
fn create_agent(dir: &std::path::Path, name: &str, content: &str) {
    let agents_dir = dir.join(".claude").join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    std::fs::write(agents_dir.join(format!("{name}.md")), content).unwrap();
}

/// Create `.mcp.json` at the project root.
fn create_mcp_json(dir: &std::path::Path, content: &str) {
    std::fs::write(dir.join(".mcp.json"), content).unwrap();
}

/// Create hooks in `.claude/settings.json`.
fn create_hooks_settings(dir: &std::path::Path, content: &str) {
    let claude_dir = dir.join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(claude_dir.join("settings.json"), content).unwrap();
}

/// Create an output style at `.claude/output-styles/<name>.md`.
fn create_output_style(dir: &std::path::Path, name: &str, content: &str) {
    let styles_dir = dir.join(".claude").join("output-styles");
    std::fs::create_dir_all(&styles_dir).unwrap();
    std::fs::write(styles_dir.join(format!("{name}.md")), content).unwrap();
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

    // Plugin directory and components exist, but no aipm.toml (no --manifest)
    assert!(dir.join(".ai/deploy/skills/deploy/SKILL.md").exists(), "SKILL.md should exist");
    assert!(
        !dir.join(".ai/deploy/aipm.toml").exists(),
        "aipm.toml should NOT exist without --manifest"
    );
    assert!(
        dir.join(".ai/deploy/.claude-plugin/plugin.json").exists(),
        "plugin.json should still exist"
    );
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

    // No aipm.toml without --manifest
    assert!(!dir.join(".ai/deploy/aipm.toml").exists());
    assert!(!dir.join(".ai/lint/aipm.toml").exists());

    // But plugins are registered in marketplace.json
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
        .stdout(predicate::str::contains("--source"))
        .stdout(predicate::str::contains("--manifest"));
}

// =========================================================================
// --manifest flag tests
// =========================================================================

#[test]
fn migrate_with_manifest_flag_generates_toml() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(
        &dir,
        "deploy",
        "---\nname: deploy\ndescription: Deploy app\n---\nDeploy instructions",
    );

    aipm().args(["migrate", "--manifest", &dir.display().to_string()]).assert().success();

    assert!(dir.join(".ai/deploy/aipm.toml").exists(), "aipm.toml should exist with --manifest");
    let toml_content = std::fs::read_to_string(dir.join(".ai/deploy/aipm.toml")).unwrap();
    assert!(toml_content.contains("name = \"deploy\""));
    assert!(toml_content.contains("type = \"skill\""));
}

#[test]
fn migrate_without_manifest_flag_skips_toml() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(&dir, "deploy", "---\nname: deploy\n---\nDeploy");

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    assert!(
        !dir.join(".ai/deploy/aipm.toml").exists(),
        "aipm.toml should NOT exist without --manifest"
    );
    // But plugin.json and components should exist
    assert!(dir.join(".ai/deploy/.claude-plugin/plugin.json").exists());
    assert!(dir.join(".ai/deploy/skills/deploy/SKILL.md").exists());
    // And registration still works
    let mp = std::fs::read_to_string(dir.join(".ai/.claude-plugin/marketplace.json")).unwrap();
    assert!(mp.contains("\"deploy\""));
}

// =========================================================================
// Scenario: Migrate agents from .claude/agents/
// =========================================================================
#[test]
fn migrate_agent_creates_plugin() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_agent(
        &dir,
        "security-reviewer",
        "---\nname: security-reviewer\ndescription: Reviews code for security\n---\nYou are a security code reviewer.",
    );

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    assert!(
        dir.join(".ai/security-reviewer/agents/security-reviewer.md").exists(),
        "agent .md should be in agents/ subdirectory"
    );
    assert!(
        dir.join(".ai/security-reviewer/.claude-plugin/plugin.json").exists(),
        "plugin.json should exist"
    );

    let plugin_json =
        std::fs::read_to_string(dir.join(".ai/security-reviewer/.claude-plugin/plugin.json"))
            .unwrap();
    assert!(plugin_json.contains("\"agents\""), "plugin.json should have agents field");
}

// =========================================================================
// Scenario: Migrate MCP servers from .mcp.json
// =========================================================================
#[test]
fn migrate_mcp_creates_plugin() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    // Ensure .claude/ directory exists for the detector scan
    std::fs::create_dir_all(dir.join(".claude")).unwrap();
    create_mcp_json(&dir, r#"{"mcpServers":{"slack":{"command":"npx","args":["slack-mcp"]}}}"#);

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    assert!(
        dir.join(".ai/project-mcp-servers/.mcp.json").exists(),
        ".mcp.json should be copied to plugin"
    );
    let plugin_json =
        std::fs::read_to_string(dir.join(".ai/project-mcp-servers/.claude-plugin/plugin.json"))
            .unwrap();
    assert!(plugin_json.contains("\"mcpServers\""), "plugin.json should have mcpServers field");
}

// =========================================================================
// Scenario: Migrate hooks from .claude/settings.json
// =========================================================================
#[test]
fn migrate_hooks_creates_plugin() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_hooks_settings(
        &dir,
        r#"{"hooks":{"PreToolUse":[{"type":"command","command":"echo check"}]}}"#,
    );

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    assert!(
        dir.join(".ai/project-hooks/hooks/hooks.json").exists(),
        "hooks.json should be in hooks/ subdirectory"
    );
    let plugin_json =
        std::fs::read_to_string(dir.join(".ai/project-hooks/.claude-plugin/plugin.json")).unwrap();
    assert!(plugin_json.contains("\"hooks\""), "plugin.json should have hooks field");
}

// =========================================================================
// Scenario: Migrate output styles from .claude/output-styles/
// =========================================================================
#[test]
fn migrate_output_style_creates_plugin() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_output_style(
        &dir,
        "concise",
        "---\nname: concise\ndescription: Short outputs\n---\nBe concise.",
    );

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    assert!(
        dir.join(".ai/concise/concise.md").exists(),
        "output style .md should be at plugin root"
    );
    let plugin_json =
        std::fs::read_to_string(dir.join(".ai/concise/.claude-plugin/plugin.json")).unwrap();
    assert!(plugin_json.contains("\"outputStyles\""), "plugin.json should have outputStyles field");
}

// =========================================================================
// Scenario: Mixed project with multiple artifact types (root-level)
// =========================================================================
#[test]
fn migrate_mixed_root_creates_separate_plugins() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(&dir, "deploy", "---\nname: deploy\n---\nDeploy");
    create_agent(&dir, "reviewer", "---\nname: reviewer\n---\nYou are a reviewer.");
    create_mcp_json(&dir, r#"{"mcpServers":{"s1":{"command":"test"}}}"#);

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    // Root-level produces separate plugins per artifact
    assert!(dir.join(".ai/deploy").exists(), "deploy plugin should exist");
    assert!(dir.join(".ai/reviewer").exists(), "reviewer plugin should exist");
    assert!(dir.join(".ai/project-mcp-servers").exists(), "MCP plugin should exist");

    let mp = std::fs::read_to_string(dir.join(".ai/.claude-plugin/marketplace.json")).unwrap();
    assert!(mp.contains("\"deploy\""));
    assert!(mp.contains("\"reviewer\""));
    assert!(mp.contains("\"project-mcp-servers\""));
}

// =========================================================================
// Scenario: Dry-run report includes all new artifact types
// =========================================================================
#[test]
fn migrate_dry_run_shows_new_artifact_types() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(&dir, "deploy", "---\nname: deploy\n---\nDeploy");
    create_agent(&dir, "reviewer", "---\nname: reviewer\n---\nReview.");
    create_output_style(&dir, "concise", "---\nname: concise\n---\nBe concise.");
    create_hooks_settings(
        &dir,
        r#"{"hooks":{"PreToolUse":[{"type":"command","command":"echo check"}]}}"#,
    );
    create_mcp_json(&dir, r#"{"mcpServers":{"s1":{"command":"test"}}}"#);

    aipm()
        .args(["migrate", "--dry-run", "--source", ".claude", &dir.display().to_string()])
        .assert()
        .success();

    let report = std::fs::read_to_string(dir.join("aipm-migrate-dryrun-report.md")).unwrap();
    assert!(report.contains("## Skills"), "report should have Skills section");
    assert!(report.contains("## Agents"), "report should have Agents section");
    assert!(report.contains("## MCP Servers"), "report should have MCP Servers section");
    assert!(report.contains("## Hooks"), "report should have Hooks section");
    assert!(report.contains("## Output Styles"), "report should have Output Styles section");
}

// =========================================================================
// Scenario: Manifest flag generates correct manifest for new types
// =========================================================================
#[test]
fn migrate_agent_with_manifest() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_agent(
        &dir,
        "writer",
        "---\nname: writer\ndescription: Writes docs\n---\nYou write documentation.",
    );

    aipm().args(["migrate", "--manifest", &dir.display().to_string()]).assert().success();

    let toml = std::fs::read_to_string(dir.join(".ai/writer/aipm.toml")).unwrap();
    assert!(toml.contains("type = \"agent\""), "manifest type should be agent");
    assert!(toml.contains("agents = [\"agents/writer.md\"]"), "manifest should list agent file");
}

// =========================================================================
// Scenario: Migrate skill with quoted description produces valid JSON
// =========================================================================
#[test]
fn migrate_skill_with_quoted_description_produces_valid_json() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(
        &dir,
        "analyze-bug",
        "---\nname: analyze-bug\ndescription: \"Analyze bugs by reading bug reports.\"\n---\nBody",
    );

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    let plugin_json_path = dir.join(".ai/analyze-bug/.claude-plugin/plugin.json");
    assert!(plugin_json_path.exists(), "plugin.json should exist");
    let content = std::fs::read_to_string(&plugin_json_path).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("plugin.json should be valid JSON");
    assert_eq!(
        parsed.get("description").and_then(serde_json::Value::as_str),
        Some("Analyze bugs by reading bug reports."),
        "description should not have extra quotes"
    );
    assert_eq!(
        parsed.get("name").and_then(serde_json::Value::as_str),
        Some("analyze-bug"),
        "name should match"
    );
}

// =========================================================================
// Scenario: marketplace.json description matches plugin.json description
// =========================================================================
#[test]
fn migrate_marketplace_description_matches_plugin_json() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(
        &dir,
        "deploy",
        "---\nname: deploy\ndescription: Deploy the application\n---\nDeploy instructions",
    );

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    // Read plugin.json
    let plugin_json_path = dir.join(".ai/deploy/.claude-plugin/plugin.json");
    assert!(plugin_json_path.exists(), "plugin.json should exist");
    let plugin_content = std::fs::read_to_string(&plugin_json_path).unwrap();
    let plugin_parsed: serde_json::Value =
        serde_json::from_str(&plugin_content).expect("plugin.json should be valid JSON");
    let plugin_desc = plugin_parsed.get("description").and_then(serde_json::Value::as_str);

    // Read marketplace.json
    let marketplace_path = dir.join(".ai/.claude-plugin/marketplace.json");
    assert!(marketplace_path.exists(), "marketplace.json should exist");
    let marketplace_content = std::fs::read_to_string(&marketplace_path).unwrap();
    let marketplace_parsed: serde_json::Value =
        serde_json::from_str(&marketplace_content).expect("marketplace.json should be valid JSON");
    let deploy_entry = marketplace_parsed
        .get("plugins")
        .and_then(|v| v.as_array())
        .and_then(|a| a.iter().find(|p| p.get("name").and_then(|n| n.as_str()) == Some("deploy")));
    let marketplace_desc =
        deploy_entry.and_then(|p| p.get("description")).and_then(serde_json::Value::as_str);

    assert_eq!(plugin_desc, Some("Deploy the application"));
    assert_eq!(
        marketplace_desc, plugin_desc,
        "marketplace.json description should match plugin.json description"
    );
}

// =========================================================================
// --destructive flag tests
// =========================================================================

#[test]
fn destructive_flag_removes_skill_source() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init_workspace(dir);
    create_skill(dir, "deploy", "---\nname: deploy\n---\nDeploy instructions");

    aipm()
        .args(["migrate", "--destructive", &dir.display().to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed"));

    // Plugin should exist
    assert!(dir.join(".ai/deploy/skills/deploy/SKILL.md").exists());
    // Source should be gone
    assert!(!dir.join(".claude/skills/deploy/SKILL.md").exists());
}

#[test]
fn destructive_flag_preserves_settings_json() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init_workspace(dir);
    create_hooks_settings(
        dir,
        r#"{"hooks":{"PreToolUse":[{"type":"command","command":"echo test"}]}}"#,
    );

    aipm().args(["migrate", "--destructive", &dir.display().to_string()]).assert().success();

    // settings.json should still exist (shared config, not removed)
    assert!(dir.join(".claude/settings.json").exists());
}

#[test]
fn without_destructive_preserves_sources_in_non_tty() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init_workspace(dir);
    create_skill(dir, "deploy", "---\nname: deploy\n---\nDeploy instructions");

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    // Source should still exist (no --destructive and non-TTY -> skip cleanup)
    assert!(dir.join(".claude/skills/deploy/SKILL.md").exists());
    // Plugin should also exist
    assert!(dir.join(".ai/deploy/skills/deploy/SKILL.md").exists());
}

#[test]
fn destructive_with_dry_run_shows_plan() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init_workspace(dir);
    create_skill(dir, "deploy", "---\nname: deploy\n---\nDeploy instructions");

    aipm()
        .args(["migrate", "--dry-run", "--destructive", &dir.display().to_string()])
        .assert()
        .success();

    // Report should contain cleanup plan
    let report = std::fs::read_to_string(dir.join("aipm-migrate-dryrun-report.md")).unwrap();
    assert!(report.contains("Cleanup Plan"), "dry-run report should contain cleanup plan");
    // Source path is absolute in the report, so check for the suffix
    assert!(
        report.contains(".claude") && report.contains("skills") && report.contains("deploy"),
        "report should list skill source path"
    );

    // Source should still exist (dry-run doesn't delete)
    assert!(dir.join(".claude/skills/deploy/SKILL.md").exists());
}

#[test]
fn destructive_recursive_cleans_all() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init_workspace(dir);
    create_skill(dir, "lint", "---\nname: lint\n---\nLint instructions");

    // Create sub-package skill
    let sub_skill_dir = dir.join("packages/auth/.claude/skills/deploy");
    std::fs::create_dir_all(&sub_skill_dir).unwrap();
    std::fs::write(sub_skill_dir.join("SKILL.md"), "---\nname: deploy\n---\nDeploy instructions")
        .unwrap();

    aipm().args(["migrate", "--destructive", &dir.display().to_string()]).assert().success();

    // Root skill source should be gone
    assert!(!dir.join(".claude/skills/lint/SKILL.md").exists());
    // Sub-package skill source should be gone
    assert!(!dir.join("packages/auth/.claude/skills/deploy/SKILL.md").exists());
}

#[test]
fn destructive_prunes_empty_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init_workspace(dir);
    create_skill(dir, "deploy", "---\nname: deploy\n---\nDeploy instructions");

    aipm().args(["migrate", "--destructive", &dir.display().to_string()]).assert().success();

    // The skills/ parent directory should be gone (was empty after deploy removed)
    assert!(!dir.join(".claude/skills").exists());
}

#[test]
fn destructive_skips_mcp_json() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init_workspace(dir);
    create_mcp_json(dir, r#"{"mcpServers":{"test":{"command":"echo","args":["hello"]}}}"#);

    aipm().args(["migrate", "--destructive", &dir.display().to_string()]).assert().success();

    // .mcp.json should still exist (shared config)
    assert!(dir.join(".mcp.json").exists());
}

// =========================================================================
// Scenario: Migrate with other files present
// =========================================================================
#[test]
fn migrate_with_other_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(&dir, "deploy", "---\nname: deploy\n---\nDeploy instructions");

    // Create extra unclaimed files in .claude/
    let claude_dir = dir.join(".claude");
    std::fs::write(claude_dir.join("README.md"), "# Project notes").unwrap();
    std::fs::create_dir_all(claude_dir.join("utils")).unwrap();
    std::fs::write(claude_dir.join("utils/helper.sh"), "#!/bin/bash\necho help").unwrap();

    aipm()
        .args(["migrate", &dir.display().to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("other file").or(predicate::str::contains("Migrated")));

    // Other files should be copied into the first created plugin directory
    let deploy_plugin_dir = dir.join(".ai/deploy");
    assert!(
        deploy_plugin_dir.join("README.md").exists(),
        "README.md should be copied to plugin directory"
    );
    assert!(
        deploy_plugin_dir.join("utils/helper.sh").exists()
            || deploy_plugin_dir.join("helper.sh").exists(),
        "helper.sh should be copied to plugin directory"
    );
}

// =========================================================================
// Scenario: Dry-run with other files shows Other Files section
// =========================================================================
#[test]
fn migrate_dry_run_with_other_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);
    create_skill(&dir, "deploy", "---\nname: deploy\n---\nDeploy instructions");

    // Create an extra unclaimed file
    std::fs::write(dir.join(".claude/README.md"), "# Notes").unwrap();

    aipm()
        .args(["migrate", "--dry-run", "--source", ".claude", &dir.display().to_string()])
        .assert()
        .success();

    let report = std::fs::read_to_string(dir.join("aipm-migrate-dryrun-report.md")).unwrap();
    assert!(report.contains("Other Files"), "dry-run report should contain Other Files section");
    assert!(report.contains("README.md"), "dry-run report should mention the unclaimed file");
}

// =========================================================================
// Scenario: Migrate with dependency script referenced by skill
// =========================================================================
#[test]
fn migrate_dependency_script_with_relative_ref() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("project");
    init_workspace(&dir);

    let skill_dir = dir.join(".claude/skills/deploy");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: deploy\n---\nRun ${CLAUDE_SKILL_DIR}/scripts/deploy.sh to deploy",
    )
    .unwrap();
    std::fs::create_dir_all(skill_dir.join("scripts")).unwrap();
    std::fs::write(skill_dir.join("scripts/deploy.sh"), "#!/bin/bash\necho deploy").unwrap();

    aipm().args(["migrate", &dir.display().to_string()]).assert().success();

    assert!(
        dir.join(".ai/deploy/scripts/deploy.sh").exists(),
        "dependency script should be migrated alongside skill"
    );
}
