//! Workspace initialization and `.ai/` marketplace scaffolding for `aipm init`.
//!
//! Creates a workspace `aipm.toml` at the repo root and/or a `.ai/` local
//! marketplace directory with a starter plugin. Tool-specific settings are
//! applied by [`ToolAdaptor`] implementations in the [`adaptors`] module.

pub mod adaptors;
pub mod error;

use std::path::Path;

use crate::fs::Fs;

pub use error::Error;

/// An adaptor integrates aipm's `.ai/` marketplace with a specific AI coding tool.
///
/// Each adaptor is responsible for writing or merging tool-specific configuration
/// files that point the tool at the `.ai/` marketplace directory.
pub trait ToolAdaptor {
    /// Human-readable name for user-facing output (e.g., "Claude Code").
    fn name(&self) -> &'static str;

    /// The engine variant this adaptor scaffolds for.
    ///
    /// Returned values come from
    /// [`libaipm_engine_spec::Engine`] so callers (notably the
    /// scaffold-set filter in [`init`]) can match adaptors against the
    /// user's selected engines without going through the human-readable
    /// `name()`.
    fn engine(&self) -> libaipm_engine_spec::Engine;

    /// Apply tool-specific settings to the workspace directory.
    ///
    /// `marketplace_name` is the user-chosen identifier for the local marketplace
    /// (e.g., `"local-repo-plugins"`). Adaptors should use it as the key when
    /// registering the marketplace in tool-specific config files and when
    /// constructing composite plugin keys (e.g., `"starter-aipm-plugin@{name}"`
    /// in `enabledPlugins`).
    ///
    /// When `no_starter` is `true`, adaptors should skip enabling the starter
    /// plugin (e.g., omit `enabledPlugins` entries) while still registering the
    /// marketplace directory.
    ///
    /// Returns `true` if files were written or modified, `false` if the tool
    /// was already configured and no changes were needed.
    ///
    /// # Errors
    ///
    /// Returns `Error` if I/O operations fail or existing config files cannot be parsed.
    fn apply(
        &self,
        dir: &Path,
        no_starter: bool,
        marketplace_name: &str,
        fs: &dyn Fs,
    ) -> Result<bool, Error>;
}

/// Options for workspace initialization.
pub struct Options<'a> {
    /// Target directory.
    pub dir: &'a Path,
    /// Generate workspace manifest.
    pub workspace: bool,
    /// Generate `.ai/` marketplace + tool settings.
    pub marketplace: bool,
    /// Skip the starter plugin (bare `.ai/` directory only).
    pub no_starter: bool,
    /// Generate `aipm.toml` plugin manifests (opt-in).
    pub manifest: bool,
    /// Marketplace name (e.g., `"local-repo-plugins"`).
    pub marketplace_name: &'a str,
    /// Engines to scaffold for. Filters the adaptor list passed to
    /// [`init`]: only adaptors whose [`ToolAdaptor::engine`] is contained
    /// in this set actually run. Empty set = no engine adaptors run.
    pub engines_scaffold: libaipm_engine_spec::EngineSet,
    /// Engines the project claims to support, written to
    /// `[workspace].engines` and `[package].engines` of the starter
    /// plugin. `None` (or `Some(EngineSet::empty())`) omits the field
    /// entirely (semantic: "all engines"). `Some(set)` writes the
    /// declared list.
    pub engines_support: Option<libaipm_engine_spec::EngineSet>,
}

/// Actions taken during initialization — used for user feedback.
///
/// Each variant represents a file or directory that was created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitAction {
    /// The workspace manifest (`aipm.toml`) was created.
    WorkspaceCreated,
    /// The `.ai/` marketplace directory was scaffolded.
    MarketplaceCreated,
    /// A tool-specific configuration was written or merged.
    /// The string is the human-readable tool name (e.g., "Claude Code").
    ToolConfigured(String),
}

/// Result of workspace initialization — list of actions taken.
pub struct InitResult {
    /// Actions that were performed.
    pub actions: Vec<InitAction>,
}

/// Initialize workspace and/or marketplace.
///
/// Tool-specific settings are applied by the provided adaptors after
/// marketplace scaffolding.
///
/// # Errors
///
/// Returns `Error` if the workspace manifest or `.ai/` directory already
/// exists, or if I/O operations fail.
pub fn init(
    opts: &Options<'_>,
    adaptors: &[Box<dyn ToolAdaptor>],
    fs: &dyn Fs,
) -> Result<InitResult, Error> {
    let mut actions = Vec::new();

    if opts.workspace {
        init_workspace(opts.dir, opts.engines_support, fs)?;
        actions.push(InitAction::WorkspaceCreated);
    }

    if opts.marketplace {
        scaffold_marketplace(
            opts.dir,
            opts.no_starter,
            opts.manifest,
            opts.marketplace_name,
            opts.engines_support,
            fs,
        )?;
        actions.push(InitAction::MarketplaceCreated);

        for adaptor in adaptors {
            // Spec G3 / Feature 9: skip adaptors whose engine is not in
            // the user's selected scaffold set.
            if !opts.engines_scaffold.contains(adaptor.engine().as_set()) {
                continue;
            }
            if adaptor.apply(opts.dir, opts.no_starter, opts.marketplace_name, fs)? {
                actions.push(InitAction::ToolConfigured(adaptor.name().to_string()));
            }
        }
    }

    Ok(InitResult { actions })
}

// =============================================================================
// Workspace manifest generation
// =============================================================================

fn init_workspace(
    dir: &Path,
    engines_support: Option<libaipm_engine_spec::EngineSet>,
    fs: &dyn Fs,
) -> Result<(), Error> {
    let manifest_path = dir.join("aipm.toml");
    if fs.exists(&manifest_path) {
        return Err(Error::WorkspaceAlreadyInitialized(dir.to_path_buf()));
    }

    let content = generate_workspace_manifest(engines_support);

    // Validate round-trip
    crate::manifest::parse_and_validate(&content, None)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    fs.create_dir_all(dir)?;
    fs.write_file(&manifest_path, content.as_bytes())?;

    Ok(())
}

fn generate_workspace_manifest(engines_support: Option<libaipm_engine_spec::EngineSet>) -> String {
    let members = vec![".ai/*".to_string()];
    let engines_vec = engine_set_to_canonical_names(engines_support);
    let engines_slice: Option<Vec<&str>> =
        engines_vec.as_ref().map(|v| v.iter().map(String::as_str).collect());
    crate::manifest::builder::build_workspace_manifest(
        &crate::manifest::builder::WorkspaceManifestOpts {
            members: &members,
            plugins_dir: Some(".ai"),
            engines: engines_slice.as_deref(),
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
        },
    )
}

// =============================================================================
// Marketplace scaffolding
// =============================================================================

/// Translate an `Options.engines_support` value into a `Vec<String>` of
/// canonical engine names suitable for the manifest builder.
///
/// Returns `None` when the field should be omitted from the on-disk
/// manifest entirely:
/// - input is `None` (no support set declared)
/// - input is `Some(EngineSet::empty())` (semantically "all engines")
/// - input is `Some(EngineSet::ALL)` (the full known set is the default,
///   no need to enumerate it)
///
/// Otherwise returns `Some(vec![...])` of canonical kebab-case names in
/// `Engine::ALL` declaration order.
fn engine_set_to_canonical_names(
    engines: Option<libaipm_engine_spec::EngineSet>,
) -> Option<Vec<String>> {
    let set = engines?;
    if set.is_empty() || set == libaipm_engine_spec::EngineSet::ALL {
        return None;
    }
    Some(
        libaipm_engine_spec::Engine::ALL
            .iter()
            .filter(|e| set.contains(e.as_set()))
            .map(|e| e.name().to_string())
            .collect(),
    )
}

fn scaffold_marketplace(
    dir: &Path,
    no_starter: bool,
    manifest: bool,
    marketplace_name: &str,
    engines_support: Option<libaipm_engine_spec::EngineSet>,
    fs: &dyn Fs,
) -> Result<(), Error> {
    let ai_dir = dir.join(".ai");
    if fs.exists(&ai_dir) {
        return Err(Error::MarketplaceAlreadyExists(dir.to_path_buf()));
    }

    // Always create .ai/ and .gitignore
    fs.create_dir_all(&ai_dir)?;
    let gitignore_header = concat!(
        "# Managed by aipm — registry-installed plugins are symlinked here.\n",
        "# Do not edit the section between the markers.\n",
        "# === aipm managed start ===\n",
    );
    let gitignore_footer = "# === aipm managed end ===\n";
    let gitignore_content = if no_starter {
        format!("{gitignore_header}{gitignore_footer}")
    } else {
        format!("{gitignore_header}.tool-usage.log\n{gitignore_footer}")
    };
    fs.write_file(&ai_dir.join(".gitignore"), gitignore_content.as_bytes())?;

    // Create marketplace.json in .ai/.claude-plugin/
    fs.create_dir_all(&ai_dir.join(".claude-plugin"))?;
    let initial_plugins = if no_starter {
        Vec::new()
    } else {
        vec![crate::generate::marketplace::Entry {
            name: "starter-aipm-plugin",
            description: "Default starter plugin \u{2014} scaffold new plugins, scan your marketplace, and log tool usage",
        }]
    };
    fs.write_file(
        &ai_dir.join(".claude-plugin").join("marketplace.json"),
        crate::generate::marketplace::create(marketplace_name, &initial_plugins).as_bytes(),
    )?;

    if no_starter {
        return Ok(());
    }

    let starter = ai_dir.join("starter-aipm-plugin");

    // Create directory tree
    fs.create_dir_all(&starter.join(".claude-plugin"))?;
    fs.create_dir_all(&starter.join("skills").join("scaffold-plugin"))?;
    fs.create_dir_all(&starter.join("scripts"))?;
    fs.create_dir_all(&starter.join("agents"))?;
    fs.create_dir_all(&starter.join("hooks"))?;

    // Write all component files before manifest validation
    fs.write_file(
        &starter.join("skills").join("scaffold-plugin").join("SKILL.md"),
        generate_skill_template().as_bytes(),
    )?;
    fs.write_file(
        &starter.join("scripts").join("scaffold-plugin.sh"),
        generate_scaffold_script().as_bytes(),
    )?;
    fs.write_file(
        &starter.join("agents").join("marketplace-scanner.md"),
        generate_agent_template().as_bytes(),
    )?;
    fs.write_file(&starter.join("hooks").join("hooks.json"), generate_hook_template().as_bytes())?;

    // .ai/starter-aipm-plugin/aipm.toml (only when --manifest is requested)
    if manifest {
        let starter_manifest = generate_starter_manifest(engines_support);
        fs.write_file(&starter.join("aipm.toml"), starter_manifest.as_bytes())?;

        // Validate starter manifest round-trips (with base_dir so component paths are checked)
        crate::manifest::parse_and_validate(&starter_manifest, Some(&starter))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    }

    // .ai/starter-aipm-plugin/.claude-plugin/plugin.json
    let plugin_json = crate::generate::plugin_json::generate(
        &crate::generate::plugin_json::Opts {
            name: "starter-aipm-plugin",
            version: "0.1.0",
            description: "Default starter plugin \u{2014} scaffold new plugins, scan your marketplace, and log tool usage",
        },
        Some(&crate::generate::plugin_json::Components {
            skills: Some("./skills/"),
            agents: Some("./agents/"),
            hooks: Some("./hooks/hooks.json"),
            ..crate::generate::plugin_json::Components::default()
        }),
    );
    fs.write_file(&starter.join(".claude-plugin").join("plugin.json"), plugin_json.as_bytes())?;

    // .ai/starter-aipm-plugin/.mcp.json
    fs.write_file(&starter.join(".mcp.json"), generate_mcp_stub().as_bytes())?;

    Ok(())
}

fn generate_starter_manifest(engines_support: Option<libaipm_engine_spec::EngineSet>) -> String {
    let skills = vec!["skills/scaffold-plugin/SKILL.md".to_string()];
    let agents = vec!["agents/marketplace-scanner.md".to_string()];
    let hooks = vec!["hooks/hooks.json".to_string()];
    let scripts = vec!["scripts/scaffold-plugin.sh".to_string()];

    let engines_vec = engine_set_to_canonical_names(engines_support);
    let engines_slice: Option<Vec<&str>> =
        engines_vec.as_ref().map(|v| v.iter().map(String::as_str).collect());
    crate::manifest::builder::build_plugin_manifest(
        &crate::manifest::builder::PluginManifestOpts {
            name: "starter-aipm-plugin",
            version: "0.1.0",
            plugin_type: Some("composite"),
            description: Some("Default starter plugin \u{2014} scaffold new plugins, scan your marketplace, and log tool usage"),
            engines: engines_slice.as_deref(),
        },
        Some(&crate::manifest::builder::PluginComponentsOpts {
            skills: Some(&skills),
            agents: Some(&agents),
            hooks: Some(&hooks),
            scripts: Some(&scripts),
            ..crate::manifest::builder::PluginComponentsOpts::default()
        }),
    )
}

fn generate_skill_template() -> String {
    "---\n\
     name: scaffold-plugin\n\
     description: Scaffold a new AI plugin in the .ai/ marketplace directory. Use when the user wants to create a new plugin, skill, agent, or hook package.\n\
     ---\n\
     \n\
     # Scaffold Plugin\n\
     \n\
     Create a new plugin in the `.ai/` marketplace directory.\n\
     \n\
     ## Instructions\n\
     \n\
     1. Ask the user for a plugin name (lowercase, hyphens allowed) if not provided.\n\
     2. Run the scaffolding script:\n\
     \x20  ```bash\n\
     \x20  bash .ai/starter-aipm-plugin/scripts/scaffold-plugin.sh <plugin-name>\n\
     \x20  ```\n\
     3. Report the created file tree to the user.\n\
     4. Suggest next steps: edit the generated `SKILL.md`, add agents or hooks.\n\
     \n\
     ## Notes\n\
     \n\
     - The script creates `.ai/<plugin-name>/` with starter plugin files, including a starter skill.\n\
     - If the directory already exists, `aipm make plugin` is idempotent (reports existing files without failing).\n\
     - After scaffolding, customize the scaffolded plugin files as needed.\n"
        .to_string()
}

fn generate_scaffold_script() -> String {
    "#!/usr/bin/env bash\n\
     set -euo pipefail\n\
     # Scaffold a new AI plugin using the aipm CLI.\n\
     # Usage: bash scaffold-plugin.sh <plugin-name> [claude|copilot|both]\n\
     aipm make plugin --name \"${1:?Plugin name required}\" --engine \"${2:-claude}\" --feature skill -y\n"
        .to_string()
}

fn generate_agent_template() -> String {
    "---\n\
     name: marketplace-scanner\n\
     description: Scan and explain the contents of the .ai/ marketplace directory. Use when the user wants to understand what plugins, skills, agents, or hooks are installed locally.\n\
     tools:\n\
     \x20 - Read\n\
     \x20 - Glob\n\
     \x20 - Grep\n\
     \x20 - LS\n\
     ---\n\
     \n\
     # Marketplace Scanner\n\
     \n\
     You are a read-only analysis agent for the `.ai/` marketplace directory.\n\
     \n\
     ## Instructions\n\
     \n\
     1. List all plugin directories under `.ai/` (each subdirectory with an `aipm.toml`).\n\
     2. For each plugin, read its `aipm.toml` and summarize:\n\
     \x20  - Package name, version, type, and description\n\
     \x20  - Declared components (skills, agents, hooks, scripts)\n\
     3. If asked about a specific component, read and explain its contents.\n\
     4. Never modify any files — you are read-only.\n\
     \n\
     ## Scope\n\
     \n\
     - Only scan files within the `.ai/` directory.\n\
     - Do not access files outside `.ai/` unless explicitly asked.\n\
     - Report any `aipm.toml` parse issues you encounter.\n"
        .to_string()
}

fn generate_hook_template() -> String {
    "{\n\
     \x20 \"hooks\": [\n\
     \x20   {\n\
     \x20     \"event\": \"PostToolUse\",\n\
     \x20     \"command\": \"echo \\\"$(date -u +%Y-%m-%dT%H:%M:%SZ) tool=$TOOL_NAME\\\" >> .ai/.tool-usage.log\"\n\
     \x20   }\n\
     \x20 ]\n\
     }\n"
    .to_string()
}

fn generate_mcp_stub() -> String {
    "{\n  \"mcpServers\": {}\n}\n".to_string()
}

// =============================================================================
// Helpers
// =============================================================================

pub(crate) fn write_file(path: &Path, content: &str, fs: &dyn Fs) -> Result<(), std::io::Error> {
    fs.write_file(path, content.as_bytes())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_dir(name: &str) -> (std::path::PathBuf, Option<Box<dyn std::any::Any>>) {
        let tmp = std::env::temp_dir().join(format!("aipm-test-wsinit-{name}"));
        if tmp.exists() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        std::fs::create_dir_all(&tmp).ok();
        (tmp, None)
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }

    fn default_adaptors() -> Vec<Box<dyn ToolAdaptor>> {
        adaptors::defaults()
    }

    #[test]
    fn workspace_manifest_round_trips() {
        let content = generate_workspace_manifest(None);
        let result = crate::manifest::parse_and_validate(&content, None);
        assert!(result.is_ok(), "workspace manifest should round-trip: {result:?}");
        let m = result.ok();
        assert!(m.is_some_and(|m| m.workspace.is_some()));
    }

    #[test]
    fn starter_manifest_round_trips() {
        let (tmp, _guard) = make_temp_dir("starter-rt");

        // Create all component files that the manifest declares
        let skill_dir = tmp.join("skills").join("scaffold-plugin");
        std::fs::create_dir_all(&skill_dir).ok();
        std::fs::File::create(skill_dir.join("SKILL.md")).ok();

        let agents_dir = tmp.join("agents");
        std::fs::create_dir_all(&agents_dir).ok();
        std::fs::File::create(agents_dir.join("marketplace-scanner.md")).ok();

        let hooks_dir = tmp.join("hooks");
        std::fs::create_dir_all(&hooks_dir).ok();
        std::fs::File::create(hooks_dir.join("hooks.json")).ok();

        let scripts_dir = tmp.join("scripts");
        std::fs::create_dir_all(&scripts_dir).ok();
        std::fs::File::create(scripts_dir.join("scaffold-plugin.sh")).ok();

        let content = generate_starter_manifest(Some(libaipm_engine_spec::EngineSet::CLAUDE));
        let result = crate::manifest::parse_and_validate(&content, Some(&tmp));
        assert!(result.is_ok(), "starter manifest should round-trip: {result:?}");

        cleanup(&tmp);
    }

    #[test]
    fn init_workspace_creates_manifest() {
        let (tmp, _guard) = make_temp_dir("ws-create");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: true,
            marketplace: false,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());
        assert!(result.is_ok_and(|r| r.actions.contains(&InitAction::WorkspaceCreated)));
        assert!(tmp.join("aipm.toml").exists());

        let content = std::fs::read_to_string(tmp.join("aipm.toml"));
        assert!(content.is_ok_and(|c| c.contains("[workspace]")));

        cleanup(&tmp);
    }

    #[test]
    fn init_marketplace_creates_tree() {
        let (tmp, _guard) = make_temp_dir("mp-create");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());
        assert!(result.is_ok_and(|r| r.actions.contains(&InitAction::MarketplaceCreated)));

        assert!(tmp.join(".ai").is_dir());
        assert!(tmp.join(".ai/starter-aipm-plugin/aipm.toml").exists());
        assert!(tmp.join(".ai/starter-aipm-plugin/.claude-plugin/plugin.json").exists());
        assert!(tmp.join(".ai/starter-aipm-plugin/skills/scaffold-plugin/SKILL.md").exists());
        assert!(tmp.join(".ai/starter-aipm-plugin/scripts/scaffold-plugin.sh").exists());
        assert!(tmp.join(".ai/starter-aipm-plugin/agents/marketplace-scanner.md").exists());
        assert!(tmp.join(".ai/starter-aipm-plugin/hooks/hooks.json").exists());
        assert!(tmp.join(".ai/starter-aipm-plugin/.mcp.json").exists());
        assert!(tmp.join(".ai/.gitignore").exists());

        cleanup(&tmp);
    }

    #[test]
    fn init_workspace_rejects_existing() {
        let (tmp, _guard) = make_temp_dir("ws-exists");
        std::fs::File::create(tmp.join("aipm.toml")).ok();

        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: true,
            marketplace: false,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("already initialized")));

        cleanup(&tmp);
    }

    #[test]
    fn init_marketplace_rejects_existing() {
        let (tmp, _guard) = make_temp_dir("mp-exists");
        std::fs::create_dir_all(tmp.join(".ai")).ok();

        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("already exists")));

        cleanup(&tmp);
    }

    #[test]
    fn init_both_creates_everything() {
        let (tmp, _guard) = make_temp_dir("both");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: true,
            marketplace: true,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());
        let r = result.ok();
        assert!(r.as_ref().is_some_and(|r| r.actions.contains(&InitAction::WorkspaceCreated)));
        assert!(r.as_ref().is_some_and(|r| r.actions.contains(&InitAction::MarketplaceCreated)));
        assert!(tmp.join("aipm.toml").exists());
        assert!(tmp.join(".ai/starter-aipm-plugin/aipm.toml").exists());

        cleanup(&tmp);
    }

    #[test]
    fn init_with_no_adaptors() {
        let (tmp, _guard) = make_temp_dir("no-adaptors");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = vec![];
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());
        assert!(tmp.join(".ai").is_dir());
        // No .claude/ directory should exist
        assert!(!tmp.join(".claude").exists());

        cleanup(&tmp);
    }

    #[test]
    fn gitignore_has_managed_markers() {
        let (tmp, _guard) = make_temp_dir("gitignore");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(tmp.join(".ai/.gitignore"));
        assert!(content.as_ref().is_ok_and(|c| c.contains("aipm managed start")));
        assert!(content.as_ref().is_ok_and(|c| c.contains("aipm managed end")));
        assert!(content.is_ok_and(|c| c.contains(".tool-usage.log")));

        cleanup(&tmp);
    }

    #[test]
    fn gitignore_no_starter_omits_tool_usage_log() {
        let (tmp, _guard) = make_temp_dir("gitignore_no_starter");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: true,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(tmp.join(".ai/.gitignore"));
        assert!(content.as_ref().is_ok_and(|c| c.contains("aipm managed start")));
        assert!(content.as_ref().is_ok_and(|c| c.contains("aipm managed end")));
        assert!(content.is_ok_and(|c| !c.contains(".tool-usage.log")));

        cleanup(&tmp);
    }

    #[test]
    fn plugin_json_is_valid() {
        let json = crate::generate::plugin_json::generate(
            &crate::generate::plugin_json::Opts {
                name: "starter-aipm-plugin",
                version: "0.1.0",
                description: "Default starter plugin",
            },
            Some(&crate::generate::plugin_json::Components {
                skills: Some("./skills/"),
                agents: Some("./agents/"),
                hooks: Some("./hooks/hooks.json"),
                ..crate::generate::plugin_json::Components::default()
            }),
        );
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(parsed.is_ok());
        let v = parsed.ok();
        assert!(v.as_ref().is_some_and(|v| v.get("name").is_some()));
        assert!(v.as_ref().is_some_and(|v| v.get("version").is_some()));
        assert!(v.as_ref().is_some_and(|v| v.get("description").is_some()));
        // Component keys should be present (fixes #356)
        assert!(v.as_ref().is_some_and(|v| v.get("skills").is_some()));
        assert!(v.is_some_and(|v| v.get("agents").is_some()));
    }

    #[test]
    fn skill_template_has_frontmatter() {
        let content = generate_skill_template();
        assert!(content.contains("description:"));
        assert!(content.starts_with("---\n"));
    }

    #[test]
    fn workspace_manifest_has_correct_members() {
        let content = generate_workspace_manifest(None);
        assert!(content.contains("members = [\".ai/*\"]"));
        assert!(content.contains("plugins_dir = \".ai\""));
    }

    #[test]
    fn agent_template_has_frontmatter() {
        let content = generate_agent_template();
        assert!(content.starts_with("---\n"));
        assert!(content.contains("name:"));
        assert!(content.contains("description:"));
        assert!(content.contains("tools:"));
        assert!(content.contains("- Read"));
        assert!(content.contains("- Glob"));
        assert!(content.contains("- Grep"));
        assert!(content.contains("- LS"));
    }

    #[test]
    fn hook_template_is_valid_json() {
        let json = generate_hook_template();
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(parsed.is_ok(), "hook template should be valid JSON: {parsed:?}");
        let v = parsed.ok();
        assert!(v.as_ref().is_some_and(|v| v.get("hooks").is_some()));
        assert!(v.is_some_and(|v| {
            v.get("hooks").and_then(|h| h.as_array()).is_some_and(|a| !a.is_empty())
        }));
    }

    #[test]
    fn scaffold_script_is_nonempty() {
        let content = generate_scaffold_script();
        assert!(!content.is_empty());
        assert!(content.contains("#!/usr/bin/env bash"));
        assert!(content.contains("set -euo pipefail"));
        assert!(content.contains("aipm make plugin"));
        assert!(content.contains("Plugin name required"));
    }

    #[test]
    fn scaffold_script_snapshot() {
        let content = generate_scaffold_script();
        insta::assert_snapshot!(content);
    }

    #[test]
    fn scaffold_script_delegates_to_aipm_cli() {
        let content = generate_scaffold_script();
        // Delegates to aipm CLI instead of doing manual fs operations
        assert!(content.contains("aipm make plugin"));
        assert!(content.contains("--name"));
        assert!(content.contains("--engine"));
        assert!(content.contains("-y"));
    }

    #[test]
    fn init_marketplace_no_starter() {
        let (tmp, _guard) = make_temp_dir("no-starter");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: true,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());
        assert!(result.is_ok_and(|r| r.actions.contains(&InitAction::MarketplaceCreated)));

        // .ai/ and .gitignore exist
        assert!(tmp.join(".ai").is_dir());
        assert!(tmp.join(".ai/.gitignore").exists());
        // starter/ does NOT exist
        assert!(!tmp.join(".ai/starter-aipm-plugin").exists());

        cleanup(&tmp);
    }

    #[test]
    fn marketplace_json_with_starter_is_valid() {
        let starter = crate::generate::marketplace::Entry {
            name: "starter-aipm-plugin",
            description: "Default starter plugin",
        };
        let json = crate::generate::marketplace::create("local-repo-plugins", &[starter]);
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(parsed.is_ok(), "marketplace.json should be valid JSON: {parsed:?}");
        let v = parsed.ok();
        assert!(v
            .as_ref()
            .is_some_and(|v| v.get("name").is_some_and(|n| n == "local-repo-plugins")));
        assert!(v.as_ref().is_some_and(|v| v.get("owner").is_some()));
        assert!(v.as_ref().is_some_and(|v| v.get("metadata").is_some()));
        let plugins = v.as_ref().and_then(|v| v.get("plugins")).and_then(|p| p.as_array());
        assert!(plugins.is_some_and(|p| p.len() == 1));
        let plugin = v
            .as_ref()
            .and_then(|v| v.get("plugins"))
            .and_then(|p| p.as_array())
            .and_then(|a| a.first());
        assert!(plugin.is_some_and(|p| {
            p.get("name").is_some_and(|n| n == "starter-aipm-plugin")
                && p.get("source").is_some_and(|s| s == "./starter-aipm-plugin")
                && p.get("description").is_some()
        }));
    }

    #[test]
    fn marketplace_json_no_starter_has_empty_plugins() {
        let json = crate::generate::marketplace::create("local-repo-plugins", &[]);
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(parsed.is_ok(), "marketplace.json should be valid JSON: {parsed:?}");
        let v = parsed.ok();
        assert!(v
            .as_ref()
            .is_some_and(|v| v.get("name").is_some_and(|n| n == "local-repo-plugins")));
        let plugins = v.as_ref().and_then(|v| v.get("plugins")).and_then(|p| p.as_array());
        assert!(plugins.is_some_and(|p| p.is_empty()));
    }

    #[test]
    fn init_marketplace_creates_marketplace_json() {
        let (tmp, _guard) = make_temp_dir("mp-json");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());

        let path = tmp.join(".ai/.claude-plugin/marketplace.json");
        assert!(path.exists(), "marketplace.json should be created");
        let content = std::fs::read_to_string(&path);
        assert!(content.is_ok());
        let parsed: Result<serde_json::Value, _> =
            serde_json::from_str(content.as_deref().unwrap_or(""));
        assert!(parsed.is_ok(), "marketplace.json should be valid JSON");
        let v = parsed.ok();
        assert!(v
            .as_ref()
            .is_some_and(|v| v.get("name").is_some_and(|n| n == "local-repo-plugins")));
        assert!(v.is_some_and(|v| {
            v.get("plugins")
                .and_then(|p| p.as_array())
                .and_then(|a| a.first())
                .and_then(|p| p.get("name"))
                .is_some_and(|n| n == "starter-aipm-plugin")
        }));

        cleanup(&tmp);
    }

    #[test]
    fn init_no_starter_creates_marketplace_json_with_empty_plugins() {
        let (tmp, _guard) = make_temp_dir("mp-json-nostarter");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: true,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());

        let path = tmp.join(".ai/.claude-plugin/marketplace.json");
        assert!(path.exists(), "marketplace.json should be created even with --no-starter");
        let content = std::fs::read_to_string(&path);
        assert!(content.is_ok());
        let parsed: Result<serde_json::Value, _> =
            serde_json::from_str(content.as_deref().unwrap_or(""));
        assert!(parsed.is_ok());
        let v = parsed.ok();
        assert!(v.is_some_and(|v| {
            v.get("plugins").and_then(|p| p.as_array()).is_some_and(|a| a.is_empty())
        }));

        cleanup(&tmp);
    }

    #[test]
    fn init_no_starter_still_configures_tools() {
        let (tmp, _guard) = make_temp_dir("no-starter-tools");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: true,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());

        // Tool settings should still be applied
        assert!(tmp.join(".claude/settings.json").exists());
        // But no starter plugin
        assert!(!tmp.join(".ai/starter-aipm-plugin").exists());

        // settings.json should have marketplace but NOT enabledPlugins with starter
        let content =
            std::fs::read_to_string(tmp.join(".claude/settings.json")).unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&content).ok().unwrap_or_default();
        assert!(
            v["extraKnownMarketplaces"]["local-repo-plugins"].is_object(),
            "marketplace should still be registered"
        );
        let has_starter = v
            .get("enabledPlugins")
            .and_then(|ep| ep.as_object())
            .is_some_and(|ep| ep.contains_key("starter-aipm-plugin@local-repo-plugins"));
        assert!(
            !has_starter,
            "enabledPlugins should not reference starter plugin when no_starter is true"
        );

        cleanup(&tmp);
    }

    #[test]
    fn init_marketplace_with_preconfigured_claude_settings() {
        let (tmp, _guard) = make_temp_dir("preconfigured");
        // Pre-create fully-configured .claude/settings.json AND
        // .github/copilot-instructions.md so both adaptors return Ok(false)
        // (the Copilot adaptor preserves any existing instructions file).
        assert!(std::fs::create_dir_all(tmp.join(".claude")).is_ok());
        assert!(std::fs::write(
            tmp.join(".claude/settings.json"),
            r#"{"extraKnownMarketplaces":{"local-repo-plugins":{"source":{"source":"directory","path":"./.ai"}}},"enabledPlugins":{"starter-aipm-plugin@local-repo-plugins":true}}"#,
        ).is_ok());
        assert!(std::fs::create_dir_all(tmp.join(".github")).is_ok());
        assert!(std::fs::write(
            tmp.join(".github/copilot-instructions.md"),
            "# user-managed instructions\n",
        )
        .is_ok());

        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            // Both adaptors must actually run so this test exercises both
            // idempotency paths (per the pre-seed setup above). With
            // `engines_scaffold: CLAUDE`, the Copilot adaptor would be
            // filtered out and only Claude's idempotent path would be tested.
            engines_scaffold: libaipm_engine_spec::EngineSet::ALL,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());
        // ToolConfigured should NOT be in actions (adaptor returned false)
        let r = result.ok();
        assert!(r.is_some_and(|r| !r
            .actions
            .iter()
            .any(|a| matches!(a, InitAction::ToolConfigured(_)))));

        cleanup(&tmp);
    }

    // =====================================================================
    // Mock Fs tests — I/O error path coverage
    // =====================================================================

    /// A mock filesystem that succeeds all operations but fails `write_file`
    /// when the target path contains `fail_suffix`.
    struct WriteFailAtSuffixFs {
        fail_suffix: &'static str,
    }

    impl crate::fs::Fs for WriteFailAtSuffixFs {
        fn exists(&self, _: &Path) -> bool {
            false
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Ok(())
        }

        fn write_file(&self, path: &Path, _: &[u8]) -> std::io::Result<()> {
            if path.to_string_lossy().contains(self.fail_suffix) {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("mock: write failed for suffix '{}'", self.fail_suffix),
                ))
            } else {
                Ok(())
            }
        }

        fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
            Ok(String::new())
        }

        fn read_dir(&self, _: &Path) -> std::io::Result<Vec<crate::fs::DirEntry>> {
            Ok(Vec::new())
        }
    }

    /// Covers the `write_file(marketplace.json)?` error branch (line 214): when
    /// writing marketplace.json fails, `scaffold_marketplace` propagates the error.
    #[test]
    fn scaffold_marketplace_write_marketplace_json_fails() {
        let tmp = std::path::PathBuf::from("/tmp/fake-mp-mktjson");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = vec![];
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: true,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let fs = WriteFailAtSuffixFs { fail_suffix: "marketplace.json" };
        let result = init(&opts, &adaptors, &fs);
        assert!(result.is_err(), "expected error when marketplace.json write fails");
    }

    /// Covers the `write_file(SKILL.md)?` error branch (line 233): when writing the
    /// skill template fails during starter-plugin scaffolding.
    #[test]
    fn scaffold_marketplace_write_skill_md_fails() {
        let tmp = std::path::PathBuf::from("/tmp/fake-mp-skillmd");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = vec![];
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let fs = WriteFailAtSuffixFs { fail_suffix: "SKILL.md" };
        let result = init(&opts, &adaptors, &fs);
        assert!(result.is_err(), "expected error when SKILL.md write fails");
    }

    /// Covers the `write_file(scaffold-plugin.sh)?` error branch (line 237).
    #[test]
    fn scaffold_marketplace_write_scaffold_sh_fails() {
        let tmp = std::path::PathBuf::from("/tmp/fake-mp-scaffoldsh");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = vec![];
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let fs = WriteFailAtSuffixFs { fail_suffix: "scaffold-plugin.sh" };
        let result = init(&opts, &adaptors, &fs);
        assert!(result.is_err(), "expected error when scaffold-plugin.sh write fails");
    }

    /// Covers the `write_file(marketplace-scanner.md)?` error branch (line 241).
    #[test]
    fn scaffold_marketplace_write_scanner_md_fails() {
        let tmp = std::path::PathBuf::from("/tmp/fake-mp-scannermd");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = vec![];
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let fs = WriteFailAtSuffixFs { fail_suffix: "marketplace-scanner.md" };
        let result = init(&opts, &adaptors, &fs);
        assert!(result.is_err(), "expected error when marketplace-scanner.md write fails");
    }

    struct FailDirFs;

    impl crate::fs::Fs for FailDirFs {
        fn exists(&self, _: &Path) -> bool {
            false
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "mock: permission denied",
            ))
        }

        fn write_file(&self, _: &Path, _: &[u8]) -> std::io::Result<()> {
            Ok(())
        }

        fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
            Ok(String::new())
        }

        fn read_dir(&self, _: &Path) -> std::io::Result<Vec<crate::fs::DirEntry>> {
            Ok(Vec::new())
        }
    }

    struct FailWriteFs;

    impl crate::fs::Fs for FailWriteFs {
        fn exists(&self, _: &Path) -> bool {
            false
        }

        fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Ok(())
        }

        fn write_file(&self, _: &Path, _: &[u8]) -> std::io::Result<()> {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "mock: disk full"))
        }

        fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
            Ok(String::new())
        }

        fn read_dir(&self, _: &Path) -> std::io::Result<Vec<crate::fs::DirEntry>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn init_workspace_fails_on_create_dir_error() {
        let tmp = std::path::PathBuf::from("/tmp/fake-ws-dir");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = vec![];
        let opts = Options {
            dir: &tmp,
            workspace: true,
            marketplace: false,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &FailDirFs);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("mock")));
    }

    #[test]
    fn init_workspace_fails_on_write_file_error() {
        let tmp = std::path::PathBuf::from("/tmp/fake-ws-write");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = vec![];
        let opts = Options {
            dir: &tmp,
            workspace: true,
            marketplace: false,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &FailWriteFs);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("mock")));
    }

    #[test]
    fn scaffold_marketplace_fails_on_create_dir_error() {
        let tmp = std::path::PathBuf::from("/tmp/fake-mp-dir");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = vec![];
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: true,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &FailDirFs);
        assert!(result.is_err());
    }

    #[test]
    fn scaffold_marketplace_fails_on_write_file_error() {
        let tmp = std::path::PathBuf::from("/tmp/fake-mp-write");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = vec![];
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: true,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &FailWriteFs);
        assert!(result.is_err());
    }

    #[test]
    fn init_marketplace_no_manifest_skips_aipm_toml() {
        let (tmp, _guard) = make_temp_dir("no-manifest");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());

        // Components should exist
        assert!(tmp.join(".ai/starter-aipm-plugin/skills/scaffold-plugin/SKILL.md").exists());
        assert!(tmp.join(".ai/starter-aipm-plugin/hooks/hooks.json").exists());
        assert!(tmp.join(".ai/starter-aipm-plugin/.claude-plugin/plugin.json").exists());
        // aipm.toml should NOT exist
        assert!(!tmp.join(".ai/starter-aipm-plugin/aipm.toml").exists());

        cleanup(&tmp);
    }

    #[test]
    fn init_marketplace_with_manifest_creates_aipm_toml() {
        let (tmp, _guard) = make_temp_dir("with-manifest");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());
        assert!(tmp.join(".ai/starter-aipm-plugin/aipm.toml").exists());

        cleanup(&tmp);
    }

    #[test]
    fn init_adaptor_returns_false_no_tool_configured_action() {
        // Covers the False branch of `if adaptor.apply(...)? { ... }` in `init()`:
        // when an adaptor returns Ok(false) (already configured, no changes needed),
        // the ToolConfigured action must NOT be pushed.
        struct NoOpAdaptor;
        impl ToolAdaptor for NoOpAdaptor {
            fn name(&self) -> &'static str {
                "NoOpAdaptor"
            }
            fn engine(&self) -> libaipm_engine_spec::Engine {
                libaipm_engine_spec::Engine::Claude
            }
            fn apply(
                &self,
                _: &Path,
                _: bool,
                _: &str,
                _: &dyn crate::fs::Fs,
            ) -> Result<bool, Error> {
                Ok(false)
            }
        }

        let (tmp, _guard) = make_temp_dir("adaptor-noop");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = vec![Box::new(NoOpAdaptor)];
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: true,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());
        let actions = result.ok().map(|r| r.actions).unwrap_or_default();
        // MarketplaceCreated should be present; ToolConfigured must NOT be present
        assert!(actions.iter().any(|a| matches!(a, InitAction::MarketplaceCreated)));
        assert!(!actions.iter().any(|a| matches!(a, InitAction::ToolConfigured(_))));

        cleanup(&tmp);
    }

    #[test]
    fn init_adaptor_error_propagates() {
        // Covers the `?` error branch at the `adaptor.apply(...)? ` call in `init()`:
        // when an adaptor returns Err, the error must propagate from `init()`.
        struct ErrorAdaptor;
        impl ToolAdaptor for ErrorAdaptor {
            fn name(&self) -> &'static str {
                "ErrorAdaptor"
            }
            fn engine(&self) -> libaipm_engine_spec::Engine {
                libaipm_engine_spec::Engine::Claude
            }
            fn apply(
                &self,
                _: &Path,
                _: bool,
                _: &str,
                _: &dyn crate::fs::Fs,
            ) -> Result<bool, Error> {
                Err(Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "mock adaptor error")))
            }
        }

        let (tmp, _guard) = make_temp_dir("adaptor-error");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = vec![Box::new(ErrorAdaptor)];
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: true,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("mock adaptor error")));

        cleanup(&tmp);
    }

    #[test]
    fn adaptor_apply_returns_false_when_already_configured() {
        // Pre-seed `.claude/settings.json` with the marketplace and starter
        // plugin already registered AND `.github/copilot-instructions.md`
        // so both default adaptors' `apply()` return `Ok(false)` —
        // exercising the `False` branch of `if adaptor.apply(…)?` at the
        // adaptor-loop in `init` (line 113).
        let (tmp, _guard) = make_temp_dir("adaptor-idempotent");

        // Pre-create the settings directory and file.
        let claude_dir = tmp.join(".claude");
        std::fs::create_dir_all(&claude_dir).ok();
        let settings = serde_json::json!({
            "extraKnownMarketplaces": {
                "local-repo-plugins": {
                    "source": { "source": "directory", "path": "./.ai" }
                }
            },
            "enabledPlugins": {
                "starter-aipm-plugin@local-repo-plugins": true
            }
        });
        std::fs::write(claude_dir.join("settings.json"), settings.to_string().as_bytes()).ok();
        // Same idempotency guarantee for the Copilot adaptor: an existing
        // instructions file means the adaptor preserves it and returns
        // `Ok(false)`.
        let github_dir = tmp.join(".github");
        std::fs::create_dir_all(&github_dir).ok();
        std::fs::write(github_dir.join("copilot-instructions.md"), b"# user content\n").ok();

        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            // Both adaptors must actually run so this test exercises both
            // idempotency paths (per the pre-seed of `.claude/settings.json`
            // AND `.github/copilot-instructions.md` above). With
            // `engines_scaffold: CLAUDE`, the Copilot adaptor would be
            // filtered out and only Claude's `Ok(false)` path would be tested.
            engines_scaffold: libaipm_engine_spec::EngineSet::ALL,
            engines_support: None,
        };
        let adaptors = default_adaptors();

        // `apply()` finds nothing to change and returns `Ok(false)`.
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(
            result.is_ok_and(|r| !r
                .actions
                .iter()
                .any(|a| matches!(a, InitAction::ToolConfigured(_)))),
            "init should NOT report ToolConfigured when settings are pre-configured"
        );

        cleanup(&tmp);
    }

    #[test]
    fn make_temp_dir_cleans_up_existing_directory() {
        // Pre-create the directory so that the `if tmp.exists()` branch in
        // `make_temp_dir` (the cleanup-before-recreate path) is exercised.
        let name = "pre-existing-cleanup";
        let tmp = std::env::temp_dir().join(format!("aipm-test-wsinit-{name}"));
        std::fs::create_dir_all(&tmp).ok();
        // Place a sentinel file to confirm the old tree is removed.
        std::fs::write(tmp.join("sentinel.txt"), b"old").ok();
        assert!(tmp.exists(), "pre-condition: directory must exist before make_temp_dir");

        let (path, _) = make_temp_dir(name);
        // The old directory (and its sentinel) was cleaned up and the dir was recreated.
        assert!(path.exists());
        assert!(!path.join("sentinel.txt").exists(), "old sentinel should have been removed");

        cleanup(&path);
    }

    #[test]
    fn init_adaptor_skips_when_settings_already_configured() {
        // Pre-populate .claude/settings.json with the marketplace and starter plugin
        // already present AND `.github/copilot-instructions.md` so both default
        // adaptors detect no changes and return `Ok(false)`. This exercises the
        // previously-uncovered False branch of `if adaptor.apply(...)`.
        let (tmp, _guard) = make_temp_dir("adaptor-skip");
        let settings_dir = tmp.join(".claude");
        std::fs::create_dir_all(&settings_dir).ok();
        // Write settings with the marketplace and plugin key already present.
        let settings_str = r#"{
  "extraKnownMarketplaces": {
    "local-repo-plugins": { "source": { "source": "directory", "path": "./.ai" } }
  },
  "enabledPlugins": {
    "starter-aipm-plugin@local-repo-plugins": true
  }
}"#;
        std::fs::write(settings_dir.join("settings.json"), settings_str.as_bytes()).ok();
        let github_dir = tmp.join(".github");
        std::fs::create_dir_all(&github_dir).ok();
        std::fs::write(github_dir.join("copilot-instructions.md"), b"# user content\n").ok();

        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: true,
            marketplace_name: "local-repo-plugins",
            // Both adaptors must run so this test actually exercises the
            // False branch of `if adaptor.apply(...)` for both engines
            // (per the pre-seed setup above).
            engines_scaffold: libaipm_engine_spec::EngineSet::ALL,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real);
        assert!(result.is_ok());
        // The adaptor returned false (nothing changed in settings), so no
        // ToolConfigured action should be present.
        assert!(result
            .is_ok_and(|r| !r.actions.iter().any(|a| matches!(a, InitAction::ToolConfigured(_)))));

        cleanup(&tmp);
    }

    // =====================================================================
    // Feature 9 — engines_scaffold filter + engines_support emission
    // =====================================================================

    #[test]
    fn init_with_claude_only_scaffold_skips_copilot_adaptor() {
        // engines_scaffold = CLAUDE only. Both default adaptors are passed
        // in, but the Copilot adaptor is filtered out by the new scaffold
        // filter, so `.github/copilot-instructions.md` is never created.
        let (tmp, _guard) = make_temp_dir("scaffold-claude-only");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::CLAUDE,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real).expect("init should succeed");
        assert!(tmp.join(".claude/settings.json").exists(), "claude scaffold expected");
        assert!(
            !tmp.join(".github/copilot-instructions.md").exists(),
            "copilot scaffold should be skipped when not in scaffold set"
        );
        // Only Claude's ToolConfigured action should be present.
        let tool_configured_count =
            result.actions.iter().filter(|a| matches!(a, InitAction::ToolConfigured(_))).count();
        assert_eq!(tool_configured_count, 1, "expected exactly one ToolConfigured action");
        cleanup(&tmp);
    }

    #[test]
    fn init_with_copilot_only_scaffold_skips_claude_adaptor() {
        // engines_scaffold = COPILOT only. The Claude adaptor is filtered
        // out so `.claude/` is NOT created (the bug from issue #724).
        let (tmp, _guard) = make_temp_dir("scaffold-copilot-only");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::COPILOT,
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real).expect("init should succeed");
        assert!(tmp.join(".github/copilot-instructions.md").exists(), "copilot scaffold expected");
        assert!(
            !tmp.join(".claude").exists(),
            "claude scaffold should be skipped when not in scaffold set (issue #724)"
        );
        let tool_configured_count =
            result.actions.iter().filter(|a| matches!(a, InitAction::ToolConfigured(_))).count();
        assert_eq!(tool_configured_count, 1, "expected exactly one ToolConfigured action");
        cleanup(&tmp);
    }

    #[test]
    fn init_with_empty_scaffold_runs_no_adaptors() {
        // engines_scaffold = empty. No adaptors run; no engine roots
        // appear on disk.
        let (tmp, _guard) = make_temp_dir("scaffold-empty");
        let adaptors = default_adaptors();
        let opts = Options {
            dir: &tmp,
            workspace: false,
            marketplace: true,
            no_starter: false,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::empty(),
            engines_support: None,
        };
        let result = init(&opts, &adaptors, &crate::fs::Real).expect("init should succeed");
        assert!(!tmp.join(".claude").exists());
        assert!(!tmp.join(".github").exists());
        // MarketplaceCreated should still be present (only the adaptor
        // loop is gated, not the marketplace scaffold itself).
        assert!(
            result.actions.iter().any(|a| matches!(a, InitAction::MarketplaceCreated)),
            "marketplace should still be scaffolded with empty engines_scaffold"
        );
        assert!(
            !result.actions.iter().any(|a| matches!(a, InitAction::ToolConfigured(_))),
            "no ToolConfigured action expected with empty engines_scaffold"
        );
        cleanup(&tmp);
    }

    #[test]
    fn init_workspace_with_narrow_support_writes_engines_field() {
        // engines_support = Some(CLAUDE) with workspace=true → workspace
        // aipm.toml gets `engines = ["claude"]`.
        let (tmp, _guard) = make_temp_dir("workspace-narrow-support");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = Vec::new();
        let opts = Options {
            dir: &tmp,
            workspace: true,
            marketplace: false,
            no_starter: false,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::empty(),
            engines_support: Some(libaipm_engine_spec::EngineSet::CLAUDE),
        };
        init(&opts, &adaptors, &crate::fs::Real).expect("init should succeed");
        let content = std::fs::read_to_string(tmp.join("aipm.toml")).unwrap_or_default();
        assert!(
            content.contains("engines = [\"claude\"]"),
            "workspace aipm.toml should contain engines = [\"claude\"]: {content}"
        );
        cleanup(&tmp);
    }

    #[test]
    fn init_workspace_with_default_support_omits_engines_field() {
        // engines_support = None → workspace aipm.toml omits engines field.
        let (tmp, _guard) = make_temp_dir("workspace-default-support");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = Vec::new();
        let opts = Options {
            dir: &tmp,
            workspace: true,
            marketplace: false,
            no_starter: false,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::empty(),
            engines_support: None,
        };
        init(&opts, &adaptors, &crate::fs::Real).expect("init should succeed");
        let content = std::fs::read_to_string(tmp.join("aipm.toml")).unwrap_or_default();
        assert!(
            !content.contains("engines ="),
            "workspace aipm.toml should NOT contain engines field: {content}"
        );
        cleanup(&tmp);
    }

    #[test]
    fn init_workspace_with_engineset_all_support_omits_engines_field() {
        // engines_support = Some(ALL) is the default state — should also
        // omit the field (no need to enumerate all known engines).
        let (tmp, _guard) = make_temp_dir("workspace-all-support");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = Vec::new();
        let opts = Options {
            dir: &tmp,
            workspace: true,
            marketplace: false,
            no_starter: false,
            manifest: false,
            marketplace_name: "local-repo-plugins",
            engines_scaffold: libaipm_engine_spec::EngineSet::empty(),
            engines_support: Some(libaipm_engine_spec::EngineSet::ALL),
        };
        init(&opts, &adaptors, &crate::fs::Real).expect("init should succeed");
        let content = std::fs::read_to_string(tmp.join("aipm.toml")).unwrap_or_default();
        assert!(
            !content.contains("engines ="),
            "workspace aipm.toml should NOT contain engines field when ALL: {content}"
        );
        cleanup(&tmp);
    }

    #[test]
    fn engine_set_to_canonical_names_truth_table() {
        use libaipm_engine_spec::EngineSet;

        assert_eq!(engine_set_to_canonical_names(None), None, "None input → None");
        assert_eq!(
            engine_set_to_canonical_names(Some(EngineSet::empty())),
            None,
            "empty bitset → None"
        );
        assert_eq!(
            engine_set_to_canonical_names(Some(EngineSet::ALL)),
            None,
            "ALL bitset → None (omit field)"
        );
        assert_eq!(
            engine_set_to_canonical_names(Some(EngineSet::CLAUDE)),
            Some(vec!["claude".to_string()]),
            "single bit → single name"
        );
        assert_eq!(
            engine_set_to_canonical_names(Some(EngineSet::COPILOT)),
            Some(vec!["copilot".to_string()]),
            "single bit → single name"
        );
    }
}
