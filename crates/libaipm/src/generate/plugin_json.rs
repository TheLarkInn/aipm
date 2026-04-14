//! Unified `plugin.json` generation for AI plugin packages.
//!
//! Replaces scattered generation in `workspace_init` (minimal starter) and
//! `migrate/emitter` (full with component paths) with a single function.

/// Core metadata for a `plugin.json` file.
pub struct Opts<'a> {
    /// Plugin name.
    pub name: &'a str,
    /// Semver version string.
    pub version: &'a str,
    /// Human-readable description.
    pub description: &'a str,
}

/// Optional component path entries for `plugin.json`.
///
/// Each `Some` value becomes a top-level key in the JSON output.
/// Use `Default` to start with everything `None`, then set the fields you need.
#[derive(Default)]
pub struct Components<'a> {
    /// Path to skills directory (JSON key: `"skills"`).
    pub skills: Option<&'a str>,
    /// Path to agents directory (JSON key: `"agents"`).
    pub agents: Option<&'a str>,
    /// Path to MCP servers config (JSON key: `"mcpServers"`).
    pub mcp_servers: Option<&'a str>,
    /// Path to hooks config (JSON key: `"hooks"`).
    pub hooks: Option<&'a str>,
    /// Path to output styles directory (JSON key: `"outputStyles"`).
    pub output_styles: Option<&'a str>,
    /// Path to LSP servers config (JSON key: `"lspServers"`).
    pub lsp_servers: Option<&'a str>,
    /// Path to extensions directory (JSON key: `"extensions"`).
    pub extensions: Option<&'a str>,
}

/// Generate a pretty-printed `plugin.json` string with a trailing newline.
///
/// When `components` is `Some`, the corresponding top-level keys are added
/// after the base fields (`name`, `version`, `description`, `author`).
pub fn generate(opts: &Opts<'_>, components: Option<&Components<'_>>) -> String {
    let mut map = serde_json::Map::new();
    map.insert("name".to_string(), serde_json::Value::String(opts.name.to_string()));
    map.insert("version".to_string(), serde_json::Value::String(opts.version.to_string()));
    map.insert("description".to_string(), serde_json::Value::String(opts.description.to_string()));

    let mut author = serde_json::Map::new();
    author.insert("name".to_string(), serde_json::Value::String("TODO".to_string()));
    author.insert("email".to_string(), serde_json::Value::String("TODO".to_string()));
    map.insert("author".to_string(), serde_json::Value::Object(author));

    if let Some(c) = components {
        if let Some(v) = c.skills {
            map.insert("skills".to_string(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = c.agents {
            map.insert("agents".to_string(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = c.mcp_servers {
            map.insert("mcpServers".to_string(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = c.hooks {
            map.insert("hooks".to_string(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = c.output_styles {
            map.insert("outputStyles".to_string(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = c.lsp_servers {
            map.insert("lspServers".to_string(), serde_json::Value::String(v.to_string()));
        }
        if let Some(v) = c.extensions {
            map.insert("extensions".to_string(), serde_json::Value::String(v.to_string()));
        }
    }

    let obj = serde_json::Value::Object(map);
    let mut output = serde_json::to_string_pretty(&obj).unwrap_or_default();
    output.push('\n');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_plugin_json() {
        let opts = Opts { name: "my-plugin", version: "1.0.0", description: "A plugin" };
        let json = generate(&opts, None);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(v.get("name").and_then(serde_json::Value::as_str), Some("my-plugin"));
        assert_eq!(v.get("version").and_then(serde_json::Value::as_str), Some("1.0.0"));
        assert_eq!(v.get("description").and_then(serde_json::Value::as_str), Some("A plugin"));
        assert!(v.get("author").is_some());
        // No component keys
        assert!(v.get("skills").is_none());
        assert!(v.get("agents").is_none());
    }

    #[test]
    fn with_components() {
        let opts = Opts { name: "test", version: "0.1.0", description: "desc" };
        let components = Components {
            skills: Some("./skills/"),
            agents: Some("./agents/"),
            hooks: Some("./hooks/hooks.json"),
            ..Default::default()
        };
        let json = generate(&opts, Some(&components));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(v.get("skills").and_then(serde_json::Value::as_str), Some("./skills/"));
        assert_eq!(v.get("agents").and_then(serde_json::Value::as_str), Some("./agents/"));
        assert_eq!(v.get("hooks").and_then(serde_json::Value::as_str), Some("./hooks/hooks.json"));
        // Not set
        assert!(v.get("mcpServers").is_none());
        assert!(v.get("outputStyles").is_none());
        assert!(v.get("lspServers").is_none());
        assert!(v.get("extensions").is_none());
    }

    #[test]
    fn all_component_keys() {
        let opts = Opts { name: "full", version: "2.0.0", description: "all" };
        let components = Components {
            skills: Some("./skills/"),
            agents: Some("./agents/"),
            mcp_servers: Some("./.mcp.json"),
            hooks: Some("./hooks/hooks.json"),
            output_styles: Some("./"),
            lsp_servers: Some("./lsp.json"),
            extensions: Some("./extensions/"),
        };
        let json = generate(&opts, Some(&components));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
        assert!(v.get("skills").is_some());
        assert!(v.get("agents").is_some());
        assert!(v.get("mcpServers").is_some());
        assert!(v.get("hooks").is_some());
        assert!(v.get("outputStyles").is_some());
        assert!(v.get("lspServers").is_some());
        assert!(v.get("extensions").is_some());
    }

    #[test]
    fn trailing_newline() {
        let opts = Opts { name: "t", version: "0.1.0", description: "d" };
        let json = generate(&opts, None);
        assert!(json.ends_with('\n'));
    }

    #[test]
    fn special_characters_in_description() {
        let opts = Opts {
            name: "test",
            version: "0.1.0",
            description: "Quotes \"here\" and backslash \\ and em dash \u{2014}",
        };
        let json = generate(&opts, None);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
        let desc = v.get("description").and_then(serde_json::Value::as_str).unwrap_or_default();
        assert!(desc.contains('"'));
        assert!(desc.contains('\\'));
        assert!(desc.contains('\u{2014}'));
    }

    #[test]
    fn empty_components_produces_no_extra_keys() {
        let opts = Opts { name: "t", version: "0.1.0", description: "d" };
        let components = Components::default();
        let json = generate(&opts, Some(&components));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
        // Base 4 keys only
        let obj = v.as_object();
        assert!(obj.is_some_and(|m| m.len() == 4));
    }

    #[test]
    fn valid_json_roundtrip() {
        let opts = Opts { name: "round", version: "0.1.0", description: "trip" };
        let components = Components { skills: Some("./s/"), ..Default::default() };
        let json = generate(&opts, Some(&components));
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(parsed.is_ok());
    }
}
