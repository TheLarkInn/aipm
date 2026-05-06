//! Rule: `valid-tool-name` — tool references incompatible with declared engines.
//!
//! For each tool name listed in an Agent / Skill / Hook frontmatter `tools`
//! field, look the name up in the schema-driven
//! [`libaipm_engine_spec::TOOL_COMPATIBILITY`] table via
//! [`libaipm_engine_spec::valid_tool_name_check`]. If the helper returns
//! `Err`, emit a [`Diagnostic`].
//!
//! Severity is decided at the call site based on whether the plugin's
//! `aipm.toml` declared any engines:
//!   * Empty `[engines]` (or missing manifest) → [`Severity::Warning`] —
//!     suggests declaring the engines that DO support the tool.
//!   * Non-empty `[engines]` that doesn't intersect the tool's support set
//!     → [`Severity::Error`].

use std::path::{Path, PathBuf};

use libaipm_engine_spec::{valid_tool_name_check, Engine, EngineSet, ToolNameViolation};

use crate::fs::Fs;
use crate::lint::diagnostic::{Diagnostic, Severity};
use crate::lint::rule::Rule;
use crate::lint::Error;

/// Rule struct for `valid-tool-name`.
pub struct ValidToolName;

impl Rule for ValidToolName {
    fn id(&self) -> &'static str {
        "valid-tool-name"
    }

    fn name(&self) -> &'static str {
        "valid tool name for declared engines"
    }

    /// Default reported when the rule itself is queried via the catalog;
    /// individual diagnostics override this based on the declared-engines
    /// state at the call site.
    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn help_url(&self) -> Option<&'static str> {
        Some("https://github.com/TheLarkInn/aipm/blob/main/docs/rules/valid-tool-name.md")
    }

    fn help_text(&self) -> Option<&'static str> {
        Some("declare engines in aipm.toml that support the referenced tools")
    }

    fn check_file(&self, file_path: &Path, fs: &dyn Fs) -> Result<Vec<Diagnostic>, Error> {
        // Live invocations route through `check_file_in` (overridden below)
        // and pass an explicit `lint_dir`. This `check_file` path is reached
        // only by direct test callers and the catalog query; an empty
        // `lint_dir` means "no upper bound on the parent walk", which
        // preserves pre-#793 behaviour for those callers.
        self.check_file_in(file_path, Path::new(""), fs)
    }

    fn check_file_in(
        &self,
        file_path: &Path,
        lint_dir: &Path,
        fs: &dyn Fs,
    ) -> Result<Vec<Diagnostic>, Error> {
        let Ok(content) = fs.read_to_string(file_path) else {
            return Ok(vec![]);
        };
        let Some(fm) = crate::frontmatter::parse(&content).ok().flatten() else {
            return Ok(vec![]);
        };
        let Some(tools_value) = fm.fields.get("tools") else {
            return Ok(vec![]);
        };

        let declared = nearest_declared_engines(file_path, lint_dir, fs);
        let source_type = super::scan::source_type_from_path(file_path).to_string();
        let tools_line = fm.field_lines.get("tools").copied();

        let mut diagnostics = Vec::new();
        for tool in parse_tools(tools_value) {
            if let Err(violation) = valid_tool_name_check(tool, declared) {
                diagnostics.push(make_diagnostic(
                    self.id(),
                    tool,
                    violation,
                    file_path,
                    &source_type,
                    tools_line,
                ));
            }
        }
        Ok(diagnostics)
    }
}

/// Split a `tools:` frontmatter value into individual tool names.
///
/// Accepts both comma-separated lists (`"Read, Write"`) and whitespace-only
/// separation (`"Read Write"`); empty fragments are filtered out.
fn parse_tools(value: &str) -> Vec<&str> {
    value
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Build a single diagnostic for one bad tool name.
fn make_diagnostic(
    rule_id: &str,
    tool: &str,
    violation: ToolNameViolation,
    file_path: &Path,
    source_type: &str,
    tools_line: Option<usize>,
) -> Diagnostic {
    let supported_names = engine_names(violation.supported_by);
    let (severity, message) = if violation.declared.is_empty() {
        let supported_list = format_engine_list(&supported_names);
        let toml_array = format_toml_string_array(&supported_names);
        (
            Severity::Warning,
            format!(
                "Tool '{tool}' is exclusive to {supported_list}; consider declaring engines = {toml_array} in aipm.toml."
            ),
        )
    } else {
        (Severity::Error, format!("Tool '{tool}' is not supported by any of the declared engines."))
    };
    Diagnostic {
        rule_id: rule_id.to_string(),
        severity,
        message,
        file_path: file_path.to_path_buf(),
        line: tools_line,
        col: Some(1),
        end_line: tools_line,
        end_col: None,
        source_type: source_type.to_string(),
        help_text: None,
        help_url: None,
    }
}

/// Collect the kebab-case names of every engine in `set`.
fn engine_names(set: EngineSet) -> Vec<&'static str> {
    Engine::ALL
        .iter()
        .filter_map(|e| {
            let bit = match e {
                Engine::Claude => EngineSet::CLAUDE,
                Engine::Copilot => EngineSet::COPILOT,
            };
            if set.contains(bit) {
                Some(e.name())
            } else {
                None
            }
        })
        .collect()
}

/// Format engine names as a human-readable comma list.
fn format_engine_list(names: &[&'static str]) -> String {
    let mut out = String::new();
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(name);
    }
    out
}

/// Format engine names as a TOML string array literal (e.g. `["claude"]`).
fn format_toml_string_array(names: &[&'static str]) -> String {
    let mut out = String::from("[");
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push('"');
        out.push_str(name);
        out.push('"');
    }
    out.push(']');
    out
}

/// Walk up from `file_path` looking for the nearest `aipm.toml` and return
/// the `EngineSet` declared via [`crate::manifest::effective_engines`].
///
/// Honors workspace-level inheritance: when the nearest manifest's
/// `[package].engines` is omitted but `[workspace].engines` is set, the
/// workspace value drives the lint (Spec G7).
///
/// Returns [`EngineSet::empty()`] when no manifest is found, the manifest
/// cannot be read or parsed, or the resolved engines are `None` /
/// `Some(EngineSet::empty())`.
fn nearest_declared_engines(file_path: &Path, lint_dir: &Path, fs: &dyn Fs) -> EngineSet {
    let Some(manifest_path) = find_nearest_manifest(file_path, lint_dir, fs) else {
        return EngineSet::empty();
    };
    let Ok(content) = fs.read_to_string(&manifest_path) else {
        return EngineSet::empty();
    };
    let Ok(manifest) = crate::manifest::parse(&content) else {
        return EngineSet::empty();
    };
    crate::manifest::effective_engines(manifest.package.as_ref(), manifest.workspace.as_ref())
        .unwrap_or_else(EngineSet::empty)
}

/// Walk parent directories looking for `aipm.toml`.
///
/// Returns the first existing `aipm.toml` encountered moving from the
/// feature file toward the filesystem root. The walk stops at `lint_dir`
/// (inclusive) — a manifest *at* `lint_dir/aipm.toml` is found, but the
/// walk does not ascend above it. An empty `lint_dir` (the convention
/// passed by the legacy `check_file` callers) disables the cap.
///
/// Issue #793 Finding 2 / spec §5.1.3 (sub-option A): without the cap,
/// the walk would terminate only at the filesystem root, allowing a
/// PR-author-controlled feature placed near the root to draw declared
/// engines from an `aipm.toml` outside the linted workspace.
fn find_nearest_manifest(file_path: &Path, lint_dir: &Path, fs: &dyn Fs) -> Option<PathBuf> {
    let cap_enabled = !lint_dir.as_os_str().is_empty();
    let mut current = file_path.parent();
    while let Some(dir) = current {
        let candidate = dir.join("aipm.toml");
        if fs.exists(&candidate) {
            return Some(candidate);
        }
        if cap_enabled && dir == lint_dir {
            return None;
        }
        current = dir.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::rules::test_helpers::MockFs;
    use std::path::PathBuf;

    fn agent_path() -> PathBuf {
        PathBuf::from(".ai/p/agents/reviewer.md")
    }

    fn manifest_path() -> PathBuf {
        PathBuf::from(".ai/p/aipm.toml")
    }

    fn add_agent_with_tools(fs: &mut MockFs, tools_field: &str) {
        let path = agent_path();
        fs.exists.insert(path.clone());
        fs.files.insert(path, format!("---\nname: reviewer\ntools: {tools_field}\n---\nPrompt"));
    }

    fn add_manifest(fs: &mut MockFs, body: &str) {
        let path = manifest_path();
        fs.exists.insert(path.clone());
        fs.files.insert(path, body.to_string());
    }

    #[test]
    fn shared_tool_always_clean_no_manifest() {
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "bash");
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn shared_tool_always_clean_with_manifest() {
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "bash");
        add_manifest(
            &mut fs,
            "[package]\nname = \"p\"\nversion = \"1.0.0\"\nengines = [\"claude\"]\n",
        );
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn undeclared_claude_only_tool_warns() {
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "Task");
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "valid-tool-name");
        assert_eq!(diags[0].severity, Severity::Warning);
        assert!(diags[0].message.contains("Task"));
        assert!(diags[0].message.contains("claude"));
    }

    #[test]
    fn undeclared_copilot_only_tool_warns() {
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "browser_navigate");
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Warning);
        assert!(diags[0].message.contains("browser_navigate"));
        assert!(diags[0].message.contains("copilot"));
    }

    #[test]
    fn declared_claude_with_claude_only_tool_clean() {
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "Task");
        add_manifest(
            &mut fs,
            "[package]\nname = \"p\"\nversion = \"1.0.0\"\nengines = [\"claude\"]\n",
        );
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn declared_claude_with_copilot_only_tool_errors() {
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "browser_navigate");
        add_manifest(
            &mut fs,
            "[package]\nname = \"p\"\nversion = \"1.0.0\"\nengines = [\"claude\"]\n",
        );
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].message.contains("browser_navigate"));
        assert!(diags[0].message.contains("not supported"));
    }

    #[test]
    fn multiple_tools_only_offenders_flagged() {
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "bash, Task, browser_navigate");
        add_manifest(
            &mut fs,
            "[package]\nname = \"p\"\nversion = \"1.0.0\"\nengines = [\"claude\"]\n",
        );
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        // bash: shared (ok). Task: claude-supported (ok). browser_navigate: error.
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].message.contains("browser_navigate"));
    }

    #[test]
    fn empty_tools_field_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = agent_path();
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: reviewer\ntools:\n---\nPrompt".to_string());
        let diags = ValidToolName.check_file(&path, &fs).ok().unwrap_or_default();
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn no_tools_field_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = agent_path();
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "---\nname: reviewer\n---\nPrompt".to_string());
        let diags = ValidToolName.check_file(&path, &fs).ok().unwrap_or_default();
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn no_frontmatter_no_diagnostic() {
        let mut fs = MockFs::new();
        let path = agent_path();
        fs.exists.insert(path.clone());
        fs.files.insert(path.clone(), "Just a prompt with no frontmatter".to_string());
        let diags = ValidToolName.check_file(&path, &fs).ok().unwrap_or_default();
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn missing_file_no_diagnostic() {
        let fs = MockFs::new();
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        assert!(diags.is_empty());
    }

    #[test]
    fn parse_tools_handles_commas_and_whitespace() {
        assert_eq!(parse_tools("a, b , c"), vec!["a", "b", "c"]);
        assert_eq!(parse_tools("a b\tc"), vec!["a", "b", "c"]);
        assert_eq!(parse_tools(""), Vec::<&str>::new());
        assert_eq!(parse_tools(" , , "), Vec::<&str>::new());
    }

    #[test]
    fn rule_metadata_matches_id() {
        let rule = ValidToolName;
        assert_eq!(rule.id(), "valid-tool-name");
        assert_eq!(rule.default_severity(), Severity::Warning);
        assert!(rule.help_url().is_some());
        assert!(rule.help_text().is_some());
        assert!(!rule.name().is_empty());
    }

    #[test]
    fn engine_names_for_all_returns_all_kebab_names() {
        let names = engine_names(EngineSet::ALL);
        assert!(names.contains(&"claude"));
        assert!(names.contains(&"copilot"));
    }

    #[test]
    fn format_toml_string_array_round_trip() {
        assert_eq!(format_toml_string_array(&["claude"]), "[\"claude\"]");
        assert_eq!(format_toml_string_array(&["claude", "copilot"]), "[\"claude\", \"copilot\"]");
    }

    #[test]
    fn manifest_unreadable_treats_as_no_declared_engines() {
        // Covers line 188: manifest path exists but read_to_string returns Err.
        // `nearest_declared_engines` should return EngineSet::empty() and not panic.
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "bash");
        // Insert manifest into `exists` but not into `files` so read_to_string fails.
        fs.exists.insert(manifest_path());
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        // `bash` is a shared tool — always clean regardless of declared engines.
        assert!(
            diags.is_empty(),
            "shared tool should be clean even when manifest is unreadable: {diags:?}"
        );
    }

    #[test]
    fn manifest_invalid_toml_treats_as_no_declared_engines() {
        // Covers line 191: manifest content fails TOML parse.
        // `nearest_declared_engines` should return EngineSet::empty() and not panic.
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "bash");
        add_manifest(&mut fs, "not valid toml [[[");
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        // `bash` is a shared tool — always clean regardless of declared engines.
        assert!(
            diags.is_empty(),
            "shared tool should be clean even with invalid manifest TOML: {diags:?}"
        );
    }

    #[test]
    fn manifest_unknown_engine_name_silently_ignored() {
        // Behavior change post-Feature 15: the lint now uses the canonical
        // `crate::manifest::parse` which REJECTS all-unknown engine lists
        // (per `engine_set_serde::deserialize`). Parse error → empty
        // EngineSet → no restriction declared → Task warns (claude-only).
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "Task");
        add_manifest(
            &mut fs,
            "[package]\nname = \"p\"\nversion = \"1.0.0\"\nengines = [\"future-engine\"]\n",
        );
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        assert_eq!(
            diags.len(),
            1,
            "expected one warning for undeclared claude-only tool: {diags:?}"
        );
        assert_eq!(diags[0].severity, Severity::Warning);
        assert!(diags[0].message.contains("Task"), "message should mention the tool name");
    }

    #[test]
    fn workspace_engines_inherited_when_package_omits() {
        // Spec G7 / Feature 15: workspace-level engines drive the lint
        // when the member package omits its own `engines` field. A plugin
        // referencing a Claude-only tool ("Task") should be CLEAN under
        // a workspace declaring `engines = ["claude"]`.
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "Task");
        add_manifest(
            &mut fs,
            "[package]\nname = \"p\"\nversion = \"1.0.0\"\n\
             [workspace]\nmembers = [\".ai/*\"]\nengines = [\"claude\"]\n",
        );
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        assert!(
            diags.is_empty(),
            "Task on a workspace declaring claude should be clean: {diags:?}"
        );
    }

    #[test]
    fn workspace_engines_rejects_copilot_only_tool_when_only_claude_declared() {
        // Workspace declares claude only, member package omits engines.
        // Tool "browser_navigate" is copilot-only → should error.
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "browser_navigate");
        add_manifest(
            &mut fs,
            "[package]\nname = \"p\"\nversion = \"1.0.0\"\n\
             [workspace]\nmembers = [\".ai/*\"]\nengines = [\"claude\"]\n",
        );
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1, "expected one error: {diags:?}");
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].message.contains("browser_navigate"));
    }

    #[test]
    fn package_engines_override_workspace_engines() {
        // Inheritance contract: package wins over workspace. Package
        // declares only copilot; workspace declares only claude.
        // "browser_navigate" is copilot-only — should be CLEAN because
        // package.engines (copilot) overrides workspace.engines (claude).
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "browser_navigate");
        add_manifest(
            &mut fs,
            "[package]\nname = \"p\"\nversion = \"1.0.0\"\nengines = [\"copilot\"]\n\
             [workspace]\nmembers = [\".ai/*\"]\nengines = [\"claude\"]\n",
        );
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        assert!(
            diags.is_empty(),
            "package.engines=[copilot] should override workspace.engines=[claude]: {diags:?}"
        );
    }

    // --- Parent-walk cap (issue #793 Finding 2 / spec G4) ---

    /// Plant `aipm.toml` at an ABOVE-lint-root location that is on the walk
    /// path. The agent file uses a copilot-only tool. Without the cap, the
    /// rule would find the manifest, see `engines = ["claude"]`, and emit
    /// an Error (declared-but-incompatible). With the cap, the manifest is
    /// invisible and the rule treats the workspace as having no declared
    /// engines (Warning, not Error).
    #[test]
    fn parent_walk_stops_at_lint_dir_above_manifest_invisible() {
        let mut fs = MockFs::new();
        // Agent at .ai/p/agents/reviewer.md uses a copilot-only tool.
        add_agent_with_tools(&mut fs, "browser_navigate");
        // Manifest planted ONE LEVEL ABOVE the lint root, AT a directory
        // the parent walk would otherwise traverse. Walk path from the
        // agent file is .ai/p/agents -> .ai/p -> .ai -> "". Without the
        // cap, the walk reaches .ai/aipm.toml and reads engines=["claude"];
        // copilot-only tool against ["claude"] → Severity::Error. With the
        // cap at lint_dir = .ai/p, the walk returns None at the boundary
        // → declared = empty → Severity::Warning. Asserting Warning here
        // pins the cap behaviour: a regression that lets the walk through
        // the boundary would flip the severity to Error and fail this test.
        let above_manifest = PathBuf::from(".ai/aipm.toml");
        fs.exists.insert(above_manifest.clone());
        fs.files.insert(
            above_manifest,
            "[package]\nname = \"p\"\nversion = \"1.0.0\"\nengines = [\"claude\"]\n".to_string(),
        );
        // lint_dir is the .ai/p directory the user ran lint against.
        let lint_dir = PathBuf::from(".ai/p");
        let diags =
            ValidToolName.check_file_in(&agent_path(), &lint_dir, &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1, "{diags:?}");
        // No declared engines visible → Warning (not Error from the
        // declared-but-incompatible branch). This is the assertion that
        // pins the cap: regressing the cap would flip severity to Error.
        assert_eq!(
            diags[0].severity,
            Severity::Warning,
            "cap regression: walk reached above-lint-root manifest and \
             severity became Error. Diagnostic was: {diags:?}"
        );
        assert!(diags[0].message.contains("browser_navigate"));
    }

    /// Companion to the above: WITHOUT the cap (legacy `check_file` path),
    /// the same fixture should emit an Error because the walk reaches the
    /// `.ai/aipm.toml` manifest. This pins the inverse behaviour — proves
    /// the test fixture is non-trivial for the cap-enabled assertion.
    #[test]
    fn parent_walk_without_cap_finds_above_lint_root_manifest() {
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "browser_navigate");
        let above_manifest = PathBuf::from(".ai/aipm.toml");
        fs.exists.insert(above_manifest.clone());
        fs.files.insert(
            above_manifest,
            "[package]\nname = \"p\"\nversion = \"1.0.0\"\nengines = [\"claude\"]\n".to_string(),
        );
        // Legacy entry point — no lint_dir cap.
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        assert_eq!(diags.len(), 1, "{diags:?}");
        // Walk reaches the manifest → declared = ["claude"] → copilot-only
        // tool against ["claude"] → Error.
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].message.contains("browser_navigate"));
    }

    /// Plant `aipm.toml` INSIDE the lint root. The cap allows the walk to
    /// reach it; the rule sees the declared engines and applies them.
    #[test]
    fn parent_walk_succeeds_inside_lint_dir() {
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "browser_navigate");
        add_manifest(
            &mut fs,
            "[package]\nname = \"p\"\nversion = \"1.0.0\"\nengines = [\"copilot\"]\n",
        );
        // lint_dir at the .ai root; manifest is at .ai/p/aipm.toml (inside it).
        let lint_dir = PathBuf::from(".ai");
        let diags =
            ValidToolName.check_file_in(&agent_path(), &lint_dir, &fs).ok().unwrap_or_default();
        // copilot tool with copilot declared → clean.
        assert!(diags.is_empty(), "{diags:?}");
    }

    /// Plant `aipm.toml` at exactly `lint_dir/aipm.toml`. The cap is
    /// inclusive — the manifest at the cap boundary IS found.
    #[test]
    fn parent_walk_finds_manifest_at_lint_dir_boundary() {
        let mut fs = MockFs::new();
        // Different agent layout for this case: agent directly under
        // lint_dir/agents, manifest at lint_dir/aipm.toml.
        let agent = PathBuf::from(".ai/agents/reviewer.md");
        fs.exists.insert(agent.clone());
        fs.files.insert(
            agent.clone(),
            "---\nname: reviewer\ntools: browser_navigate\n---\nPrompt".to_string(),
        );
        let manifest = PathBuf::from(".ai/aipm.toml");
        fs.exists.insert(manifest.clone());
        fs.files.insert(
            manifest,
            "[package]\nname = \"p\"\nversion = \"1.0.0\"\nengines = [\"copilot\"]\n".to_string(),
        );
        // lint_dir == .ai (the directory containing the manifest).
        let lint_dir = PathBuf::from(".ai");
        let diags = ValidToolName.check_file_in(&agent, &lint_dir, &fs).ok().unwrap_or_default();
        // Manifest at the cap boundary is found → engines visible → clean.
        assert!(diags.is_empty(), "{diags:?}");
    }

    /// Confirms the legacy `check_file` entry point (no `lint_dir` cap)
    /// still walks to filesystem root. Direct test callers and the
    /// catalog query rely on this — only the live lint pipeline routes
    /// through `check_file_in` with a real cap.
    #[test]
    fn legacy_check_file_path_disables_cap() {
        let mut fs = MockFs::new();
        add_agent_with_tools(&mut fs, "browser_navigate");
        // Manifest at .ai/p/aipm.toml (the standard test layout).
        add_manifest(
            &mut fs,
            "[package]\nname = \"p\"\nversion = \"1.0.0\"\nengines = [\"copilot\"]\n",
        );
        // Calling check_file directly (no lint_dir) — must still find
        // the manifest.
        let diags = ValidToolName.check_file(&agent_path(), &fs).ok().unwrap_or_default();
        assert!(diags.is_empty(), "{diags:?}");
    }
}
