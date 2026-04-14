//! E2E tests for `aipm pack init` — migrated from `aipm-pack init`.
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
// Scenario: Initialize a new plugin in an empty directory
// =========================================================================
#[test]
fn pack_init_in_empty_directory_creates_manifest() {
    let tmp = tempfile::TempDir::new();
    assert!(tmp.is_ok(), "should create temp dir");
    let tmp = tmp.unwrap();
    let plugin_dir = tmp.path().join("my-plugin");

    aipm()
        .args(["pack", "init", &plugin_dir.display().to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));

    assert!(plugin_dir.join("aipm.toml").exists());

    let content = std::fs::read_to_string(plugin_dir.join("aipm.toml"));
    assert!(content.is_ok());
    let content = content.unwrap();

    assert!(content.contains("name = \"my-plugin\""), "should use directory name");
    assert!(content.contains("version = \"0.1.0\""), "should have version 0.1.0");
}

// =========================================================================
// Scenario: Initialize a new plugin with a custom name
// =========================================================================
#[test]
fn pack_init_with_custom_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("workspace");

    aipm()
        .args(["pack", "init", "--name", "hello-world", &dir.display().to_string()])
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("name = \"hello-world\""));
}

// =========================================================================
// Scenario: Reject initialization in a directory with an existing manifest
// =========================================================================
#[test]
fn pack_init_rejects_existing_manifest() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("existing");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join("aipm.toml"), "[package]\n").ok();

    aipm()
        .args(["pack", "init", &dir.display().to_string()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already initialized"));
}

// =========================================================================
// Scenario: Initialize creates a standard directory layout
// =========================================================================
#[test]
fn pack_init_creates_standard_directory_layout() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-plugin");

    aipm().args(["pack", "init", &dir.display().to_string()]).assert().success();

    assert!(dir.join("skills").is_dir(), "skills/ should exist");
    assert!(dir.join("agents").is_dir(), "agents/ should exist");
    assert!(dir.join("hooks").is_dir(), "hooks/ should exist");
    assert!(dir.join("skills/.gitkeep").exists(), "skills/.gitkeep should exist");
}

// =========================================================================
// Scenario Outline: Initialize with a specific plugin type
// =========================================================================
#[test]
fn pack_init_with_type_skill() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-plugin");

    aipm().args(["pack", "init", "--type", "skill", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("type = \"skill\""));
    assert!(dir.join("skills/default/SKILL.md").exists(), "skill template should be created");
}

#[test]
fn pack_init_with_type_agent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-plugin");

    aipm().args(["pack", "init", "--type", "agent", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("type = \"agent\""));
    assert!(dir.join("agents").is_dir());
}

#[test]
fn pack_init_with_type_mcp() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-plugin");

    aipm().args(["pack", "init", "--type", "mcp", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("type = \"mcp\""));
    assert!(dir.join("mcp").is_dir());
}

#[test]
fn pack_init_with_type_hook() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-plugin");

    aipm().args(["pack", "init", "--type", "hook", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("type = \"hook\""));
    assert!(dir.join("hooks").is_dir());
}

#[test]
fn pack_init_with_type_composite() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-plugin");

    aipm()
        .args(["pack", "init", "--type", "composite", &dir.display().to_string()])
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
fn pack_init_rejects_invalid_package_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("workspace");

    aipm()
        .args(["pack", "init", "--name", "INVALID_Name!", &dir.display().to_string()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid package name"));
}

// =========================================================================
// Additional coverage: LSP type, scoped names, invalid type, --help
// =========================================================================
#[test]
fn pack_init_with_type_lsp() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("my-lsp");

    aipm().args(["pack", "init", "--type", "lsp", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("type = \"lsp\""));
}

#[test]
fn pack_init_with_scoped_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("scoped");

    aipm()
        .args(["pack", "init", "--name", "@myorg/cool-plugin", &dir.display().to_string()])
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("name = \"@myorg/cool-plugin\""));
}

#[test]
fn pack_init_rejects_invalid_type() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("bad-type");

    aipm()
        .args(["pack", "init", "--type", "invalid-type", &dir.display().to_string()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid plugin type"));
}

#[test]
fn pack_init_help_shows_usage() {
    aipm()
        .args(["pack", "init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialize"));
}

#[test]
fn pack_init_generated_manifest_is_valid_toml() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("roundtrip");

    aipm()
        .args([
            "pack",
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
}

// =========================================================================
// --yes / -y flag tests
// =========================================================================

#[test]
fn pack_init_yes_flag_creates_default_package() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("yes-pkg");
    std::fs::create_dir_all(&dir).unwrap();

    aipm().args(["pack", "init", "-y", &dir.display().to_string()]).assert().success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("type = \"composite\""));
    assert!(content.contains("version = \"0.1.0\""));
}

#[test]
fn pack_init_yes_long_form_works() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("yes-long-pkg");
    std::fs::create_dir_all(&dir).unwrap();

    aipm().args(["pack", "init", "--yes", &dir.display().to_string()]).assert().success();

    assert!(dir.join("aipm.toml").exists());
}

#[test]
fn pack_init_yes_flag_with_name_override() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("yes-name-pkg");
    std::fs::create_dir_all(&dir).unwrap();

    aipm()
        .args(["pack", "init", "-y", "--name", "custom-name", &dir.display().to_string()])
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("name = \"custom-name\""));
}

#[test]
fn pack_init_yes_flag_with_type_override() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("yes-type-pkg");
    std::fs::create_dir_all(&dir).unwrap();

    aipm()
        .args(["pack", "init", "-y", "--type", "skill", &dir.display().to_string()])
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.join("aipm.toml")).unwrap();
    assert!(content.contains("type = \"skill\""));
}

// =========================================================================
// Scenario: Init with default "." directory resolves to current_dir
// =========================================================================

#[test]
fn pack_init_defaults_to_current_directory() {
    let tmp = tempfile::TempDir::new().unwrap();
    let plugin_dir = tmp.path().join("my-dot-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    aipm()
        .current_dir(&plugin_dir)
        .args(["pack", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));

    assert!(plugin_dir.join("aipm.toml").exists(), "aipm.toml should be created in cwd");

    let content = std::fs::read_to_string(plugin_dir.join("aipm.toml")).unwrap();
    assert!(content.contains("name = \"my-dot-plugin\""), "name should come from directory name");
}
