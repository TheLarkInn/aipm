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
// --source validation: unsupported source
// =========================================================================

#[test]
fn lint_unsupported_source_errors() {
    let tmp = tempfile::tempdir().unwrap();

    aipm()
        .args(["lint", "--source", ".vscode", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported source"));
}

// =========================================================================
// --source validation: nonexistent directory
// =========================================================================

#[test]
fn lint_nonexistent_ai_source_dir_succeeds() {
    let tmp = tempfile::tempdir().unwrap();

    // Unified discovery: --source .ai with no .ai/ dir succeeds with no findings,
    // consistent with .claude and .github behaviour.
    aipm()
        .args(["lint", "--source", ".ai", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("no issues found"));
}

#[test]
fn lint_nonexistent_claude_source_dir_succeeds() {
    let tmp = tempfile::tempdir().unwrap();

    // .claude/.github use recursive discovery — missing root dir is fine (no findings)
    aipm()
        .args(["lint", "--source", ".claude", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("no issues found"));
}

// =========================================================================
// Config: [workspace.lints] suppress rule
// =========================================================================

#[test]
fn lint_config_allow_suppresses_rule() {
    let tmp = tempfile::tempdir().unwrap();

    let ai_dir = tmp.path().join(".ai");
    let skills_dir = ai_dir.join("test-plugin").join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();
    std::fs::write(skills_dir.join("SKILL.md"), "---\nname: test\n---\nBody\n").unwrap();

    std::fs::write(
        tmp.path().join("aipm.toml"),
        "[workspace]\nmembers = [\".ai/*\"]\n\n[workspace.lints]\n\"skill/missing-description\" = \"allow\"\n",
    )
    .unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("skill/missing-description").not());
}

// =========================================================================
// Config: severity override to error
// =========================================================================

#[test]
fn lint_config_severity_override() {
    let tmp = tempfile::tempdir().unwrap();

    let ai_dir = tmp.path().join(".ai");
    let skills_dir = ai_dir.join("test-plugin").join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();
    std::fs::write(skills_dir.join("SKILL.md"), "---\nname: test\n---\nBody\n").unwrap();

    std::fs::write(
        tmp.path().join("aipm.toml"),
        "[workspace]\nmembers = [\".ai/*\"]\n\n[workspace.lints]\n\"skill/missing-description\" = \"error\"\n",
    )
    .unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("error[skill/missing-description]"));
}

// =========================================================================
// Config: global ignore paths
// =========================================================================

#[test]
fn lint_config_ignore_paths() {
    let tmp = tempfile::tempdir().unwrap();

    let ai_dir = tmp.path().join(".ai");
    let skills_dir = ai_dir.join("ignored-plugin").join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();
    std::fs::write(skills_dir.join("SKILL.md"), "---\ndescription: no name\n---\nBody\n").unwrap();

    std::fs::write(
        tmp.path().join("aipm.toml"),
        "[workspace]\nmembers = [\".ai/*\"]\n\n[workspace.lints.ignore]\npaths = [\"**/ignored-plugin/**\"]\n",
    )
    .unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("no issues found"));
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

// =========================================================================
// Monorepo: nested .claude/ directories discovered recursively
// =========================================================================

#[test]
fn lint_monorepo_finds_nested_misplaced_features() {
    let tmp = tempfile::tempdir().unwrap();

    // Create .ai/ marketplace at root
    std::fs::create_dir_all(tmp.path().join(".ai")).unwrap();
    // Create nested .claude/skills/default/SKILL.md (misplaced feature in a package)
    let nested =
        tmp.path().join("packages").join("auth").join(".claude").join("skills").join("default");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(
        nested.join("SKILL.md"),
        "---\nname: nested-skill\ndescription: test\n---\nbody\n",
    )
    .unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("source/misplaced-features"));
}

// =========================================================================
// Monorepo: --source .claude with no root .claude/ but nested .claude/
// =========================================================================

#[test]
fn lint_source_claude_no_root_dir_succeeds_with_nested() {
    let tmp = tempfile::tempdir().unwrap();

    // Create .ai/ marketplace at root
    std::fs::create_dir_all(tmp.path().join(".ai")).unwrap();
    // No root .claude/ — only nested with an actual SKILL.md
    let nested =
        tmp.path().join("packages").join("auth").join(".claude").join("skills").join("default");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(
        nested.join("SKILL.md"),
        "---\nname: nested-skill\ndescription: test\n---\nbody\n",
    )
    .unwrap();

    aipm()
        .args(["lint", "--source", ".claude", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("source/misplaced-features"));
}

// =========================================================================
// Monorepo: --max-depth limits recursive discovery
// =========================================================================

#[test]
fn lint_max_depth_cli_flag() {
    let tmp = tempfile::tempdir().unwrap();

    // Create .ai/ marketplace at root
    std::fs::create_dir_all(tmp.path().join(".ai")).unwrap();

    // Root SKILL.md at .claude/skills/default/SKILL.md (depth 4 from project root)
    let root_skill = tmp.path().join(".claude").join("skills").join("default");
    std::fs::create_dir_all(&root_skill).unwrap();
    std::fs::write(
        root_skill.join("SKILL.md"),
        "---\nname: root-skill\ndescription: test\n---\nbody\n",
    )
    .unwrap();

    // Nested SKILL.md at packages/auth/.claude/skills/default/SKILL.md (depth 6)
    let nested =
        tmp.path().join("packages").join("auth").join(".claude").join("skills").join("default");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(
        nested.join("SKILL.md"),
        "---\nname: nested-skill\ndescription: test\n---\nbody\n",
    )
    .unwrap();

    // --max-depth 5 finds depth-4 SKILL.md (root) but not depth-6 (nested)
    let output = aipm()
        .args(["lint", "--max-depth", "5", tmp.path().to_str().unwrap()])
        .output()
        .expect("command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should find misplaced features (root .claude/skills/default/SKILL.md)
    assert!(stdout.contains("source/misplaced-features"));
    // The nested path should NOT appear in output
    assert!(!stdout.contains("auth"));
}

// =========================================================================
// .github/ source: misplaced-features without .ai/
// =========================================================================

#[test]
fn lint_github_skills_without_marketplace_warns_misplaced() {
    let tmp = tempfile::tempdir().unwrap();

    // .github/skills/default/SKILL.md — no .ai/ marketplace
    let skills_dir = tmp.path().join(".github").join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();
    std::fs::write(
        skills_dir.join("SKILL.md"),
        "---\nname: gh-skill\ndescription: test\n---\nbody\n",
    )
    .unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("source/misplaced-features"));
}

#[test]
fn lint_github_agents_without_marketplace_warns_misplaced() {
    let tmp = tempfile::tempdir().unwrap();

    // .github/agents/reviewer.md — no .ai/ marketplace
    let agents_dir = tmp.path().join(".github").join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    std::fs::write(
        agents_dir.join("reviewer.md"),
        "---\nname: reviewer\ntools: Read\n---\nprompt\n",
    )
    .unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("source/misplaced-features"));
}

// =========================================================================
// .github/ source: quality rules also fire on misplaced features
// =========================================================================

#[test]
fn lint_github_skill_missing_name_and_misplaced() {
    let tmp = tempfile::tempdir().unwrap();

    // .github/skills/default/SKILL.md missing name — should get both rules
    let skills_dir = tmp.path().join(".github").join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();
    std::fs::write(skills_dir.join("SKILL.md"), "---\ndescription: no name here\n---\nbody\n")
        .unwrap();

    let output =
        aipm().args(["lint", tmp.path().to_str().unwrap()]).output().expect("command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("source/misplaced-features"), "expected misplaced-features");
    assert!(stdout.contains("skill/missing-name"), "expected missing-name");
}

// =========================================================================
// .claude/ without .ai/: help text mentions 'aipm init'
// =========================================================================

#[test]
fn lint_claude_without_marketplace_suggests_init() {
    let tmp = tempfile::tempdir().unwrap();

    // .claude/skills/default/SKILL.md — no .ai/ marketplace
    let skills_dir = tmp.path().join(".claude").join("skills").join("default");
    std::fs::create_dir_all(&skills_dir).unwrap();
    std::fs::write(
        skills_dir.join("SKILL.md"),
        "---\nname: claude-skill\ndescription: test\n---\nbody\n",
    )
    .unwrap();

    aipm()
        .args(["lint", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("source/misplaced-features"))
        .stdout(predicate::str::contains("aipm init"));
}

// =========================================================================
// --source .github filters to only .github features
// =========================================================================

#[test]
fn lint_source_github_filters_to_github_only() {
    let tmp = tempfile::tempdir().unwrap();

    // Both .claude and .github have skills
    let claude_skills = tmp.path().join(".claude").join("skills").join("default");
    std::fs::create_dir_all(&claude_skills).unwrap();
    std::fs::write(
        claude_skills.join("SKILL.md"),
        "---\nname: claude-skill\ndescription: test\n---\nbody\n",
    )
    .unwrap();

    let github_skills = tmp.path().join(".github").join("skills").join("default");
    std::fs::create_dir_all(&github_skills).unwrap();
    std::fs::write(
        github_skills.join("SKILL.md"),
        "---\nname: github-skill\ndescription: test\n---\nbody\n",
    )
    .unwrap();

    let output = aipm()
        .args(["lint", "--source", ".github", tmp.path().to_str().unwrap()])
        .output()
        .expect("command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // .github skill should appear
    assert!(stdout.contains(".github"), "expected .github path in output");
    // .claude skill should NOT appear (filtered out)
    assert!(!stdout.contains(".claude"), "unexpected .claude path in output");
}
