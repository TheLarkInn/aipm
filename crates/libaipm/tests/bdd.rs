//! BDD test harness — cucumber-rs step implementations for all `.feature` files.
//!
//! Steps implemented here execute the actual `aipm` and `aipm-pack` binaries
//! and verify their behavior against the Gherkin specifications.
//!
//! Scenarios with no matching step implementation are reported as **skipped**,
//! giving a clear progress view of what's wired up vs. pending.

// cucumber-rs requires async fn + &mut World signatures; relax lints accordingly.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unused_async,
    clippy::needless_pass_by_ref_mut,
    clippy::option_if_let_else,
    clippy::indexing_slicing,
    clippy::branches_sharing_code,
    clippy::used_underscore_binding,
    clippy::no_effect_underscore_binding
)]

use std::collections::HashMap;
use std::path::PathBuf;

use assert_cmd::Command;
use cucumber::{given, then, when, World};
use libaipm::version;

// =========================================================================
// World — shared state across steps in a single scenario
// =========================================================================

#[derive(Debug, Default, World)]
pub struct AipmWorld {
    /// Root temp directory for this scenario.
    root: Option<tempfile::TempDir>,
    /// Named directories within root.
    dirs: HashMap<String, PathBuf>,
    /// Most recent command stdout.
    last_stdout: String,
    /// Most recent command stderr.
    last_stderr: String,
    /// Most recent command exit code.
    last_exit_code: Option<i32>,
    /// Current manifest content.
    manifest_content: Option<String>,
    /// Active directory name for the scenario.
    active_dir: Option<String>,
    /// Parsed version requirement (for versioning scenarios).
    version_req: Option<version::Requirement>,
    /// Validation result for version scenarios.
    validation_errors: Vec<String>,
    /// Registry version candidates (for resolution scenarios).
    registry_versions: Vec<version::Version>,
    /// Selected version from resolution.
    selected_version: Option<version::Version>,
}

impl AipmWorld {
    fn root_path(&self) -> &std::path::Path {
        self.root.as_ref().expect("root tempdir").path()
    }

    fn ensure_root(&mut self) {
        if self.root.is_none() {
            self.root = Some(tempfile::TempDir::new().expect("create tempdir"));
        }
    }

    fn dir_path(&self, name: &str) -> PathBuf {
        self.dirs.get(name).cloned().unwrap_or_else(|| self.root_path().join(name))
    }

    fn active_dir_path(&self) -> PathBuf {
        match &self.active_dir {
            Some(name) => self.dir_path(name),
            None => self.root_path().to_path_buf(),
        }
    }

    fn read_manifest(&self) -> String {
        let dir = self.active_dir_path();
        std::fs::read_to_string(dir.join("aipm.toml"))
            .unwrap_or_else(|e| panic!("read aipm.toml in {}: {e}", dir.display()))
    }
}

// =========================================================================
// Helpers
// =========================================================================

fn run_command(world: &mut AipmWorld, full_cmd: &str, working_dir: Option<&str>) {
    let parts: Vec<&str> = full_cmd.split_whitespace().collect();
    assert!(!parts.is_empty(), "empty command");

    let binary = parts[0];
    let args = &parts[1..];

    let cwd = match working_dir {
        Some(dir) => world.dir_path(dir),
        None => world.active_dir_path(),
    };

    let mut cmd = Command::cargo_bin(binary)
        .unwrap_or_else(|e| panic!("cargo bin '{binary}' not found: {e}"));

    // For "aipm-pack init" in a dir, pass the dir as the positional arg
    if binary == "aipm-pack" && args.first() == Some(&"init") && working_dir.is_some() {
        cmd.args(args);
        cmd.arg(cwd.to_str().unwrap());
    } else {
        cmd.args(args);
        cmd.current_dir(&cwd);
    }

    let output = cmd.output().expect("execute command");
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.last_exit_code = output.status.code();
}

// =========================================================================
// GIVEN — setup steps
// =========================================================================

#[given(expr = "an empty directory {string}")]
async fn given_empty_dir(world: &mut AipmWorld, name: String) {
    world.ensure_root();
    let path = world.root_path().join(&name);
    std::fs::create_dir_all(&path).expect("create dir");
    world.dirs.insert(name.clone(), path);
    world.active_dir = Some(name);
}

#[given(expr = "a directory {string} containing an {string}")]
async fn given_dir_with_file(world: &mut AipmWorld, dir: String, file: String) {
    world.ensure_root();
    let path = world.root_path().join(&dir);
    std::fs::create_dir_all(&path).expect("create dir");
    std::fs::write(path.join(&file), "[package]\nname = \"existing\"\nversion = \"0.1.0\"\n")
        .expect("write file");
    world.dirs.insert(dir.clone(), path);
    world.active_dir = Some(dir);
}

#[given(expr = "a plugin directory {string} with a valid manifest")]
async fn given_plugin_dir_valid_manifest(world: &mut AipmWorld, name: String) {
    world.ensure_root();
    let path = world.root_path().join(&name);
    std::fs::create_dir_all(&path).expect("create dir");
    let manifest = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\ntype = \"composite\"\nedition = \"2024\"\n"
    );
    std::fs::write(path.join("aipm.toml"), &manifest).expect("write manifest");
    world.manifest_content = Some(manifest);
    world.dirs.insert(name.clone(), path);
    world.active_dir = Some(name);
}

#[given(expr = "the manifest is missing the {string} field")]
async fn given_manifest_missing_field(world: &mut AipmWorld, field: String) {
    let dir = world.active_dir_path();
    let manifest = match field.as_str() {
        "name" => "[package]\nversion = \"0.1.0\"\n".to_string(),
        "version" => "[package]\nname = \"test-plugin\"\n".to_string(),
        _ => panic!("unknown field: {field}"),
    };
    std::fs::write(dir.join("aipm.toml"), &manifest).expect("write manifest");
    world.manifest_content = Some(manifest);
}

#[given(expr = "the manifest version is {string}")]
async fn given_manifest_version(world: &mut AipmWorld, version: String) {
    let dir = world.active_dir_path();
    let manifest = format!("[package]\nname = \"test-plugin\"\nversion = \"{version}\"\n");
    std::fs::write(dir.join("aipm.toml"), &manifest).expect("write manifest");
    world.manifest_content = Some(manifest);
}

#[given(expr = "the manifest declares a dependency {string} with version {string}")]
async fn given_manifest_dep(world: &mut AipmWorld, dep: String, version: String) {
    let dir = world.active_dir_path();
    let manifest = format!(
        "[package]\nname = \"test-plugin\"\nversion = \"0.1.0\"\n\n[dependencies]\n{dep} = \"{version}\"\n"
    );
    std::fs::write(dir.join("aipm.toml"), &manifest).expect("write manifest");
    world.manifest_content = Some(manifest);
}

#[given(expr = "the manifest declares a skill at {string}")]
async fn given_manifest_skill(world: &mut AipmWorld, skill_path: String) {
    let dir = world.active_dir_path();
    let manifest = format!(
        "[package]\nname = \"test-plugin\"\nversion = \"0.1.0\"\n\n[components]\nskills = [\"{skill_path}\"]\n"
    );
    std::fs::write(dir.join("aipm.toml"), &manifest).expect("write manifest");
    world.manifest_content = Some(manifest);
}

#[given(expr = "the file {string} does not exist")]
async fn given_file_not_exist(_world: &mut AipmWorld, _path: String) {
    // No-op — file doesn't exist by default
}

#[given(expr = "the manifest has type {string}")]
async fn given_manifest_type(world: &mut AipmWorld, plugin_type: String) {
    let dir = world.active_dir_path();
    let manifest = format!(
        "[package]\nname = \"test-plugin\"\nversion = \"0.1.0\"\ntype = \"{plugin_type}\"\n"
    );
    std::fs::write(dir.join("aipm.toml"), &manifest).expect("write manifest");
    world.manifest_content = Some(manifest);
}

// --- Versioning steps ---

#[given(expr = "a manifest with version {string}")]
async fn given_manifest_with_version(world: &mut AipmWorld, ver: String) {
    world.ensure_root();
    let dir = world.active_dir_path();
    std::fs::create_dir_all(&dir).ok();
    let manifest = format!("[package]\nname = \"test-plugin\"\nversion = \"{ver}\"\n");
    std::fs::write(dir.join("aipm.toml"), &manifest).expect("write manifest");
    world.manifest_content = Some(manifest);
}

#[given(expr = "a dependency with version requirement {string}")]
async fn given_dep_version_req(world: &mut AipmWorld, req: String) {
    match version::Requirement::parse(&req) {
        Ok(r) => world.version_req = Some(r),
        Err(_) => world.validation_errors.push(format!("invalid requirement: {req}")),
    }
}

#[given(expr = "the registry contains versions {string}, {string}, {string}")]
async fn given_registry_versions_3(world: &mut AipmWorld, v1: String, v2: String, v3: String) {
    world.registry_versions.clear();
    for v in [&v1, &v2, &v3] {
        if let Ok(parsed) = version::Version::parse(v) {
            world.registry_versions.push(parsed);
        }
    }
}

// =========================================================================
// WHEN — command execution
// =========================================================================

#[when(expr = "the manifest is validated")]
async fn when_manifest_validated(world: &mut AipmWorld) {
    let dir = world.active_dir_path();
    let manifest_path = dir.join("aipm.toml");
    let content = std::fs::read_to_string(&manifest_path).expect("read manifest");
    match libaipm::manifest::parse_and_validate(&content, Some(&dir)) {
        Ok(_) => world.validation_errors.clear(),
        Err(e) => world.validation_errors.push(e.to_string()),
    }
}

#[when(expr = "the requirement is parsed")]
async fn when_requirement_parsed(_world: &mut AipmWorld) {
    // Parsing already happened in the given step; this is a no-op.
}

#[when(expr = "dependencies are resolved")]
async fn when_deps_resolved(world: &mut AipmWorld) {
    if let Some(req) = &world.version_req {
        world.selected_version = req.select_best(&world.registry_versions).cloned();
    }
}

#[when(expr = "the user runs {string} in {string}")]
async fn when_run_in_dir(world: &mut AipmWorld, cmd: String, dir: String) {
    run_command(world, &cmd, Some(&dir));
}

#[when(expr = "the user runs {string}")]
async fn when_run(world: &mut AipmWorld, cmd: String) {
    run_command(world, &cmd, None);
}

// =========================================================================
// THEN — assertions
// =========================================================================

#[then(expr = "the version is accepted")]
async fn then_version_accepted(world: &mut AipmWorld) {
    assert!(
        world.validation_errors.is_empty(),
        "expected version to be accepted but got errors: {:?}",
        world.validation_errors
    );
}

#[then(expr = "the version is rejected with {string}")]
async fn then_version_rejected(world: &mut AipmWorld, expected_msg: String) {
    assert!(
        !world.validation_errors.is_empty(),
        "expected version to be rejected with '{expected_msg}' but no errors"
    );
    let combined = world.validation_errors.join("; ");
    assert!(
        combined.contains(&expected_msg),
        "expected '{expected_msg}' in errors, got: {combined}"
    );
}

#[then(expr = "it matches version {string}")]
async fn then_matches_version(world: &mut AipmWorld, ver: String) {
    let req = world.version_req.as_ref().expect("version requirement set");
    let version = version::Version::parse(&ver).expect("valid version");
    assert!(req.matches(&version), "expected requirement '{req}' to match '{ver}'");
}

#[then(expr = "it does not match version {string}")]
async fn then_does_not_match_version(world: &mut AipmWorld, ver: String) {
    if ver.is_empty() {
        return; // wildcard * has no no_match example
    }
    let req = world.version_req.as_ref().expect("version requirement set");
    let version = version::Version::parse(&ver).expect("valid version");
    assert!(!req.matches(&version), "expected requirement '{req}' to NOT match '{ver}'");
}

#[then(expr = "version {string} is selected")]
async fn then_version_selected(world: &mut AipmWorld, expected: String) {
    let selected = world.selected_version.as_ref().expect("a version was selected");
    assert_eq!(
        selected.to_string(),
        expected,
        "expected '{expected}' to be selected, got '{selected}'"
    );
}

#[then(expr = "version {string} is not considered")]
async fn then_version_not_considered(world: &mut AipmWorld, excluded: String) {
    let selected = world.selected_version.as_ref().map(ToString::to_string);
    assert_ne!(
        selected.as_deref(),
        Some(excluded.as_str()),
        "version '{excluded}' should not have been selected"
    );
}

#[then(expr = "the command succeeds")]
async fn then_succeeds(world: &mut AipmWorld) {
    assert_eq!(
        world.last_exit_code,
        Some(0),
        "expected success but got {:?}\nstdout: {}\nstderr: {}",
        world.last_exit_code,
        world.last_stdout,
        world.last_stderr
    );
}

#[then(expr = "the command fails with {string}")]
async fn then_fails_with(world: &mut AipmWorld, msg: String) {
    assert_ne!(
        world.last_exit_code,
        Some(0),
        "expected failure but succeeded\nstdout: {}\nstderr: {}",
        world.last_stdout,
        world.last_stderr
    );
    let combined = format!("{}{}", world.last_stdout, world.last_stderr);
    assert!(
        combined.contains(&msg),
        "expected '{msg}' in output\nstdout: {}\nstderr: {}",
        world.last_stdout,
        world.last_stderr
    );
}

#[then(expr = "no warnings are emitted")]
async fn then_no_warnings(world: &mut AipmWorld) {
    assert!(
        !world.last_stderr.to_lowercase().contains("warning"),
        "unexpected warnings: {}",
        world.last_stderr
    );
}

#[then(expr = "a file {string} is created in {string}")]
async fn then_file_created(world: &mut AipmWorld, file: String, dir: String) {
    let path = world.dir_path(&dir).join(&file);
    assert!(path.exists(), "expected {} to exist", path.display());
}

#[then(expr = "the manifest contains the directory name {string} as the package name")]
async fn then_manifest_has_dir_name(world: &mut AipmWorld, name: String) {
    let content = world.read_manifest();
    let expected = format!("name = \"{name}\"");
    assert!(content.contains(&expected), "expected '{expected}' in manifest\ngot: {content}");
}

#[then(expr = "the manifest contains a version of {string}")]
async fn then_manifest_has_version(world: &mut AipmWorld, version: String) {
    let content = world.read_manifest();
    let expected = format!("version = \"{version}\"");
    assert!(content.contains(&expected), "expected '{expected}'\ngot: {content}");
}

#[then(expr = "the manifest contains an edition field")]
async fn then_manifest_has_edition(world: &mut AipmWorld) {
    let content = world.read_manifest();
    assert!(content.contains("edition"), "expected 'edition' in manifest\ngot: {content}");
}

#[then(expr = "the manifest contains the package name {string}")]
async fn then_manifest_has_name(world: &mut AipmWorld, name: String) {
    let content = world.read_manifest();
    let expected = format!("name = \"{name}\"");
    assert!(content.contains(&expected), "expected '{expected}'\ngot: {content}");
}

#[then(expr = "the manifest contains the plugin type {string}")]
async fn then_manifest_has_type(world: &mut AipmWorld, plugin_type: String) {
    let content = world.read_manifest();
    let expected = format!("type = \"{plugin_type}\"");
    assert!(content.contains(&expected), "expected '{expected}'\ngot: {content}");
}

#[then(expr = "a file {string} exists in {string}")]
async fn then_file_exists_in(world: &mut AipmWorld, file: String, dir: String) {
    let path = world.dir_path(&dir).join(&file);
    assert!(path.exists(), "expected {} to exist", path.display());
}

#[then(expr = "a starter template for {string} is created")]
async fn then_starter_template(world: &mut AipmWorld, plugin_type: String) {
    let dir = world.active_dir_path();
    match plugin_type.as_str() {
        "skill" => assert!(dir.join("skills/default/SKILL.md").exists()),
        "agent" => assert!(dir.join("agents").is_dir()),
        "mcp" => assert!(dir.join("mcp").is_dir()),
        "hook" => assert!(dir.join("hooks").is_dir()),
        "composite" => {
            assert!(dir.join("skills").is_dir());
            assert!(dir.join("agents").is_dir());
            assert!(dir.join("hooks").is_dir());
        },
        _ => panic!("unknown type: {plugin_type}"),
    }
}

#[then(expr = "the error message explains the naming rules")]
async fn then_error_explains_naming(world: &mut AipmWorld) {
    let combined = format!("{}{}", world.last_stdout, world.last_stderr);
    assert!(
        combined.contains("lowercase") || combined.contains("alphanumeric"),
        "error should explain naming rules\ngot: {combined}"
    );
}

#[then(expr = "all declared component paths are verified to exist")]
async fn then_components_verified(_world: &mut AipmWorld) {
    // Command success = validation passed
}

// =========================================================================
// Workspace init — directory/file existence steps
// =========================================================================

#[then(expr = "the following directories exist in {string}:")]
async fn then_directories_exist(
    world: &mut AipmWorld,
    dir: String,
    step: &cucumber::gherkin::Step,
) {
    let base = world.dir_path(&dir);
    if let Some(table) = step.table.as_ref() {
        for row in &table.rows {
            if let Some(cell) = row.first() {
                // Skip header row
                if cell == "directory" {
                    continue;
                }
                let path = base.join(cell);
                assert!(path.is_dir(), "expected directory {} to exist", path.display());
            }
        }
    }
}

#[then(expr = "there is no directory {string} in {string}")]
async fn then_no_directory(world: &mut AipmWorld, subdir: String, dir: String) {
    let path = world.dir_path(&dir).join(&subdir);
    assert!(!path.exists(), "expected {} to NOT exist", path.display());
}

#[then(expr = "there is no file {string} in {string}")]
async fn then_no_file(world: &mut AipmWorld, file: String, dir: String) {
    let path = world.dir_path(&dir).join(&file);
    assert!(!path.exists(), "expected {} to NOT exist", path.display());
}

#[given(expr = "a directory {string} exists")]
async fn given_directory_exists(world: &mut AipmWorld, dir_path: String) {
    let full_path = world.root_path().join(&dir_path);
    std::fs::create_dir_all(&full_path).expect("create directory");
}

// =========================================================================
// Workspace init — starter plugin manifest steps
// =========================================================================

#[then(expr = "the starter plugin manifest contains the package name {string}")]
async fn then_starter_manifest_name(world: &mut AipmWorld, name: String) {
    let dir = world.active_dir_path();
    let content = std::fs::read_to_string(dir.join(".ai/starter-aipm-plugin/aipm.toml"))
        .expect("read starter manifest");
    let expected = format!("name = \"{name}\"");
    assert!(content.contains(&expected), "expected '{expected}'\ngot: {content}");
}

#[then(expr = "the starter plugin manifest contains a version of {string}")]
async fn then_starter_manifest_version(world: &mut AipmWorld, version: String) {
    let dir = world.active_dir_path();
    let content = std::fs::read_to_string(dir.join(".ai/starter-aipm-plugin/aipm.toml"))
        .expect("read starter manifest");
    let expected = format!("version = \"{version}\"");
    assert!(content.contains(&expected), "expected '{expected}'\ngot: {content}");
}

#[then(expr = "the starter plugin manifest contains the plugin type {string}")]
async fn then_starter_manifest_type(world: &mut AipmWorld, plugin_type: String) {
    let dir = world.active_dir_path();
    let content = std::fs::read_to_string(dir.join(".ai/starter-aipm-plugin/aipm.toml"))
        .expect("read starter manifest");
    let expected = format!("type = \"{plugin_type}\"");
    assert!(content.contains(&expected), "expected '{expected}'\ngot: {content}");
}

#[then("the starter plugin manifest is valid according to aipm schema")]
async fn then_starter_manifest_valid(world: &mut AipmWorld) {
    let dir = world.active_dir_path();
    let starter_dir = dir.join(".ai/starter-aipm-plugin");
    let content =
        std::fs::read_to_string(starter_dir.join("aipm.toml")).expect("read starter manifest");
    let result = libaipm::manifest::parse_and_validate(&content, Some(&starter_dir));
    assert!(result.is_ok(), "starter manifest should be valid: {result:?}");
}

#[then(expr = "the starter skill contains {string} in the frontmatter")]
async fn then_starter_skill_frontmatter(world: &mut AipmWorld, expected: String) {
    let dir = world.active_dir_path();
    let content = std::fs::read_to_string(
        dir.join(".ai/starter-aipm-plugin/skills/scaffold-plugin/SKILL.md"),
    )
    .expect("read starter skill");
    assert!(content.contains(&expected), "expected '{expected}' in skill\ngot: {content}");
}

// =========================================================================
// Workspace init — marketplace.json steps
// =========================================================================

fn read_marketplace_json(world: &AipmWorld) -> serde_json::Value {
    let dir = world.active_dir_path();
    let content = std::fs::read_to_string(dir.join(".ai/.claude-plugin/marketplace.json"))
        .expect("read marketplace.json");
    serde_json::from_str(&content).expect("parse marketplace.json")
}

#[then(expr = "the marketplace.json name is {string}")]
async fn then_marketplace_name(world: &mut AipmWorld, expected: String) {
    let v = read_marketplace_json(world);
    assert_eq!(v["name"], expected, "marketplace name mismatch");
}

#[then(expr = "the marketplace.json contains a plugin named {string}")]
async fn then_marketplace_has_plugin(world: &mut AipmWorld, name: String) {
    let v = read_marketplace_json(world);
    let plugins = v["plugins"].as_array().expect("plugins is array");
    let found = plugins.iter().any(|p| p["name"] == name);
    assert!(found, "expected plugin '{name}' in marketplace.json plugins");
}

#[then(expr = "the marketplace.json plugin {string} has source {string}")]
async fn then_marketplace_plugin_source(world: &mut AipmWorld, name: String, source: String) {
    let v = read_marketplace_json(world);
    let plugins = v["plugins"].as_array().expect("plugins is array");
    let plugin = plugins.iter().find(|p| p["name"] == name);
    assert!(plugin.is_some(), "plugin '{name}' not found in marketplace.json");
    assert_eq!(plugin.unwrap()["source"], source, "source mismatch for plugin '{name}'");
}

#[then("the marketplace.json plugins array is empty")]
async fn then_marketplace_plugins_empty(world: &mut AipmWorld) {
    let v = read_marketplace_json(world);
    let plugins = v["plugins"].as_array().expect("plugins is array");
    assert!(plugins.is_empty(), "expected empty plugins array, got {}", plugins.len());
}

// =========================================================================
// Workspace init — Claude settings.json steps
// =========================================================================

fn read_claude_settings(world: &AipmWorld) -> serde_json::Value {
    let dir = world.active_dir_path();
    let content =
        std::fs::read_to_string(dir.join(".claude/settings.json")).expect("read claude settings");
    serde_json::from_str(&content).expect("parse claude settings")
}

#[then(expr = "the Claude settings contain {string}")]
async fn then_claude_settings_contain(world: &mut AipmWorld, expected: String) {
    let dir = world.active_dir_path();
    let content =
        std::fs::read_to_string(dir.join(".claude/settings.json")).expect("read claude settings");
    assert!(content.contains(&expected), "expected '{expected}' in settings.json\ngot: {content}");
}

#[then(expr = "the Claude settings marketplace path is {string}")]
async fn then_claude_settings_path(world: &mut AipmWorld, expected: String) {
    let v = read_claude_settings(world);
    let path = &v["extraKnownMarketplaces"]["local-repo-plugins"]["source"]["path"];
    assert_eq!(path, &expected, "marketplace path mismatch");
}

#[then(expr = "the Claude settings contain {string} at the top level")]
async fn then_claude_settings_top_level(world: &mut AipmWorld, key: String) {
    let v = read_claude_settings(world);
    assert!(v.get(&key).is_some(), "expected top-level key '{key}' in settings.json");
}

#[then(expr = "the Claude settings enable {string}")]
async fn then_claude_settings_enable(world: &mut AipmWorld, plugin_key: String) {
    let v = read_claude_settings(world);
    assert_eq!(
        v["enabledPlugins"][&plugin_key], true,
        "expected enabledPlugins['{plugin_key}'] to be true"
    );
}

#[then("the Claude settings file preserves the custom content")]
async fn then_claude_settings_preserves_custom(world: &mut AipmWorld) {
    let v = read_claude_settings(world);
    assert!(v.get("custom").is_some(), "expected 'custom' key to be preserved");
    assert!(v.get("permissions").is_some(), "expected 'permissions' key to be preserved");
}

#[given(expr = "a file {string} with custom content exists in {string}")]
async fn given_file_with_custom_content(world: &mut AipmWorld, file: String, dir: String) {
    let base = world.dir_path(&dir);
    let path = base.join(&file);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
    std::fs::write(&path, "{\"custom\": true, \"permissions\": {\"allow\": [\"Read\"]}}")
        .expect("write custom content");
}

// =========================================================================
// Workspace init — workspace manifest and gitignore steps
// =========================================================================

#[then(expr = "the manifest contains a {string} section")]
async fn then_manifest_contains_section(world: &mut AipmWorld, section: String) {
    let content = world.read_manifest();
    assert!(content.contains(&section), "expected '{section}' in manifest\ngot: {content}");
}

#[then(expr = "the manifest contains members {string}")]
async fn then_manifest_members(world: &mut AipmWorld, members: String) {
    let content = world.read_manifest();
    let expected = format!("members = [\"{members}\"]");
    assert!(content.contains(&expected), "expected '{expected}'\ngot: {content}");
}

#[then(expr = "the manifest contains plugins_dir {string}")]
async fn then_manifest_plugins_dir(world: &mut AipmWorld, plugins_dir: String) {
    let content = world.read_manifest();
    let expected = format!("plugins_dir = \"{plugins_dir}\"");
    assert!(content.contains(&expected), "expected '{expected}'\ngot: {content}");
}

#[then(expr = "the gitignore contains {string}")]
async fn then_gitignore_contains(world: &mut AipmWorld, expected: String) {
    let dir = world.active_dir_path();
    let content = std::fs::read_to_string(dir.join(".ai/.gitignore")).expect("read .ai/.gitignore");
    assert!(content.contains(&expected), "expected '{expected}' in .gitignore\ngot: {content}");
}

// =========================================================================
// Main
// =========================================================================

fn main() {
    // Only run feature files with fully wired step implementations.
    // validation.feature requires `aipm validate` (not yet implemented).
    // See GitHub issues for enabling remaining feature files and directories.
    let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/features/manifest");
    futures::executor::block_on(AipmWorld::cucumber().with_default_cli().filter_run(
        base,
        |feat, _, _| {
            let name = feat
                .path
                .as_deref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            matches!(
                name.as_ref(),
                "init.feature" | "versioning.feature" | "workspace-init.feature"
            )
        },
    ));
}
