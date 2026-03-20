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
