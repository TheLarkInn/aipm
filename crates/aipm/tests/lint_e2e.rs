//! End-to-end integration tests for `aipm lint`.

// Integration test crates inherit workspace lints. Relax restrictions that are
// appropriate for test code (unwrap, expect, panic are normal in tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use predicates::prelude::*;

fn aipm() -> Command {
    Command::cargo_bin("aipm").expect("aipm binary should be built")
}

// =========================================================================
// Clean workspace — no issues
// =========================================================================

#[test]
fn lint_clean_workspace_succeeds() {
    let tmp = tempfile::tempdir().unwrap();

    // Create a minimal marketplace with a well-formed plugin
    let ai_dir = tmp.path().join(".ai");
    let plugin_dir = ai_dir.join("test-plugin");
    let skills_dir = plugin_dir.join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();

    std::fs::write(
        skills_dir.join("SKILL.md"),
        "---\nname: test-skill\ndescription: A test skill\n---\nBody content\n",
    )
    .unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("no issues found"));
}

// =========================================================================
// Missing description — warning, exit 0
// =========================================================================

#[test]
fn lint_missing_description_warns() {
    let tmp = tempfile::tempdir().unwrap();

    let ai_dir = tmp.path().join(".ai");
    let skills_dir = ai_dir.join("test-plugin").join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();

    std::fs::write(skills_dir.join("SKILL.md"), "---\nname: test-skill\n---\nBody\n").unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success() // warnings don't cause non-zero exit
        .stdout(predicate::str::contains("skill/missing-description"));
}

// =========================================================================
// Missing name — warning
// =========================================================================

#[test]
fn lint_missing_name_warns() {
    let tmp = tempfile::tempdir().unwrap();

    let ai_dir = tmp.path().join(".ai");
    let skills_dir = ai_dir.join("test-plugin").join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();

    std::fs::write(skills_dir.join("SKILL.md"), "---\ndescription: A test skill\n---\nBody\n")
        .unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("skill/missing-name"));
}

// =========================================================================
// Oversized skill — warning
// =========================================================================

#[test]
fn lint_oversized_skill_warns() {
    let tmp = tempfile::tempdir().unwrap();

    let ai_dir = tmp.path().join(".ai");
    let skills_dir = ai_dir.join("test-plugin").join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();

    // Create a skill file that exceeds 15,000 characters
    let big_content =
        format!("---\nname: big-skill\ndescription: test\n---\n{}", "x".repeat(16_000));
    std::fs::write(skills_dir.join("SKILL.md"), big_content).unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("skill/oversized"));
}

// =========================================================================
// Unknown hook event — error, exit 1
// =========================================================================

#[test]
fn lint_unknown_hook_event_errors() {
    let tmp = tempfile::tempdir().unwrap();

    let ai_dir = tmp.path().join(".ai");
    let hooks_dir = ai_dir.join("test-plugin").join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();

    std::fs::write(
        hooks_dir.join("hooks.json"),
        r#"{ "InvalidEvent": [{ "type": "command", "command": "echo hi" }] }"#,
    )
    .unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .failure() // errors cause non-zero exit
        .stdout(predicate::str::contains("hook/unknown-event"))
        .stdout(predicate::str::contains("InvalidEvent"));
}

// =========================================================================
// --source filter
// =========================================================================

#[test]
fn lint_source_filter_only_scans_selected() {
    let tmp = tempfile::tempdir().unwrap();

    // Create both .claude and .ai directories
    let claude_skills = tmp.path().join(".claude").join("skills");
    std::fs::create_dir_all(&claude_skills).unwrap();
    let ai_dir = tmp.path().join(".ai");
    let skills_dir = ai_dir.join("test-plugin").join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();

    // Put a skill missing name in .ai
    std::fs::write(skills_dir.join("SKILL.md"), "---\ndescription: no name\n---\nBody\n").unwrap();

    // Lint only .claude — should not find .ai issues
    aipm()
        .args(["lint", "--source", ".claude", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("no issues found").or(
            // misplaced-features might fire if .ai exists
            predicate::str::contains("source/misplaced-features"),
        ));
}

// =========================================================================
// --format json
// =========================================================================

#[test]
fn lint_json_format_produces_valid_output() {
    let tmp = tempfile::tempdir().unwrap();

    let ai_dir = tmp.path().join(".ai");
    let skills_dir = ai_dir.join("test-plugin").join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();

    std::fs::write(skills_dir.join("SKILL.md"), "---\nname: test\ndescription: test\n---\nBody\n")
        .unwrap();

    aipm()
        .args(["lint", "--format", "json", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"diagnostics\""))
        .stdout(predicate::str::contains("\"summary\""));
}

// =========================================================================
// Empty directory — no issues
// =========================================================================

#[test]
fn lint_empty_directory_succeeds() {
    let tmp = tempfile::tempdir().unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("no issues found"));
}

// =========================================================================
// Invalid shell — error
// =========================================================================

#[test]
fn lint_invalid_shell_errors() {
    let tmp = tempfile::tempdir().unwrap();

    let ai_dir = tmp.path().join(".ai");
    let skills_dir = ai_dir.join("test-plugin").join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();

    std::fs::write(
        skills_dir.join("SKILL.md"),
        "---\nname: test\ndescription: test\nshell: zsh\n---\nBody\n",
    )
    .unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("skill/invalid-shell"))
        .stdout(predicate::str::contains("zsh"));
}

// =========================================================================
// Legacy hook event name — warning
// =========================================================================

#[test]
fn lint_legacy_hook_event_warns() {
    let tmp = tempfile::tempdir().unwrap();

    let ai_dir = tmp.path().join(".ai");
    let hooks_dir = ai_dir.join("test-plugin").join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();

    // "Stop" is a valid Claude event but a legacy Copilot name
    std::fs::write(
        hooks_dir.join("hooks.json"),
        r#"{ "Stop": [{ "type": "command", "command": "echo bye" }] }"#,
    )
    .unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success() // warning, not error
        .stdout(predicate::str::contains("hook/legacy-event-name"))
        .stdout(predicate::str::contains("agentStop"));
}

// =========================================================================
// Name too long — warning
// =========================================================================

#[test]
fn lint_name_too_long_warns() {
    let tmp = tempfile::tempdir().unwrap();

    let ai_dir = tmp.path().join(".ai");
    let skills_dir = ai_dir.join("test-plugin").join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();

    let long_name = "a".repeat(65);
    let content = format!("---\nname: {long_name}\ndescription: test\n---\nBody\n");
    std::fs::write(skills_dir.join("SKILL.md"), content).unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("skill/name-too-long"));
}

// =========================================================================
// Agent missing tools — warning
// =========================================================================

#[test]
fn lint_agent_missing_tools_warns() {
    let tmp = tempfile::tempdir().unwrap();

    let ai_dir = tmp.path().join(".ai");
    let agents_dir = ai_dir.join("test-plugin").join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    std::fs::write(
        agents_dir.join("reviewer.md"),
        "---\nname: reviewer\ndescription: code review\n---\nPrompt\n",
    )
    .unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("agent/missing-tools"));
}
