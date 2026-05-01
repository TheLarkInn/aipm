//! End-to-end integration tests for issue #725.
//!
//! These tests drive the compiled `aipm` binary against the customer's
//! exact directory layout (`.github/copilot/skills/<x>/SKILL.md` plus
//! `.github/copilot/copilot-instructions.md`) and assert the
//! AIPM_UNIFIED_DISCOVERY=1 path discovers everything the legacy detectors
//! miss.
//!
//! Test 4 is a "regression fence" that pins today's pre-fix behavior
//! (legacy path silently misses #725) so we'd notice if the legacy
//! detectors started covering the layout by accident.

// Integration test crates inherit workspace lints. Relax restrictions
// that are appropriate for test code (unwrap/expect/panic are normal in
// tests, and clippy.toml already allows them in #[test] scope).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use assert_cmd::Command;

/// Build a fresh `aipm` command bound to the compiled binary, with
/// `AIPM_LOG` cleared so external env doesn't leak tracing output into
/// the assertions.
fn aipm() -> Command {
    let mut cmd = Command::cargo_bin("aipm").expect("aipm binary should be built");
    cmd.env_remove("AIPM_LOG");
    cmd
}

/// Build the issue #725 customer fixture under `root`:
///
/// ```text
/// <root>/
/// |-- .ai/.claude-plugin/marketplace.json
/// |-- .github/copilot/
/// |   |-- skills/
/// |   |   |-- skill-alpha/SKILL.md
/// |   |   |-- skill-beta/SKILL.md
/// |   |   `-- skill-gamma/SKILL.md
/// |   `-- copilot-instructions.md
/// ```
fn build_issue_725_fixture(root: &Path) {
    // Marketplace stub so `aipm migrate` doesn't bail with MarketplaceNotFound.
    let marketplace_dir = root.join(".ai").join(".claude-plugin");
    std::fs::create_dir_all(&marketplace_dir).unwrap();
    std::fs::write(marketplace_dir.join("marketplace.json"), "{\"name\":\"test\",\"plugins\":[]}")
        .unwrap();

    // Three skills under .github/copilot/skills/<x>/SKILL.md.
    let skills_root = root.join(".github").join("copilot").join("skills");
    for name in ["skill-alpha", "skill-beta", "skill-gamma"] {
        let skill_dir = skills_root.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let body = format!("---\nname: {name}\ndescription: {name} skill\n---\n# {name} body\n");
        std::fs::write(skill_dir.join("SKILL.md"), body).unwrap();
    }

    // copilot-instructions.md sibling to skills/.
    let instructions_path = root.join(".github").join("copilot").join("copilot-instructions.md");
    std::fs::write(instructions_path, "Use copilot effectively in this repo.\n").unwrap();
}

// =========================================================================
// Test 1: migrate under unified discovery finds all 3 skills + instruction
// =========================================================================

#[test]
fn migrate_unified_finds_issue_725_skills() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    build_issue_725_fixture(root);

    let output = aipm()
        .env("AIPM_UNIFIED_DISCOVERY", "1")
        .args(["migrate", root.to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "exit code should be 0\nstdout: {stdout}\nstderr: {stderr}");

    // "1 instruction" is singular per format_counts (count == 1).
    assert!(
        stderr.contains("matched 3 skills, 1 instruction"),
        "stderr should contain the unified scan summary; got: {stderr}"
    );

    // Three "Migrated skill" lines in stdout — one per skill the unified
    // adapter pipeline produced.
    let migrated_lines = stdout.lines().filter(|l| l.starts_with("Migrated skill")).count();
    assert_eq!(migrated_lines, 3, "expected 3 'Migrated skill' lines in stdout; got: {stdout}");

    // Each skill should have a SKILL.md inside its emitted plugin directory:
    //   <root>/.ai/<plugin>/skills/<plugin>/SKILL.md
    for name in ["skill-alpha", "skill-beta", "skill-gamma"] {
        let skill_md = root.join(".ai").join(name).join("skills").join(name).join("SKILL.md");
        assert!(skill_md.exists(), "expected SKILL.md at {} after migrate", skill_md.display());
    }
}

// =========================================================================
// Test 2: lint under unified discovery prints scan summary on stderr
// =========================================================================

#[test]
fn lint_unified_summary_visible() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    build_issue_725_fixture(root);

    let output = aipm()
        .env("AIPM_UNIFIED_DISCOVERY", "1")
        .args(["lint", root.to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Skills validate (name + description present) so lint exits 0 even
    // though source/misplaced-features warnings fire for the .github
    // location.
    assert!(output.status.success(), "exit code should be 0\nstdout: {stdout}\nstderr: {stderr}");

    assert!(
        stderr.contains("matched 3 skills, 1 instruction"),
        "stderr should contain the unified scan summary; got: {stderr}"
    );

    // Sensible non-error human output: no error[ severity tags should
    // appear (warnings are fine — skills are misplaced, not invalid).
    assert!(
        !stdout.contains("error["),
        "stdout should not contain hard error diagnostics; got: {stdout}"
    );
}

// =========================================================================
// Test 3: --no-summary suppresses the stderr summary line
// =========================================================================

#[test]
fn lint_no_summary_suppresses_stderr_summary() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    build_issue_725_fixture(root);

    let output = aipm()
        .env("AIPM_UNIFIED_DISCOVERY", "1")
        .args(["lint", "--no-summary", root.to_str().unwrap()])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("Scanned"),
        "--no-summary should suppress the 'Scanned …' summary on stderr; got: {stderr}"
    );
}

// =========================================================================
// Test 4: legacy path (default OFF) silently misses #725 — regression fence
// =========================================================================

#[test]
fn migrate_legacy_path_misses_issue_725() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    build_issue_725_fixture(root);

    let output = aipm()
        .env_remove("AIPM_UNIFIED_DISCOVERY")
        .args(["migrate", root.to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Legacy path silently succeeds (the bug behind #725).
    assert!(
        output.status.success(),
        "legacy migrate should silent-succeed\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Regression fence: zero "Migrated skill" lines — pins today's
    // pre-fix behavior so a future change to the legacy detectors that
    // accidentally fixes this would surface here.
    let migrated_lines = stdout.lines().filter(|l| l.starts_with("Migrated skill")).count();
    assert_eq!(
        migrated_lines, 0,
        "legacy path should NOT migrate any #725 skills; got stdout: {stdout}"
    );
}

// =========================================================================
// Test 5: ci-github reporter — diagnostics on stdout, summary on stderr
// =========================================================================

#[test]
fn lint_ci_github_unified_summary_on_stderr_diagnostics_on_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    build_issue_725_fixture(root);

    let output = aipm()
        .env("AIPM_UNIFIED_DISCOVERY", "1")
        .args(["lint", "--reporter", "ci-github", root.to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // No hard `::error::` lines — skill content is valid (name +
    // description present). The misplaced-features rule fires as a
    // warning, which is acceptable here; we only assert against errors.
    assert!(
        !stdout.contains("::error::"),
        "ci-github stdout should not contain ::error:: lines; got: {stdout}"
    );

    // The scan summary still goes to stderr, keeping stdout cleanly
    // machine-parseable.
    assert!(
        stderr.contains("matched 3 skills"),
        "stderr should contain the unified scan summary; got: {stderr}"
    );
}
