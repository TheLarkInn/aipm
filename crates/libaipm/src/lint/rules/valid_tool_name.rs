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
        let Ok(content) = fs.read_to_string(file_path) else {
            return Ok(vec![]);
        };
        let Some(fm) = crate::frontmatter::parse(&content).ok().flatten() else {
            return Ok(vec![]);
        };
        let Some(tools_value) = fm.fields.get("tools") else {
            return Ok(vec![]);
        };

        let declared = nearest_declared_engines(file_path, fs);
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
        (Severity::Error, format!("Tool '{tool}' is not supported by all declared engines."))
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
                Engine::CopilotCli => EngineSet::COPILOT_CLI,
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
/// the `EngineSet` declared in its `[package].engines` field.
///
/// Returns [`EngineSet::empty()`] when no manifest is found, the manifest
/// cannot be read or parsed, or the `engines` field is missing or empty.
/// Names that don't parse via [`Engine::from_name`] are silently ignored.
fn nearest_declared_engines(file_path: &Path, fs: &dyn Fs) -> EngineSet {
    let Some(manifest_path) = find_nearest_manifest(file_path, fs) else {
        return EngineSet::empty();
    };
    let Ok(content) = fs.read_to_string(&manifest_path) else {
        return EngineSet::empty();
    };
    let Ok(manifest) = toml::from_str::<MinimalManifest>(&content) else {
        return EngineSet::empty();
    };
    let names = manifest.package.and_then(|p| p.engines).unwrap_or_default();
    let mut set = EngineSet::empty();
    for name in names {
        if let Some(engine) = Engine::from_name(&name) {
            match engine {
                Engine::Claude => set |= EngineSet::CLAUDE,
                Engine::CopilotCli => set |= EngineSet::COPILOT_CLI,
            }
        }
    }
    set
}

/// Walk parent directories looking for `aipm.toml`.
///
/// Returns the first existing `aipm.toml` encountered moving from the
/// feature file toward the filesystem root.
fn find_nearest_manifest(file_path: &Path, fs: &dyn Fs) -> Option<PathBuf> {
    let mut current = file_path.parent();
    while let Some(dir) = current {
        let candidate = dir.join("aipm.toml");
        if fs.exists(&candidate) {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

/// Subset of `aipm.toml` consumed by this lint — mirrors the pattern in
/// `crate::engine::MinimalManifest`.
#[derive(serde::Deserialize, Default)]
struct MinimalManifest {
    #[serde(default)]
    package: Option<MinimalPackage>,
}

#[derive(serde::Deserialize, Default)]
struct MinimalPackage {
    #[serde(default)]
    engines: Option<Vec<String>>,
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
        assert!(diags[0].message.contains("copilot-cli"));
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
        assert!(names.contains(&"copilot-cli"));
    }

    #[test]
    fn format_toml_string_array_round_trip() {
        assert_eq!(format_toml_string_array(&["claude"]), "[\"claude\"]");
        assert_eq!(
            format_toml_string_array(&["claude", "copilot-cli"]),
            "[\"claude\", \"copilot-cli\"]"
        );
    }
}
