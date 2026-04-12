//! CLI integration tests for the `aipm` binary.
//!
//! Exercises the branches in `main.rs` via `assert_cmd`.

// Integration test crates inherit workspace lints. Relax restrictions that are
// appropriate for test code (unwrap, expect, panic are normal in tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use predicates::prelude::*;

fn aipm() -> assert_cmd::Command {
    Command::cargo_bin("aipm").expect("aipm binary should be built")
}

// =========================================================================
// `aipm` (no subcommand) — prints version + help hint
// =========================================================================

#[test]
fn no_subcommand_prints_version() {
    aipm().assert().success().stdout(predicate::str::contains("aipm"));
}

#[test]
fn no_subcommand_prints_help_hint() {
    aipm().assert().success().stdout(predicate::str::contains("--help"));
}

// =========================================================================
// `list` — no lockfile branch
// =========================================================================

#[test]
fn list_no_lockfile_shows_message() {
    let tmp = tempfile::tempdir().unwrap();
    aipm()
        .args(["list", "--dir", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("No lockfile found"));
}

// =========================================================================
// `list` — lockfile exists but is empty
// =========================================================================

#[test]
fn list_empty_lockfile_shows_no_packages() {
    let tmp = tempfile::tempdir().unwrap();

    // Write a minimal lockfile with zero packages
    let lockfile_content = r#"[metadata]
lockfile_version = 1
generated_by = "aipm-test"
"#;
    std::fs::write(tmp.path().join("aipm.lock"), lockfile_content).unwrap();

    aipm()
        .args(["list", "--dir", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("No packages installed"));
}

// =========================================================================
// `list --linked` — no link state file (empty entries)
// =========================================================================

#[test]
fn list_linked_no_overrides() {
    let tmp = tempfile::tempdir().unwrap();
    aipm()
        .args(["list", "--linked", "--dir", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("No active dev link overrides"));
}

// =========================================================================
// `list --dir .` — resolves current directory
// =========================================================================

#[test]
fn list_with_dot_dir_uses_cwd() {
    // --dir . should resolve to the current directory without error
    aipm().args(["list", "--dir", "."]).assert().success();
}

// =========================================================================
// `link` — missing aipm.toml in target (error path)
// =========================================================================

#[test]
fn link_missing_manifest_returns_error() {
    let tmp = tempfile::tempdir().unwrap();
    let no_manifest_dir = tmp.path().join("no-pkg");
    std::fs::create_dir_all(&no_manifest_dir).unwrap();

    aipm()
        .args(["link", no_manifest_dir.to_str().unwrap(), "--dir", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no aipm.toml found"));
}

// =========================================================================
// `link` — relative path argument (is_relative branch)
// =========================================================================

#[test]
fn link_relative_path_resolves_against_dir() {
    let tmp = tempfile::tempdir().unwrap();

    // Create a sub-directory that IS a valid package (has aipm.toml)
    let pkg_dir = tmp.path().join("my-plugin");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    let manifest_content =
        "[package]\nname = \"my-plugin\"\nversion = \"0.1.0\"\ndescription = \"test\"\n";
    std::fs::write(pkg_dir.join("aipm.toml"), manifest_content).unwrap();

    // Pass a relative path ("my-plugin") with an explicit --dir
    aipm()
        .args(["link", "my-plugin", "--dir", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Linked 'my-plugin'"));
}

// =========================================================================
// `unlink` — gitignore does not exist (no-op branch)
// =========================================================================

#[test]
fn unlink_no_gitignore_succeeds() {
    let tmp = tempfile::tempdir().unwrap();

    // Create directory structure that unlink_package expects to be tolerant of
    let links_dir = tmp.path().join(".aipm/links");
    let plugins_dir = tmp.path().join(".ai");
    std::fs::create_dir_all(&links_dir).unwrap();
    std::fs::create_dir_all(&plugins_dir).unwrap();

    // .ai/.gitignore deliberately NOT created → tests the `if gitignore_path.exists()` branch
    aipm()
        .args(["unlink", "nonexistent-pkg", "--dir", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Unlinked 'nonexistent-pkg'"));
}

// =========================================================================
// `unlink` — gitignore exists (remove_entry branch)
// =========================================================================

#[test]
fn unlink_with_gitignore_removes_entry() {
    let tmp = tempfile::tempdir().unwrap();

    let plugins_dir = tmp.path().join(".ai");
    std::fs::create_dir_all(&plugins_dir).unwrap();

    // Create .ai/.gitignore with a dummy entry
    std::fs::write(plugins_dir.join(".gitignore"), "some-pkg\n").unwrap();

    let links_dir = tmp.path().join(".aipm/links");
    std::fs::create_dir_all(&links_dir).unwrap();

    aipm()
        .args(["unlink", "some-pkg", "--dir", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Unlinked 'some-pkg'"));
}

// =========================================================================
// `install --registry` — registry warning branch
// =========================================================================

#[test]
fn install_registry_flag_emits_warning() {
    let tmp = tempfile::tempdir().unwrap();

    // Write a minimal manifest so install can at least start
    let manifest = "[package]\nname = \"test\"\nversion = \"0.1.0\"\n";
    std::fs::write(tmp.path().join("aipm.toml"), manifest).unwrap();

    // We expect the warning on stderr; install may fail (no deps), that's fine
    let output = aipm()
        .args(["install", "--registry", "my-reg", "--dir", tmp.path().to_str().unwrap()])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--registry is not yet supported"),
        "expected registry warning, got: {stderr}"
    );
}

// =========================================================================
// `list` — lockfile with packages shows them
// =========================================================================

#[test]
fn list_with_packages_shows_names() {
    let tmp = tempfile::tempdir().unwrap();

    // Write a lockfile that has one package entry
    let lockfile_content = r#"[metadata]
lockfile_version = 1
generated_by = "aipm-test"

[[package]]
name = "my-tool"
version = "1.2.3"
source = "git+https://example.com"
checksum = "sha512-abc"
dependencies = []
"#;
    std::fs::write(tmp.path().join("aipm.lock"), lockfile_content).unwrap();

    aipm()
        .args(["list", "--dir", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("my-tool@1.2.3"));
}

// =========================================================================
// `list --linked` — with entries shows them
// =========================================================================

#[test]
fn list_linked_with_entries_shows_them() {
    let tmp = tempfile::tempdir().unwrap();

    // Write a links.toml with one entry
    let links_dir = tmp.path().join(".aipm");
    std::fs::create_dir_all(&links_dir).unwrap();
    let links_toml = r#"# Managed by aipm
[[link]]
name = "dev-tool"
path = "/local/dev-tool"
linked_at = "2026-01-01T00:00:00Z"
"#;
    std::fs::write(links_dir.join("links.toml"), links_toml).unwrap();

    aipm()
        .args(["list", "--linked", "--dir", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("dev-tool"));
}

// =========================================================================
// `list --global` — no installed.json (empty registry branch)
// =========================================================================

#[test]
fn list_global_no_installed_plugins() {
    // Use an isolated HOME so no real ~/.aipm/installed.json is read.
    let tmp_home = tempfile::tempdir().unwrap();
    aipm()
        .args(["list", "--global"])
        .env("HOME", tmp_home.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No globally installed plugins."));
}

// =========================================================================
// `install --global` — installs a local plugin into the global registry
// =========================================================================

#[test]
fn install_global_local_plugin() {
    // Isolated HOME so the test writes to a temp ~/.aipm/installed.json.
    let tmp_home = tempfile::tempdir().unwrap();

    aipm()
        .args(["install", "--global", "local:./my-plugin"])
        .env("HOME", tmp_home.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed 'local:./my-plugin' globally"));

    // Verify the registry was persisted.
    let installed =
        std::fs::read_to_string(tmp_home.path().join(".aipm").join("installed.json")).unwrap();
    assert!(installed.contains("local:./my-plugin"), "installed.json should record the spec");
}

// =========================================================================
// `install --global` (re-install) — "Updated" branch when plugin already exists
// =========================================================================

#[test]
fn install_global_updates_existing_plugin() {
    let tmp_home = tempfile::tempdir().unwrap();

    // First install.
    aipm()
        .args(["install", "--global", "local:./my-plugin"])
        .env("HOME", tmp_home.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed 'local:./my-plugin' globally"));

    // Re-install the same spec — should print "Updated" instead of "Installed".
    aipm()
        .args(["install", "--global", "local:./my-plugin"])
        .env("HOME", tmp_home.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated 'local:./my-plugin' in global registry"));
}

// =========================================================================
// `list --global` — plugins present (non-empty registry branch)
// =========================================================================

#[test]
fn list_global_with_installed_plugins() {
    let tmp_home = tempfile::tempdir().unwrap();

    // Install two plugins so the registry is non-empty.
    aipm()
        .args(["install", "--global", "local:./plugin-a"])
        .env("HOME", tmp_home.path())
        .assert()
        .success();
    aipm()
        .args(["install", "--global", "--engine", "claude", "local:./plugin-b"])
        .env("HOME", tmp_home.path())
        .assert()
        .success();

    // List should show "Globally installed plugins:" header and both entries.
    aipm()
        .args(["list", "--global"])
        .env("HOME", tmp_home.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Globally installed plugins:"))
        .stdout(predicate::str::contains("local:./plugin-a"))
        .stdout(predicate::str::contains("all engines"))
        .stdout(predicate::str::contains("local:./plugin-b"))
        .stdout(predicate::str::contains("claude"));
}
