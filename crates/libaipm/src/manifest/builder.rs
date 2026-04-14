//! TOML manifest builder using `toml_edit` for round-trip fidelity.
//!
//! Provides a single entry-point for generating `aipm.toml` content for both
//! plugin member manifests and workspace root manifests.  All four existing
//! generation sites (init, `workspace_init` × 2, migrate/emitter) can be
//! replaced by calls to [`build_plugin_manifest`] or [`build_workspace_manifest`].

use toml_edit::{value, Array, DocumentMut, Item, Table};

/// Options for generating a plugin `aipm.toml`.
#[derive(Debug, Clone)]
pub struct PluginManifestOpts<'a> {
    /// Package name (required).
    pub name: &'a str,
    /// Semver version string (required).
    pub version: &'a str,
    /// Plugin type (e.g. `"skill"`, `"composite"`).  `None` omits the field.
    pub plugin_type: Option<&'a str>,
    /// Human-readable description.  `None` omits the field.
    pub description: Option<&'a str>,
}

/// Options for the `[components]` section of a plugin manifest.
///
/// All fields are optional — omitted fields produce no TOML key.
#[derive(Debug, Default, Clone)]
pub struct PluginComponentsOpts<'a> {
    pub skills: Option<&'a [String]>,
    pub agents: Option<&'a [String]>,
    pub mcp_servers: Option<&'a [String]>,
    pub hooks: Option<&'a [String]>,
    pub output_styles: Option<&'a [String]>,
    pub scripts: Option<&'a [String]>,
}

/// Options for generating a workspace root `aipm.toml`.
#[derive(Debug, Clone)]
pub struct WorkspaceManifestOpts<'a> {
    /// Glob patterns for `[workspace].members`.
    pub members: &'a [String],
    /// Default plugins directory.
    pub plugins_dir: Option<&'a str>,
    /// Optional header comment lines (each line is prefixed with `# `).
    pub header_comments: Option<&'a [&'a str]>,
    /// Optional trailing comment lines appended after the workspace section.
    pub trailing_comments: Option<&'a [&'a str]>,
}

/// Build a plugin `aipm.toml` string from the given options.
///
/// Produces a `[package]` section and, when `components` is `Some` with at
/// least one non-empty field, a `[components]` section.
pub fn build_plugin_manifest(
    opts: &PluginManifestOpts<'_>,
    components: Option<&PluginComponentsOpts<'_>>,
) -> String {
    let mut doc = DocumentMut::new();

    // ── [package] ──────────────────────────────────────────────────────
    let mut pkg = Table::new();
    pkg.insert("name", value(opts.name));
    pkg.insert("version", value(opts.version));
    if let Some(t) = opts.plugin_type {
        pkg.insert("type", value(t));
    }
    if let Some(d) = opts.description {
        pkg.insert("description", value(d));
    }
    doc.insert("package", Item::Table(pkg));

    // ── [components] (optional) ────────────────────────────────────────
    if let Some(c) = components {
        let mut tbl = Table::new();
        insert_string_array(&mut tbl, "skills", c.skills);
        insert_string_array(&mut tbl, "agents", c.agents);
        insert_string_array(&mut tbl, "mcp_servers", c.mcp_servers);
        insert_string_array(&mut tbl, "hooks", c.hooks);
        insert_string_array(&mut tbl, "output_styles", c.output_styles);
        insert_string_array(&mut tbl, "scripts", c.scripts);

        if !tbl.is_empty() {
            doc.insert("components", Item::Table(tbl));
        }
    }

    doc.to_string()
}

/// Build a workspace root `aipm.toml` string from the given options.
///
/// Produces optional header comments, a `[workspace]` section, and optional
/// trailing comments.
pub fn build_workspace_manifest(opts: &WorkspaceManifestOpts<'_>) -> String {
    let mut doc = DocumentMut::new();

    // ── [workspace] ────────────────────────────────────────────────────
    let mut ws = Table::new();

    let mut members_arr = Array::new();
    for m in opts.members {
        members_arr.push(m.as_str());
    }
    ws.insert("members", value(members_arr));

    if let Some(dir) = opts.plugins_dir {
        ws.insert("plugins_dir", value(dir));
    }

    doc.insert("workspace", Item::Table(ws));

    // ── Assemble output ────────────────────────────────────────────────
    let mut output = String::new();

    // Header comments
    if let Some(lines) = opts.header_comments {
        for line in lines {
            push_comment_line(&mut output, line);
        }
        output.push('\n');
    }

    output.push_str(&doc.to_string());

    // Trailing comments
    if let Some(lines) = opts.trailing_comments {
        output.push('\n');
        for line in lines {
            push_comment_line(&mut output, line);
        }
    }

    output
}

/// Append a single comment line to `output`.  Empty strings produce a blank line.
fn push_comment_line(output: &mut String, line: &str) {
    if !line.is_empty() {
        output.push_str("# ");
        output.push_str(line);
    }
    output.push('\n');
}

/// Insert a string-array key into a TOML table if the slice is `Some` and non-empty.
fn insert_string_array(table: &mut Table, key: &str, values: Option<&[String]>) {
    if let Some(items) = values {
        if !items.is_empty() {
            let mut arr = Array::new();
            for item in items {
                arr.push(item.as_str());
            }
            table.insert(key, value(arr));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Plugin manifest tests ──────────────────────────────────────────

    #[test]
    fn minimal_plugin_manifest() {
        let opts = PluginManifestOpts {
            name: "my-plugin",
            version: "0.1.0",
            plugin_type: None,
            description: None,
        };
        let output = build_plugin_manifest(&opts, None);
        assert!(output.contains("[package]"), "missing [package]: {output}");
        assert!(output.contains("name = \"my-plugin\""), "missing name: {output}");
        assert!(output.contains("version = \"0.1.0\""), "missing version: {output}");
        assert!(!output.contains("type"), "should not have type: {output}");
        assert!(!output.contains("description"), "should not have description: {output}");
        assert!(!output.contains("[components]"), "should not have components: {output}");
    }

    #[test]
    fn plugin_manifest_with_type() {
        let opts = PluginManifestOpts {
            name: "test-skill",
            version: "1.0.0",
            plugin_type: Some("skill"),
            description: None,
        };
        let output = build_plugin_manifest(&opts, None);
        assert!(output.contains("type = \"skill\""), "missing type: {output}");
    }

    #[test]
    fn plugin_manifest_with_description() {
        let opts = PluginManifestOpts {
            name: "described",
            version: "2.0.0",
            plugin_type: Some("composite"),
            description: Some("A plugin with a description"),
        };
        let output = build_plugin_manifest(&opts, None);
        assert!(
            output.contains("description = \"A plugin with a description\""),
            "missing description: {output}"
        );
    }

    #[test]
    fn plugin_manifest_with_components() {
        let skills = vec!["skills/lint/SKILL.md".to_string()];
        let agents = vec!["agents/reviewer.md".to_string()];
        let hooks = vec!["hooks/hooks.json".to_string()];
        let scripts = vec!["scripts/build.sh".to_string()];

        let opts = PluginManifestOpts {
            name: "full-plugin",
            version: "0.1.0",
            plugin_type: Some("composite"),
            description: Some("Full-featured plugin"),
        };
        let components = PluginComponentsOpts {
            skills: Some(&skills),
            agents: Some(&agents),
            hooks: Some(&hooks),
            scripts: Some(&scripts),
            ..PluginComponentsOpts::default()
        };
        let output = build_plugin_manifest(&opts, Some(&components));

        assert!(output.contains("[components]"), "missing [components]: {output}");
        assert!(output.contains("skills = [\"skills/lint/SKILL.md\"]"), "missing skills: {output}");
        assert!(output.contains("agents = [\"agents/reviewer.md\"]"), "missing agents: {output}");
        assert!(output.contains("hooks = [\"hooks/hooks.json\"]"), "missing hooks: {output}");
        assert!(output.contains("scripts = [\"scripts/build.sh\"]"), "missing scripts: {output}");
        assert!(!output.contains("mcp_servers"), "should not have mcp_servers: {output}");
        assert!(!output.contains("output_styles"), "should not have output_styles: {output}");
    }

    #[test]
    fn plugin_manifest_empty_components_omitted() {
        let opts = PluginManifestOpts {
            name: "no-comps",
            version: "0.1.0",
            plugin_type: None,
            description: None,
        };
        let components = PluginComponentsOpts::default();
        let output = build_plugin_manifest(&opts, Some(&components));
        assert!(!output.contains("[components]"), "empty components should be omitted: {output}");
    }

    #[test]
    fn plugin_manifest_special_characters_in_description() {
        let opts = PluginManifestOpts {
            name: "special",
            version: "0.1.0",
            plugin_type: None,
            description: Some("Contains \"quotes\" and \\ backslashes"),
        };
        let output = build_plugin_manifest(&opts, None);
        // toml_edit should properly escape the quotes and backslashes
        let parsed: Result<toml::Value, _> = toml::from_str(&output);
        assert!(parsed.is_ok(), "output should be valid TOML: {output}");
    }

    #[test]
    fn plugin_manifest_scoped_name() {
        let opts = PluginManifestOpts {
            name: "@company/ci-tools",
            version: "1.2.3",
            plugin_type: Some("composite"),
            description: None,
        };
        let output = build_plugin_manifest(&opts, None);
        assert!(output.contains("name = \"@company/ci-tools\""), "missing scoped name: {output}");
    }

    #[test]
    fn plugin_manifest_mcp_and_output_styles() {
        let mcp = vec![".mcp.json".to_string()];
        let styles = vec!["custom.md".to_string()];
        let opts = PluginManifestOpts {
            name: "mcp-plugin",
            version: "0.1.0",
            plugin_type: Some("mcp"),
            description: None,
        };
        let components = PluginComponentsOpts {
            mcp_servers: Some(&mcp),
            output_styles: Some(&styles),
            ..PluginComponentsOpts::default()
        };
        let output = build_plugin_manifest(&opts, Some(&components));
        assert!(output.contains("mcp_servers = [\".mcp.json\"]"), "missing mcp_servers: {output}");
        assert!(
            output.contains("output_styles = [\"custom.md\"]"),
            "missing output_styles: {output}"
        );
    }

    #[test]
    fn plugin_manifest_round_trips_through_toml_parse() {
        let skills = vec!["skills/main/SKILL.md".to_string()];
        let opts = PluginManifestOpts {
            name: "roundtrip",
            version: "0.1.0",
            plugin_type: Some("skill"),
            description: Some("Test round-trip"),
        };
        let components =
            PluginComponentsOpts { skills: Some(&skills), ..PluginComponentsOpts::default() };
        let output = build_plugin_manifest(&opts, Some(&components));
        let parsed: Result<crate::manifest::types::Manifest, _> = toml::from_str(&output);
        assert!(parsed.is_ok(), "should parse as valid Manifest: {output}");
        let m = parsed.unwrap_or_default();
        let pkg = m.package.as_ref();
        assert!(pkg.is_some_and(|p| p.name == "roundtrip"));
        assert!(pkg.is_some_and(|p| p.version == "0.1.0"));
        assert!(pkg.is_some_and(|p| p.plugin_type.as_deref() == Some("skill")));
        assert!(pkg.is_some_and(|p| p.description.as_deref() == Some("Test round-trip")));
        let comps = m.components.as_ref();
        assert!(comps.is_some_and(|c| c.skills.as_ref().is_some_and(|s| s.len() == 1)));
    }

    // ── Workspace manifest tests ───────────────────────────────────────

    #[test]
    fn minimal_workspace_manifest() {
        let members = vec![".ai/*".to_string()];
        let opts = WorkspaceManifestOpts {
            members: &members,
            plugins_dir: None,
            header_comments: None,
            trailing_comments: None,
        };
        let output = build_workspace_manifest(&opts);
        assert!(output.contains("[workspace]"), "missing [workspace]: {output}");
        assert!(output.contains("members = [\".ai/*\"]"), "missing members: {output}");
        assert!(!output.contains("plugins_dir"), "should not have plugins_dir: {output}");
    }

    #[test]
    fn workspace_manifest_with_plugins_dir() {
        let members = vec![".ai/*".to_string()];
        let opts = WorkspaceManifestOpts {
            members: &members,
            plugins_dir: Some(".ai"),
            header_comments: None,
            trailing_comments: None,
        };
        let output = build_workspace_manifest(&opts);
        assert!(output.contains("plugins_dir = \".ai\""), "missing plugins_dir: {output}");
    }

    #[test]
    fn workspace_manifest_with_header_comments() {
        let members = vec![".ai/*".to_string()];
        let opts = WorkspaceManifestOpts {
            members: &members,
            plugins_dir: Some(".ai"),
            header_comments: Some(&[
                "AI Plugin Manager — Workspace Configuration",
                "Docs: https://github.com/thelarkinn/aipm",
            ]),
            trailing_comments: None,
        };
        let output = build_workspace_manifest(&opts);
        assert!(
            output.starts_with("# AI Plugin Manager"),
            "should start with header comment: {output}"
        );
        assert!(
            output.contains("# Docs: https://github.com/thelarkinn/aipm"),
            "missing docs comment: {output}"
        );
    }

    #[test]
    fn workspace_manifest_with_trailing_comments() {
        let members = vec![".ai/*".to_string()];
        let opts = WorkspaceManifestOpts {
            members: &members,
            plugins_dir: Some(".ai"),
            header_comments: None,
            trailing_comments: Some(&[
                "Shared dependency versions for all workspace members.",
                "Members reference these via: dep = { workspace = \"*\" }",
                "[workspace.dependencies]",
            ]),
        };
        let output = build_workspace_manifest(&opts);
        assert!(
            output.contains("# Shared dependency versions"),
            "missing trailing comment: {output}"
        );
    }

    #[test]
    fn workspace_manifest_header_empty_line() {
        let members = vec![".ai/*".to_string()];
        let opts = WorkspaceManifestOpts {
            members: &members,
            plugins_dir: None,
            header_comments: Some(&["Line one", "", "Line three"]),
            trailing_comments: None,
        };
        let output = build_workspace_manifest(&opts);
        // The empty string should produce a blank line (no "# " prefix)
        assert!(output.contains("# Line one\n\n# Line three"), "empty line handling: {output}");
    }

    #[test]
    fn workspace_manifest_multiple_members() {
        let members = vec!["plugins/*".to_string(), "tools/*".to_string()];
        let opts = WorkspaceManifestOpts {
            members: &members,
            plugins_dir: None,
            header_comments: None,
            trailing_comments: None,
        };
        let output = build_workspace_manifest(&opts);
        assert!(output.contains("plugins/*"), "missing plugins/*: {output}");
        assert!(output.contains("tools/*"), "missing tools/*: {output}");
    }

    #[test]
    fn workspace_manifest_round_trips_toml_section() {
        let members = vec![".ai/*".to_string()];
        let opts = WorkspaceManifestOpts {
            members: &members,
            plugins_dir: Some(".ai"),
            header_comments: None,
            trailing_comments: None,
        };
        let output = build_workspace_manifest(&opts);
        let parsed: Result<crate::manifest::types::Manifest, _> = toml::from_str(&output);
        assert!(parsed.is_ok(), "should parse as valid Manifest: {output}");
        let m = parsed.unwrap_or_default();
        let ws = m.workspace.as_ref();
        assert!(ws.is_some_and(|w| w.members.len() == 1));
        assert!(ws.is_some_and(|w| w.plugins_dir.as_deref() == Some(".ai")));
    }

    #[test]
    fn full_workspace_manifest_snapshot() {
        let members = vec![".ai/*".to_string()];
        let opts = WorkspaceManifestOpts {
            members: &members,
            plugins_dir: Some(".ai"),
            header_comments: Some(&[
                "AI Plugin Manager — Workspace Configuration",
                "Docs: https://github.com/thelarkinn/aipm",
            ]),
            trailing_comments: Some(&[
                "Shared dependency versions for all workspace members.",
                "Members reference these via: dep = { workspace = \"*\" }",
                "[workspace.dependencies]",
                "",
                "Direct registry installs (available project-wide).",
                "[dependencies]",
                "",
                "Environment requirements for all plugins in this workspace.",
                "[environment]",
                "requires = [\"git\"]",
            ]),
        };
        let output = build_workspace_manifest(&opts);

        // Verify structure
        assert!(output.starts_with("# AI Plugin Manager"), "bad header start: {output}");
        assert!(output.contains("[workspace]"), "missing [workspace]: {output}");
        assert!(output.contains("plugins_dir = \".ai\""), "missing plugins_dir: {output}");
        assert!(
            output.contains("# [workspace.dependencies]"),
            "missing trailing comment: {output}"
        );
    }

    #[test]
    fn insert_string_array_ignores_some_empty_slice() {
        // Exercises the False branch of `if !items.is_empty()` inside
        // `insert_string_array`: when the value is `Some(&[])` (present but empty),
        // the key must NOT be written to the table.
        let opts = PluginManifestOpts {
            name: "test-plugin",
            version: "0.1.0",
            plugin_type: None,
            description: None,
        };
        let empty: Vec<String> = Vec::new();
        let components =
            PluginComponentsOpts { skills: Some(&empty), ..PluginComponentsOpts::default() };
        let output = build_plugin_manifest(&opts, Some(&components));
        // An empty slice must not produce a [components] section or a "skills" key.
        assert!(
            !output.contains("[components]"),
            "empty skills slice should not produce [components]: {output}"
        );
        assert!(!output.contains("skills"), "empty skills slice should not appear: {output}");
    }
}
