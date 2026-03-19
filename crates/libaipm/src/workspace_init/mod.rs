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
        scaffold_marketplace(opts.dir)?;
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

fn scaffold_marketplace(dir: &Path) -> Result<(), Error> {
    let ai_dir = dir.join(".ai");
    if ai_dir.exists() {
        return Err(Error::MarketplaceAlreadyExists(dir.to_path_buf()));
    }

    let starter = ai_dir.join("starter");

    // Create directory tree
    std::fs::create_dir_all(starter.join(".claude-plugin"))?;
    std::fs::create_dir_all(starter.join("skills").join("hello"))?;
    std::fs::create_dir_all(starter.join("agents"))?;
    std::fs::create_dir_all(starter.join("hooks"))?;

    // .ai/.gitignore
    write_file(
        &ai_dir.join(".gitignore"),
        "# Managed by aipm — registry-installed plugins are symlinked here.\n\
         # Do not edit the section between the markers.\n\
         # === aipm managed start ===\n\
         # === aipm managed end ===\n",
    )?;

    // .ai/starter/skills/hello/SKILL.md (must be written before manifest validation)
    write_file(&starter.join("skills").join("hello").join("SKILL.md"), &generate_skill_template())?;

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

    // .gitkeep files
    write_file(&starter.join("agents").join(".gitkeep"), "")?;
    write_file(&starter.join("hooks").join(".gitkeep"), "")?;

    Ok(())
}

fn generate_starter_manifest() -> String {
    "[package]\n\
     name = \"starter\"\n\
     version = \"0.1.0\"\n\
     type = \"composite\"\n\
     edition = \"2024\"\n\
     description = \"Starter plugin — customize or rename this directory\"\n\
     \n\
     # [dependencies]\n\
     # Add registry dependencies here, e.g.:\n\
     # shared-skill = \"^1.0\"\n\
     \n\
     [components]\n\
     skills = [\"skills/hello/SKILL.md\"]\n"
        .to_string()
}

fn generate_plugin_json() -> String {
    "{\n\
     \x20 \"name\": \"starter\",\n\
     \x20 \"version\": \"0.1.0\",\n\
     \x20 \"description\": \"Starter plugin — customize or rename this directory\"\n\
     }\n"
    .to_string()
}

fn generate_skill_template() -> String {
    "---\n\
     description: A starter skill — describe what it does so Claude knows when to use it\n\
     ---\n\
     \n\
     # Hello Skill\n\
     \n\
     This is a starter skill template. Customize the description in the frontmatter\n\
     above so your AI coding tool can auto-discover when to invoke this skill.\n\
     \n\
     ## Instructions\n\
     \n\
     Replace this content with instructions for the AI agent when this skill is active.\n"
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
        let skill_dir = tmp.join("skills").join("hello");
        std::fs::create_dir_all(&skill_dir).ok();
        std::fs::File::create(skill_dir.join("SKILL.md")).ok();

        let content = generate_starter_manifest();
        let result = crate::manifest::parse_and_validate(&content, Some(&tmp));
        assert!(result.is_ok(), "starter manifest should round-trip: {result:?}");

        cleanup(&tmp);
    }

    #[test]
    fn init_workspace_creates_manifest() {
        let (tmp, _guard) = make_temp_dir("ws-create");
        let adaptors = default_adaptors();
        let result = init(&Options { dir: &tmp, workspace: true, marketplace: false }, &adaptors);
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
        let result = init(&Options { dir: &tmp, workspace: false, marketplace: true }, &adaptors);
        assert!(result.is_ok());
        assert!(result.is_ok_and(|r| r.actions.contains(&InitAction::MarketplaceCreated)));

        assert!(tmp.join(".ai").is_dir());
        assert!(tmp.join(".ai/starter/aipm.toml").exists());
        assert!(tmp.join(".ai/starter/.claude-plugin/plugin.json").exists());
        assert!(tmp.join(".ai/starter/skills/hello/SKILL.md").exists());
        assert!(tmp.join(".ai/starter/.mcp.json").exists());
        assert!(tmp.join(".ai/starter/agents/.gitkeep").exists());
        assert!(tmp.join(".ai/starter/hooks/.gitkeep").exists());
        assert!(tmp.join(".ai/.gitignore").exists());

        cleanup(&tmp);
    }

    #[test]
    fn init_workspace_rejects_existing() {
        let (tmp, _guard) = make_temp_dir("ws-exists");
        std::fs::File::create(tmp.join("aipm.toml")).ok();

        let adaptors = default_adaptors();
        let result = init(&Options { dir: &tmp, workspace: true, marketplace: false }, &adaptors);
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
        let result = init(&Options { dir: &tmp, workspace: false, marketplace: true }, &adaptors);
        assert!(result.is_err());
        let err = result.err();
        assert!(err.is_some_and(|e| e.to_string().contains("already exists")));

        cleanup(&tmp);
    }

    #[test]
    fn init_both_creates_everything() {
        let (tmp, _guard) = make_temp_dir("both");
        let adaptors = default_adaptors();
        let result = init(&Options { dir: &tmp, workspace: true, marketplace: true }, &adaptors);
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
        let result = init(&Options { dir: &tmp, workspace: false, marketplace: true }, &adaptors);
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
        let result = init(&Options { dir: &tmp, workspace: false, marketplace: true }, &adaptors);
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
}
