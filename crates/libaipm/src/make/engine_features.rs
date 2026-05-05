//! Engine-to-feature mapping for `aipm make plugin`.
//!
//! Codifies which AI feature types are available for each engine and
//! provides validation. The data side is sourced from the schema-driven
//! [`libaipm_engine_spec::FEATURES_BY_ENGINE`] table; presentation
//! strings (CLI names and wizard labels) are kept in this file because
//! they're a libaipm concern.

use libaipm_engine_spec::{Engine, EngineFeatureSet, EngineSet, FeatureKind, FEATURES_BY_ENGINE};

/// Backward-compatible alias so existing callers can keep using
/// [`Feature`] while the canonical type lives in
/// [`libaipm_engine_spec::FeatureKind`].
pub type Feature = FeatureKind;

/// CLI-name / wizard-label presentation for [`FeatureKind`] variants.
///
/// The data-side type lives in `libaipm-engine-spec` and is shared with
/// the schema. Presentation strings stay here because they are a
/// libaipm concern.
pub trait FeatureExt {
    /// CLI flag value (used in `--feature`).
    fn cli_name(self) -> &'static str;
    /// Human-readable label for wizard prompts.
    fn label(self) -> &'static str;
    /// Parse from a CLI flag value.
    fn from_cli_name(s: &str) -> Option<Self>
    where
        Self: Sized;
    /// All known features.
    fn all() -> &'static [Self]
    where
        Self: Sized;
}

impl FeatureExt for FeatureKind {
    fn cli_name(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Agent => "agent",
            Self::Mcp => "mcp",
            Self::Hook => "hook",
            Self::OutputStyle => "output-style",
            Self::Lsp => "lsp",
            Self::Extension => "extension",
            Self::Command => "command",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Skill => "Skills (prompt templates)",
            Self::Agent => "Agents (autonomous sub-agents)",
            Self::Mcp => "MCP Servers (tool providers)",
            Self::Hook => "Hooks (lifecycle events)",
            Self::OutputStyle => "Output Styles (response formatting)",
            Self::Lsp => "LSP Servers (language intelligence)",
            Self::Extension => "Extensions (Copilot extensions)",
            Self::Command => "Slash Commands",
        }
    }

    fn from_cli_name(s: &str) -> Option<Self> {
        match s {
            "skill" => Some(Self::Skill),
            "agent" => Some(Self::Agent),
            "mcp" => Some(Self::Mcp),
            "hook" => Some(Self::Hook),
            "output-style" => Some(Self::OutputStyle),
            "lsp" => Some(Self::Lsp),
            "extension" => Some(Self::Extension),
            "command" => Some(Self::Command),
            _ => None,
        }
    }

    fn all() -> &'static [Self] {
        &[
            Self::Skill,
            Self::Agent,
            Self::Mcp,
            Self::Hook,
            Self::OutputStyle,
            Self::Lsp,
            Self::Extension,
            Self::Command,
        ]
    }
}

/// Map a single [`FeatureKind`] to its [`EngineFeatureSet`] bit.
#[must_use]
pub const fn feature_kind_bit(kind: FeatureKind) -> EngineFeatureSet {
    match kind {
        FeatureKind::Skill => EngineFeatureSet::SKILL,
        FeatureKind::Agent => EngineFeatureSet::AGENT,
        FeatureKind::Mcp => EngineFeatureSet::MCP,
        FeatureKind::Hook => EngineFeatureSet::HOOK,
        FeatureKind::OutputStyle => EngineFeatureSet::OUTPUT_STYLE,
        FeatureKind::Lsp => EngineFeatureSet::LSP,
        FeatureKind::Extension => EngineFeatureSet::EXTENSION,
        FeatureKind::Command => EngineFeatureSet::COMMAND,
    }
}

/// Returns the [`EngineFeatureSet`] bitmask for a given engine, sourced
/// from the schema-driven [`FEATURES_BY_ENGINE`] table.
#[must_use]
pub fn features_for_engine(engine: Engine) -> EngineFeatureSet {
    FEATURES_BY_ENGINE
        .iter()
        .find(|(e, _)| *e == engine)
        .map_or(EngineFeatureSet::empty(), |(_, set)| *set)
}

/// Returns the union of features across multiple engines (e.g. for
/// `"both"` CLI input that targets every variant in
/// [`Engine::ALL`](libaipm_engine_spec::Engine::ALL)).
#[must_use]
pub fn features_for_engines(engines: &[Engine]) -> EngineFeatureSet {
    engines.iter().fold(EngineFeatureSet::empty(), |acc, e| acc | features_for_engine(*e))
}

/// Returns the union of features for every engine in an
/// [`EngineSet`] bitset.
#[must_use]
pub fn features_for_engine_set(set: EngineSet) -> EngineFeatureSet {
    Engine::ALL
        .iter()
        .filter(|e| set.contains(engine_to_set_bit(**e)))
        .fold(EngineFeatureSet::empty(), |acc, e| acc | features_for_engine(*e))
}

/// Map a single [`Engine`] variant to its corresponding single-bit
/// [`EngineSet`] flag.
#[must_use]
pub const fn engine_to_set_bit(engine: Engine) -> EngineSet {
    match engine {
        Engine::Claude => EngineSet::CLAUDE,
        Engine::Copilot => EngineSet::COPILOT,
    }
}

/// Parse the user-facing engine CLI string (`"claude"`, `"copilot"`,
/// `"both"`) into an [`EngineSet`].
///
/// Also accepts the canonical kebab-case engine names returned by
/// [`Engine::name`](libaipm_engine_spec::Engine::name) (e.g.
/// `"copilot"`).
///
/// Returns `None` for unrecognised input.
#[must_use]
pub fn parse_engine_arg(s: &str) -> Option<EngineSet> {
    match s {
        "claude" => Some(EngineSet::CLAUDE),
        "copilot" => Some(EngineSet::COPILOT),
        "both" => Some(EngineSet::ALL),
        other => Engine::from_name(other).map(engine_to_set_bit),
    }
}

/// Validates that every requested feature is supported by the engine.
///
/// Returns `Ok(())` when every feature's bit is contained in the
/// engine's [`EngineFeatureSet`], or `Err(unsupported)` listing the
/// features that are not.
///
/// # Errors
///
/// Returns the unsupported features in the order they appear in
/// `features`.
pub fn validate_features(engine: Engine, features: &[FeatureKind]) -> Result<(), Vec<FeatureKind>> {
    validate_features_for_set(features_for_engine(engine), features)
}

/// Validates that every requested feature is supported by every engine
/// in `engines`. Returns the features unsupported by *any* of those
/// engines.
///
/// # Errors
///
/// Returns the unsupported features. Useful for the `"both"` CLI form
/// where a feature must be valid for both Claude and Copilot to be
/// accepted.
pub fn validate_features_for_engines(
    engines: &[Engine],
    features: &[FeatureKind],
) -> Result<(), Vec<FeatureKind>> {
    let supported =
        engines.iter().fold(EngineFeatureSet::all(), |acc, e| acc & features_for_engine(*e));
    validate_features_for_set(supported, features)
}

fn validate_features_for_set(
    supported: EngineFeatureSet,
    features: &[FeatureKind],
) -> Result<(), Vec<FeatureKind>> {
    let mut unsupported = Vec::new();
    for f in features {
        if !supported.contains(feature_kind_bit(*f)) {
            unsupported.push(*f);
        }
    }
    if unsupported.is_empty() {
        Ok(())
    } else {
        Err(unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_features_claude() {
        let feats = features_for_engine(Engine::Claude);
        assert!(feats.contains(EngineFeatureSet::SKILL));
        assert!(feats.contains(EngineFeatureSet::AGENT));
        assert!(feats.contains(EngineFeatureSet::MCP));
        assert!(feats.contains(EngineFeatureSet::HOOK));
        assert!(feats.contains(EngineFeatureSet::OUTPUT_STYLE));
        // Claude does NOT support LSP or Extension
        assert!(!feats.contains(EngineFeatureSet::LSP));
        assert!(!feats.contains(EngineFeatureSet::EXTENSION));
    }

    #[test]
    fn engine_features_copilot() {
        let feats = features_for_engine(Engine::Copilot);
        assert!(feats.contains(EngineFeatureSet::SKILL));
        assert!(feats.contains(EngineFeatureSet::AGENT));
        assert!(feats.contains(EngineFeatureSet::MCP));
        assert!(feats.contains(EngineFeatureSet::HOOK));
        assert!(feats.contains(EngineFeatureSet::LSP));
        assert!(feats.contains(EngineFeatureSet::EXTENSION));
        // Copilot does NOT support OutputStyle
        assert!(!feats.contains(EngineFeatureSet::OUTPUT_STYLE));
    }

    #[test]
    fn engine_features_both_is_union() {
        let feats = features_for_engines(Engine::ALL);
        // Every variant in the schema should appear.
        for kind in FeatureKind::all() {
            // Command may not be wired into either engine yet — only
            // assert presence for kinds that one of the engines claims.
            let any_engine_has_it = Engine::ALL
                .iter()
                .any(|e| features_for_engine(*e).contains(feature_kind_bit(*kind)));
            if any_engine_has_it {
                assert!(
                    feats.contains(feature_kind_bit(*kind)),
                    "missing feature in union: {kind:?}"
                );
            }
        }
    }

    #[test]
    fn validate_features_rejects_lsp_for_claude() {
        let result = validate_features(Engine::Claude, &[FeatureKind::Lsp]);
        assert!(result.is_err());
        let unsupported = result.unwrap_err();
        assert_eq!(unsupported, vec![FeatureKind::Lsp]);
    }

    #[test]
    fn validate_features_rejects_output_style_for_copilot() {
        let result = validate_features(Engine::Copilot, &[FeatureKind::OutputStyle]);
        assert!(result.is_err());
        let unsupported = result.unwrap_err();
        assert_eq!(unsupported, vec![FeatureKind::OutputStyle]);
    }

    #[test]
    fn validate_features_accepts_valid_combination() {
        assert!(
            validate_features(Engine::Claude, &[FeatureKind::Skill, FeatureKind::Agent]).is_ok()
        );
        assert!(validate_features(Engine::Copilot, &[FeatureKind::Skill, FeatureKind::Lsp]).is_ok());
        // For "both", every feature must be supported by every engine —
        // pick the intersection: skill, agent, mcp, hook.
        assert!(validate_features_for_engines(
            Engine::ALL,
            &[FeatureKind::Skill, FeatureKind::Agent, FeatureKind::Mcp, FeatureKind::Hook],
        )
        .is_ok());
    }

    #[test]
    fn validate_features_reports_multiple_unsupported() {
        let result = validate_features(Engine::Claude, &[FeatureKind::Lsp, FeatureKind::Extension]);
        assert!(result.is_err());
        let unsupported = result.unwrap_err();
        assert_eq!(unsupported.len(), 2);
        assert!(unsupported.contains(&FeatureKind::Lsp));
        assert!(unsupported.contains(&FeatureKind::Extension));
    }

    #[test]
    fn from_cli_name_roundtrip() {
        for feature in FeatureKind::all() {
            let name = feature.cli_name();
            let parsed = FeatureKind::from_cli_name(name);
            assert_eq!(parsed, Some(*feature), "roundtrip failed for {name}");
        }
    }

    #[test]
    fn from_cli_name_unknown_returns_none() {
        assert_eq!(FeatureKind::from_cli_name("widget"), None);
        assert_eq!(FeatureKind::from_cli_name(""), None);
    }

    #[test]
    fn labels_are_non_empty() {
        for feature in FeatureKind::all() {
            assert!(!feature.label().is_empty(), "empty label for {feature:?}");
        }
    }

    #[test]
    fn parse_engine_arg_accepts_legacy_strings() {
        assert_eq!(parse_engine_arg("claude"), Some(EngineSet::CLAUDE));
        assert_eq!(parse_engine_arg("copilot"), Some(EngineSet::COPILOT));
        assert_eq!(parse_engine_arg("both"), Some(EngineSet::ALL));
    }

    #[test]
    fn parse_engine_arg_rejects_legacy_copilot_cli_form() {
        // Legacy "copilot-cli" identifier was renamed to "copilot" canonically.
        // The deserializer no longer accepts the old form.
        assert_eq!(parse_engine_arg("copilot-cli"), None);
    }

    #[test]
    fn parse_engine_arg_rejects_unknown() {
        assert_eq!(parse_engine_arg("widget"), None);
        assert_eq!(parse_engine_arg(""), None);
    }

    #[test]
    fn engine_to_set_bit_matches_engine_set_constants() {
        assert_eq!(engine_to_set_bit(Engine::Claude), EngineSet::CLAUDE);
        assert_eq!(engine_to_set_bit(Engine::Copilot), EngineSet::COPILOT);
    }

    #[test]
    fn features_for_engine_set_matches_features_for_engines() {
        let from_set = features_for_engine_set(EngineSet::ALL);
        let from_slice = features_for_engines(Engine::ALL);
        assert_eq!(from_set, from_slice);
    }
}
