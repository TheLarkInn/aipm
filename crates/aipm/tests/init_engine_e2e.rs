//! E2E tests for `aipm init`'s engine-aware behavior (Spec G9 / Feature 18).
//!
//! Maps to BDD scenarios in `tests/features/manifest/workspace-init.feature`
//! under "Rule: Engine-aware init scaffolds only chosen engines" (added by
//! Feature 20).

// Integration test crates inherit workspace lints. Relax restrictions that
// are appropriate for test code (unwrap, expect, panic are normal in tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use predicates::prelude::*;

fn aipm() -> assert_cmd::Command {
    Command::cargo_bin("aipm").expect("aipm binary should be built")
}

// =========================================================================
// Scenario: --engine copilot scaffolds only .github/copilot-instructions.md
// =========================================================================
#[test]
fn init_with_engine_copilot_creates_only_github() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("copilot-only");

    aipm().args(["init", "--engine", "copilot", &dir.display().to_string()]).assert().success();

    assert!(
        dir.join(".github/copilot-instructions.md").exists(),
        "expected .github/copilot-instructions.md to exist"
    );
    assert!(!dir.join(".claude").exists(), "expected .claude/ NOT to exist (issue #724 fix)");
}

// =========================================================================
// Scenario: --engine claude,copilot scaffolds both engine roots
// =========================================================================
#[test]
fn init_with_engine_both_creates_both() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("both-engines");

    aipm()
        .args(["init", "--engine", "claude,copilot", &dir.display().to_string()])
        .assert()
        .success();

    assert!(dir.join(".claude/settings.json").exists(), "expected .claude/settings.json to exist");
    assert!(
        dir.join(".github/copilot-instructions.md").exists(),
        "expected .github/copilot-instructions.md to exist"
    );
}

// =========================================================================
// Scenario: --engine claude --engine copilot (repeated flag form) merges
// =========================================================================
#[test]
fn init_with_repeated_engine_flag_merges() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("repeated-flags");

    aipm()
        .args(["init", "--engine", "claude", "--engine", "copilot", &dir.display().to_string()])
        .assert()
        .success();

    assert!(dir.join(".claude/settings.json").exists());
    assert!(dir.join(".github/copilot-instructions.md").exists());
}

// =========================================================================
// Scenario: --yes mode without --engine defaults to Copilot only (Spec G5)
// =========================================================================
#[test]
fn init_yes_default_scaffolds_copilot() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("yes-default");

    aipm().args(["init", "--yes", &dir.display().to_string()]).assert().success();

    assert!(
        dir.join(".github/copilot-instructions.md").exists(),
        "expected .github/copilot-instructions.md to exist (--yes default per Spec G5)"
    );
    assert!(
        !dir.join(".claude").exists(),
        "expected .claude/ NOT to exist when --yes default is Copilot"
    );
}

// =========================================================================
// Scenario: --engine gemini errors with helpful message
// =========================================================================
#[test]
fn init_engine_unknown_errors() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("unknown-engine");

    let assert = aipm()
        .args(["init", "--yes", "--engine", "gemini", &dir.display().to_string()])
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("unknown engine 'gemini'"),
        "stderr should mention unknown engine: {stderr}"
    );
    assert!(stderr.contains("claude"), "stderr should list known engines: {stderr}");
}

// =========================================================================
// Scenario: --engine '' errors with helpful message
// =========================================================================
#[test]
fn init_engine_empty_string_errors() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("empty-engine");

    let assert = aipm()
        .args(["init", "--yes", "--engine", "", &dir.display().to_string()])
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(stderr.contains("must not be empty"), "stderr should mention empty value: {stderr}");
}

// =========================================================================
// Scenario: --engine claude alone does not scaffold .github/copilot-instructions.md
// (regression coverage for the issue #724 fix from a different angle)
// =========================================================================
#[test]
fn init_engine_claude_alone_does_not_create_github_copilot_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("claude-only");

    aipm().args(["init", "--engine", "claude", &dir.display().to_string()]).assert().success();

    assert!(dir.join(".claude/settings.json").exists());
    assert!(
        !dir.join(".github/copilot-instructions.md").exists(),
        "expected no Copilot file when only claude was requested"
    );
}

// =========================================================================
// Scenario: Headless --yes never writes the engines field (Spec §5.2.3)
// =========================================================================
#[test]
fn init_yes_omits_engines_field_in_workspace_toml() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("yes-no-engines");

    aipm()
        .args(["init", "--yes", "--workspace", "--engine", "claude", &dir.display().to_string()])
        .assert()
        .success();

    let toml_path = dir.join("aipm.toml");
    assert!(toml_path.exists(), "workspace aipm.toml should exist");
    let content = std::fs::read_to_string(&toml_path).expect("read workspace toml");
    assert!(
        !content.contains("engines ="),
        "workspace aipm.toml should NOT contain engines field in headless mode: {content}"
    );
}

// =========================================================================
// Help text mentions --engine flag (smoke test for clap registration)
// =========================================================================
#[test]
fn init_help_documents_engine_flag() {
    aipm().args(["init", "--help"]).assert().success().stdout(predicate::str::contains("--engine"));
}
