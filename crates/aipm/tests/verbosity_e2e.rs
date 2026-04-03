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
    Command::cargo_bin("aipm").expect("aipm binary should be built")
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
    let output =
        aipm().args(["-q", "list", "--dir", tmp.path().to_str().unwrap()]).output().unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("WARN"), "quiet mode should suppress warnings on stderr: {stderr}");
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
    let tmp = tempfile::tempdir().unwrap();
    aipm().args(["-v", "list", "--dir", tmp.path().to_str().unwrap()]).assert().success();

    // Check that at least one aipm*.log file exists in the system temp dir
    let temp_dir = std::env::temp_dir();
    let has_log = std::fs::read_dir(&temp_dir).unwrap().filter_map(|e| e.ok()).any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name.starts_with("aipm") && name.ends_with(".log")
    });

    assert!(has_log, "expected aipm*.log file in {}", temp_dir.display());
}

// =========================================================================
// --log-format=json produces JSON on stderr (when there's output)
// =========================================================================

#[test]
fn json_format_produces_json_on_stderr_with_verbose() {
    let tmp = tempfile::tempdir().unwrap();
    // Use -v to ensure at least some tracing output
    let output = aipm()
        .args(["--log-format", "json", "-v", "list", "--dir", tmp.path().to_str().unwrap()])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    // If there's any output, each line should be valid JSON
    for line in stderr.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        assert!(
            serde_json::from_str::<serde_json::Value>(trimmed).is_ok(),
            "stderr line should be valid JSON: {trimmed}"
        );
    }
}
