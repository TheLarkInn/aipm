//! Manifest schema types for `aipm.toml`.
//!
//! These structs model the full manifest format used by both workspace root
//! manifests and plugin member manifests. Deserialization is handled via serde.

use serde::Deserialize;
use std::collections::BTreeMap;

/// Top-level manifest — may contain `[package]`, `[workspace]`, or both.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Package metadata (present in member manifests).
    pub package: Option<Package>,

    /// Workspace configuration (present in root manifests).
    pub workspace: Option<Workspace>,

    /// Direct dependencies.
    pub dependencies: Option<BTreeMap<String, DependencySpec>>,

    /// Dependency overrides (root-level only).
    pub overrides: Option<BTreeMap<String, String>>,

    /// Component declarations.
    pub components: Option<Components>,

    /// Feature definitions.
    pub features: Option<BTreeMap<String, Vec<String>>>,

    /// Environment requirements.
    pub environment: Option<Environment>,

    /// Installation behavior controls.
    pub install: Option<Install>,

    /// Default catalog (root-level only).
    pub catalog: Option<BTreeMap<String, String>>,

    /// Named catalogs (root-level only).
    pub catalogs: Option<BTreeMap<String, BTreeMap<String, String>>>,
}

/// `[package]` section — core package metadata.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    /// Package name — lowercase alphanumeric + hyphens, optional `@scope/`.
    pub name: String,

    /// Semantic version string.
    pub version: String,

    /// Human-readable description.
    pub description: Option<String>,

    /// Plugin type.
    #[serde(rename = "type")]
    pub plugin_type: Option<String>,

    /// File allowlist for transfer format.
    pub files: Option<Vec<String>>,
}

/// `[workspace]` section — monorepo configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Workspace {
    /// Glob patterns for member directories.
    pub members: Vec<String>,

    /// Default plugin directory name.
    pub plugins_dir: Option<String>,

    /// Shared dependency catalog for workspace members.
    pub dependencies: Option<BTreeMap<String, DependencySpec>>,
}

/// A dependency specification — either a version string or a detailed object.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum DependencySpec {
    /// Simple version string, e.g. `"^1.0"`.
    Simple(String),

    /// Detailed specification with optional fields.
    Detailed(DetailedDependency),
}

/// Detailed dependency with version, workspace ref, optional flag, features.
#[derive(Debug, Clone, Deserialize)]
pub struct DetailedDependency {
    /// Version requirement string.
    pub version: Option<String>,

    /// Workspace protocol reference (`"^"`, `"="`, `"*"`).
    pub workspace: Option<String>,

    /// Whether this dependency is optional (activated by features).
    pub optional: Option<bool>,

    /// Whether to use default features.
    #[serde(rename = "default-features")]
    pub default_features: Option<bool>,

    /// Specific features to enable.
    pub features: Option<Vec<String>>,
}

/// `[components]` section — declares plugin component files.
///
/// Mirrors the Claude Code plugin component model: skills, commands (legacy),
/// agents, hooks, MCP servers, LSP servers, scripts, output styles, and settings.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct Components {
    /// Skill definition paths (`skills/<name>/SKILL.md`).
    pub skills: Option<Vec<String>>,

    /// Command file paths (`commands/*.md`) — legacy skill format.
    pub commands: Option<Vec<String>>,

    /// Agent definition paths (`agents/*.md`).
    pub agents: Option<Vec<String>>,

    /// Hook configuration paths (`hooks/hooks.json`).
    pub hooks: Option<Vec<String>>,

    /// MCP server config paths (`.mcp.json`).
    pub mcp_servers: Option<Vec<String>>,

    /// LSP server config paths (`.lsp.json`).
    pub lsp_servers: Option<Vec<String>>,

    /// Utility script paths (`scripts/`).
    pub scripts: Option<Vec<String>>,

    /// Output style paths.
    pub output_styles: Option<Vec<String>>,

    /// Settings file path (`settings.json`).
    pub settings: Option<Vec<String>>,
}

/// `[environment]` section — system and runtime requirements.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct Environment {
    /// Required system tools on PATH.
    pub requires: Option<Vec<String>>,

    /// Minimum aipm version.
    pub aipm: Option<String>,

    /// Supported platforms.
    pub platforms: Option<Vec<String>>,

    /// Strict mode — fail hard on missing deps.
    pub strict: Option<bool>,

    /// Environment variable requirements.
    pub variables: Option<EnvironmentVariables>,

    /// Runtime version constraints.
    pub runtime: Option<BTreeMap<String, String>>,
}

/// Environment variable declarations — supports simple list or detailed specs.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EnvironmentVariables {
    /// Simple list of required variable names.
    Simple {
        /// Required variable names.
        required: Vec<String>,
    },

    /// Detailed variable specifications.
    Detailed {
        /// Required variable names.
        required: Option<Vec<String>>,
        /// Detailed specs for individual variables.
        spec: Option<Vec<VariableSpec>>,
    },
}

/// Detailed specification for a single environment variable.
#[derive(Debug, Clone, Deserialize)]
pub struct VariableSpec {
    /// Variable name.
    pub name: String,
    /// Human-readable description.
    pub description: Option<String>,
    /// Whether the variable is required.
    pub required: Option<bool>,
    /// Default value if not set.
    pub default: Option<String>,
}

/// `[install]` section — installation behavior controls.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct Install {
    /// Allowlist of lifecycle scripts permitted to execute.
    pub allowed_build_scripts: Option<Vec<String>>,
}

/// Valid plugin types — matches Claude Code's component model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginType {
    /// A skill definition (`skills/<name>/SKILL.md`).
    Skill,
    /// An agent definition (`agents/*.md`).
    Agent,
    /// An MCP server (`.mcp.json`).
    Mcp,
    /// A hook definition (`hooks/hooks.json`).
    Hook,
    /// An LSP server (`.lsp.json`) for code intelligence.
    Lsp,
    /// A composite package with multiple component types.
    Composite,
}

impl std::str::FromStr for PluginType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "skill" => Ok(Self::Skill),
            "agent" => Ok(Self::Agent),
            "mcp" => Ok(Self::Mcp),
            "hook" => Ok(Self::Hook),
            "lsp" => Ok(Self::Lsp),
            "composite" => Ok(Self::Composite),
            other => Err(format!(
                "invalid plugin type: {other} — expected one of: skill, agent, mcp, hook, lsp, composite"
            )),
        }
    }
}
