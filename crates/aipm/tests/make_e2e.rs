//! E2E tests for `aipm make plugin` — verifies plugin scaffolding inside
//! an initialised marketplace directory.
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
// Scenario: Make a skill plugin targeting the Claude engine
// =========================================================================
#[test]
fn make_plugin_skill_claude() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("skill-claude");

    // Initialise workspace first
    aipm().args(["init", "-y", &dir.display().to_string()]).assert().success();

    // Create plugin
    aipm()
        .args([
            "make",
            "plugin",
            "--name",
            "foo",
            "--engine",
            "claude",
            "--feature",
            "skill",
            "-y",
            "--dir",
            &dir.display().to_string(),
        ])
        .assert()
        .success();

    // Plugin directory structure
    assert!(dir.join(".ai/foo").exists(), ".ai/foo/ should exist");
    assert!(
        dir.join(".ai/foo/skills/foo/SKILL.md").exists(),
        "SKILL.md should exist at .ai/foo/skills/foo/SKILL.md"
    );
    assert!(
        dir.join(".ai/foo/.claude-plugin/plugin.json").exists(),
        "plugin.json should exist at .ai/foo/.claude-plugin/plugin.json"
    );

    // marketplace.json should reference the new plugin
    let mp = std::fs::read_to_string(dir.join(".ai/.claude-plugin/marketplace.json")).unwrap();
    assert!(mp.contains("foo"), "marketplace.json should contain 'foo'");

    // settings.json should have enabledPlugins
    let settings = std::fs::read_to_string(dir.join(".claude/settings.json")).unwrap();
    assert!(settings.contains("enabledPlugins"), "settings.json should contain 'enabledPlugins'");
}

// =========================================================================
// Scenario: Make a composite plugin with skill, agent, and hook features
// =========================================================================
#[test]
fn make_plugin_composite() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("composite");

    aipm().args(["init", "-y", &dir.display().to_string()]).assert().success();

    aipm()
        .args([
            "make",
            "plugin",
            "--name",
            "multi",
            "--engine",
            "claude",
            "--feature",
            "skill",
            "--feature",
            "agent",
            "--feature",
            "hook",
            "-y",
            "--dir",
            &dir.display().to_string(),
        ])
        .assert()
        .success();

    assert!(
        dir.join(".ai/multi/skills/multi/SKILL.md").exists(),
        "skills directory should contain SKILL.md"
    );
    assert!(
        dir.join(".ai/multi/agents/multi.md").exists(),
        "agents directory should contain agent definition"
    );
    assert!(
        dir.join(".ai/multi/hooks/hooks.json").exists(),
        "hooks directory should contain hooks.json"
    );
}

// =========================================================================
// Scenario: Make a plugin targeting the Copilot engine
// =========================================================================
#[test]
fn make_plugin_copilot() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("copilot");

    aipm().args(["init", "-y", &dir.display().to_string()]).assert().success();

    aipm()
        .args([
            "make",
            "plugin",
            "--name",
            "baz",
            "--engine",
            "copilot",
            "--feature",
            "skill",
            "-y",
            "--dir",
            &dir.display().to_string(),
        ])
        .assert()
        .success();

    assert!(dir.join(".ai/baz").exists(), ".ai/baz/ should exist");
    assert!(!dir.join(".github/copilot").exists(), ".github/copilot/ should NOT be created");
}

// =========================================================================
// Scenario: Running make plugin twice with the same name is idempotent
// =========================================================================
#[test]
fn make_plugin_idempotent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("idempotent");

    aipm().args(["init", "-y", &dir.display().to_string()]).assert().success();

    let make_args = [
        "make",
        "plugin",
        "--name",
        "dup",
        "--engine",
        "claude",
        "--feature",
        "skill",
        "-y",
        "--dir",
        &dir.display().to_string(),
    ];

    // First run
    aipm().args(make_args).assert().success();

    // Second run should also succeed and mention "exists"
    aipm()
        .args(make_args)
        .assert()
        .success()
        .stdout(predicate::str::is_match("(?i)exists").unwrap());
}

// =========================================================================
// Scenario: Missing --name flag produces an error
// =========================================================================
#[test]
fn make_plugin_missing_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("no-name");

    aipm().args(["init", "-y", &dir.display().to_string()]).assert().success();

    aipm()
        .args([
            "make",
            "plugin",
            "--engine",
            "claude",
            "--feature",
            "skill",
            "-y",
            "--dir",
            &dir.display().to_string(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::is_match("(?i)missing").unwrap());
}

// =========================================================================
// Scenario: Missing --feature flag produces an error
// =========================================================================
#[test]
fn make_plugin_missing_feature() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("no-feature");

    aipm().args(["init", "-y", &dir.display().to_string()]).assert().success();

    aipm()
        .args([
            "make",
            "plugin",
            "--name",
            "foo",
            "--engine",
            "claude",
            "-y",
            "--dir",
            &dir.display().to_string(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::is_match("(?i)missing").unwrap());
}

// =========================================================================
// Scenario: Make plugin without init (no marketplace) fails
// =========================================================================
#[test]
fn make_plugin_no_marketplace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("no-marketplace");
    std::fs::create_dir_all(&dir).unwrap();

    aipm()
        .args([
            "make",
            "plugin",
            "--name",
            "foo",
            "--engine",
            "claude",
            "--feature",
            "skill",
            "-y",
            "--dir",
            &dir.display().to_string(),
        ])
        .assert()
        .failure();
}

// =========================================================================
// Scenario: Invalid engine produces an error
// =========================================================================
#[test]
fn make_plugin_invalid_engine() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("bad-engine");

    aipm().args(["init", "-y", &dir.display().to_string()]).assert().success();

    aipm()
        .args([
            "make",
            "plugin",
            "--name",
            "foo",
            "--engine",
            "foobar",
            "--feature",
            "skill",
            "-y",
            "--dir",
            &dir.display().to_string(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::is_match("(?i)invalid engine").unwrap());
}

// =========================================================================
// Scenario: Feature not valid for the chosen engine
// =========================================================================
#[test]
fn make_plugin_invalid_feature_for_engine() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("bad-feature");

    aipm().args(["init", "-y", &dir.display().to_string()]).assert().success();

    // lsp is not supported by claude engine
    aipm()
        .args([
            "make",
            "plugin",
            "--name",
            "foo",
            "--engine",
            "claude",
            "--feature",
            "lsp",
            "-y",
            "--dir",
            &dir.display().to_string(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::is_match("(?i)(not supported|unsupported)").unwrap());
}
