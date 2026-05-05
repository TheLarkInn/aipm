//! Canonical Rust types for the engine API schema.
//!
//! These types are the single source of truth for the schema dialect the
//! reverse-binary-analysis workflow emits to `data/engine-api-schema.json`.
//! They are mirrored to JSON Schema via `schemars` and serialized to
//! `schemas/engine-api.schema.json` by `bin/export-schema.rs`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Top-level meta-schema version. Bumped on breaking schema changes; the
/// data file's `meta_schema_version` must match this string at build time.
pub const META_SCHEMA_VERSION: &str = "2.0.0";

/// Root document — the shape of `data/engine-api-schema.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EngineApiSchemaFile {
    /// Self-referential pointer to the schema this file conforms to.
    #[serde(rename = "$schema")]
    pub schema_uri: String,
    pub meta_schema_version: String,
    /// ISO-8601 generation timestamp written by the workflow.
    pub generated_at: String,
    pub engines: Vec<EngineBootstrap>,
    pub versions: BTreeMap<String, String>,
    pub apis: BTreeMap<String, EngineApi>,
    pub tool_compatibility: ToolCompatibility,
    pub suggestions: BTreeMap<String, EngineSuggestions>,
}

/// Bootstrap information for an engine — how to acquire its binary.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EngineBootstrap {
    /// Engine identifier (e.g. "claude", "copilot").
    pub name: String,
    /// Distribution channel ("npm", "github-release", ...).
    pub source: String,
    /// Package identifier in the source channel.
    pub package: String,
}

/// Per-engine API surface, keyed by engine name in the parent map.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EngineApi {
    pub manifest_fields: Vec<ManifestFieldSpec>,
    #[serde(default)]
    pub manifest_search_paths: Vec<String>,
    pub settings_paths: Vec<String>,
    pub folder_conventions: Vec<String>,
    pub convention_files: Vec<ConventionFile>,
    #[serde(default)]
    pub skill_registration: serde_json::Value,
    #[serde(default)]
    pub lsp_config: serde_json::Value,
    #[serde(default)]
    pub mcp_config: serde_json::Value,
    #[serde(default)]
    pub output_styles: Vec<String>,
    pub size_limits: SizeLimits,
    /// Documentation only. Not consumed by Rust code.
    #[serde(default)]
    pub detection_notes: Vec<String>,
    /// Documentation only. Not consumed by Rust code.
    #[serde(default)]
    pub discovery_notes: Vec<String>,
    /// Documentation only. Not consumed by Rust code.
    #[serde(default)]
    pub rule_notes: Vec<String>,
    pub tool_calls: Vec<ToolCall>,
    pub hook_events: Vec<HookEvent>,
    #[serde(default)]
    pub agent_commands: Vec<String>,
    #[serde(default)]
    pub feature_flags: Vec<String>,
    pub features: Vec<FeatureSpec>,
}

/// Specification for a single manifest field.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManifestFieldSpec {
    pub name: String,
    /// JSON-schema-style primitive: "string" | "number" | "boolean" | "array" | "object" | "url" | ...
    pub r#type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub constraints: Constraints,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Optional value-level constraints attached to a manifest field.
#[derive(Default, Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Constraints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

/// A "convention file" (CLAUDE.md, AGENTS.md, ...) and the directories it can live in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConventionFile {
    pub filename: String,
    pub convention_paths: Vec<String>,
}

/// Engine-imposed size limits (e.g. `Bash.timeout`, `plugin.name`).
///
/// Numeric ceilings are flattened — every unknown numeric key collected at
/// the top level of `size_limits` becomes an entry in `numeric`. The
/// optional `notes` field surfaces qualitative caveats from the workflow.
#[derive(Default, Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct SizeLimits {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub numeric: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// A single tool call recognised by an engine.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolCall {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub notes: Option<String>,
}

/// A hook event recognised by an engine.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HookEvent {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub notes: Option<String>,
}

/// A single discoverable feature an engine supports (skills, agents, hooks, ...).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FeatureSpec {
    pub kind: FeatureKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

/// Closed enumeration of feature kinds. Hand-written so the variants are
/// stable across schema regenerations and consumers can `match` exhaustively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeatureKind {
    Skill,
    Agent,
    Mcp,
    Hook,
    OutputStyle,
    Lsp,
    Extension,
    Command,
}

bitflags::bitflags! {
    /// Bitflag set over [`FeatureKind`] variants, used by the generated
    /// `FEATURES_BY_ENGINE` const to describe the kinds each engine
    /// supports without allocating a `Vec<FeatureKind>`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct EngineFeatureSet: u8 {
        const SKILL        = 0b0000_0001;
        const AGENT        = 0b0000_0010;
        const MCP          = 0b0000_0100;
        const HOOK         = 0b0000_1000;
        const OUTPUT_STYLE = 0b0001_0000;
        const LSP          = 0b0010_0000;
        const EXTENSION    = 0b0100_0000;
        const COMMAND      = 0b1000_0000;
    }
}

/// Cross-engine tool compatibility classification.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolCompatibility {
    pub shared_tools: Vec<String>,
    pub engine_exclusive_tools: BTreeMap<String, ToolSupport>,
}

/// The set of engines that do / do not support a given tool name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolSupport {
    /// Engine name strings (matched against `engines[].name`).
    pub supported_by: Vec<String>,
    pub unsupported_by: Vec<String>,
}

/// Free-form remediation suggestions emitted by the workflow per engine.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EngineSuggestions {
    #[serde(default)]
    pub adaptor_fixes: Vec<String>,
    #[serde(default)]
    pub test_cases: Vec<String>,
    #[serde(default)]
    pub behaviour_variants: Vec<String>,
}

/// Static const-table form of a hook event, emitted by `build.rs` into
/// the generated `HOOK_EVENTS_BY_ENGINE` const. The wire-format
/// counterpart is [`HookEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookEventStatic {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub deprecated: bool,
    pub notes: Option<&'static str>,
}

/// Static specification for a single engine, emitted by `build.rs` into
/// the generated `ENGINES` const table. All fields are `&'static` so the
/// table is usable in const contexts and never allocates.
///
/// `marker_paths` and `marketplace_manifest_path` are not tracked
/// directly in the schema today; `build.rs` keeps engine-name-keyed
/// helpers for them (with the copilot `.toml`→`.json` correction per
/// the schema-wins decision in Q2b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineSpec {
    pub name: &'static str,
    pub package: &'static str,
    pub version: &'static str,
    pub marker_paths: &'static [&'static str],
    pub marketplace_manifest_path: &'static str,
    pub manifest_search_paths: &'static [&'static str],
    pub settings_paths: &'static [&'static str],
    pub folder_conventions: &'static [&'static str],
    pub convention_files: &'static [(&'static str, &'static [&'static str])],
}
