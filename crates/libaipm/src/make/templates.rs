//! Minimal, lint-passing component templates for `aipm make plugin`.
//!
//! Each template generates the exact content needed to pass `aipm lint`
//! with zero errors — required frontmatter fields with placeholder values.

use std::fmt::Write;

/// Generate a SKILL.md template with the required `name` and `description`
/// frontmatter fields.
#[must_use]
pub fn skill_template(name: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "---");
    let _ = writeln!(out, "name: {name}");
    let _ = writeln!(out, "description: \"TODO: Describe {name}\"");
    let _ = writeln!(out, "---");
    let _ = writeln!(out);
    let _ = writeln!(out, "# {name}");
    let _ = writeln!(out);
    let _ = writeln!(out, "Add your skill instructions here.");
    out
}

/// Generate an agent `.md` template with `name`, `description`, and `tools`
/// frontmatter fields.
#[must_use]
pub fn agent_template(name: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "---");
    let _ = writeln!(out, "name: {name}");
    let _ = writeln!(out, "description: \"TODO: Describe {name}\"");
    let _ = writeln!(out, "tools: []");
    let _ = writeln!(out, "---");
    let _ = writeln!(out);
    let _ = writeln!(out, "# {name}");
    let _ = writeln!(out);
    let _ = writeln!(out, "Add your agent instructions here.");
    out
}

/// Generate a `.mcp.json` template with an empty `mcpServers` object.
#[must_use]
pub fn mcp_template() -> String {
    "{\n  \"mcpServers\": {}\n}\n".to_string()
}

/// Generate a `hooks.json` template with an empty hooks array.
#[must_use]
pub fn hook_template() -> String {
    "{\n  \"hooks\": []\n}\n".to_string()
}

/// Generate an output style `.md` template with a `name` frontmatter field.
#[must_use]
pub fn output_style_template(name: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "---");
    let _ = writeln!(out, "name: {name}");
    let _ = writeln!(out, "---");
    let _ = writeln!(out);
    let _ = writeln!(out, "Add your output style definition here.");
    out
}

/// Generate an `.lsp.json` template with an empty `lspServers` object.
#[must_use]
pub fn lsp_template() -> String {
    "{\n  \"lspServers\": {}\n}\n".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_template_snapshot() {
        let content = skill_template("my-plugin");
        insta::assert_snapshot!(content);
    }

    #[test]
    fn agent_template_snapshot() {
        let content = agent_template("my-plugin");
        insta::assert_snapshot!(content);
    }

    #[test]
    fn mcp_template_snapshot() {
        let content = mcp_template();
        insta::assert_snapshot!(content);
    }

    #[test]
    fn hook_template_snapshot() {
        let content = hook_template();
        insta::assert_snapshot!(content);
    }

    #[test]
    fn output_style_template_snapshot() {
        let content = output_style_template("my-plugin");
        insta::assert_snapshot!(content);
    }

    #[test]
    fn lsp_template_snapshot() {
        let content = lsp_template();
        insta::assert_snapshot!(content);
    }

    #[test]
    fn skill_template_has_required_frontmatter() {
        let content = skill_template("test-skill");
        assert!(content.contains("name: test-skill"));
        assert!(content.contains("description:"));
        // Verify name is within lint limits (64 chars)
        assert!(content.lines().any(|l| l.starts_with("name: ") && l.len() < 70));
    }

    #[test]
    fn agent_template_has_required_frontmatter() {
        let content = agent_template("test-agent");
        assert!(content.contains("name: test-agent"));
        assert!(content.contains("description:"));
        assert!(content.contains("tools:"));
    }

    #[test]
    fn json_templates_are_valid() {
        let mcp: serde_json::Value =
            serde_json::from_str(&mcp_template()).expect("mcp_template is not valid JSON");
        assert!(mcp.get("mcpServers").is_some());

        let hooks: serde_json::Value =
            serde_json::from_str(&hook_template()).expect("hook_template is not valid JSON");
        assert!(hooks.get("hooks").is_some());

        let lsp: serde_json::Value =
            serde_json::from_str(&lsp_template()).expect("lsp_template is not valid JSON");
        assert!(lsp.get("lspServers").is_some());
    }
}
