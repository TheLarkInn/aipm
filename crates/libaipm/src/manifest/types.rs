//! Manifest schema types for `aipm.toml`.
//!
//! These structs model the full manifest format used by both workspace root
//! manifests and plugin member manifests. Deserialization is handled via serde.

use libaipm_engine_spec::EngineSet;
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

    /// Engine compatibility list (e.g., `["claude", "copilot-cli"]`).
    /// `None` (field omitted) or `Some(EngineSet::empty())` (explicit
    /// empty list `engines = []`) means all engines.
    ///
    /// On disk this is stored as a TOML string array; in memory it is
    /// represented as an [`EngineSet`] bitflag set so callers can perform
    /// set-membership checks against `libaipm_engine_spec::EngineSet`
    /// directly. The TOML round-trip is handled by [`engine_set_serde`].
    ///
    /// **Validation:** if the manifest writes `engines = [...]` with a
    /// non-empty list whose entries are ALL unknown (no entry maps to an
    /// `Engine` variant), deserialization returns an error so the user's
    /// intended restriction isn't silently widened to "all engines". Mixed
    /// lists (some known + some unknown) drop the unknowns and keep the
    /// known bits.
    /// Engine names that are not recognised by the bundled engine schema
    /// are silently dropped on deserialize so manifests targeting future
    /// engines aipm doesn't yet know about still parse.
    #[serde(default, deserialize_with = "engine_set_serde::deserialize")]
    pub engines: Option<EngineSet>,

    /// Source redirect for marketplace stubs.
    pub source: Option<SourceRedirect>,
}

/// A source redirect declared in `[package.source]`.
///
/// Indicates this plugin is a stub whose actual code lives in an external
/// repository.  The CLI follows this redirect (max 1 level) to fetch the
/// real plugin content.
#[derive(Debug, Clone, Deserialize)]
pub struct SourceRedirect {
    /// Source type (e.g., `"git"`).
    #[serde(rename = "type")]
    pub redirect_type: Option<String>,
    /// Git clone URL.
    pub url: String,
    /// Optional subdirectory within the repository.
    pub path: Option<String>,
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
///
/// Supports both registry deps (`version`) and source deps (`git`, `github`,
/// `path`, `marketplace`).  Source fields are mutually exclusive with `version`.
#[derive(Debug, Clone, Deserialize)]
pub struct DetailedDependency {
    /// Version requirement string (registry deps).
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

    /// Git clone URL for source deps (e.g., `https://github.com/org/repo`).
    pub git: Option<String>,

    /// GitHub shorthand (`owner/repo`) — sugar for git.
    pub github: Option<String>,

    /// Local filesystem path for source deps.
    pub path: Option<String>,

    /// Marketplace name for marketplace source deps.
    pub marketplace: Option<String>,

    /// Plugin name within a marketplace (defaults to dep key name).
    pub name: Option<String>,

    /// Git ref (branch, tag, or commit SHA) for git/marketplace deps.
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
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

/// Serde adapter that deserializes a TOML string array into
/// `Option<EngineSet>`.
///
/// On deserialize:
///   * Field omitted → `None` (= "all engines" per `Package.engines`).
///   * Explicit empty list `engines = []` → `Some(EngineSet::empty())`
///     (= "all engines"). The two cases are equivalent at the lint
///     level today.
///   * Non-empty list with at least one known engine name → the bitset
///     of recognised names; unknown names are silently dropped so
///     manifests can target a known engine + an unknown future engine
///     without aipm rejecting them.
///   * Non-empty list whose names are ALL unknown → deserialization
///     error. This prevents the silent-widening bug where a list of
///     unknown future-engine names would resolve to `EngineSet::empty()`
///     and look indistinguishable from "no restriction".
///
/// A symmetric serializer can be added when [`Manifest`] gains a
/// `Serialize` derive; right now the type is deserialize-only and
/// emitting an unused `serialize` function would trip the workspace's
/// `dead_code` (and `ref_option`) lints.
mod engine_set_serde {
    use libaipm_engine_spec::{Engine, EngineSet};
    use serde::de::Error as DeError;
    use serde::{Deserialize, Deserializer};

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Option<EngineSet>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw: Option<Vec<String>> = Option::deserialize(deserializer)?;
        let Some(names) = raw else {
            return Ok(None);
        };
        if names.is_empty() {
            return Ok(Some(EngineSet::empty()));
        }
        let mut set = EngineSet::empty();
        for name in &names {
            if let Some(engine) = Engine::from_name(name) {
                set |= engine.as_set();
            }
        }
        if set.is_empty() {
            // All entries were unknown — reject so the user's restriction
            // intent isn't silently widened to "all engines".
            let known: Vec<&'static str> = Engine::ALL.iter().map(|e| e.name()).collect();
            return Err(D::Error::custom(format!(
                "[package].engines = {names:?} contains no known engine names; \
                 valid names are {known:?} (unknown names are dropped, \
                 but at least one known name must remain)"
            )));
        }
        Ok(Some(set))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Directly invoke `engine_set_serde::deserialize` with a JSON `null` value so
    /// that the `let Some(names) = raw else { return Ok(None) }` arm (line 333) is
    /// covered.  This path cannot be reached through TOML (which has no null literal),
    /// so it must be exercised at the deserializer boundary.
    #[test]
    fn engine_set_serde_null_returns_none() {
        use serde::de::IntoDeserializer;
        // serde_json::Value::Null implements IntoDeserializer and will make
        // Option::<Vec<String>>::deserialize return None, hitting the else arm.
        let de: serde_json::Value = serde_json::Value::Null;
        let result = engine_set_serde::deserialize(de.into_deserializer());
        assert!(result.is_ok(), "deserializing null should succeed: {result:?}");
        let result: Result<Option<EngineSet>, _> = result;
        assert!(result.unwrap().is_none(), "null engines should produce None");
    }
}
