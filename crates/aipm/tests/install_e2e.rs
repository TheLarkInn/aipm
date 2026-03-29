//! E2E tests for `aipm install` using the fixtures in `fixtures/`.
//!
//! Each test copies a fixture directory to a temporary location, runs the
//! install command via `assert_cmd`, and verifies output and side effects
//! (lockfile content, directory links, etc.).

// Integration test crates inherit workspace lints. Relax restrictions that are
// appropriate for test code (unwrap, expect, panic are normal in tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

fn aipm() -> assert_cmd::Command {
    Command::cargo_bin("aipm").expect("aipm binary should be built")
}

/// Locate the workspace-level `fixtures/` directory relative to this test file.
fn fixtures_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    // crates/aipm/ -> repo root -> fixtures/
    manifest_dir.parent().expect("crates/").parent().expect("repo root").join("fixtures")
}

/// Copy an entire directory tree from `src` to `dst`.
fn copy_dir_recursive(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let file_type = entry.file_type().unwrap();
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            std::fs::copy(&src_path, &dst_path).unwrap();
        }
    }
}

/// Copy a named fixture to a temp directory and return the temp dir (kept alive
/// by the caller holding the `TempDir`).
fn setup_fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let fixture_src = fixtures_root().join(name);
    assert!(fixture_src.exists(), "fixture not found: {}", fixture_src.display());
    let dest = tmp.path().join(name);
    copy_dir_recursive(&fixture_src, &dest);
    (tmp, dest)
}

// =========================================================================
// workspace-transitive-deps: install resolves both direct + transitive deps
// =========================================================================

#[test]
fn install_workspace_transitive_deps() {
    let (_tmp, dir) = setup_fixture("workspace-transitive-deps");

    aipm()
        .args(["install", "--dir", dir.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed 2 package(s)"));

    // Lockfile should contain both packages
    let lockfile = std::fs::read_to_string(dir.join("aipm.lock")).unwrap();
    assert!(lockfile.contains("name = \"print-clock\""), "lockfile should contain print-clock");
    assert!(
        lockfile.contains("name = \"get-current-time\""),
        "lockfile should contain get-current-time"
    );
    assert!(
        lockfile.contains("source = \"workspace\""),
        "both packages should have workspace source"
    );

    // Transitive dependency recorded
    assert!(
        lockfile.contains("get-current-time"),
        "print-clock should list get-current-time as a dependency"
    );
}

// =========================================================================
// workspace-transitive-deps: second install is idempotent (up-to-date)
// =========================================================================

#[test]
fn install_workspace_transitive_deps_idempotent() {
    let (_tmp, dir) = setup_fixture("workspace-transitive-deps");

    // First install
    aipm()
        .args(["install", "--dir", dir.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed 2 package(s)"));

    // Second install — packages are already in place
    aipm()
        .args(["install", "--dir", dir.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed 2 package(s)"));
}

// =========================================================================
// workspace-no-deps: install with no root [dependencies] reports 0 packages
// =========================================================================

#[test]
fn install_workspace_no_deps() {
    let (_tmp, dir) = setup_fixture("workspace-no-deps");

    aipm()
        .args(["install", "--dir", dir.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed 0 package(s)"));

    // Lockfile should exist but be empty
    let lockfile = std::fs::read_to_string(dir.join("aipm.lock")).unwrap();
    assert!(!lockfile.contains("[[package]]"), "lockfile should have no packages");
}

// =========================================================================
// workspace-separate-plugins-dir: install creates junctions in plugins/
// =========================================================================

#[test]
fn install_workspace_separate_plugins_dir() {
    let (_tmp, dir) = setup_fixture("workspace-separate-plugins-dir");

    aipm()
        .args(["install", "--dir", dir.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed 2 package(s)"));

    // Verify plugins/ directory was created with links
    let plugins_dir = dir.join("plugins");
    assert!(plugins_dir.exists(), "plugins/ directory should be created");
    assert!(plugins_dir.join("greeter").exists(), "greeter should be linked in plugins/");
    assert!(plugins_dir.join("formatter").exists(), "formatter should be linked in plugins/");

    // Verify the links are actual directory links (junctions/symlinks), not copies
    assert!(
        libaipm::linker::directory_link::is_link(&plugins_dir.join("greeter")),
        "greeter should be a directory link"
    );
    assert!(
        libaipm::linker::directory_link::is_link(&plugins_dir.join("formatter")),
        "formatter should be a directory link"
    );

    // Verify we can read through the links
    assert!(
        plugins_dir.join("greeter/aipm.toml").exists(),
        "should read greeter/aipm.toml through link"
    );
    assert!(
        plugins_dir.join("formatter/skills/format/SKILL.md").exists(),
        "should read formatter skill through link"
    );

    // Lockfile should have both packages
    let lockfile = std::fs::read_to_string(dir.join("aipm.lock")).unwrap();
    assert!(lockfile.contains("name = \"greeter\""));
    assert!(lockfile.contains("name = \"formatter\""));
}

// =========================================================================
// standalone-plugin: install on a non-workspace package (no deps)
// =========================================================================

#[test]
fn install_standalone_plugin() {
    let (_tmp, dir) = setup_fixture("standalone-plugin");

    aipm()
        .args(["install", "--dir", dir.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed 0 package(s)"));
}

// =========================================================================
// workspace-transitive-deps: list after install shows packages
// =========================================================================

#[test]
fn list_after_install_shows_workspace_packages() {
    let (_tmp, dir) = setup_fixture("workspace-transitive-deps");

    // Install first
    aipm().args(["install", "--dir", dir.to_str().unwrap()]).assert().success();

    // List should show both packages
    aipm()
        .args(["list", "--dir", dir.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("print-clock"))
        .stdout(predicate::str::contains("get-current-time"));
}
