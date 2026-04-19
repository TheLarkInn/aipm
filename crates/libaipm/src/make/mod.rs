//! Scaffolding command — `aipm make plugin`.
//!
//! This module composes the existing atomic CRUD primitives in
//! `generate/`, `manifest/`, and `init` into an ordered, idempotent
//! action pipeline exposed through the `aipm make` CLI command.

pub mod action;
pub mod discovery;
pub mod engine_features;
pub mod error;
pub mod templates;

pub use action::Action;
pub use engine_features::Feature;
pub use error::Error;

use std::path::Path;

use crate::fs::Fs;

/// Options for [`plugin`].
pub struct PluginOpts<'a> {
    /// The `.ai/` marketplace directory.
    pub marketplace_dir: &'a Path,
    /// Plugin name.
    pub name: &'a str,
    /// Target engine: `"claude"`, `"copilot"`, or `"both"`.
    pub engine: &'a str,
    /// Features to scaffold.
    pub features: &'a [Feature],
}

/// Result of [`plugin`] — a list of actions that were performed.
pub struct PluginResult {
    /// Ordered list of actions taken (or skipped).
    pub actions: Vec<Action>,
}

/// Orchestrate the creation of a new plugin inside a marketplace directory.
///
/// Performs a 9-step sequence:
///
/// 1-2. Guard existing directory with idempotent early return.
/// 3.   Create the plugin directory.
/// 4.   Create the `.claude-plugin/` metadata subdirectory.
/// 5.   For each requested feature, create directories and write templates.
/// 6.   Generate and write `plugin.json`.
/// 7.   Register the plugin in `marketplace.json`.
/// 8.   Update engine settings (Claude only; Copilot deferred).
/// 9.   Emit a summary `PluginCreated` action.
///
/// # Errors
///
/// Returns `make::Error` on I/O or JSON failures.
pub fn plugin(opts: &PluginOpts<'_>, fs: &dyn Fs) -> Result<PluginResult, Error> {
    let mut actions = Vec::new();
    let plugin_dir = opts.marketplace_dir.join(opts.name);

    // Step 1-2: Guard existing dir with idempotent early return
    if fs.exists(&plugin_dir) {
        actions.push(Action::DirectoryAlreadyExists { path: plugin_dir });
        return Ok(PluginResult { actions });
    }

    // Step 3: Create plugin dir
    fs.create_dir_all(&plugin_dir)?;
    actions.push(Action::DirectoryCreated { path: plugin_dir.clone() });

    // Step 4: Create .claude-plugin/ subdir
    let claude_plugin_dir = plugin_dir.join(".claude-plugin");
    fs.create_dir_all(&claude_plugin_dir)?;
    actions.push(Action::DirectoryCreated { path: claude_plugin_dir.clone() });

    // Step 5: Scaffold features and build component paths for plugin.json
    let components = scaffold_features(opts, fs, &plugin_dir, &mut actions)?;

    // Step 6: Generate and write plugin.json
    write_plugin_json(opts, fs, &claude_plugin_dir, &components, &mut actions)?;

    // Step 7: Register in marketplace.json
    let marketplace_json = opts.marketplace_dir.join(".claude-plugin").join("marketplace.json");
    register_in_marketplace(opts, fs, &marketplace_json, &mut actions)?;

    // Step 8: Engine settings (Claude or both; Copilot deferred)
    if opts.engine == "claude" || opts.engine == "both" {
        update_engine_settings(opts, fs, &marketplace_json, &mut actions)?;
    }

    // Step 9: Summary action
    let feature_names: Vec<String> =
        opts.features.iter().map(|f| f.cli_name().to_string()).collect();
    actions.push(Action::PluginCreated {
        name: opts.name.to_string(),
        path: plugin_dir,
        features: feature_names,
        engine: opts.engine.to_string(),
    });

    Ok(PluginResult { actions })
}

// ---------------------------------------------------------------------------
// Private helpers (extracted to satisfy too-many-lines lint)
// ---------------------------------------------------------------------------

/// Scaffold feature directories and files, returning the component paths.
fn scaffold_features(
    opts: &PluginOpts<'_>,
    fs: &dyn Fs,
    plugin_dir: &Path,
    actions: &mut Vec<Action>,
) -> Result<crate::generate::plugin_json::Components<'static>, Error> {
    let mut components = crate::generate::plugin_json::Components::default();

    for feature in opts.features {
        scaffold_single_feature(opts, fs, plugin_dir, *feature, &mut components, actions)?;
    }
    Ok(components)
}

/// Scaffold a single feature inside `plugin_dir`.
fn scaffold_single_feature(
    opts: &PluginOpts<'_>,
    fs: &dyn Fs,
    plugin_dir: &Path,
    feature: Feature,
    components: &mut crate::generate::plugin_json::Components<'_>,
    actions: &mut Vec<Action>,
) -> Result<(), Error> {
    match feature {
        Feature::Skill => scaffold_skill(opts, fs, plugin_dir, components, actions),
        Feature::Agent => scaffold_agent(opts, fs, plugin_dir, components, actions),
        Feature::Mcp => scaffold_mcp(fs, plugin_dir, components, actions),
        Feature::Hook => scaffold_hook(fs, plugin_dir, components, actions),
        Feature::OutputStyle => scaffold_output_style(opts, fs, plugin_dir, components, actions),
        Feature::Lsp => scaffold_lsp(fs, plugin_dir, components, actions),
        Feature::Extension => scaffold_extension(fs, plugin_dir, components, actions),
    }
}

fn scaffold_skill(
    opts: &PluginOpts<'_>,
    fs: &dyn Fs,
    plugin_dir: &Path,
    components: &mut crate::generate::plugin_json::Components<'_>,
    actions: &mut Vec<Action>,
) -> Result<(), Error> {
    let skill_dir = plugin_dir.join("skills").join(opts.name);
    fs.create_dir_all(&skill_dir)?;
    actions.push(Action::DirectoryCreated { path: skill_dir.clone() });
    let skill_path = skill_dir.join("SKILL.md");
    fs.write_file(&skill_path, templates::skill_template(opts.name).as_bytes())?;
    actions.push(Action::FileWritten {
        path: skill_path,
        description: "Skill definition".to_string(),
    });
    components.skills = Some("./skills/");
    Ok(())
}

fn scaffold_agent(
    opts: &PluginOpts<'_>,
    fs: &dyn Fs,
    plugin_dir: &Path,
    components: &mut crate::generate::plugin_json::Components<'_>,
    actions: &mut Vec<Action>,
) -> Result<(), Error> {
    let agents_dir = plugin_dir.join("agents");
    fs.create_dir_all(&agents_dir)?;
    actions.push(Action::DirectoryCreated { path: agents_dir.clone() });
    let agent_path = agents_dir.join(format!("{}.md", opts.name));
    fs.write_file(&agent_path, templates::agent_template(opts.name).as_bytes())?;
    actions.push(Action::FileWritten {
        path: agent_path,
        description: "Agent definition".to_string(),
    });
    components.agents = Some("./agents/");
    Ok(())
}

fn scaffold_mcp(
    fs: &dyn Fs,
    plugin_dir: &Path,
    components: &mut crate::generate::plugin_json::Components<'_>,
    actions: &mut Vec<Action>,
) -> Result<(), Error> {
    let mcp_path = plugin_dir.join(".mcp.json");
    fs.write_file(&mcp_path, templates::mcp_template().as_bytes())?;
    actions
        .push(Action::FileWritten { path: mcp_path, description: "MCP server config".to_string() });
    components.mcp_servers = Some("./.mcp.json");
    Ok(())
}

fn scaffold_hook(
    fs: &dyn Fs,
    plugin_dir: &Path,
    components: &mut crate::generate::plugin_json::Components<'_>,
    actions: &mut Vec<Action>,
) -> Result<(), Error> {
    let hooks_dir = plugin_dir.join("hooks");
    fs.create_dir_all(&hooks_dir)?;
    actions.push(Action::DirectoryCreated { path: hooks_dir.clone() });
    let hook_path = hooks_dir.join("hooks.json");
    fs.write_file(&hook_path, templates::hook_template().as_bytes())?;
    actions.push(Action::FileWritten { path: hook_path, description: "Hook config".to_string() });
    components.hooks = Some("./hooks/hooks.json");
    Ok(())
}

fn scaffold_output_style(
    opts: &PluginOpts<'_>,
    fs: &dyn Fs,
    plugin_dir: &Path,
    components: &mut crate::generate::plugin_json::Components<'_>,
    actions: &mut Vec<Action>,
) -> Result<(), Error> {
    let styles_dir = plugin_dir.join("output-styles");
    fs.create_dir_all(&styles_dir)?;
    actions.push(Action::DirectoryCreated { path: styles_dir.clone() });
    let style_path = styles_dir.join(format!("{}.md", opts.name));
    fs.write_file(&style_path, templates::output_style_template(opts.name).as_bytes())?;
    actions.push(Action::FileWritten { path: style_path, description: "Output style".to_string() });
    components.output_styles = Some("./output-styles/");
    Ok(())
}

fn scaffold_lsp(
    fs: &dyn Fs,
    plugin_dir: &Path,
    components: &mut crate::generate::plugin_json::Components<'_>,
    actions: &mut Vec<Action>,
) -> Result<(), Error> {
    let lsp_path = plugin_dir.join(".lsp.json");
    fs.write_file(&lsp_path, templates::lsp_template().as_bytes())?;
    actions
        .push(Action::FileWritten { path: lsp_path, description: "LSP server config".to_string() });
    components.lsp_servers = Some("./.lsp.json");
    Ok(())
}

fn scaffold_extension(
    fs: &dyn Fs,
    plugin_dir: &Path,
    components: &mut crate::generate::plugin_json::Components<'_>,
    actions: &mut Vec<Action>,
) -> Result<(), Error> {
    let ext_dir = plugin_dir.join("extensions");
    fs.create_dir_all(&ext_dir)?;
    actions.push(Action::DirectoryCreated { path: ext_dir.clone() });
    let gitkeep = ext_dir.join(".gitkeep");
    fs.write_file(&gitkeep, b"")?;
    actions.push(Action::FileWritten {
        path: gitkeep,
        description: "Extension placeholder".to_string(),
    });
    components.extensions = Some("./extensions/");
    Ok(())
}

/// Generate and write `plugin.json` into the `.claude-plugin/` directory.
fn write_plugin_json(
    opts: &PluginOpts<'_>,
    fs: &dyn Fs,
    claude_plugin_dir: &Path,
    components: &crate::generate::plugin_json::Components<'_>,
    actions: &mut Vec<Action>,
) -> Result<(), Error> {
    let description = format!("TODO: Describe {}", opts.name);
    let plugin_json_opts = crate::generate::plugin_json::Opts {
        name: opts.name,
        version: "0.1.0",
        description: &description,
    };
    let plugin_json_content =
        crate::generate::plugin_json::generate(&plugin_json_opts, Some(components));
    let plugin_json_path = claude_plugin_dir.join("plugin.json");
    fs.write_file(&plugin_json_path, plugin_json_content.as_bytes())?;
    actions.push(Action::FileWritten {
        path: plugin_json_path,
        description: "Plugin manifest".to_string(),
    });
    Ok(())
}

/// Register the plugin in `marketplace.json`.
fn register_in_marketplace(
    opts: &PluginOpts<'_>,
    fs: &dyn Fs,
    marketplace_json: &Path,
    actions: &mut Vec<Action>,
) -> Result<(), Error> {
    let description = format!("TODO: Describe {}", opts.name);
    let entry = crate::generate::marketplace::Entry { name: opts.name, description: &description };
    let was_registered = is_plugin_registered(fs, marketplace_json, opts.name);
    crate::generate::marketplace::register(fs, marketplace_json, &entry)?;
    if was_registered {
        actions.push(Action::PluginAlreadyRegistered { name: opts.name.to_string() });
    } else {
        actions.push(Action::PluginRegistered {
            name: opts.name.to_string(),
            marketplace_path: marketplace_json.to_path_buf(),
        });
    }
    Ok(())
}

/// Update Claude engine settings with the plugin entry.
fn update_engine_settings(
    opts: &PluginOpts<'_>,
    fs: &dyn Fs,
    marketplace_json: &Path,
    actions: &mut Vec<Action>,
) -> Result<(), Error> {
    // The .claude/ dir is a sibling of .ai/ (one level up from marketplace_dir)
    if let Some(project_root) = opts.marketplace_dir.parent() {
        let settings_dir = project_root.join(".claude");
        fs.create_dir_all(&settings_dir)?;
        let settings_path = settings_dir.join("settings.json");
        let mut settings = crate::generate::settings::read_or_create(fs, &settings_path)?;

        let marketplace_name = read_marketplace_name(fs, marketplace_json);
        let plugin_key = format!("{}@{}", opts.name, marketplace_name);

        let changed = crate::generate::settings::enable_plugin(&mut settings, &plugin_key);
        if changed {
            crate::generate::settings::write(fs, &settings_path, &settings)?;
            actions.push(Action::PluginEnabled { plugin_key, settings_path });
        } else {
            actions.push(Action::PluginAlreadyEnabled { plugin_key });
        }
    }
    Ok(())
}

/// Check if a plugin is already registered in marketplace.json.
fn is_plugin_registered(fs: &dyn Fs, marketplace_path: &Path, name: &str) -> bool {
    let Ok(content) = fs.read_to_string(marketplace_path) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };
    json.get("plugins").and_then(serde_json::Value::as_array).is_some_and(|plugins| {
        plugins.iter().any(|p| p.get("name").and_then(serde_json::Value::as_str) == Some(name))
    })
}

/// Read the marketplace name from marketplace.json, defaulting to
/// `"local-repo-plugins"`.
fn read_marketplace_name(fs: &dyn Fs, marketplace_path: &Path) -> String {
    let Ok(content) = fs.read_to_string(marketplace_path) else {
        return "local-repo-plugins".to_string();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return "local-repo-plugins".to_string();
    };
    json.get("name").and_then(serde_json::Value::as_str).unwrap_or("local-repo-plugins").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Mutex;

    struct MockFs {
        files: Mutex<HashMap<PathBuf, Vec<u8>>>,
    }

    impl MockFs {
        fn new() -> Self {
            Self { files: Mutex::new(HashMap::new()) }
        }

        fn seed(&self, path: &Path, content: &[u8]) {
            self.files
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(path.to_path_buf(), content.to_vec());
        }

        fn get_content(&self, path: &Path) -> Option<String> {
            self.files
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(path)
                .and_then(|b| String::from_utf8(b.clone()).ok())
        }
    }

    impl crate::fs::Fs for MockFs {
        fn exists(&self, path: &Path) -> bool {
            self.files.lock().unwrap_or_else(|p| p.into_inner()).contains_key(path)
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Ok(())
        }

        fn write_file(&self, path: &Path, content: &[u8]) -> std::io::Result<()> {
            self.files
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(path.to_path_buf(), content.to_vec());
            Ok(())
        }

        fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
            self.files
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(path)
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("not found: {}", path.display()),
                    )
                })
        }

        fn read_dir(&self, _: &Path) -> std::io::Result<Vec<crate::fs::DirEntry>> {
            Ok(Vec::new())
        }
    }

    /// Seed a minimal marketplace.json into the MockFs so that
    /// `marketplace::register` can read-modify-write it.
    fn seed_marketplace(fs: &MockFs, marketplace_dir: &Path) {
        let marketplace_json = marketplace_dir.join(".claude-plugin").join("marketplace.json");
        let content = crate::generate::marketplace::create("test-marketplace", &[]);
        fs.seed(&marketplace_json, content.as_bytes());
    }

    #[test]
    fn make_plugin_creates_skill_plugin() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        seed_marketplace(&fs, marketplace_dir);

        let opts = PluginOpts {
            marketplace_dir,
            name: "my-skill",
            engine: "claude",
            features: &[Feature::Skill],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        let dir_created_count =
            result.actions.iter().filter(|a| matches!(a, Action::DirectoryCreated { .. })).count();
        assert!(
            dir_created_count >= 3,
            "expected at least 3 DirectoryCreated, got {dir_created_count}"
        );

        let file_written_count =
            result.actions.iter().filter(|a| matches!(a, Action::FileWritten { .. })).count();
        assert!(
            file_written_count >= 2,
            "expected at least 2 FileWritten, got {file_written_count}"
        );

        assert!(
            result
                .actions
                .iter()
                .any(|a| matches!(a, Action::PluginRegistered { name, .. } if name == "my-skill")),
            "expected PluginRegistered for my-skill"
        );

        assert!(
            result.actions.iter().any(|a| matches!(a, Action::PluginEnabled { .. })),
            "expected PluginEnabled action"
        );

        assert!(
            result
                .actions
                .iter()
                .any(|a| matches!(a, Action::PluginCreated { name, .. } if name == "my-skill")),
            "expected PluginCreated for my-skill"
        );
    }

    #[test]
    fn make_plugin_creates_composite() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        seed_marketplace(&fs, marketplace_dir);

        let opts = PluginOpts {
            marketplace_dir,
            name: "composite",
            engine: "claude",
            features: &[Feature::Skill, Feature::Agent, Feature::Hook],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        let file_written: Vec<_> = result
            .actions
            .iter()
            .filter_map(|a| {
                if let Action::FileWritten { description, .. } = a {
                    Some(description.as_str())
                } else {
                    None
                }
            })
            .collect();

        assert!(file_written.contains(&"Skill definition"), "missing Skill definition");
        assert!(file_written.contains(&"Agent definition"), "missing Agent definition");
        assert!(file_written.contains(&"Hook config"), "missing Hook config");
        assert!(file_written.contains(&"Plugin manifest"), "missing Plugin manifest");

        // Check summary — last action should be PluginCreated with 3 features.
        let last = result.actions.last();
        assert!(
            matches!(last, Some(Action::PluginCreated { features, .. }) if features.len() == 3),
            "last action should be PluginCreated with 3 features"
        );
    }

    #[test]
    fn make_plugin_idempotent() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        let plugin_dir = marketplace_dir.join("existing-plugin");
        // Pre-create the plugin directory so it already exists.
        fs.seed(&plugin_dir, b"marker");

        let opts = PluginOpts {
            marketplace_dir,
            name: "existing-plugin",
            engine: "claude",
            features: &[Feature::Skill],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        assert_eq!(result.actions.len(), 1);
        assert!(matches!(result.actions.first(), Some(Action::DirectoryAlreadyExists { .. })));
    }

    #[test]
    fn make_plugin_registers_in_marketplace() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        seed_marketplace(&fs, marketplace_dir);

        let opts = PluginOpts {
            marketplace_dir,
            name: "registered-plugin",
            engine: "copilot",
            features: &[Feature::Mcp],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());

        let marketplace_json = marketplace_dir.join(".claude-plugin").join("marketplace.json");
        let content = fs.get_content(&marketplace_json);
        assert!(content.is_some(), "marketplace.json should exist");
        let content = content.unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        let plugins = v.get("plugins").and_then(serde_json::Value::as_array);
        assert!(plugins.is_some());
        let has_plugin = plugins.is_some_and(|arr| {
            arr.iter().any(|p| {
                p.get("name").and_then(serde_json::Value::as_str) == Some("registered-plugin")
            })
        });
        assert!(has_plugin, "registered-plugin should be in marketplace.json");
    }

    #[test]
    fn make_plugin_enables_in_settings() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        seed_marketplace(&fs, marketplace_dir);

        let opts = PluginOpts {
            marketplace_dir,
            name: "settings-test",
            engine: "claude",
            features: &[Feature::Skill],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());

        let settings_path = Path::new("/project/.claude/settings.json");
        let content = fs.get_content(settings_path);
        assert!(content.is_some(), ".claude/settings.json should exist");
        let content = content.unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        let enabled = v.get("enabledPlugins");
        assert!(enabled.is_some(), "enabledPlugins key should exist");
        let plugin_key = enabled.and_then(|e| e.get("settings-test@test-marketplace"));
        assert_eq!(plugin_key, Some(&serde_json::json!(true)));
    }

    #[test]
    fn make_plugin_copilot_no_settings() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        seed_marketplace(&fs, marketplace_dir);

        let opts = PluginOpts {
            marketplace_dir,
            name: "copilot-only",
            engine: "copilot",
            features: &[Feature::Skill],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        assert!(
            !result.actions.iter().any(|a| matches!(
                a,
                Action::PluginEnabled { .. } | Action::PluginAlreadyEnabled { .. }
            )),
            "copilot engine should not produce settings actions"
        );

        let settings_path = Path::new("/project/.claude/settings.json");
        assert!(fs.get_content(settings_path).is_none(), "no settings.json for copilot engine");
    }

    #[test]
    fn make_plugin_both_engines() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        seed_marketplace(&fs, marketplace_dir);

        let opts = PluginOpts {
            marketplace_dir,
            name: "both-engines",
            engine: "both",
            features: &[Feature::Skill],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        assert!(
            result.actions.iter().any(|a| matches!(a, Action::PluginEnabled { .. })),
            "both engine should produce PluginEnabled action"
        );
    }

    #[test]
    fn make_plugin_output_style_feature() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        seed_marketplace(&fs, marketplace_dir);

        let opts = PluginOpts {
            marketplace_dir,
            name: "style-plugin",
            engine: "claude",
            features: &[Feature::OutputStyle],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        // Verify output-styles directory was created
        assert!(
            result.actions.iter().any(|a| matches!(
                a,
                Action::DirectoryCreated { path } if path.to_string_lossy().contains("output-styles")
            )),
            "expected DirectoryCreated for output-styles"
        );

        // Verify the style markdown file was written
        assert!(
            result.actions.iter().any(|a| matches!(
                a,
                Action::FileWritten { description, .. } if description == "Output style"
            )),
            "expected FileWritten for Output style"
        );

        // Verify plugin.json was written
        let plugin_json_path =
            marketplace_dir.join("style-plugin").join(".claude-plugin").join("plugin.json");
        let content = fs.get_content(&plugin_json_path);
        assert!(content.is_some(), "plugin.json should exist");

        // Verify the summary action lists output-style
        assert!(
            result.actions.iter().any(|a| matches!(
                a,
                Action::PluginCreated { features, .. } if features.contains(&"output-style".to_string())
            )),
            "PluginCreated should list output-style feature"
        );
    }

    #[test]
    fn make_plugin_lsp_feature() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        seed_marketplace(&fs, marketplace_dir);

        let opts = PluginOpts {
            marketplace_dir,
            name: "lsp-plugin",
            engine: "copilot",
            features: &[Feature::Lsp],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        // Verify the LSP config file was written
        assert!(
            result.actions.iter().any(|a| matches!(
                a,
                Action::FileWritten { description, .. } if description == "LSP server config"
            )),
            "expected FileWritten for LSP server config"
        );

        // Verify .lsp.json content
        let lsp_path = marketplace_dir.join("lsp-plugin").join(".lsp.json");
        let content = fs.get_content(&lsp_path);
        assert!(content.is_some(), ".lsp.json should exist");
        let content = content.unwrap_or_default();
        assert!(content.contains("lspServers"), ".lsp.json should contain lspServers");

        // Copilot engine should NOT produce settings actions
        assert!(
            !result.actions.iter().any(|a| matches!(
                a,
                Action::PluginEnabled { .. } | Action::PluginAlreadyEnabled { .. }
            )),
            "copilot engine should not produce settings actions"
        );
    }

    #[test]
    fn make_plugin_extension_feature() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        seed_marketplace(&fs, marketplace_dir);

        let opts = PluginOpts {
            marketplace_dir,
            name: "ext-plugin",
            engine: "copilot",
            features: &[Feature::Extension],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        // Verify extensions directory was created
        assert!(
            result.actions.iter().any(|a| matches!(
                a,
                Action::DirectoryCreated { path } if path.to_string_lossy().contains("extensions")
            )),
            "expected DirectoryCreated for extensions"
        );

        // Verify the .gitkeep placeholder was written
        assert!(
            result.actions.iter().any(|a| matches!(
                a,
                Action::FileWritten { description, .. } if description == "Extension placeholder"
            )),
            "expected FileWritten for Extension placeholder"
        );

        // Verify .gitkeep file exists and is empty
        let gitkeep_path = marketplace_dir.join("ext-plugin").join("extensions").join(".gitkeep");
        let content = fs.get_content(&gitkeep_path);
        assert!(content.is_some(), ".gitkeep should exist");
        assert_eq!(content.unwrap_or_default(), "", ".gitkeep should be empty");
    }

    #[test]
    fn make_plugin_mcp_feature() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        seed_marketplace(&fs, marketplace_dir);

        let opts = PluginOpts {
            marketplace_dir,
            name: "mcp-standalone",
            engine: "claude",
            features: &[Feature::Mcp],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        // Verify MCP config file was written
        assert!(
            result.actions.iter().any(|a| matches!(
                a,
                Action::FileWritten { description, .. } if description == "MCP server config"
            )),
            "expected FileWritten for MCP server config"
        );

        // Verify .mcp.json content
        let mcp_path = marketplace_dir.join("mcp-standalone").join(".mcp.json");
        let content = fs.get_content(&mcp_path);
        assert!(content.is_some(), ".mcp.json should exist");
        let content = content.unwrap_or_default();
        assert!(content.contains("mcpServers"), ".mcp.json should contain mcpServers");

        // MCP creates only a file, no extra feature directory — only the
        // plugin dir and .claude-plugin dir should be DirectoryCreated.
        let dir_created_count =
            result.actions.iter().filter(|a| matches!(a, Action::DirectoryCreated { .. })).count();
        assert_eq!(
            dir_created_count, 2,
            "MCP should produce exactly 2 DirectoryCreated (plugin dir + .claude-plugin)"
        );
    }

    #[test]
    fn make_plugin_already_registered_in_marketplace() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");

        // Seed marketplace.json with the plugin already registered
        let marketplace_json = marketplace_dir.join(".claude-plugin").join("marketplace.json");
        let content = serde_json::json!({
            "name": "test-marketplace",
            "version": "0.1.0",
            "plugins": [
                { "name": "pre-registered", "description": "Already here" }
            ]
        });
        fs.seed(&marketplace_json, content.to_string().as_bytes());

        let opts = PluginOpts {
            marketplace_dir,
            name: "pre-registered",
            engine: "copilot",
            features: &[Feature::Skill],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        // Should see PluginAlreadyRegistered instead of PluginRegistered
        assert!(
            result.actions.iter().any(|a| matches!(
                a,
                Action::PluginAlreadyRegistered { name } if name == "pre-registered"
            )),
            "expected PluginAlreadyRegistered for pre-registered"
        );
        assert!(
            !result.actions.iter().any(
                |a| matches!(a, Action::PluginRegistered { name, .. } if name == "pre-registered")
            ),
            "should NOT see PluginRegistered for pre-registered"
        );
    }

    #[test]
    fn make_plugin_already_enabled_in_settings() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        seed_marketplace(&fs, marketplace_dir);

        // Pre-create settings.json with the plugin already enabled
        let settings_path = Path::new("/project/.claude/settings.json");
        let settings = serde_json::json!({
            "enabledPlugins": {
                "already-enabled@test-marketplace": true
            }
        });
        fs.seed(settings_path, settings.to_string().as_bytes());

        let opts = PluginOpts {
            marketplace_dir,
            name: "already-enabled",
            engine: "claude",
            features: &[Feature::Skill],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        // Should see PluginAlreadyEnabled instead of PluginEnabled
        assert!(
            result.actions.iter().any(|a| matches!(
                a,
                Action::PluginAlreadyEnabled { plugin_key } if plugin_key.contains("already-enabled")
            )),
            "expected PluginAlreadyEnabled for already-enabled"
        );
        assert!(
            !result.actions.iter().any(|a| matches!(a, Action::PluginEnabled { .. })),
            "should NOT see PluginEnabled when already enabled"
        );
    }

    #[test]
    fn read_marketplace_name_fallback_missing_file() {
        let fs = MockFs::new();
        let path = Path::new("/nonexistent/marketplace.json");
        let name = read_marketplace_name(&fs, path);
        assert_eq!(name, "local-repo-plugins");
    }

    #[test]
    fn read_marketplace_name_fallback_invalid_json() {
        let fs = MockFs::new();
        let path = Path::new("/project/marketplace.json");
        fs.seed(path, b"not valid json {{{");
        let name = read_marketplace_name(&fs, path);
        assert_eq!(name, "local-repo-plugins");
    }

    #[test]
    fn read_marketplace_name_fallback_missing_name_field() {
        let fs = MockFs::new();
        let path = Path::new("/project/marketplace.json");
        fs.seed(path, b"{\"version\": \"1.0\"}");
        let name = read_marketplace_name(&fs, path);
        assert_eq!(name, "local-repo-plugins");
    }

    #[test]
    fn read_marketplace_name_returns_actual_name() {
        let fs = MockFs::new();
        let path = Path::new("/project/marketplace.json");
        fs.seed(path, b"{\"name\": \"my-custom-marketplace\"}");
        let name = read_marketplace_name(&fs, path);
        assert_eq!(name, "my-custom-marketplace");
    }

    #[test]
    fn is_plugin_registered_returns_false_for_missing_file() {
        let fs = MockFs::new();
        let path = Path::new("/nonexistent/marketplace.json");
        assert!(!is_plugin_registered(&fs, path, "any-plugin"));
    }

    #[test]
    fn is_plugin_registered_returns_false_for_invalid_json() {
        let fs = MockFs::new();
        let path = Path::new("/project/marketplace.json");
        fs.seed(path, b"not json");
        assert!(!is_plugin_registered(&fs, path, "any-plugin"));
    }

    #[test]
    fn is_plugin_registered_returns_false_when_not_present() {
        let fs = MockFs::new();
        let path = Path::new("/project/marketplace.json");
        let content = serde_json::json!({
            "plugins": [
                { "name": "other-plugin" }
            ]
        });
        fs.seed(path, content.to_string().as_bytes());
        assert!(!is_plugin_registered(&fs, path, "my-plugin"));
    }

    #[test]
    fn is_plugin_registered_returns_true_when_present() {
        let fs = MockFs::new();
        let path = Path::new("/project/marketplace.json");
        let content = serde_json::json!({
            "plugins": [
                { "name": "my-plugin" }
            ]
        });
        fs.seed(path, content.to_string().as_bytes());
        assert!(is_plugin_registered(&fs, path, "my-plugin"));
    }

    #[test]
    fn make_plugin_marketplace_dir_at_root_skips_settings() {
        // When marketplace_dir has no parent (e.g. "/"), update_engine_settings
        // must silently skip the settings update rather than panicking.
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/");
        seed_marketplace(&fs, marketplace_dir);

        let opts = PluginOpts {
            marketplace_dir,
            name: "root-plugin",
            engine: "claude",
            features: &[Feature::Skill],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        // Plugin should still be created.
        assert!(
            result
                .actions
                .iter()
                .any(|a| matches!(a, Action::PluginCreated { name, .. } if name == "root-plugin")),
            "expected PluginCreated for root-plugin"
        );

        // No settings actions because marketplace_dir.parent() is None.
        assert!(
            !result.actions.iter().any(|a| matches!(
                a,
                Action::PluginEnabled { .. } | Action::PluginAlreadyEnabled { .. }
            )),
            "should not produce settings actions when marketplace_dir has no parent"
        );
    }

    #[test]
    fn make_plugin_both_engine_updates_settings() {
        // Covers the `opts.engine == "both"` branch: the first condition
        // (`opts.engine == "claude"`) is false but the second (`opts.engine == "both"`)
        // is true, so `update_engine_settings` must still be called.
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        seed_marketplace(&fs, marketplace_dir);

        let opts = PluginOpts {
            marketplace_dir,
            name: "dual-plugin",
            engine: "both",
            features: &[Feature::Skill],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        // "both" engine must produce a PluginEnabled settings action.
        assert!(
            result.actions.iter().any(|a| matches!(a, Action::PluginEnabled { .. })),
            "expected PluginEnabled action for 'both' engine"
        );

        // Summary action should record engine = "both".
        assert!(
            result.actions.iter().any(|a| matches!(
                a,
                Action::PluginCreated { engine, .. } if engine == "both"
            )),
            "PluginCreated should record engine = 'both'"
        );
    }

    #[test]
    fn make_plugin_emits_already_registered_for_existing_plugin() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");
        let marketplace_json = marketplace_dir.join(".claude-plugin").join("marketplace.json");
        // Seed marketplace.json with "existing-skill" already registered.
        let content = crate::generate::marketplace::create(
            "test-marketplace",
            &[crate::generate::marketplace::Entry {
                name: "existing-skill",
                description: "already here",
            }],
        );
        fs.seed(&marketplace_json, content.as_bytes());

        let opts = PluginOpts {
            marketplace_dir,
            name: "existing-skill",
            engine: "copilot",
            features: &[Feature::Skill],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        assert!(
            result.actions.iter().any(
                |a| matches!(a, Action::PluginAlreadyRegistered { name, .. } if name == "existing-skill")
            ),
            "expected PluginAlreadyRegistered when plugin already in marketplace"
        );
    }

    #[test]
    fn is_plugin_registered_returns_false_when_file_not_found() {
        let fs = MockFs::new();
        // No file seeded — read_to_string returns NotFound, function returns false.
        let path = Path::new("/nonexistent/marketplace.json");
        assert!(!is_plugin_registered(&fs, path, "any-plugin"));
    }

    #[test]
    fn is_plugin_registered_returns_false_when_json_invalid() {
        let fs = MockFs::new();
        let path = Path::new("/project/marketplace.json");
        fs.seed(path, b"not valid json {{ }}");
        assert!(!is_plugin_registered(&fs, path, "any-plugin"));
    }

    #[test]
    fn read_marketplace_name_returns_default_when_file_not_found() {
        let fs = MockFs::new();
        let path = Path::new("/nonexistent/marketplace.json");
        let name = read_marketplace_name(&fs, path);
        assert_eq!(name, "local-repo-plugins");
    }

    #[test]
    fn read_marketplace_name_returns_default_when_json_invalid() {
        let fs = MockFs::new();
        let path = Path::new("/project/marketplace.json");
        fs.seed(path, b"not valid json {{ }}");
        let name = read_marketplace_name(&fs, path);
        assert_eq!(name, "local-repo-plugins");
    }

    /// Covers the `if was_registered` True branch in `register_in_marketplace`.
    ///
    /// When the plugin is already present in marketplace.json before `plugin()` is
    /// called, `is_plugin_registered` returns true and the function emits
    /// `Action::PluginAlreadyRegistered` instead of `Action::PluginRegistered`.
    #[test]
    fn make_plugin_already_registered_emits_already_registered_action() {
        let fs = MockFs::new();
        let marketplace_dir = Path::new("/project/.ai");

        // Seed marketplace.json with the plugin already registered.
        let marketplace_json = marketplace_dir.join(".claude-plugin").join("marketplace.json");
        let content = crate::generate::marketplace::create(
            "test-marketplace",
            &[crate::generate::marketplace::Entry {
                name: "pre-registered",
                description: "Already here",
            }],
        );
        fs.seed(&marketplace_json, content.as_bytes());

        // The plugin directory does NOT exist yet, so the early-return guard is skipped.
        let opts = PluginOpts {
            marketplace_dir,
            name: "pre-registered",
            engine: "copilot",
            features: &[Feature::Skill],
        };

        let result = plugin(&opts, &fs);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| PluginResult { actions: Vec::new() });

        assert!(
            result
                .actions
                .iter()
                .any(|a| matches!(a, Action::PluginAlreadyRegistered { name } if name == "pre-registered")),
            "expected PluginAlreadyRegistered action when plugin is pre-registered"
        );
        assert!(
            !result.actions.iter().any(|a| matches!(a, Action::PluginRegistered { .. })),
            "must not emit PluginRegistered when plugin was already registered"
        );
    }
}
