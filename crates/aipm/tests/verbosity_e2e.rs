//! Integration tests for verbosity flags and logging infrastructure.
//!
//! Verifies that `-v`, `-q`, `--log-format`, and `AIPM_LOG` flags
//! behave as specified.

// Integration test crates inherit workspace lints. Relax restrictions that are
// appropriate for test code (unwrap, expect, panic are normal in tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use predicates::prelude::*;

fn aipm() -> assert_cmd::Command {
    let mut cmd = Command::cargo_bin("aipm").expect("aipm binary should be built");
    // Clear AIPM_LOG so external env doesn't make tests flaky
    cmd.env_remove("AIPM_LOG");
    cmd
}

// =========================================================================
// Smoke tests — basic CLI still works with new flags
// =========================================================================

#[test]
fn help_flag_still_works() {
    aipm().arg("--help").assert().success().stdout(predicate::str::contains("--verbose"));
}

#[test]
fn help_shows_log_format_flag() {
    aipm().arg("--help").assert().success().stdout(predicate::str::contains("--log-format"));
}

#[test]
fn help_shows_quiet_flag() {
    aipm().arg("--help").assert().success().stdout(predicate::str::contains("--quiet"));
}

#[test]
fn no_subcommand_still_prints_version() {
    aipm().assert().success().stdout(predicate::str::contains("aipm"));
}

// =========================================================================
// Verbosity flags parse correctly
// =========================================================================

#[test]
fn verbose_flag_accepted() {
    aipm().args(["-v", "list", "--dir", "."]).assert().success();
}

#[test]
fn double_verbose_flag_accepted() {
    aipm().args(["-vv", "list", "--dir", "."]).assert().success();
}

#[test]
fn triple_verbose_flag_accepted() {
    aipm().args(["-vvv", "list", "--dir", "."]).assert().success();
}

#[test]
fn quiet_flag_accepted() {
    aipm().args(["-q", "list", "--dir", "."]).assert().success();
}

#[test]
fn double_quiet_flag_accepted() {
    aipm().args(["-qq", "list", "--dir", "."]).assert().success();
}

// =========================================================================
// --log-format flag
// =========================================================================

#[test]
fn log_format_text_accepted() {
    aipm().args(["--log-format", "text", "list", "--dir", "."]).assert().success();
}

#[test]
fn log_format_json_accepted() {
    aipm().args(["--log-format", "json", "list", "--dir", "."]).assert().success();
}

#[test]
fn log_format_invalid_rejected() {
    aipm()
        .args(["--log-format", "xml"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}

// =========================================================================
// Default verbosity — no tracing noise on stderr for clean operations
// =========================================================================

#[test]
fn default_verbosity_no_tracing_on_stderr() {
    let tmp = tempfile::tempdir().unwrap();
    let output = aipm().args(["list", "--dir", tmp.path().to_str().unwrap()]).output().unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    // At default (Warn), a clean `list` on an empty dir should not produce
    // any tracing output on stderr (no warnings to report)
    assert!(
        !stderr.contains("INFO") && !stderr.contains("DEBUG") && !stderr.contains("TRACE"),
        "default verbosity should not show info/debug/trace on stderr: {stderr}"
    );
}

// =========================================================================
// -q suppresses warnings on stderr
// =========================================================================

#[test]
fn quiet_suppresses_warnings() {
    let tmp = tempfile::tempdir().unwrap();

    // Write invalid TOML so load_lint_config emits a WARN-level tracing event
    std::fs::write(tmp.path().join("aipm.toml"), "not valid toml = [").unwrap();

    // Without -q, the warning should appear
    let noisy = aipm().args(["lint", tmp.path().to_str().unwrap()]).output().unwrap();
    let noisy_stderr = String::from_utf8_lossy(&noisy.stderr);
    assert!(
        noisy_stderr.contains("WARN"),
        "non-quiet should emit warning for invalid config: {noisy_stderr}"
    );

    // With -q, the warning should be suppressed
    let quiet = aipm().args(["-q", "lint", tmp.path().to_str().unwrap()]).output().unwrap();
    let quiet_stderr = String::from_utf8_lossy(&quiet.stderr);
    assert!(
        !quiet_stderr.contains("WARN"),
        "quiet mode should suppress warnings on stderr: {quiet_stderr}"
    );
}

// =========================================================================
// AIPM_LOG env var overrides CLI flags
// =========================================================================

#[test]
fn aipm_log_env_var_accepted() {
    let tmp = tempfile::tempdir().unwrap();
    // Setting AIPM_LOG=off should work without error
    aipm()
        .env("AIPM_LOG", "off")
        .args(["list", "--dir", tmp.path().to_str().unwrap()])
        .assert()
        .success();
}

// =========================================================================
// File log — verify a log file is created in the temp dir
// =========================================================================

#[test]
fn file_log_created_in_temp_dir() {
    let work_dir = tempfile::tempdir().unwrap();
    let log_dir = tempfile::tempdir().unwrap();

    // Point TMPDIR at an isolated directory so we can deterministically check
    aipm()
        .env("TMPDIR", log_dir.path())
        .env("TEMP", log_dir.path())
        .env("TMP", log_dir.path())
        .args(["-v", "list", "--dir", work_dir.path().to_str().unwrap()])
        .assert()
        .success();

    let has_log = std::fs::read_dir(log_dir.path()).unwrap().filter_map(|e| e.ok()).any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name.starts_with("aipm") && name.ends_with(".log")
    });

    assert!(has_log, "expected aipm*.log file in {}", log_dir.path().display());
}

// =========================================================================
// --log-format=json produces JSON on stderr (when there's output)
// =========================================================================

#[test]
fn json_format_produces_json_on_stderr_with_verbose() {
    let tmp = tempfile::tempdir().unwrap();

    // Write invalid TOML to trigger a WARN event so we guarantee JSON output
    std::fs::write(tmp.path().join("aipm.toml"), "not valid toml = [").unwrap();

    let output = aipm()
        .args(["--log-format", "json", "-v", "lint", tmp.path().to_str().unwrap()])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    let non_empty_lines: Vec<&str> =
        stderr.lines().map(str::trim).filter(|line| !line.is_empty()).collect();

    assert!(
        !non_empty_lines.is_empty(),
        "expected at least one JSON log line on stderr, got empty stderr"
    );

    for line in non_empty_lines {
        assert!(
            serde_json::from_str::<serde_json::Value>(line).is_ok(),
            "stderr line should be valid JSON: {line}"
        );
    }
}
