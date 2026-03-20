//! Workspace initialization and `.ai/` marketplace scaffolding for `aipm init`.
//!
//! Creates a workspace `aipm.toml` at the repo root and/or a `.ai/` local
//! marketplace directory with a starter plugin. Tool-specific settings are
//! applied by [`ToolAdaptor`] implementations in the [`adaptors`] module.

pub mod adaptors;

use std::io::Write;
use std::path::{Path, PathBuf};

/// An adaptor integrates aipm's `.ai/` marketplace with a specific AI coding tool.
///
/// Each adaptor is responsible for writing or merging tool-specific configuration
/// files that point the tool at the `.ai/` marketplace directory.
pub trait ToolAdaptor {
    /// Human-readable name for user-facing output (e.g., "Claude Code").
    fn name(&self) -> &'static str;

    /// Apply tool-specific settings to the workspace directory.
    ///
    /// Returns `true` if files were written or modified, `false` if the tool
    /// was already configured and no changes were needed.
    ///
    /// # Errors
    ///
    /// Returns `Error` if I/O operations fail or existing config files cannot be parsed.
    fn apply(&self, dir: &Path) -> Result<bool, Error>;
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

/// Errors specific to workspace init.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The directory already has an `aipm.toml`.
    #[error("already initialized: aipm.toml already exists in {}", .0.display())]
    WorkspaceAlreadyInitialized(PathBuf),

    /// The `.ai/` marketplace directory already exists.
    #[error(".ai/ marketplace already exists in {}", .0.display())]
    MarketplaceAlreadyExists(PathBuf),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parse error in an existing settings file.
    #[error("JSON parse error in {}: {source}", path.display())]
    JsonParse {
        /// Path to the file that failed to parse.
        path: PathBuf,
        /// The underlying `serde_json` error.
        source: serde_json::Error,
    },
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
pub fn init(opts: &Options<'_>, adaptors: &[Box<dyn ToolAdaptor>]) -> Result<InitResult, Error> {
    let mut actions = Vec::new();

    if opts.workspace {
        init_workspace(opts.dir)?;
        actions.push(InitAction::WorkspaceCreated);
    }

    if opts.marketplace {
        scaffold_marketplace(opts.dir, opts.no_starter)?;
        actions.push(InitAction::MarketplaceCreated);

        for adaptor in adaptors {
            if adaptor.apply(opts.dir)? {
                actions.push(InitAction::ToolConfigured(adaptor.name().to_string()));
            }
        }
    }

    Ok(InitResult { actions })
}

// =============================================================================
// Workspace manifest generation
// =============================================================================

fn init_workspace(dir: &Path) -> Result<(), Error> {
    let manifest_path = dir.join("aipm.toml");
    if manifest_path.exists() {
        return Err(Error::WorkspaceAlreadyInitialized(dir.to_path_buf()));
    }

    let content = generate_workspace_manifest();

    // Validate round-trip
    crate::manifest::parse_and_validate(&content, None)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    std::fs::create_dir_all(dir)?;
    let mut file = std::fs::File::create(&manifest_path)?;
    file.write_all(content.as_bytes())?;

    Ok(())
}

fn generate_workspace_manifest() -> String {
    "# AI Plugin Manager — Workspace Configuration\n\
     # Docs: https://github.com/thelarkinn/aipm\n\
     \n\
     [workspace]\n\
     members = [\".ai/*\"]\n\
     plugins_dir = \".ai\"\n\
     \n\
     # Shared dependency versions for all workspace members.\n\
     # Members reference these via: dep = { workspace = \"^\" }\n\
     # [workspace.dependencies]\n\
     \n\
     # Direct registry installs (available project-wide).\n\
     # [dependencies]\n\
     \n\
     # Environment requirements for all plugins in this workspace.\n\
     # [environment]\n\
     # requires = [\"git\"]\n"
        .to_string()
}

// =============================================================================
// Marketplace scaffolding
// =============================================================================

fn scaffold_marketplace(dir: &Path, no_starter: bool) -> Result<(), Error> {
    let ai_dir = dir.join(".ai");
    if ai_dir.exists() {
        return Err(Error::MarketplaceAlreadyExists(dir.to_path_buf()));
    }

    // Always create .ai/ and .gitignore
    std::fs::create_dir_all(&ai_dir)?;
    write_file(
        &ai_dir.join(".gitignore"),
        "# Managed by aipm — registry-installed plugins are symlinked here.\n\
         # Do not edit the section between the markers.\n\
         # === aipm managed start ===\n\
         # === aipm managed end ===\n",
    )?;

    if no_starter {
        return Ok(());
    }

    let starter = ai_dir.join("starter");

    // Create directory tree
    std::fs::create_dir_all(starter.join(".claude-plugin"))?;
    std::fs::create_dir_all(starter.join("skills").join("scaffold-plugin"))?;
    std::fs::create_dir_all(starter.join("scripts"))?;
    std::fs::create_dir_all(starter.join("agents"))?;
    std::fs::create_dir_all(starter.join("hooks"))?;

    // Write all component files before manifest validation
    write_file(
        &starter.join("skills").join("scaffold-plugin").join("SKILL.md"),
        &generate_skill_template(),
    )?;
    write_file(&starter.join("scripts").join("scaffold-plugin.ts"), &generate_scaffold_script())?;
    write_file(&starter.join("agents").join("marketplace-scanner.md"), &generate_agent_template())?;
    write_file(&starter.join("hooks").join("hooks.json"), &generate_hook_template())?;

    // .ai/starter/aipm.toml
    let starter_manifest = generate_starter_manifest();
    write_file(&starter.join("aipm.toml"), &starter_manifest)?;

    // Validate starter manifest round-trips (with base_dir so component paths are checked)
    crate::manifest::parse_and_validate(&starter_manifest, Some(&starter))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    // .ai/starter/.claude-plugin/plugin.json
    write_file(&starter.join(".claude-plugin").join("plugin.json"), &generate_plugin_json())?;

    // .ai/starter/.mcp.json
    write_file(&starter.join(".mcp.json"), &generate_mcp_stub())?;

    Ok(())
}

fn generate_starter_manifest() -> String {
    "[package]\n\
     name = \"starter\"\n\
     version = \"0.1.0\"\n\
     type = \"composite\"\n\
     edition = \"2024\"\n\
     description = \"Default starter plugin — scaffold new plugins, scan your marketplace, and log tool usage\"\n\
     \n\
     # [dependencies]\n\
     # Add registry dependencies here, e.g.:\n\
     # shared-skill = \"^1.0\"\n\
     \n\
     [components]\n\
     skills = [\"skills/scaffold-plugin/SKILL.md\"]\n\
     agents = [\"agents/marketplace-scanner.md\"]\n\
     hooks = [\"hooks/hooks.json\"]\n\
     scripts = [\"scripts/scaffold-plugin.ts\"]\n"
        .to_string()
}

fn generate_plugin_json() -> String {
    "{\n\
     \x20 \"name\": \"starter\",\n\
     \x20 \"version\": \"0.1.0\",\n\
     \x20 \"description\": \"Default starter plugin — scaffold new plugins, scan your marketplace, and log tool usage\"\n\
     }\n"
    .to_string()
}

fn generate_skill_template() -> String {
    "---\n\
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
     \x20  node --experimental-strip-types .ai/starter/scripts/scaffold-plugin.ts <plugin-name>\n\
     \x20  ```\n\
     3. Report the created file tree to the user.\n\
     4. Suggest next steps: edit the generated `SKILL.md`, add agents or hooks, update `aipm.toml`.\n\
     \n\
     ## Notes\n\
     \n\
     - The script creates `.ai/<plugin-name>/` with a valid `aipm.toml` and starter skill.\n\
     - If the directory already exists, the script exits with an error message.\n\
     - After scaffolding, the user should customize the generated files.\n"
        .to_string()
}

fn generate_scaffold_script() -> String {
    "import { mkdirSync, writeFileSync, existsSync } from \"fs\";\n\
     import { join } from \"path\";\n\
     \n\
     const name = process.argv[2];\n\
     if (!name) {\n\
     \x20 process.stderr.write(\"Usage: node --experimental-strip-types scaffold-plugin.ts <plugin-name>\\n\");\n\
     \x20 process.exit(1);\n\
     }\n\
     \n\
     const aiDir = join(process.cwd(), \".ai\");\n\
     const pluginDir = join(aiDir, name);\n\
     \n\
     if (existsSync(pluginDir)) {\n\
     \x20 process.stderr.write(`Error: .ai/${name}/ already exists\\n`);\n\
     \x20 process.exit(1);\n\
     }\n\
     \n\
     mkdirSync(join(pluginDir, \".claude-plugin\"), { recursive: true });\n\
     mkdirSync(join(pluginDir, \"skills\", name), { recursive: true });\n\
     mkdirSync(join(pluginDir, \"agents\"), { recursive: true });\n\
     mkdirSync(join(pluginDir, \"hooks\"), { recursive: true });\n\
     \n\
     writeFileSync(\n\
     \x20 join(pluginDir, \"aipm.toml\"),\n\
     \x20 `[package]\\nname = \"${name}\"\\nversion = \"0.1.0\"\\ntype = \"composite\"\\nedition = \"2024\"\\ndescription = \"TODO: describe ${name}\"\\n\\n[components]\\nskills = [\"skills/${name}/SKILL.md\"]\\n`\n\
     );\n\
     \n\
     writeFileSync(\n\
     \x20 join(pluginDir, \"skills\", name, \"SKILL.md\"),\n\
     \x20 `---\\ndescription: TODO — describe when this skill should be invoked\\n---\\n\\n# ${name}\\n\\nReplace this with instructions for the AI agent.\\n`\n\
     );\n\
     \n\
     writeFileSync(\n\
     \x20 join(pluginDir, \".claude-plugin\", \"plugin.json\"),\n\
     \x20 JSON.stringify({ name, version: \"0.1.0\", description: `TODO: describe ${name}` }, null, 2) + \"\\n\"\n\
     );\n\
     \n\
     process.stdout.write(`Created .ai/${name}/ with starter structure\\n`);\n"
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

pub(crate) fn write_file(path: &Path, content: &str) -> Result<(), std::io::Error> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
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
        let content = generate_workspace_manifest();
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
        std::fs::File::create(scripts_dir.join("scaffold-plugin.ts")).ok();

        let content = generate_starter_manifest();
        let result = crate::manifest::parse_and_validate(&content, Some(&tmp));
        assert!(result.is_ok(), "starter manifest should round-trip: {result:?}");

        cleanup(&tmp);
    }

    #[test]
    fn init_workspace_creates_manifest() {
        let (tmp, _guard) = make_temp_dir("ws-create");
        let adaptors = default_adaptors();
        let opts = Options { dir: &tmp, workspace: true, marketplace: false, no_starter: false };
        let result = init(&opts, &adaptors);
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
        let opts = Options { dir: &tmp, workspace: false, marketplace: true, no_starter: false };
        let result = init(&opts, &adaptors);
        assert!(result.is_ok());
        assert!(result.is_ok_and(|r| r.actions.contains(&InitAction::MarketplaceCreated)));

        assert!(tmp.join(".ai").is_dir());
        assert!(tmp.join(".ai/starter/aipm.toml").exists());
        assert!(tmp.join(".ai/starter/.claude-plugin/plugin.json").exists());
        assert!(tmp.join(".ai/starter/skills/scaffold-plugin/SKILL.md").exists());
        assert!(tmp.join(".ai/starter/scripts/scaffold-plugin.ts").exists());
        assert!(tmp.join(".ai/starter/agents/marketplace-scanner.md").exists());
        assert!(tmp.join(".ai/starter/hooks/hooks.json").exists());
        assert!(tmp.join(".ai/starter/.mcp.json").exists());
        assert!(tmp.join(".ai/.gitignore").exists());

        cleanup(&tmp);
    }

    #[test]
    fn init_workspace_rejects_existing() {
        let (tmp, _guard) = make_temp_dir("ws-exists");
        std::fs::File::create(tmp.join("aipm.toml")).ok();

        let adaptors = default_adaptors();
        let opts = Options { dir: &tmp, workspace: true, marketplace: false, no_starter: false };
        let result = init(&opts, &adaptors);
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
        let opts = Options { dir: &tmp, workspace: false, marketplace: true, no_starter: false };
        let result = init(&opts, &adaptors);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("already exists")));

        cleanup(&tmp);
    }

    #[test]
    fn init_both_creates_everything() {
        let (tmp, _guard) = make_temp_dir("both");
        let adaptors = default_adaptors();
        let opts = Options { dir: &tmp, workspace: true, marketplace: true, no_starter: false };
        let result = init(&opts, &adaptors);
        assert!(result.is_ok());
        let r = result.ok();
        assert!(r.as_ref().is_some_and(|r| r.actions.contains(&InitAction::WorkspaceCreated)));
        assert!(r.as_ref().is_some_and(|r| r.actions.contains(&InitAction::MarketplaceCreated)));
        assert!(tmp.join("aipm.toml").exists());
        assert!(tmp.join(".ai/starter/aipm.toml").exists());

        cleanup(&tmp);
    }

    #[test]
    fn init_with_no_adaptors() {
        let (tmp, _guard) = make_temp_dir("no-adaptors");
        let adaptors: Vec<Box<dyn ToolAdaptor>> = vec![];
        let opts = Options { dir: &tmp, workspace: false, marketplace: true, no_starter: false };
        let result = init(&opts, &adaptors);
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
        let opts = Options { dir: &tmp, workspace: false, marketplace: true, no_starter: false };
        let result = init(&opts, &adaptors);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(tmp.join(".ai/.gitignore"));
        assert!(content.as_ref().is_ok_and(|c| c.contains("aipm managed start")));
        assert!(content.is_ok_and(|c| c.contains("aipm managed end")));

        cleanup(&tmp);
    }

    #[test]
    fn plugin_json_is_valid() {
        let json = generate_plugin_json();
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(parsed.is_ok());
        let v = parsed.ok();
        assert!(v.as_ref().is_some_and(|v| v.get("name").is_some()));
        assert!(v.as_ref().is_some_and(|v| v.get("version").is_some()));
        assert!(v.is_some_and(|v| v.get("description").is_some()));
    }

    #[test]
    fn skill_template_has_frontmatter() {
        let content = generate_skill_template();
        assert!(content.contains("description:"));
        assert!(content.starts_with("---\n"));
    }

    #[test]
    fn workspace_manifest_has_correct_members() {
        let content = generate_workspace_manifest();
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
        assert!(content.contains("mkdirSync"));
        assert!(content.contains("writeFileSync"));
        assert!(content.contains("experimental-strip-types"));
    }

    #[test]
    fn init_marketplace_no_starter() {
        let (tmp, _guard) = make_temp_dir("no-starter");
        let adaptors = default_adaptors();
        let opts = Options { dir: &tmp, workspace: false, marketplace: true, no_starter: true };
        let result = init(&opts, &adaptors);
        assert!(result.is_ok());
        assert!(result.is_ok_and(|r| r.actions.contains(&InitAction::MarketplaceCreated)));

        // .ai/ and .gitignore exist
        assert!(tmp.join(".ai").is_dir());
        assert!(tmp.join(".ai/.gitignore").exists());
        // starter/ does NOT exist
        assert!(!tmp.join(".ai/starter").exists());

        cleanup(&tmp);
    }

    #[test]
    fn init_no_starter_still_configures_tools() {
        let (tmp, _guard) = make_temp_dir("no-starter-tools");
        let adaptors = default_adaptors();
        let opts = Options { dir: &tmp, workspace: false, marketplace: true, no_starter: true };
        let result = init(&opts, &adaptors);
        assert!(result.is_ok());

        // Tool settings should still be applied
        assert!(tmp.join(".claude/settings.json").exists());
        // But no starter plugin
        assert!(!tmp.join(".ai/starter").exists());

        cleanup(&tmp);
    }
}
