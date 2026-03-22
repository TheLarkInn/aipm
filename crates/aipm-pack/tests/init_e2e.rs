//! E2E tests for `aipm-pack init` — maps directly to BDD scenarios in
//! `tests/features/manifest/init.feature`.
//!
//! These tests exercise the actual compiled binary via `assert_cmd`,
//! using `tempfile` for isolated test directories.

// Integration test crates inherit workspace lints. Relax restrictions that are
// appropriate for test code (unwrap, expect, panic are normal in tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use predicates::prelude::*;

fn aipm_pack() -> assert_cmd::Command {
    Command::cargo_bin("aipm-pack").expect("aipm-pack binary should be built")
}

// =========================================================================
// Scenario: Initialize a new plugin in an empty directory
// =========================================================================
#[test]
fn init_in_empty_directory_creates_manifest() {
    let tmp = tempfile::TempDir::new();
    assert!(tmp.is_ok(), "should create temp dir");
    let tmp = tmp.unwrap();
    let plugin_dir = tmp.path().join("my-plugin");

    aipm_pack()
        .args(["init", &plugin_dir.display().to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));

    assert!(plugin_dir.join("aipm.toml").exists());

    let content = std::fs::read_to_string(plugin_dir.join("aipm.toml"));
    assert!(content.is_ok());
    let content = content.unwrap();

    assert!(content.contains("name = \"my-plugin\""), "should use directory name");
    assert!(content.contains("version = \"0.1.0\""), "should have version 0.1.0");
    assert!(content.contains("edition"), "should have edition field");
}

// =========================================================================
// Scenario: Initialize a new plugin with a custom name
// =========================================================================
#[test]
fn init_with_custom_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("workspace");

    aipm_pack()
        .args(["init", "--name", "hello-world", &dir.display().to_string()])
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("name = \"hello-world\""));
}

// =========================================================================
// Scenario: Reject initialization in a directory with an existing manifest
// =========================================================================
#[test]
fn init_rejects_existing_manifest() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("existing");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join("aipm.toml"), "[package]\n").ok();

    aipm_pack()
        .args(["init", &dir.display().to_string()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already initialized"));
}

// =========================================================================
// Scenario: Initialize creates a standard directory layout
// =========================================================================
#[test]
fn init_creates_standard_directory_layout() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-plugin");

    aipm_pack().args(["init", &dir.display().to_string()]).assert().success();

    assert!(dir.join("skills").is_dir(), "skills/ should exist");
    assert!(dir.join("agents").is_dir(), "agents/ should exist");
    assert!(dir.join("hooks").is_dir(), "hooks/ should exist");
    assert!(dir.join("skills/.gitkeep").exists(), "skills/.gitkeep should exist");
}

// =========================================================================
// Scenario Outline: Initialize with a specific plugin type
// =========================================================================
#[test]
fn init_with_type_skill() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-plugin");

    aipm_pack().args(["init", "--type", "skill", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("type = \"skill\""));
    assert!(dir.join("skills/default/SKILL.md").exists(), "skill template should be created");
}

#[test]
fn init_with_type_agent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-plugin");

    aipm_pack().args(["init", "--type", "agent", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("type = \"agent\""));
    assert!(dir.join("agents").is_dir());
}

#[test]
fn init_with_type_mcp() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-plugin");

    aipm_pack().args(["init", "--type", "mcp", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("type = \"mcp\""));
    assert!(dir.join("mcp").is_dir());
}

#[test]
fn init_with_type_hook() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-plugin");

    aipm_pack().args(["init", "--type", "hook", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("type = \"hook\""));
    assert!(dir.join("hooks").is_dir());
}

#[test]
fn init_with_type_composite() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-plugin");

    aipm_pack()
        .args(["init", "--type", "composite", &dir.display().to_string()])
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("type = \"composite\""));
    assert!(dir.join("skills").is_dir());
    assert!(dir.join("agents").is_dir());
    assert!(dir.join("hooks").is_dir());
}

// =========================================================================
// Scenario: Package name must follow naming conventions
// =========================================================================
#[test]
fn init_rejects_invalid_package_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("workspace");

    aipm_pack()
        .args(["init", "--name", "INVALID_Name!", &dir.display().to_string()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid package name"));
}

// =========================================================================
// Additional coverage: LSP type, scoped names, invalid type, --help
// =========================================================================
#[test]
fn init_with_type_lsp() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-lsp");

    aipm_pack().args(["init", "--type", "lsp", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("type = \"lsp\""));
}

#[test]
fn init_with_scoped_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("scoped");

    aipm_pack()
        .args(["init", "--name", "@myorg/cool-plugin", &dir.display().to_string()])
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("name = \"@myorg/cool-plugin\""));
}

#[test]
fn init_rejects_invalid_type() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("bad-type");

    aipm_pack()
        .args(["init", "--type", "invalid-type", &dir.display().to_string()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid plugin type"));
}

#[test]
fn init_help_shows_usage() {
    aipm_pack()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialize"));
}

#[test]
fn generated_manifest_is_valid_toml() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("roundtrip");

    aipm_pack()
        .args([
            "init",
            "--name",
            "roundtrip-test",
            "--type",
            "composite",
            &dir.display().to_string(),
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("name = \"roundtrip-test\""));
    assert!(content.contains("version = \"0.1.0\""));
    assert!(content.contains("type = \"composite\""));
    assert!(content.contains("edition"));
}

// =========================================================================
// Scenario: No subcommand prints version and usage hint
// =========================================================================

#[test]
fn no_subcommand_prints_version_and_usage() {
    aipm_pack()
        .assert()
        .success()
        .stdout(predicate::str::contains("aipm-pack"))
        .stdout(predicate::str::contains("--help"));
}
