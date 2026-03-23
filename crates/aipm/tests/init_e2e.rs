//! E2E tests for `aipm init` — maps directly to BDD scenarios in
//! `tests/features/manifest/workspace-init.feature`.
//!
//! These tests exercise the actual compiled binary via `assert_cmd`,
//! using `tempfile` for isolated test directories.

// Integration test crates inherit workspace lints. Relax restrictions that are
// appropriate for test code (unwrap, expect, panic are normal in tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use predicates::prelude::*;

fn aipm() -> assert_cmd::Command {
    Command::cargo_bin("aipm").expect("aipm binary should be built")
}

// =========================================================================
// Scenario: Default init with no flags creates marketplace only
// =========================================================================
#[test]
fn init_default_creates_marketplace_only() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-project");

    aipm().args(["init", &dir.display().to_string()]).assert().success();

    assert!(!dir.join("aipm.toml").exists(), "aipm.toml should NOT exist");
    assert!(
        dir.join(".ai/starter-aipm-plugin/aipm.toml").exists(),
        ".ai/starter-aipm-plugin/aipm.toml should exist"
    );
    assert!(dir.join(".claude/settings.json").exists(), ".claude/settings.json should exist");
}

// =========================================================================
// Scenario: --workspace only
// =========================================================================
#[test]
fn init_workspace_only() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("ws-only");

    aipm().args(["init", "--workspace", &dir.display().to_string()]).assert().success();

    assert!(dir.join("aipm.toml").exists(), "aipm.toml should exist");
    assert!(!dir.join(".ai").exists(), ".ai/ should NOT exist");
}

// =========================================================================
// Scenario: --marketplace only
// =========================================================================
#[test]
fn init_marketplace_only() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("mp-only");
    std::fs::create_dir_all(&dir).ok();

    aipm().args(["init", "--marketplace", &dir.display().to_string()]).assert().success();

    assert!(
        dir.join(".ai/starter-aipm-plugin/aipm.toml").exists(),
        ".ai/starter-aipm-plugin should exist"
    );
    assert!(!dir.join("aipm.toml").exists(), "aipm.toml should NOT exist");
}

// =========================================================================
// Scenario: Reject if aipm.toml already exists
// =========================================================================
#[test]
fn init_rejects_existing_workspace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("existing-ws");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join("aipm.toml"), "[package]\n").ok();

    aipm()
        .args(["init", "--workspace", &dir.display().to_string()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already initialized"));
}

// =========================================================================
// Scenario: Reject if .ai/ already exists
// =========================================================================
#[test]
fn init_rejects_existing_marketplace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("existing-mp");
    std::fs::create_dir_all(dir.join(".ai")).ok();

    aipm()
        .args(["init", "--marketplace", &dir.display().to_string()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

// =========================================================================
// Scenario: Help shows usage
// =========================================================================
#[test]
fn init_help_shows_usage() {
    aipm()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialize"));
}

// =========================================================================
// Scenario: No subcommand shows version
// =========================================================================
#[test]
fn no_subcommand_shows_version() {
    aipm().assert().success().stdout(predicate::str::contains("aipm"));
}

// =========================================================================
// Scenario: Claude settings generated
// =========================================================================
#[test]
fn init_claude_settings_generated() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("claude-gen");

    aipm().args(["init", "--marketplace", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join(".claude/settings.json")).unwrap();
    assert!(content.contains("extraKnownMarketplaces"));
    assert!(content.contains(".ai"));
}

// =========================================================================
// Scenario: Starter manifest is valid TOML
// =========================================================================
#[test]
fn init_starter_manifest_valid_toml() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("starter-valid");

    aipm().args(["init", "--marketplace", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join(".ai/starter-aipm-plugin/aipm.toml")).unwrap();
    assert!(content.contains("name = \"starter-aipm-plugin\""));
    assert!(content.contains("version = \"0.1.0\""));
    assert!(content.contains("type = \"composite\""));
}

// =========================================================================
// Scenario: Marketplace.json is generated with correct structure
// =========================================================================
#[test]
fn init_marketplace_json_generated() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("mp-json");

    aipm().args(["init", "--marketplace", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join(".ai/.claude-plugin/marketplace.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(v["name"], "local-repo-plugins");
    assert_eq!(v["owner"]["name"], "local");
    assert_eq!(v["metadata"]["description"], "Local plugins for this repository");
    assert_eq!(v["plugins"][0]["name"], "starter-aipm-plugin");
    assert_eq!(v["plugins"][0]["source"], "./starter-aipm-plugin");
}

// =========================================================================
// Scenario: Marketplace.json with --no-starter has empty plugins
// =========================================================================
#[test]
fn init_marketplace_json_no_starter_empty_plugins() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("mp-json-nostarter");

    aipm().args(["init", "--no-starter", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join(".ai/.claude-plugin/marketplace.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(v["name"], "local-repo-plugins");
    assert!(v["plugins"].as_array().unwrap().is_empty());
}

// =========================================================================
// Scenario: Settings.json has correct marketplace name and enabled plugins
// =========================================================================
#[test]
fn init_settings_json_marketplace_name_and_enabled_plugins() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("settings-check");

    aipm().args(["init", "--marketplace", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join(".claude/settings.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(v["extraKnownMarketplaces"]["local-repo-plugins"].is_object());
    assert_eq!(v["extraKnownMarketplaces"]["local-repo-plugins"]["source"]["path"], "./.ai");
    assert_eq!(v["enabledPlugins"]["starter-aipm-plugin@local-repo-plugins"], true);
}

// =========================================================================
// Scenario: Generated workspace manifest is valid
// =========================================================================
#[test]
fn init_generated_workspace_manifest_valid() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("ws-valid");

    aipm().args(["init", "--workspace", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("[workspace]"));
    assert!(content.contains("members = [\".ai/*\"]"));
    assert!(content.contains("plugins_dir = \".ai\""));
}

// =========================================================================
// Scaffold script e2e tests (require Node.js >= 22.6.0)
// =========================================================================

fn has_node_with_strip_types() -> bool {
    let output = match std::process::Command::new("node").arg("--version").output() {
        Ok(o) if o.status.success() => o,
        _ => return false,
    };
    // Parse version like "v22.6.0" — need >= 22.6.0 for --experimental-strip-types
    let version = String::from_utf8_lossy(&output.stdout);
    let version = version.trim().trim_start_matches('v');
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 2 {
        return false;
    }
    let major: u32 = parts[0].parse().unwrap_or(0);
    let minor: u32 = parts[1].parse().unwrap_or(0);
    major > 22 || (major == 22 && minor >= 6)
}

fn run_scaffold(dir: &std::path::Path, plugin_name: &str) -> std::process::Output {
    let script =
        dir.join(".ai/starter-aipm-plugin/scripts/scaffold-plugin.ts").display().to_string();
    std::process::Command::new("node")
        .args(["--experimental-strip-types", &script, plugin_name])
        .current_dir(dir)
        .output()
        .expect("run scaffold script")
}

#[test]
fn scaffold_script_registers_in_marketplace_json() {
    if !has_node_with_strip_types() {
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("scaffold-mp");

    aipm().args(["init", "--marketplace", &dir.display().to_string()]).assert().success();

    let output = run_scaffold(&dir, "my-new-plugin");
    assert!(
        output.status.success(),
        "scaffold should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = std::fs::read_to_string(dir.join(".ai/.claude-plugin/marketplace.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    let plugins = v["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 2, "should have starter + new plugin");
    assert_eq!(plugins[1]["name"], "my-new-plugin");
    assert_eq!(plugins[1]["source"], "./my-new-plugin");
}

#[test]
fn scaffold_script_enables_in_settings_json() {
    if !has_node_with_strip_types() {
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("scaffold-settings");

    aipm().args(["init", "--marketplace", &dir.display().to_string()]).assert().success();

    let output = run_scaffold(&dir, "my-new-plugin");
    assert!(
        output.status.success(),
        "scaffold should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = std::fs::read_to_string(dir.join(".claude/settings.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(v["enabledPlugins"]["my-new-plugin@local-repo-plugins"], true);
    assert_eq!(v["enabledPlugins"]["starter-aipm-plugin@local-repo-plugins"], true);
}

#[test]
fn scaffold_script_creates_plugin_directory() {
    if !has_node_with_strip_types() {
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("scaffold-dir");

    aipm().args(["init", "--marketplace", &dir.display().to_string()]).assert().success();

    let output = run_scaffold(&dir, "my-new-plugin");
    assert!(
        output.status.success(),
        "scaffold should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(dir.join(".ai/my-new-plugin/aipm.toml").exists());
    assert!(dir.join(".ai/my-new-plugin/.claude-plugin/plugin.json").exists());
    assert!(dir.join(".ai/my-new-plugin/skills/my-new-plugin/SKILL.md").exists());
}

#[test]
fn scaffold_script_multiple_plugins_no_duplicates() {
    if !has_node_with_strip_types() {
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("scaffold-multi");

    aipm().args(["init", "--marketplace", &dir.display().to_string()]).assert().success();

    let out_a = run_scaffold(&dir, "plugin-a");
    assert!(out_a.status.success(), "plugin-a: {}", String::from_utf8_lossy(&out_a.stderr));
    let out_b = run_scaffold(&dir, "plugin-b");
    assert!(out_b.status.success(), "plugin-b: {}", String::from_utf8_lossy(&out_b.stderr));

    let mp_content =
        std::fs::read_to_string(dir.join(".ai/.claude-plugin/marketplace.json")).unwrap();
    let mp: serde_json::Value = serde_json::from_str(&mp_content).unwrap();
    let plugins = mp["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 3, "should have starter + a + b");

    let settings_content = std::fs::read_to_string(dir.join(".claude/settings.json")).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings_content).unwrap();
    let enabled = settings["enabledPlugins"].as_object().unwrap();
    assert_eq!(enabled.len(), 3, "should have 3 enabled plugins");
}

#[test]
fn scaffold_script_rejects_existing_plugin() {
    if !has_node_with_strip_types() {
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("scaffold-dup");

    aipm().args(["init", "--marketplace", &dir.display().to_string()]).assert().success();

    let out1 = run_scaffold(&dir, "my-plugin");
    assert!(out1.status.success(), "first scaffold: {}", String::from_utf8_lossy(&out1.stderr));

    let out2 = run_scaffold(&dir, "my-plugin");
    assert!(!out2.status.success(), "second scaffold should fail");
    let stderr = String::from_utf8_lossy(&out2.stderr);
    assert!(stderr.contains("already exists"), "stderr should mention 'already exists': {stderr}");
}

// =========================================================================
// --yes / -y flag tests
// =========================================================================

#[test]
fn yes_flag_creates_default_marketplace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("yes-test");

    aipm().args(["init", "-y", &dir.display().to_string()]).assert().success();

    assert!(!dir.join("aipm.toml").exists(), "aipm.toml should NOT exist (marketplace only)");
    assert!(dir.join(".ai/starter-aipm-plugin/aipm.toml").exists(), "starter plugin should exist");
}

#[test]
fn yes_long_form_works() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("yes-long");

    aipm().args(["init", "--yes", &dir.display().to_string()]).assert().success();

    assert!(dir.join(".ai").exists(), ".ai directory should exist");
}

#[test]
fn yes_flag_with_workspace_and_marketplace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("yes-both");

    aipm()
        .args(["init", "-y", "--workspace", "--marketplace", &dir.display().to_string()])
        .assert()
        .success();

    assert!(dir.join("aipm.toml").exists(), "aipm.toml should exist");
    assert!(dir.join(".ai").exists(), ".ai directory should exist");
}

#[test]
fn yes_flag_marketplace_only_no_workspace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("yes-mkt");

    aipm().args(["init", "-y", &dir.display().to_string()]).assert().success();

    // Default is marketplace only, no workspace manifest
    assert!(!dir.join("aipm.toml").exists(), "default -y should NOT create aipm.toml");
    assert!(dir.join(".ai").exists(), ".ai should exist");
}
